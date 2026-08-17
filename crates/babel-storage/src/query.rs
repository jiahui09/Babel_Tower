use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::project::{DraftDisposition, DraftRecovery, UnitPageItem};

const QUERY_CACHE_KIB: i64 = 16 * 1024;

pub struct ProjectQuery {
    connection: Connection,
}

impl ProjectQuery {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.pragma_update(None, "query_only", true)?;
        connection.pragma_update(None, "busy_timeout", 5_000)?;
        connection.pragma_update(None, "cache_size", -QUERY_CACHE_KIB)?;
        Ok(Self { connection })
    }

    pub fn commit_sequence(&self) -> rusqlite::Result<i64> {
        self.connection.query_row(
            "SELECT commit_sequence FROM project_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
    }

    pub fn page_after(
        &self,
        after_local_index: i64,
        limit: usize,
    ) -> rusqlite::Result<Vec<UnitPageItem>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT u.unit_id, u.source_unit_key, u.local_index, u.source_text, r.text
             FROM unit u
             LEFT JOIN unit_head h ON h.unit_id = u.unit_id
             LEFT JOIN translation_revision r ON r.revision_id = h.revision_id
             WHERE u.local_index > ?1
             ORDER BY u.local_index
             LIMIT ?2",
        )?;
        statement
            .query_map(params![after_local_index, limit as i64], |row| {
                Ok(UnitPageItem {
                    unit_id: row.get(0)?,
                    source_unit_key: row.get(1)?,
                    local_index: row.get(2)?,
                    source_text: row.get(3)?,
                    translation: row.get(4)?,
                })
            })?
            .collect()
    }

    pub fn diagnostic_count(&self) -> rusqlite::Result<u64> {
        self.connection
            .query_row("SELECT count(*) FROM diagnostic_event", [], |row| {
                row.get::<_, i64>(0).map(|value| value as u64)
            })
    }

    pub fn draft_for(
        &self,
        unit_id: &[u8],
        client_session_id: &str,
    ) -> rusqlite::Result<Option<DraftRecovery>> {
        self.connection
            .query_row(
                "SELECT d.unit_id, d.base_revision_id, h.revision_id,
                        d.client_session_id, d.patch, d.updated_at_ms
                 FROM draft_session d
                 LEFT JOIN unit_head h ON h.unit_id = d.unit_id
                 WHERE d.unit_id = ?1 AND d.client_session_id = ?2",
                params![unit_id, client_session_id],
                |row| {
                    let base_revision_id = row.get(1)?;
                    let current_revision_id = row.get(2)?;
                    Ok(DraftRecovery {
                        unit_id: row.get(0)?,
                        base_revision_id,
                        current_revision_id,
                        client_session_id: row.get(3)?,
                        patch: row.get(4)?,
                        updated_at_ms: row.get(5)?,
                        disposition: if base_revision_id == current_revision_id {
                            DraftDisposition::BaseUnchanged
                        } else {
                            DraftDisposition::BaseChanged
                        },
                    })
                },
            )
            .optional()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::project::ProjectStore;

    #[test]
    fn query_connection_cannot_modify_authoritative_state() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("project.sqlite3");
        let store = ProjectStore::open(&path).unwrap();
        drop(store);
        let query = ProjectQuery::open(path).unwrap();
        assert_eq!(query.commit_sequence().unwrap(), 0);
        assert!(
            query
                .connection
                .execute("UPDATE project_state SET commit_sequence = 99", [])
                .is_err()
        );
    }
}
