use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

pub const CURRENT_SCHEMA_VERSION: i64 = 13;

pub fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    migrate_to(connection, CURRENT_SCHEMA_VERSION)
}

pub(crate) fn migrate_to(connection: &mut Connection, target_version: i64) -> rusqlite::Result<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > target_version || target_version > CURRENT_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }

    for target in (version + 1)..=target_version {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match target {
            1 => migration_1(&transaction)?,
            2 => migration_2(&transaction)?,
            3 => migration_3(&transaction)?,
            4 => migration_4(&transaction)?,
            5 => migration_5(&transaction)?,
            6 => migration_6(&transaction)?,
            7 => migration_7(&transaction)?,
            8 => migration_8(&transaction)?,
            9 => migration_9(&transaction)?,
            10 => migration_10(&transaction)?,
            11 => migration_11(&transaction)?,
            12 => migration_12(&transaction)?,
            13 => migration_13(&transaction)?,
            _ => unreachable!(),
        }
        transaction.pragma_update(None, "user_version", target)?;
        transaction.execute(
            "INSERT INTO migration_record(schema_version, completed_at_ms)
             VALUES (?1, unixepoch('subsec') * 1000)",
            [target],
        )?;
        transaction.commit()?;
    }
    Ok(())
}

pub fn schema_version(connection: &Connection) -> rusqlite::Result<i64> {
    connection.pragma_query_value(None, "user_version", |row| row.get(0))
}

fn migration_1(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE migration_record (
            schema_version INTEGER PRIMARY KEY,
            completed_at_ms INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE project_state (
            singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
            project_id BLOB NOT NULL CHECK(length(project_id) = 16),
            commit_sequence INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE unit (
            unit_id BLOB PRIMARY KEY CHECK(length(unit_id) = 16),
            source_unit_key BLOB NOT NULL UNIQUE CHECK(length(source_unit_key) = 32),
            local_index INTEGER NOT NULL UNIQUE,
            source_text TEXT NOT NULL
        ) STRICT;
        CREATE TABLE translation_revision (
            revision_id INTEGER PRIMARY KEY,
            unit_id BLOB NOT NULL REFERENCES unit(unit_id),
            command_id BLOB NOT NULL UNIQUE CHECK(length(command_id) = 32),
            commit_sequence INTEGER NOT NULL UNIQUE,
            parent_revision_id INTEGER REFERENCES translation_revision(revision_id),
            text TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE unit_head (
            unit_id BLOB PRIMARY KEY REFERENCES unit(unit_id),
            revision_id INTEGER NOT NULL REFERENCES translation_revision(revision_id)
        ) STRICT;
        CREATE TABLE command_receipt (
            command_id BLOB PRIMARY KEY CHECK(length(command_id) = 32),
            revision_id INTEGER NOT NULL REFERENCES translation_revision(revision_id),
            commit_sequence INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE search_dirty (
            unit_id BLOB PRIMARY KEY REFERENCES unit(unit_id),
            dirty_sequence INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE draft_session (
            unit_id BLOB NOT NULL REFERENCES unit(unit_id),
            base_revision_id INTEGER REFERENCES translation_revision(revision_id),
            client_session_id TEXT NOT NULL,
            patch BLOB NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(unit_id, client_session_id)
        ) STRICT;
        CREATE VIRTUAL TABLE unit_search
        USING fts5(unit_id UNINDEXED, source, translation, tokenize = 'unicode61');",
    )
}

fn migration_2(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE object_record (
            object_hash BLOB PRIMARY KEY CHECK(length(object_hash) = 32),
            byte_length INTEGER NOT NULL CHECK(byte_length >= 0),
            media_type TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE object_reference (
            owner_kind TEXT NOT NULL,
            owner_id BLOB NOT NULL,
            object_hash BLOB NOT NULL REFERENCES object_record(object_hash),
            PRIMARY KEY(owner_kind, owner_id, object_hash)
        ) STRICT;
        CREATE TABLE task_record (
            task_id BLOB PRIMARY KEY CHECK(length(task_id) = 16),
            task_kind TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN (
                'Pending', 'Running', 'Paused', 'Completed', 'Failed', 'Cancelled'
            )),
            priority INTEGER NOT NULL CHECK(priority BETWEEN 0 AND 3),
            progress_current INTEGER NOT NULL DEFAULT 0,
            progress_total INTEGER,
            failure_code TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE diagnostic_event (
            event_id INTEGER PRIMARY KEY,
            severity TEXT NOT NULL CHECK(severity IN ('Info', 'Warning', 'Error')),
            code TEXT NOT NULL,
            user_message TEXT NOT NULL,
            technical_detail TEXT,
            created_at_ms INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE backup_lease (
            lease_id BLOB PRIMARY KEY CHECK(length(lease_id) = 16),
            commit_sequence INTEGER NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('Active', 'Completed', 'Abandoned')),
            created_at_ms INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE backup_root (
            lease_id BLOB NOT NULL REFERENCES backup_lease(lease_id),
            object_hash BLOB NOT NULL REFERENCES object_record(object_hash),
            PRIMARY KEY(lease_id, object_hash)
        ) STRICT;",
    )
}

fn migration_3(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "ALTER TABLE translation_revision
         ADD COLUMN revision_kind TEXT NOT NULL DEFAULT 'Edit'
         CHECK(revision_kind IN ('Edit', 'Undo', 'Redo'));
        ALTER TABLE translation_revision
         ADD COLUMN restores_revision_id INTEGER REFERENCES translation_revision(revision_id);
        CREATE TABLE undo_group (
            command_id BLOB PRIMARY KEY CHECK(length(command_id) = 32),
            unit_id BLOB NOT NULL REFERENCES unit(unit_id),
            created_revision_id INTEGER NOT NULL REFERENCES translation_revision(revision_id),
            restored_revision_id INTEGER NOT NULL REFERENCES translation_revision(revision_id),
            expected_head_revision_id INTEGER NOT NULL REFERENCES translation_revision(revision_id),
            revision_kind TEXT NOT NULL CHECK(revision_kind IN ('Undo', 'Redo'))
        ) STRICT;",
    )
}

fn migration_4(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "ALTER TABLE object_reference
         ADD COLUMN media_type TEXT NOT NULL DEFAULT 'application/octet-stream';",
    )
}

fn migration_5(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE export_record (
            export_id INTEGER PRIMARY KEY,
            state TEXT NOT NULL CHECK(state IN (
                'Preparing', 'PublishIntentRecorded', 'Published',
                'CancelledAfterCrash', 'Failed'
            )),
            expected_hash BLOB CHECK(expected_hash IS NULL OR length(expected_hash) = 32)
        ) STRICT;",
    )
}

fn migration_6(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE import_generation (
            generation_id BLOB PRIMARY KEY CHECK(length(generation_id) = 16),
            source_snapshot_hash BLOB NOT NULL CHECK(length(source_snapshot_hash) = 32),
            adapter_id TEXT NOT NULL,
            adapter_build TEXT NOT NULL,
            identity_version INTEGER NOT NULL CHECK(identity_version > 0),
            state TEXT NOT NULL CHECK(state IN ('Building', 'Validated', 'Active', 'Retired', 'Failed')),
            created_at_ms INTEGER NOT NULL,
            activated_at_ms INTEGER
        ) STRICT;
        CREATE UNIQUE INDEX one_active_generation
            ON import_generation(state) WHERE state = 'Active';
        CREATE TABLE generation_batch_receipt (
            generation_id BLOB NOT NULL REFERENCES import_generation(generation_id),
            batch_id BLOB NOT NULL CHECK(length(batch_id) = 32),
            payload_hash BLOB NOT NULL CHECK(length(payload_hash) = 32),
            item_count INTEGER NOT NULL CHECK(item_count >= 0),
            PRIMARY KEY(generation_id, batch_id)
        ) STRICT;
        CREATE TABLE generation_resource (
            generation_id BLOB NOT NULL REFERENCES import_generation(generation_id),
            resource_id BLOB NOT NULL CHECK(length(resource_id) = 16),
            resource_key BLOB NOT NULL CHECK(length(resource_key) = 32),
            kind TEXT NOT NULL,
            semantic_path TEXT NOT NULL,
            locator_json BLOB NOT NULL,
            PRIMARY KEY(generation_id, resource_id),
            UNIQUE(generation_id, resource_key)
        ) STRICT;
        CREATE TABLE generation_edge (
            generation_id BLOB NOT NULL REFERENCES import_generation(generation_id),
            from_resource_id BLOB NOT NULL CHECK(length(from_resource_id) = 16),
            to_resource_id BLOB NOT NULL CHECK(length(to_resource_id) = 16),
            edge_kind TEXT NOT NULL,
            ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
            PRIMARY KEY(generation_id, from_resource_id, to_resource_id, edge_kind, ordinal),
            FOREIGN KEY(generation_id, from_resource_id)
                REFERENCES generation_resource(generation_id, resource_id),
            FOREIGN KEY(generation_id, to_resource_id)
                REFERENCES generation_resource(generation_id, resource_id)
        ) STRICT;
        CREATE TABLE generation_unit (
            generation_id BLOB NOT NULL REFERENCES import_generation(generation_id),
            extracted_unit_id BLOB NOT NULL CHECK(length(extracted_unit_id) = 16),
            source_unit_key BLOB NOT NULL CHECK(length(source_unit_key) = 32),
            resource_id BLOB NOT NULL CHECK(length(resource_id) = 16),
            locator_json BLOB NOT NULL,
            tir_json BLOB NOT NULL,
            reading_order INTEGER NOT NULL CHECK(reading_order >= 0),
            PRIMARY KEY(generation_id, extracted_unit_id),
            FOREIGN KEY(generation_id, resource_id)
                REFERENCES generation_resource(generation_id, resource_id)
        ) STRICT;
        CREATE INDEX generation_unit_source_key
            ON generation_unit(generation_id, source_unit_key);
        CREATE TABLE generation_binding (
            binding_id BLOB PRIMARY KEY CHECK(length(binding_id) = 16),
            generation_id BLOB NOT NULL,
            extracted_unit_id BLOB NOT NULL CHECK(length(extracted_unit_id) = 16),
            disposition TEXT NOT NULL CHECK(disposition IN ('Exact', 'Shifted', 'Ambiguous', 'Orphaned')),
            selected_unit_id BLOB CHECK(selected_unit_id IS NULL OR length(selected_unit_id) = 16),
            policy_version INTEGER NOT NULL CHECK(policy_version > 0),
            candidates_json BLOB NOT NULL,
            candidates_hash BLOB NOT NULL CHECK(length(candidates_hash) = 32),
            FOREIGN KEY(generation_id, extracted_unit_id)
                REFERENCES generation_unit(generation_id, extracted_unit_id)
        ) STRICT;
        CREATE TABLE binding_decision (
            command_id BLOB PRIMARY KEY CHECK(length(command_id) = 32),
            binding_id BLOB NOT NULL REFERENCES generation_binding(binding_id),
            selected_unit_id BLOB NOT NULL CHECK(length(selected_unit_id) = 16),
            candidate_set_hash BLOB NOT NULL CHECK(length(candidate_set_hash) = 32),
            reason_code TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        ) STRICT;",
    )
}

fn migration_7(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "ALTER TABLE generation_unit
         ADD COLUMN unit_id BLOB CHECK(unit_id IS NULL OR length(unit_id) = 16);
        CREATE INDEX generation_unit_resolved_unit
            ON generation_unit(generation_id, unit_id);
        CREATE INDEX generation_unit_reading_order
            ON generation_unit(generation_id, reading_order);
        CREATE INDEX generation_binding_unit
            ON generation_binding(generation_id, extracted_unit_id);
        CREATE INDEX generation_binding_selected_unit
            ON generation_binding(generation_id, selected_unit_id);",
    )
}

fn migration_8(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE term (
            term_id BLOB PRIMARY KEY CHECK(length(term_id) = 16),
            source_text TEXT NOT NULL,
            preferred_translation TEXT NOT NULL,
            notes TEXT NOT NULL DEFAULT '',
            state TEXT NOT NULL CHECK(state IN ('Active', 'Deprecated')),
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        ) STRICT;
        CREATE UNIQUE INDEX term_source_active
            ON term(source_text) WHERE state = 'Active';
        CREATE TABLE term_variant (
            term_id BLOB NOT NULL REFERENCES term(term_id) ON DELETE CASCADE,
            variant TEXT NOT NULL,
            PRIMARY KEY(term_id, variant)
        ) STRICT;
        CREATE TABLE annotation (
            annotation_id BLOB PRIMARY KEY CHECK(length(annotation_id) = 16),
            unit_id BLOB NOT NULL REFERENCES unit(unit_id),
            base_revision_id INTEGER REFERENCES translation_revision(revision_id),
            grapheme_start INTEGER NOT NULL CHECK(grapheme_start >= 0),
            grapheme_end INTEGER NOT NULL CHECK(grapheme_end >= grapheme_start),
            body TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('Active', 'Resolved')),
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        ) STRICT;
        CREATE INDEX annotation_unit_state
            ON annotation(unit_id, state);
        CREATE TABLE marker (
            marker_id BLOB PRIMARY KEY CHECK(length(marker_id) = 16),
            unit_id BLOB NOT NULL REFERENCES unit(unit_id),
            kind TEXT NOT NULL,
            label TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            UNIQUE(unit_id, kind, label)
        ) STRICT;
        CREATE INDEX marker_unit
            ON marker(unit_id);
        CREATE TABLE translation_batch (
            batch_id BLOB PRIMARY KEY CHECK(length(batch_id) = 32),
            find_text TEXT NOT NULL,
            replacement_text TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            commit_sequence_start INTEGER NOT NULL,
            commit_sequence_end INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE translation_batch_member (
            batch_id BLOB NOT NULL REFERENCES translation_batch(batch_id),
            unit_id BLOB NOT NULL REFERENCES unit(unit_id),
            before_revision_id INTEGER NOT NULL REFERENCES translation_revision(revision_id),
            after_revision_id INTEGER NOT NULL REFERENCES translation_revision(revision_id),
            PRIMARY KEY(batch_id, unit_id)
        ) STRICT;",
    )
}

fn migration_9(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE project_navigation (
            singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
            schema_version INTEGER NOT NULL CHECK(schema_version > 0),
            project_id BLOB NOT NULL CHECK(length(project_id) = 16),
            view TEXT NOT NULL CHECK(view IN ('LongForm', 'Units', 'Resources')),
            unit_id BLOB CHECK(unit_id IS NULL OR length(unit_id) = 16),
            resource_id BLOB CHECK(resource_id IS NULL OR length(resource_id) = 16),
            region_id BLOB CHECK(region_id IS NULL OR length(region_id) = 16),
            scroll_anchor_unit_id BLOB
                CHECK(scroll_anchor_unit_id IS NULL OR length(scroll_anchor_unit_id) = 16),
            scroll_offset_px INTEGER NOT NULL,
            zoom_millionths INTEGER NOT NULL
                CHECK(zoom_millionths BETWEEN 100000 AND 8000000),
            filters_json BLOB NOT NULL,
            client_session_id TEXT NOT NULL,
            position_sequence INTEGER NOT NULL CHECK(position_sequence >= 0),
            updated_at_ms INTEGER NOT NULL
        ) STRICT;",
    )
}

fn migration_10(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE image_region_ocr_cache (
            generation_id BLOB NOT NULL CHECK(length(generation_id) = 16),
            region_resource_id BLOB NOT NULL CHECK(length(region_resource_id) = 16),
            model_hash BLOB NOT NULL CHECK(length(model_hash) = 32),
            candidate_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(generation_id, region_resource_id, model_hash),
            FOREIGN KEY(generation_id, region_resource_id)
                REFERENCES generation_resource(generation_id, resource_id)
        ) STRICT;
        CREATE TABLE image_region_revision (
            revision_id INTEGER PRIMARY KEY,
            unit_id BLOB NOT NULL REFERENCES unit(unit_id),
            generation_id BLOB NOT NULL CHECK(length(generation_id) = 16),
            region_resource_id BLOB NOT NULL CHECK(length(region_resource_id) = 16),
            command_id BLOB NOT NULL UNIQUE CHECK(length(command_id) = 32),
            commit_sequence INTEGER NOT NULL UNIQUE,
            parent_revision_id INTEGER REFERENCES image_region_revision(revision_id),
            corrected_source_text TEXT,
            render_parameters_json BLOB NOT NULL,
            derived_object_hash BLOB
                CHECK(derived_object_hash IS NULL OR length(derived_object_hash) = 32),
            created_at_ms INTEGER NOT NULL,
            FOREIGN KEY(generation_id, region_resource_id)
                REFERENCES generation_resource(generation_id, resource_id),
            FOREIGN KEY(derived_object_hash) REFERENCES object_record(object_hash)
        ) STRICT;
        CREATE INDEX image_region_revision_unit
            ON image_region_revision(unit_id, revision_id);
        CREATE TABLE image_region_head (
            unit_id BLOB PRIMARY KEY REFERENCES unit(unit_id),
            revision_id INTEGER NOT NULL REFERENCES image_region_revision(revision_id)
        ) STRICT;",
    )
}

fn migration_11(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "ALTER TABLE translation_revision
         ADD COLUMN document_schema_version INTEGER
         CHECK(document_schema_version IS NULL OR document_schema_version > 0);
         ALTER TABLE translation_revision
         ADD COLUMN document_json BLOB;",
    )
}

fn migration_12(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE workspace_operation_log (
            operation_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL CHECK(kind IN ('create-folder', 'rename', 'move', 'trash', 'restore', 'reveal')),
            state TEXT NOT NULL CHECK(state IN ('Preparing', 'Completed', 'CancelledAfterCrash', 'Failed')),
            source_node_id TEXT,
            target_node_id TEXT,
            source_path TEXT,
            target_path TEXT,
            recycle_path TEXT,
            commit_sequence INTEGER CHECK(commit_sequence IS NULL OR commit_sequence >= 0),
            error TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER
        ) STRICT;
        CREATE INDEX workspace_operation_log_state
            ON workspace_operation_log(state, updated_at_ms);",
    )
}

fn migration_13(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "ALTER TABLE export_record ADD COLUMN destination_path TEXT;
         ALTER TABLE export_record ADD COLUMN format TEXT;
         ALTER TABLE export_record ADD COLUMN created_at_ms INTEGER;
         ALTER TABLE export_record ADD COLUMN updated_at_ms INTEGER;
         ALTER TABLE export_record ADD COLUMN error TEXT;",
    )
}

#[cfg(test)]
fn has_table(connection: &Connection, table: &str) -> rusqlite::Result<bool> {
    has_table_runtime(connection, table)
}

pub(crate) fn has_table_runtime(connection: &Connection, table: &str) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_ordered_and_idempotent() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        migrate(&mut connection).unwrap();
        migrate(&mut connection).unwrap();
        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(has_table(&connection, "task_record").unwrap());
        assert!(has_table(&connection, "export_record").unwrap());
        assert!(has_table(&connection, "import_generation").unwrap());
        assert!(has_table(&connection, "term").unwrap());
        assert!(has_table(&connection, "project_navigation").unwrap());
        assert!(has_table(&connection, "image_region_ocr_cache").unwrap());
        assert!(has_table(&connection, "image_region_revision").unwrap());
        assert!(has_table(&connection, "image_region_head").unwrap());
        let document_columns: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('translation_revision')
                 WHERE name IN ('document_schema_version', 'document_json')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(document_columns, 2);
        let count: i64 = connection
            .query_row("SELECT count(*) FROM migration_record", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
            .unwrap();
        assert!(migrate(&mut connection).is_err());
    }

    #[test]
    fn older_schema_advances_through_every_recorded_step() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        migration_1(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO migration_record(schema_version, completed_at_ms) VALUES (1, 1)",
                [],
            )
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();

        migrate(&mut connection).unwrap();
        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(has_table(&connection, "undo_group").unwrap());
        let kinds: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('translation_revision')
                 WHERE name IN ('revision_kind', 'restores_revision_id')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kinds, 2);
        let reference_media_type: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('object_reference')
                 WHERE name = 'media_type'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reference_media_type, 1);
        assert!(has_table(&connection, "export_record").unwrap());
        assert!(has_table(&connection, "generation_binding").unwrap());
        assert!(has_table(&connection, "annotation").unwrap());
        assert!(has_table(&connection, "project_navigation").unwrap());
        assert!(has_table(&connection, "image_region_ocr_cache").unwrap());
        assert!(has_table(&connection, "image_region_revision").unwrap());
        let document_columns: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('translation_revision')
                 WHERE name IN ('document_schema_version', 'document_json')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(document_columns, 2);
        let resolved_unit_id: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('generation_unit')
                 WHERE name = 'unit_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved_unit_id, 1);
    }
}
