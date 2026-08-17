use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::{cas, project::ProjectStore};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcCandidate {
    pub hash: [u8; 32],
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcReport {
    pub candidates: Vec<GcCandidate>,
    pub deleted: usize,
}

pub fn dry_run(
    store: &ProjectStore,
    objects_root: &Path,
    older_than: SystemTime,
) -> rusqlite::Result<GcReport> {
    let reachable = store.reachable_object_hashes()?;
    inventory(objects_root, older_than, &reachable)
}

pub fn dry_run_database(
    database_path: &Path,
    objects_root: &Path,
    older_than: SystemTime,
) -> rusqlite::Result<GcReport> {
    let connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let reachable = reachable_from_connection(&connection)?;
    inventory(objects_root, older_than, &reachable)
}

pub fn sweep(
    store: &ProjectStore,
    objects_root: &Path,
    older_than: SystemTime,
) -> rusqlite::Result<GcReport> {
    let mut report = dry_run(store, objects_root, older_than)?;
    for candidate in &report.candidates {
        match fs::remove_file(&candidate.path) {
            Ok(()) => report.deleted += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(to_sqlite_error(error)),
        }
    }
    Ok(report)
}

pub fn sweep_candidates(
    store: &ProjectStore,
    objects_root: &Path,
    candidates: Vec<GcCandidate>,
    max_items: usize,
    max_duration: Duration,
) -> rusqlite::Result<GcReport> {
    let reachable = store.reachable_object_hashes()?;
    let started = Instant::now();
    let mut report = GcReport {
        candidates: Vec::new(),
        deleted: 0,
    };
    for candidate in candidates.into_iter().take(max_items) {
        if started.elapsed() >= max_duration {
            break;
        }
        let expected_path = cas::object_path(objects_root, &candidate.hash);
        if candidate.path != expected_path || reachable.contains(&candidate.hash) {
            continue;
        }
        match fs::remove_file(&candidate.path) {
            Ok(()) => report.deleted += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(to_sqlite_error(error)),
        }
        report.candidates.push(candidate);
    }
    Ok(report)
}

fn inventory(
    objects_root: &Path,
    older_than: SystemTime,
    reachable: &std::collections::BTreeSet<[u8; 32]>,
) -> rusqlite::Result<GcReport> {
    let mut candidates = Vec::new();
    let sha_root = objects_root.join("sha256");
    let prefixes = match fs::read_dir(&sha_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(GcReport {
                candidates,
                deleted: 0,
            });
        }
        Err(error) => return Err(to_sqlite_error(error)),
    };

    for prefix in prefixes {
        let prefix = prefix.map_err(to_sqlite_error)?;
        if !prefix.file_type().map_err(to_sqlite_error)?.is_dir() {
            continue;
        }
        let prefix_name = prefix.file_name();
        let prefix_name = prefix_name.to_string_lossy();
        for entry in fs::read_dir(prefix.path()).map_err(to_sqlite_error)? {
            let entry = entry.map_err(to_sqlite_error)?;
            if !entry.file_type().map_err(to_sqlite_error)?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let encoded = format!("{prefix_name}{name}");
            let Ok(bytes) = hex::decode(encoded) else {
                continue;
            };
            let Ok(hash) = <Vec<u8> as TryInto<[u8; 32]>>::try_into(bytes) else {
                continue;
            };
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .map_err(to_sqlite_error)?;
            if !reachable.contains(&hash) && modified <= older_than {
                candidates.push(GcCandidate {
                    hash,
                    path: entry.path(),
                });
            }
        }
    }
    candidates.sort_by_key(|candidate| candidate.hash);
    Ok(GcReport {
        candidates,
        deleted: 0,
    })
}

fn reachable_from_connection(
    connection: &Connection,
) -> rusqlite::Result<std::collections::BTreeSet<[u8; 32]>> {
    let mut statement = connection.prepare(
        "SELECT object_hash FROM object_reference
         UNION
         SELECT r.object_hash
         FROM backup_root r
         JOIN backup_lease l ON l.lease_id = r.lease_id
         WHERE l.state = 'Active'",
    )?;
    statement
        .query_map([], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            let length = bytes.len();
            bytes
                .try_into()
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, length as i64))
        })?
        .collect()
}

fn to_sqlite_error(error: io::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::{cas, project::ObjectRecord};

    #[test]
    fn active_backup_pin_prevents_collection_after_live_reference_is_removed() {
        let temp = TempDir::new().unwrap();
        let objects = temp.path().join("objects");
        let (hash, path) = cas::publish_bytes(&objects, b"pinned").unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        store
            .register_object_reference(
                "source",
                &[1; 16],
                &ObjectRecord {
                    hash,
                    byte_length: 6,
                    media_type: "text/plain".to_owned(),
                },
                1,
            )
            .unwrap();
        let lease = [8; 16];
        let pin = store.begin_backup_pin(&lease, 2).unwrap();
        assert_eq!(pin.object_hashes, vec![hash]);
        store
            .connection()
            .execute("DELETE FROM object_reference", [])
            .unwrap();

        let pinned = sweep(&store, &objects, SystemTime::now()).unwrap();
        assert_eq!(pinned.deleted, 0);
        assert!(path.exists());

        store.finish_backup_pin(&lease, true).unwrap();
        let released = sweep(&store, &objects, SystemTime::now()).unwrap();
        assert_eq!(released.deleted, 1);
        assert!(!path.exists());
    }

    #[test]
    fn orphan_collection_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let objects = temp.path().join("objects");
        cas::publish_bytes(&objects, b"orphan").unwrap();
        let store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        assert_eq!(
            sweep(&store, &objects, SystemTime::now()).unwrap().deleted,
            1
        );
        assert_eq!(
            sweep(&store, &objects, SystemTime::now()).unwrap().deleted,
            0
        );
    }
}
