use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{cas::sync_directory, project::ProjectStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrashPoint {
    AfterPreparing,
    AfterCandidateWrite,
    AfterCandidateSync,
    AfterPublishIntent,
    AfterFinalRename,
    AfterPublished,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryOutcome {
    pub final_state: String,
    pub staging_cleaned: bool,
    pub intent_reconciled: bool,
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("export record {0} is missing")]
    MissingRecord(i64),
    #[error("export {0} is missing its expected hash")]
    MissingHash(i64),
    #[error("export file {path} is missing or has the wrong hash")]
    InvalidPublishedFile { path: PathBuf },
    #[error("export {0} has unknown state {1}")]
    UnknownState(i64, String),
    #[error("export {0} already has an authoritative record")]
    DuplicateExport(i64),
    #[error("export target already exists and will not be overwritten: {0}")]
    TargetExists(PathBuf),
    #[error("export staging directory already exists and will not be reused: {0}")]
    StagingExists(PathBuf),
}

pub fn initialize(root: &Path) -> Result<(), RecoveryError> {
    fs::create_dir_all(root.join("staging/export"))?;
    fs::create_dir_all(root.join("exports"))?;
    ProjectStore::open(root.join("project.sqlite3"))?;
    Ok(())
}

pub fn run_export_with_hook<F>(
    root: &Path,
    export_id: i64,
    bytes: &[u8],
    mut after_stage: F,
) -> Result<(), RecoveryError>
where
    F: FnMut(CrashPoint),
{
    let destination = root.join("exports").join(format!("{export_id}.bin"));
    run_export_to_path_with_hook(root, export_id, bytes, &destination, "bin", 0, after_stage)
}

pub fn run_export_to_path_with_hook<F>(
    root: &Path,
    export_id: i64,
    bytes: &[u8],
    destination: &Path,
    format: &str,
    created_at_ms: i64,
    mut after_stage: F,
) -> Result<(), RecoveryError>
where
    F: FnMut(CrashPoint),
{
    initialize(root)?;
    let connection = open_database(root)?;
    let staging_directory = root.join("staging/export").join(export_id.to_string());
    let candidate_path = staging_directory.join("candidate.bin");
    let final_path = destination.to_owned();
    if let Some(parent) = final_path.parent() { fs::create_dir_all(parent)?; }
    if final_path.exists() {
        return Err(RecoveryError::TargetExists(final_path));
    }
    if staging_directory.exists() {
        return Err(RecoveryError::StagingExists(staging_directory));
    }
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM export_record WHERE export_id = ?1)",
        [export_id],
        |row| row.get::<_, bool>(0),
    )?;
    if exists {
        return Err(RecoveryError::DuplicateExport(export_id));
    }
    connection.execute(
        "INSERT INTO export_record(export_id, state, destination_path, format, created_at_ms, updated_at_ms)
         VALUES (?1, 'Preparing', ?2, ?3, ?4, ?4)",
        params![export_id, destination.to_string_lossy(), format, created_at_ms],
    )?;
    after_stage(CrashPoint::AfterPreparing);

    fs::create_dir_all(&staging_directory)?;
    let mut candidate = File::create(&candidate_path)?;
    candidate.write_all(bytes)?;
    after_stage(CrashPoint::AfterCandidateWrite);
    candidate.sync_all()?;
    sync_directory(&staging_directory)?;
    after_stage(CrashPoint::AfterCandidateSync);

    let expected_hash: [u8; 32] = Sha256::digest(bytes).into();
    connection.execute(
        "UPDATE export_record
         SET state = 'PublishIntentRecorded', expected_hash = ?2
         WHERE export_id = ?1",
        params![export_id, expected_hash.as_slice()],
    )?;
    after_stage(CrashPoint::AfterPublishIntent);

    publish_no_clobber(&candidate_path, &final_path)?;
    after_stage(CrashPoint::AfterFinalRename);
    connection.execute(
        "UPDATE export_record SET state = 'Published', updated_at_ms = ?2 WHERE export_id = ?1",
        params![export_id, created_at_ms],
    )?;
    after_stage(CrashPoint::AfterPublished);
    Ok(())
}

pub fn recover(root: &Path, export_id: i64) -> Result<RecoveryOutcome, RecoveryError> {
    let connection = open_database(root)?;
    let record = connection
        .query_row(
            "SELECT state, expected_hash, destination_path
             FROM export_record WHERE export_id = ?1",
            [export_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?, row.get::<_, Option<String>>(2)?)),
        )
        .optional()?
        .ok_or(RecoveryError::MissingRecord(export_id))?;
    let (state, expected_hash, destination_path) = record;
    let staging_directory = root.join("staging/export").join(export_id.to_string());
    let candidate_path = staging_directory.join("candidate.bin");
    let final_path = destination_path.map(PathBuf::from).unwrap_or_else(|| root.join("exports").join(format!("{export_id}.bin")));

    match state.as_str() {
        "Preparing" => {
            let staging_cleaned = staging_directory.exists();
            if staging_cleaned {
                fs::remove_dir_all(&staging_directory)?;
            }
            connection.execute(
                "UPDATE export_record SET state = 'CancelledAfterCrash' WHERE export_id = ?1",
                [export_id],
            )?;
            Ok(RecoveryOutcome {
                final_state: "CancelledAfterCrash".to_owned(),
                staging_cleaned,
                intent_reconciled: false,
            })
        }
        "PublishIntentRecorded" => {
            let expected_hash = expected_hash.ok_or(RecoveryError::MissingHash(export_id))?;
            if final_path.exists() {
                verify_hash(&final_path, &expected_hash)?;
            } else {
                verify_hash(&candidate_path, &expected_hash)?;
                publish_no_clobber(&candidate_path, &final_path)?;
            }
            connection.execute(
                "UPDATE export_record SET state = 'Published' WHERE export_id = ?1",
                [export_id],
            )?;
            if staging_directory.exists() {
                fs::remove_dir_all(&staging_directory)?;
            }
            Ok(RecoveryOutcome {
                final_state: "Published".to_owned(),
                staging_cleaned: true,
                intent_reconciled: true,
            })
        }
        "Published" => {
            let expected_hash = expected_hash.ok_or(RecoveryError::MissingHash(export_id))?;
            verify_hash(&final_path, &expected_hash)?;
            Ok(RecoveryOutcome {
                final_state: "Published".to_owned(),
                staging_cleaned: false,
                intent_reconciled: false,
            })
        }
        "CancelledAfterCrash" | "Failed" => Ok(RecoveryOutcome {
            final_state: state,
            staging_cleaned: !staging_directory.exists(),
            intent_reconciled: false,
        }),
        unknown => Err(RecoveryError::UnknownState(export_id, unknown.to_owned())),
    }
}

fn open_database(root: &Path) -> Result<Connection, RecoveryError> {
    let connection = Connection::open(root.join("project.sqlite3"))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    Ok(connection)
}

fn verify_hash(path: &Path, expected_hash: &[u8]) -> Result<(), RecoveryError> {
    let expected: [u8; 32] =
        expected_hash
            .try_into()
            .map_err(|_| RecoveryError::InvalidPublishedFile {
                path: path.to_owned(),
            })?;
    if crate::cas::verify_object(path, &expected).is_err() {
        return Err(RecoveryError::InvalidPublishedFile {
            path: path.to_owned(),
        });
    }
    Ok(())
}

fn publish_no_clobber(candidate: &Path, final_path: &Path) -> Result<(), RecoveryError> {
    match fs::hard_link(candidate, final_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RecoveryError::TargetExists(final_path.to_owned()));
        }
        Err(error) => return Err(RecoveryError::Io(error)),
    }
    sync_directory(final_path.parent().expect("export target has a parent"))?;
    fs::remove_file(candidate)?;
    sync_directory(candidate.parent().expect("export candidate has a parent"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn preparing_state_is_cancelled_without_publishing_partial_output() {
        let temp = TempDir::new().unwrap();
        let result = std::panic::catch_unwind(|| {
            run_export_with_hook(temp.path(), 1, b"translation", |point| {
                if point == CrashPoint::AfterCandidateWrite {
                    panic!("simulated interruption");
                }
            })
            .unwrap();
        });
        assert!(result.is_err());
        let outcome = recover(temp.path(), 1).unwrap();
        assert_eq!(outcome.final_state, "CancelledAfterCrash");
        assert!(!temp.path().join("exports/1.bin").exists());
    }

    #[test]
    fn publish_intent_can_resume_from_synced_candidate() {
        let temp = TempDir::new().unwrap();
        struct Stop;
        let result = std::panic::catch_unwind(|| {
            run_export_with_hook(temp.path(), 2, b"translation", |point| {
                if point == CrashPoint::AfterPublishIntent {
                    std::panic::panic_any(Stop);
                }
            })
            .unwrap();
        });
        assert!(result.is_err());
        let outcome = recover(temp.path(), 2).unwrap();
        assert_eq!(outcome.final_state, "Published");
        assert!(outcome.intent_reconciled);
    }

    #[test]
    fn existing_output_is_never_overwritten() {
        let temp = TempDir::new().unwrap();
        initialize(temp.path()).unwrap();
        let final_path = temp.path().join("exports/3.bin");
        fs::write(&final_path, b"existing").unwrap();

        let error = run_export_with_hook(temp.path(), 3, b"replacement", |_| {}).unwrap_err();

        assert!(matches!(error, RecoveryError::TargetExists(path) if path == final_path));
        assert_eq!(fs::read(final_path).unwrap(), b"existing");
    }

    #[test]
    fn export_id_cannot_replace_an_existing_record() {
        let temp = TempDir::new().unwrap();
        let result = std::panic::catch_unwind(|| {
            run_export_with_hook(temp.path(), 4, b"first", |point| {
                if point == CrashPoint::AfterPreparing {
                    panic!("simulated interruption");
                }
            })
            .unwrap();
        });
        assert!(result.is_err());

        let error = run_export_with_hook(temp.path(), 4, b"second", |_| {}).unwrap_err();

        assert!(matches!(error, RecoveryError::DuplicateExport(4)));
        assert!(!temp.path().join("exports/4.bin").exists());
    }

    #[test]
    fn export_schema_does_not_store_filesystem_paths() {
        let temp = TempDir::new().unwrap();
        initialize(temp.path()).unwrap();
        let connection = open_database(temp.path()).unwrap();
        let path_columns: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('export_record')
                 WHERE name LIKE '%path%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(path_columns, 0);
    }
}
