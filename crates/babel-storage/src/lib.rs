//! Authoritative SQLite, immutable object storage, migration and recovery primitives.

pub mod backup;
pub mod cas;
pub mod gc;
pub mod migration;
pub mod project;
pub mod query;
pub mod recovery;
pub mod schema;

#[cfg(test)]
mod fault_injection {
    use std::{
        process::{Command, exit},
        time::SystemTime,
    };

    use tempfile::TempDir;

    use crate::{
        cas, gc,
        project::{ObjectRecord, ProjectStore, SavePoint},
    };

    #[test]
    fn translation_crash_child() {
        let Ok(database) = std::env::var("BABEL_FAULT_DATABASE") else {
            return;
        };
        let stage = std::env::var("BABEL_FAULT_STAGE").unwrap();
        let source_key: [u8; 32] = hex::decode(std::env::var("BABEL_SOURCE_KEY").unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let mut store = ProjectStore::open(database).unwrap();
        store
            .save_translation_with_hook(
                &source_key,
                &[5; 32],
                "process durable",
                1,
                |point| match (stage.as_str(), point) {
                    ("before_commit", SavePoint::BeforeCommit) => exit(91),
                    ("after_commit", SavePoint::AfterCommit) => exit(92),
                    _ => {}
                },
            )
            .unwrap();
    }

    #[test]
    fn object_crash_child() {
        let Ok(root) = std::env::var("BABEL_FAULT_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let stage = std::env::var("BABEL_FAULT_STAGE").unwrap();
        let bytes = b"immutable source";
        let (hash, _) = cas::publish_bytes(&root.join("objects"), bytes).unwrap();
        if stage == "before_reference" {
            exit(93);
        }
        let mut store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        store
            .register_object_reference(
                "source",
                &[3; 16],
                &ObjectRecord {
                    hash,
                    byte_length: bytes.len() as u64,
                    media_type: "text/plain".to_owned(),
                },
                1,
            )
            .unwrap();
        exit(94);
    }

    #[test]
    fn process_termination_preserves_translation_transaction_boundaries() {
        for (stage, expected_code, expected_sequence) in
            [("before_commit", 91, 0), ("after_commit", 92, 1)]
        {
            let temp = TempDir::new().unwrap();
            let database = temp.path().join("project.sqlite3");
            let mut store = ProjectStore::open(&database).unwrap();
            store.seed_units(1).unwrap();
            let source_key: [u8; 32] = store.page_after(-1, 1).unwrap()[0]
                .source_unit_key
                .clone()
                .try_into()
                .unwrap();
            drop(store);

            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "fault_injection::translation_crash_child"])
                .env("BABEL_FAULT_DATABASE", &database)
                .env("BABEL_FAULT_STAGE", stage)
                .env("BABEL_SOURCE_KEY", hex::encode(source_key))
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(expected_code));

            let mut reopened = ProjectStore::open(&database).unwrap();
            assert_eq!(reopened.commit_sequence().unwrap(), expected_sequence);
            if stage == "after_commit" {
                let receipt = reopened
                    .save_translation(&source_key, &[5; 32], "must not replace", 2)
                    .unwrap();
                assert!(receipt.replayed);
            } else {
                assert!(reopened.page_after(-1, 1).unwrap()[0].translation.is_none());
            }
        }
    }

    #[test]
    fn process_termination_never_creates_a_dangling_object_reference() {
        for (stage, expected_code, expected_references, expected_deleted) in [
            ("before_reference", 93, 0, 1),
            ("after_reference", 94, 1, 0),
        ] {
            let temp = TempDir::new().unwrap();
            let root = temp.path().join("book.babel");
            std::fs::create_dir_all(&root).unwrap();
            let store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
            drop(store);
            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "fault_injection::object_crash_child"])
                .env("BABEL_FAULT_ROOT", &root)
                .env("BABEL_FAULT_STAGE", stage)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(expected_code));

            let store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
            let references: i64 = store
                .connection()
                .query_row("SELECT count(*) FROM object_reference", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(references, expected_references);
            let report = gc::sweep(&store, &root.join("objects"), SystemTime::now()).unwrap();
            assert_eq!(report.deleted, expected_deleted);
        }
    }
}
