use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, backup::Backup};
use thiserror::Error;
use uuid::Uuid;

use crate::{cas, schema};

#[derive(Debug, Error)]
pub enum MigrationBackupError {
    #[error("migration backup I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("migration backup database failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration backup source object is missing or corrupt: {0}")]
    InvalidObject(PathBuf),
}

pub fn backup_before_migration(
    source: &Connection,
    database_path: &Path,
    from_version: i64,
) -> Result<PathBuf, MigrationBackupError> {
    let project_root = database_path.parent().unwrap_or_else(|| Path::new("."));
    let backup_parent = project_root.join("migration-backups");
    fs::create_dir_all(&backup_parent)?;
    let target = backup_parent.join(format!("pre-v{from_version}-{}", Uuid::new_v4()));
    fs::create_dir(&target)?;

    let backup_database_path = target.join("project.sqlite3");
    let mut destination = Connection::open(&backup_database_path)?;
    let backup = Backup::new(source, &mut destination)?;
    backup.run_to_completion(128, Duration::from_millis(1), None)?;
    drop(backup);
    let integrity: String =
        destination.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(MigrationBackupError::Sqlite(rusqlite::Error::InvalidQuery));
    }
    drop(destination);
    File::open(&backup_database_path)?.sync_all()?;

    if from_version >= 2 && schema::has_table_runtime(source, "object_reference")? {
        copy_reachable_objects(
            source,
            &project_root.join("objects"),
            &target.join("objects"),
        )?;
    }

    let mut manifest = File::create(target.join("migration-backup.txt"))?;
    writeln!(manifest, "schema_version={from_version}")?;
    manifest.sync_all()?;
    cas::sync_directory(&target)?;
    cas::sync_directory(&backup_parent)?;
    Ok(target)
}

fn copy_reachable_objects(
    source: &Connection,
    source_objects: &Path,
    target_objects: &Path,
) -> Result<(), MigrationBackupError> {
    let mut statement =
        source.prepare("SELECT DISTINCT object_hash FROM object_reference ORDER BY object_hash")?;
    let hashes = statement
        .query_map([], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            let length = bytes.len();
            bytes
                .try_into()
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, length as i64))
        })?
        .collect::<rusqlite::Result<Vec<[u8; 32]>>>()?;

    for hash in hashes {
        let source_path = cas::object_path(source_objects, &hash);
        cas::verify_object(&source_path, &hash)
            .map_err(|_| MigrationBackupError::InvalidObject(source_path.clone()))?;
        let target_path = cas::object_path(target_objects, &hash);
        fs::create_dir_all(target_path.parent().unwrap())?;
        fs::copy(&source_path, &target_path)?;
        cas::verify_object(&target_path, &hash)
            .map_err(|_| MigrationBackupError::InvalidObject(target_path))?;
    }
    Ok(())
}
