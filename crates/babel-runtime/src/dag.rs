use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DagError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("artifact result was rejected because its fencing token is stale")]
    StaleFence,
    #[error("artifact record violates the state invariant: {0}")]
    InvalidRecord(&'static str),
    #[error("artifact dependency would create a cycle")]
    Cycle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Claim {
    Acquired { fencing_token: i64 },
    Busy { owner: String, lease_until_ms: i64 },
    Ready { output_hash: Vec<u8> },
    Blocked { pending_dependencies: i64 },
}

pub fn initialize(connection: &Connection) -> Result<(), DagError> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS artifact_record (
            artifact_key BLOB PRIMARY KEY CHECK(length(artifact_key) = 32),
            state TEXT NOT NULL CHECK(state IN ('Pending', 'Running', 'Ready', 'Failed')),
            owner TEXT,
            fencing_token INTEGER NOT NULL,
            lease_until_ms INTEGER,
            output_hash BLOB,
            error TEXT,
            updated_at_ms INTEGER NOT NULL,
            CHECK (
                (state = 'Pending' AND owner IS NULL AND lease_until_ms IS NULL
                    AND output_hash IS NULL)
                OR (state = 'Running' AND owner IS NOT NULL AND lease_until_ms IS NOT NULL
                    AND output_hash IS NULL)
                OR (state = 'Ready' AND owner IS NULL AND lease_until_ms IS NULL
                    AND output_hash IS NOT NULL)
                OR (state = 'Failed' AND owner IS NULL AND lease_until_ms IS NULL)
            )
        ) STRICT;
        CREATE TABLE IF NOT EXISTS artifact_dependency (
            artifact_key BLOB NOT NULL,
            dependency_key BLOB NOT NULL,
            PRIMARY KEY (artifact_key, dependency_key),
            FOREIGN KEY (artifact_key) REFERENCES artifact_record(artifact_key),
            FOREIGN KEY (dependency_key) REFERENCES artifact_record(artifact_key)
        ) STRICT;",
    )?;
    Ok(())
}

pub fn register(
    connection: &Connection,
    artifact_key: &[u8; 32],
    now_ms: i64,
) -> Result<(), DagError> {
    connection.execute(
        "INSERT OR IGNORE INTO artifact_record(
            artifact_key, state, fencing_token, updated_at_ms
         ) VALUES (?1, 'Pending', 0, ?2)",
        params![artifact_key.as_slice(), now_ms],
    )?;
    Ok(())
}

pub fn add_dependency(
    connection: &Connection,
    artifact_key: &[u8; 32],
    dependency_key: &[u8; 32],
) -> Result<(), DagError> {
    if artifact_key == dependency_key
        || dependency_reaches(connection, dependency_key, artifact_key)?
    {
        return Err(DagError::Cycle);
    }
    connection.execute(
        "INSERT OR IGNORE INTO artifact_dependency(artifact_key, dependency_key)
         VALUES (?1, ?2)",
        params![artifact_key.as_slice(), dependency_key.as_slice()],
    )?;
    Ok(())
}

fn dependency_reaches(
    connection: &Connection,
    start: &[u8; 32],
    target: &[u8; 32],
) -> Result<bool, DagError> {
    let found: i64 = connection.query_row(
        "WITH RECURSIVE reachable(key) AS (
            SELECT ?1
            UNION
            SELECT d.dependency_key
            FROM artifact_dependency d JOIN reachable r ON d.artifact_key = r.key
         )
         SELECT EXISTS(SELECT 1 FROM reachable WHERE key = ?2)",
        params![start.as_slice(), target.as_slice()],
        |row| row.get(0),
    )?;
    Ok(found != 0)
}

pub fn claim(
    connection: &mut Connection,
    artifact_key: &[u8; 32],
    owner: &str,
    now_ms: i64,
    lease_ms: i64,
) -> Result<Claim, DagError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let pending_dependencies: i64 = transaction.query_row(
        "SELECT count(*)
         FROM artifact_dependency d
         JOIN artifact_record dependency ON dependency.artifact_key = d.dependency_key
         WHERE d.artifact_key = ?1 AND dependency.state != 'Ready'",
        [artifact_key.as_slice()],
        |row| row.get(0),
    )?;
    if pending_dependencies > 0 {
        transaction.commit()?;
        return Ok(Claim::Blocked {
            pending_dependencies,
        });
    }
    let current = transaction
        .query_row(
            "SELECT state, owner, fencing_token, lease_until_ms, output_hash
             FROM artifact_record WHERE artifact_key = ?1",
            [artifact_key.as_slice()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                ))
            },
        )
        .optional()?;

    let claim = match current {
        None => {
            transaction.execute(
                "INSERT INTO artifact_record (
                    artifact_key, state, owner, fencing_token, lease_until_ms, updated_at_ms
                 ) VALUES (?1, 'Running', ?2, 1, ?3, ?4)",
                params![artifact_key.as_slice(), owner, now_ms + lease_ms, now_ms],
            )?;
            Claim::Acquired { fencing_token: 1 }
        }
        Some((state, _, _, _, Some(output_hash))) if state == "Ready" => {
            Claim::Ready { output_hash }
        }
        Some((state, _, _, _, None)) if state == "Ready" => {
            return Err(DagError::InvalidRecord("Ready requires output_hash"));
        }
        Some((state, current_owner, _fencing_token, lease_until, _))
            if state == "Running" && lease_until.is_some_and(|until| until > now_ms) =>
        {
            Claim::Busy {
                owner: current_owner.ok_or(DagError::InvalidRecord("Running requires owner"))?,
                lease_until_ms: lease_until
                    .ok_or(DagError::InvalidRecord("Running requires lease_until_ms"))?,
            }
        }
        Some((_state, _current_owner, fencing_token, _lease_until, _)) => {
            let next_fence = fencing_token + 1;
            transaction.execute(
                "UPDATE artifact_record
                 SET state = 'Running', owner = ?2, fencing_token = ?3,
                     lease_until_ms = ?4, output_hash = NULL, error = NULL, updated_at_ms = ?5
                 WHERE artifact_key = ?1",
                params![
                    artifact_key.as_slice(),
                    owner,
                    next_fence,
                    now_ms + lease_ms,
                    now_ms
                ],
            )?;
            Claim::Acquired {
                fencing_token: next_fence,
            }
        }
    };
    transaction.commit()?;
    Ok(claim)
}

pub fn heartbeat(
    connection: &Connection,
    artifact_key: &[u8; 32],
    owner: &str,
    fencing_token: i64,
    now_ms: i64,
    lease_ms: i64,
) -> Result<(), DagError> {
    let changed = connection.execute(
        "UPDATE artifact_record
         SET lease_until_ms = ?4, updated_at_ms = ?3
         WHERE artifact_key = ?1 AND state = 'Running' AND owner = ?2 AND fencing_token = ?5",
        params![
            artifact_key.as_slice(),
            owner,
            now_ms,
            now_ms + lease_ms,
            fencing_token
        ],
    )?;
    if changed == 1 {
        Ok(())
    } else {
        Err(DagError::StaleFence)
    }
}

pub fn publish(
    connection: &Connection,
    artifact_key: &[u8; 32],
    owner: &str,
    fencing_token: i64,
    output_hash: &[u8; 32],
    now_ms: i64,
) -> Result<(), DagError> {
    let changed = connection.execute(
        "UPDATE artifact_record
         SET state = 'Ready', owner = NULL, output_hash = ?4,
             lease_until_ms = NULL, updated_at_ms = ?5
         WHERE artifact_key = ?1 AND state = 'Running' AND owner = ?2 AND fencing_token = ?3",
        params![
            artifact_key.as_slice(),
            owner,
            fencing_token,
            output_hash.as_slice(),
            now_ms
        ],
    )?;
    if changed == 1 {
        Ok(())
    } else {
        Err(DagError::StaleFence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_owner_can_hold_a_live_lease() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&connection).unwrap();
        let key = [7; 32];

        assert_eq!(
            claim(&mut connection, &key, "worker-a", 100, 50).unwrap(),
            Claim::Acquired { fencing_token: 1 }
        );
        assert_eq!(
            claim(&mut connection, &key, "worker-b", 120, 50).unwrap(),
            Claim::Busy {
                owner: "worker-a".to_owned(),
                lease_until_ms: 150
            }
        );
    }

    #[test]
    fn expired_owner_cannot_publish_after_a_new_fence_is_issued() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&connection).unwrap();
        let key = [8; 32];
        let output = [9; 32];

        assert_eq!(
            claim(&mut connection, &key, "old", 100, 10).unwrap(),
            Claim::Acquired { fencing_token: 1 }
        );
        assert_eq!(
            claim(&mut connection, &key, "new", 111, 10).unwrap(),
            Claim::Acquired { fencing_token: 2 }
        );
        assert!(matches!(
            publish(&connection, &key, "old", 1, &output, 112),
            Err(DagError::StaleFence)
        ));
        publish(&connection, &key, "new", 2, &output, 113).unwrap();
        assert_eq!(
            claim(&mut connection, &key, "reader", 114, 10).unwrap(),
            Claim::Ready {
                output_hash: output.to_vec()
            }
        );
    }

    #[test]
    fn heartbeat_requires_current_owner_and_fence() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&connection).unwrap();
        let key = [6; 32];
        claim(&mut connection, &key, "worker", 100, 10).unwrap();

        heartbeat(&connection, &key, "worker", 1, 105, 10).unwrap();
        assert!(matches!(
            heartbeat(&connection, &key, "worker", 2, 106, 10),
            Err(DagError::StaleFence)
        ));
    }

    #[test]
    fn dependency_blocks_claim_until_ready_and_cycle_is_rejected() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&connection).unwrap();
        let upstream = [1_u8; 32];
        let downstream = [2_u8; 32];
        register(&connection, &upstream, 0).unwrap();
        register(&connection, &downstream, 0).unwrap();
        add_dependency(&connection, &downstream, &upstream).unwrap();

        assert_eq!(
            claim(&mut connection, &downstream, "worker", 1, 10).unwrap(),
            Claim::Blocked {
                pending_dependencies: 1
            }
        );
        assert!(matches!(
            add_dependency(&connection, &upstream, &downstream),
            Err(DagError::Cycle)
        ));

        let Claim::Acquired { fencing_token } =
            claim(&mut connection, &upstream, "worker", 1, 10).unwrap()
        else {
            panic!("upstream should be runnable");
        };
        publish(&connection, &upstream, "worker", fencing_token, &[3; 32], 2).unwrap();
        assert!(matches!(
            claim(&mut connection, &downstream, "worker", 3, 10).unwrap(),
            Claim::Acquired { .. }
        ));
    }

    #[test]
    fn corrupt_ready_record_is_not_silently_accepted() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE artifact_record (
                    artifact_key BLOB PRIMARY KEY,
                    state TEXT NOT NULL,
                    owner TEXT,
                    fencing_token INTEGER NOT NULL,
                    lease_until_ms INTEGER,
                    output_hash BLOB,
                    error TEXT,
                    updated_at_ms INTEGER NOT NULL
                ) STRICT;
                CREATE TABLE artifact_dependency (
                    artifact_key BLOB NOT NULL,
                    dependency_key BLOB NOT NULL,
                    PRIMARY KEY (artifact_key, dependency_key)
                ) STRICT;",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO artifact_record(
                    artifact_key, state, fencing_token, updated_at_ms
                 ) VALUES (?1, 'Ready', 1, 0)",
                [[5_u8; 32].as_slice()],
            )
            .unwrap();

        assert!(matches!(
            claim(&mut connection, &[5_u8; 32], "worker", 0, 10),
            Err(DagError::InvalidRecord(_))
        ));
    }
}
