use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, backup::Backup};
use thiserror::Error;

use crate::cas;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("backup target already exists: {0}")]
    TargetExists(PathBuf),
    #[error("backup I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("backup database failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("referenced source object is missing or corrupt: {0}")]
    InvalidObject(PathBuf),
    #[error("backup snapshot moved from commit {expected} to commit {actual}")]
    SnapshotMoved { expected: i64, actual: i64 },
}

pub struct BackupSnapshot {
    source: Connection,
    commit_sequence: i64,
    object_hashes: Vec<[u8; 32]>,
}

impl BackupSnapshot {
    #[cfg(test)]
    pub fn capture(database_path: &Path) -> Result<Self, BackupError> {
        Self::capture_with_roots(database_path, None, None)
    }

    pub fn capture_pinned(
        database_path: &Path,
        expected_sequence: i64,
        object_hashes: Vec<[u8; 32]>,
    ) -> Result<Self, BackupError> {
        Self::capture_with_roots(database_path, Some(expected_sequence), Some(object_hashes))
    }

    fn capture_with_roots(
        database_path: &Path,
        expected_sequence: Option<i64>,
        pinned_hashes: Option<Vec<[u8; 32]>>,
    ) -> Result<Self, BackupError> {
        let source = Connection::open_with_flags(
            database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        source.execute_batch("BEGIN")?;
        let commit_sequence = source.query_row(
            "SELECT commit_sequence FROM project_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        if let Some(expected) = expected_sequence
            && commit_sequence != expected
        {
            return Err(BackupError::SnapshotMoved {
                expected,
                actual: commit_sequence,
            });
        }
        let object_hashes = if let Some(hashes) = pinned_hashes {
            hashes
        } else {
            let mut statement = source.prepare(
                "SELECT DISTINCT object_hash FROM object_reference ORDER BY object_hash",
            )?;
            statement
                .query_map([], |row| {
                    let bytes: Vec<u8> = row.get(0)?;
                    let length = bytes.len();
                    bytes
                        .try_into()
                        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, length as i64))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(Self {
            source,
            commit_sequence,
            object_hashes,
        })
    }

    pub const fn commit_sequence(&self) -> i64 {
        self.commit_sequence
    }

    pub fn object_hashes(&self) -> &[[u8; 32]] {
        &self.object_hashes
    }

    pub fn materialize(
        &self,
        source_objects: &Path,
        target_root: &Path,
    ) -> Result<(), BackupError> {
        if target_root.exists() {
            return Err(BackupError::TargetExists(target_root.to_owned()));
        }
        fs::create_dir(target_root)?;
        let target_objects = target_root.join("objects");
        fs::create_dir(&target_objects)?;

        let mut destination = Connection::open(target_root.join("project.sqlite3"))?;
        let backup = Backup::new(&self.source, &mut destination)?;
        backup.run_to_completion(128, Duration::from_millis(1), None)?;
        drop(backup);
        destination.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        let integrity: String =
            destination.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(BackupError::Sqlite(rusqlite::Error::InvalidQuery));
        }

        for hash in &self.object_hashes {
            let source = cas::object_path(source_objects, hash);
            cas::verify_object(&source, hash)
                .map_err(|_| BackupError::InvalidObject(source.clone()))?;
            let target = cas::object_path(&target_objects, hash);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::copy(&source, &target)?;
            cas::verify_object(&target, hash).map_err(|_| BackupError::InvalidObject(target))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::project::{ObjectRecord, ProjectStore};

    #[test]
    fn snapshot_copies_database_and_reachable_object_closure() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("source.babel");
        fs::create_dir_all(root.join("objects")).unwrap();
        let bytes = b"source";
        let (hash, _) = cas::publish_bytes(&root.join("objects"), bytes).unwrap();
        let mut store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        store
            .register_object_reference(
                "source",
                &[1; 16],
                &ObjectRecord {
                    hash,
                    byte_length: bytes.len() as u64,
                    media_type: "text/plain".to_owned(),
                },
                1,
            )
            .unwrap();
        drop(store);

        let snapshot = BackupSnapshot::capture(&root.join("project.sqlite3")).unwrap();
        let target = temp.path().join("backup.babel");
        snapshot
            .materialize(&root.join("objects"), &target)
            .unwrap();
        assert!(target.join("project.sqlite3").exists());
        assert!(cas::object_path(&target.join("objects"), &hash).exists());
    }

    #[test]
    fn existing_target_is_never_overwritten() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("source.babel");
        fs::create_dir_all(&root).unwrap();
        let store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        drop(store);
        let snapshot = BackupSnapshot::capture(&root.join("project.sqlite3")).unwrap();
        let target = temp.path().join("existing");
        fs::create_dir(&target).unwrap();
        assert!(matches!(
            snapshot.materialize(&root.join("objects"), &target),
            Err(BackupError::TargetExists(_))
        ));
    }
}
