use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rusqlite::{
    Connection, OptionalExtension, TransactionBehavior, functions::FunctionFlags, params,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use babel_domain::{
    core::{ProjectId, RevisionKind, TaskId, TaskState, WorkPriority},
    workbench::NavigationPosition,
};

use crate::{migration, schema};

const PROJECT_PAGE_SIZE_BYTES: i64 = 32 * 1024;
const WRITER_CACHE_KIB: i64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveReceipt {
    pub revision_id: i64,
    pub commit_sequence: i64,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavePoint {
    RevisionWritten,
    BeforeCommit,
    AfterCommit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitPageItem {
    pub unit_id: Vec<u8>,
    pub source_unit_key: Vec<u8>,
    pub local_index: i64,
    pub source_text: String,
    pub translation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointResult {
    pub busy: i64,
    pub log_pages: i64,
    pub checkpointed_pages: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectRecord {
    pub hash: [u8; 32],
    pub byte_length: u64,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupPin {
    pub commit_sequence: i64,
    pub object_hashes: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: [u8; 16],
    pub task_kind: String,
    pub state: TaskState,
    pub priority: WorkPriority,
    pub progress_current: u64,
    pub progress_total: Option<u64>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftDisposition {
    BaseUnchanged,
    BaseChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftRecovery {
    pub unit_id: Vec<u8>,
    pub base_revision_id: Option<i64>,
    pub current_revision_id: Option<i64>,
    pub client_session_id: String,
    pub patch: Vec<u8>,
    pub updated_at_ms: i64,
    pub disposition: DraftDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationSaveReceipt {
    pub position_sequence: u64,
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationDescriptor {
    pub generation_id: [u8; 16],
    pub source_snapshot_hash: [u8; 32],
    pub adapter_id: String,
    pub adapter_build: String,
    pub identity_version: u32,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSnapshotDescriptor {
    pub generation_id: [u8; 16],
    pub source_snapshot_hash: [u8; 32],
    pub adapter_id: String,
    pub adapter_build: String,
    pub identity_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationResourceRecord {
    pub resource_id: [u8; 16],
    pub resource_key: [u8; 32],
    pub kind: String,
    pub semantic_path: String,
    pub locator_json: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationEdgeRecord {
    pub from_resource_id: [u8; 16],
    pub to_resource_id: [u8; 16],
    pub edge_kind: String,
    pub ordinal: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationUnitRecord {
    pub extracted_unit_id: [u8; 16],
    pub source_unit_key: [u8; 32],
    pub resource_id: [u8; 16],
    pub locator_json: Vec<u8>,
    pub tir_json: Vec<u8>,
    pub reading_order: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationUnitView {
    pub extracted_unit_id: [u8; 16],
    pub unit_id: [u8; 16],
    pub source_unit_key: [u8; 32],
    pub resource_id: [u8; 16],
    pub locator_json: Vec<u8>,
    pub tir_json: Vec<u8>,
    pub reading_order: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenUnitSnapshot {
    pub extracted_unit_id: [u8; 16],
    pub unit_id: [u8; 16],
    pub source_unit_key: [u8; 32],
    pub resource_id: [u8; 16],
    pub locator_json: Vec<u8>,
    pub tir_json: Vec<u8>,
    pub reading_order: u64,
    pub source_text: String,
    pub translation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermRecord {
    pub term_id: [u8; 16],
    pub source_text: String,
    pub preferred_translation: String,
    pub notes: String,
    pub state: String,
    pub variants: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertTermRequest {
    pub term_id: [u8; 16],
    pub source_text: String,
    pub preferred_translation: String,
    pub variants: Vec<String>,
    pub notes: String,
    pub state: String,
    pub timestamp_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationRecord {
    pub annotation_id: [u8; 16],
    pub unit_id: Vec<u8>,
    pub base_revision_id: Option<i64>,
    pub current_revision_id: Option<i64>,
    pub grapheme_start: u64,
    pub grapheme_end: u64,
    pub body: String,
    pub state: String,
    pub stale: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkerRecord {
    pub marker_id: [u8; 16],
    pub unit_id: Vec<u8>,
    pub kind: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationHistoryItem {
    pub unit_id: Vec<u8>,
    pub source_text: String,
    pub revision_id: i64,
    pub commit_sequence: i64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateSourceGroup {
    pub source_text: String,
    pub canonical_hash: [u8; 32],
    pub unit_ids: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacePreviewItem {
    pub unit_id: Vec<u8>,
    pub source_unit_key: Vec<u8>,
    pub expected_head_revision_id: i64,
    pub before_text: String,
    pub after_text: String,
    pub occurrences: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchReplaceReceipt {
    pub affected_units: usize,
    pub commit_sequence_start: i64,
    pub commit_sequence_end: i64,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationBindingRecord {
    pub binding_id: [u8; 16],
    pub extracted_unit_id: [u8; 16],
    pub disposition: String,
    pub selected_unit_id: Option<[u8; 16]>,
    pub policy_version: u32,
    pub candidates_json: Vec<u8>,
    pub candidates_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationBindingView {
    pub binding_id: [u8; 16],
    pub extracted_unit_id: [u8; 16],
    pub disposition: String,
    pub selected_unit_id: Option<[u8; 16]>,
    pub candidates: Vec<[u8; 16]>,
    pub candidates_hash: [u8; 32],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GenerationBatch {
    pub resources: Vec<GenerationResourceRecord>,
    pub edges: Vec<GenerationEdgeRecord>,
    pub units: Vec<GenerationUnitRecord>,
    pub bindings: Vec<GenerationBindingRecord>,
}

impl GenerationBatch {
    pub fn item_count(&self) -> usize {
        self.resources.len() + self.edges.len() + self.units.len() + self.bindings.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenerationBatchReceipt {
    pub item_count: usize,
    pub replayed: bool,
}

pub fn candidate_set_hash(canonical_json: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"babel-binding-candidates-v1");
    hasher.update((canonical_json.len() as u64).to_be_bytes());
    hasher.update(canonical_json);
    hasher.finalize().into()
}

pub struct ProjectStore {
    connection: Connection,
}

impl ProjectStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let path = path.as_ref();
        let mut connection = Connection::open(path)?;
        let version = schema::schema_version(&connection)?;
        if version > schema::CURRENT_SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if version > 0 && version < schema::CURRENT_SCHEMA_VERSION {
            migration::backup_before_migration(&connection, path, version)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
        if version == 0 {
            connection.pragma_update(None, "page_size", PROJECT_PAGE_SIZE_BYTES)?;
        }
        configure_connection(&connection)?;
        schema::migrate(&mut connection)?;
        initialize_project_identity(&connection)?;
        Ok(Self { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn seed_units(&mut self, unit_count: usize) -> rusqlite::Result<()> {
        const BATCH_SIZE: usize = 2_000;
        for start in (0..unit_count).step_by(BATCH_SIZE) {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            {
                let mut insert = transaction.prepare_cached(
                    "INSERT INTO unit (
                        unit_id, source_unit_key, local_index, source_text
                     ) VALUES (?1, ?2, ?3, ?4)",
                )?;
                for index in start..(start + BATCH_SIZE).min(unit_count) {
                    let source_text = format!(
                        "源文段落 {index}：Babel Tower keeps translation work offline and safe."
                    );
                    let unit_hash = hash_parts(&[b"unit", &(index as u64).to_be_bytes()]);
                    let unit_id = &unit_hash[..16];
                    let source_key = hash_parts(&[
                        b"source-unit-v1",
                        &(index as u64).to_be_bytes(),
                        source_text.as_bytes(),
                    ]);
                    insert.execute(params![
                        unit_id,
                        source_key.as_slice(),
                        index as i64,
                        source_text
                    ])?;
                }
            }
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn rebuild_search(&mut self) -> rusqlite::Result<usize> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM unit_search", [])?;
        let inserted = transaction.execute(
            "INSERT INTO unit_search(unit_id, source, translation)
             SELECT hex(u.unit_id), u.source_text, COALESCE(r.text, '')
             FROM unit u
             LEFT JOIN unit_head h ON h.unit_id = u.unit_id
             LEFT JOIN translation_revision r ON r.revision_id = h.revision_id
             WHERE u.local_index >= 0",
            [],
        )?;
        transaction.execute("DELETE FROM search_dirty", [])?;
        transaction.commit()?;
        Ok(inserted)
    }

    pub fn flush_search_dirty(&mut self, limit: usize) -> rusqlite::Result<usize> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS search_flush_batch (
                unit_id BLOB PRIMARY KEY
            ) WITHOUT ROWID;
            DELETE FROM search_flush_batch;",
        )?;
        let flushed = transaction.execute(
            "INSERT INTO search_flush_batch(unit_id)
             SELECT unit_id FROM search_dirty ORDER BY dirty_sequence LIMIT ?1",
            [limit as i64],
        )?;
        transaction.execute(
            "DELETE FROM unit_search
             WHERE unit_id IN (SELECT hex(unit_id) FROM search_flush_batch)",
            [],
        )?;
        transaction.execute(
            "INSERT INTO unit_search(unit_id, source, translation)
             SELECT hex(u.unit_id), u.source_text, COALESCE(r.text, '')
             FROM search_flush_batch b
             JOIN unit u ON u.unit_id = b.unit_id
             LEFT JOIN unit_head h ON h.unit_id = u.unit_id
             LEFT JOIN translation_revision r ON r.revision_id = h.revision_id
             WHERE u.local_index >= 0",
            [],
        )?;
        transaction.execute(
            "DELETE FROM search_dirty
             WHERE unit_id IN (SELECT unit_id FROM search_flush_batch)",
            [],
        )?;
        transaction.commit()?;
        Ok(flushed)
    }

    pub fn save_translation(
        &mut self,
        source_unit_key: &[u8; 32],
        command_id: &[u8; 32],
        text: &str,
        created_at_ms: i64,
    ) -> rusqlite::Result<SaveReceipt> {
        self.save_translation_with_hook(source_unit_key, command_id, text, created_at_ms, |_| {})
    }

    pub fn save_navigation_position(
        &mut self,
        position: &NavigationPosition,
        client_session_id: &str,
        position_sequence: u64,
        updated_at_ms: i64,
    ) -> rusqlite::Result<NavigationSaveReceipt> {
        position
            .validate()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        if position.project_id != self.project_id()? || client_session_id.is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let stored_sequence = i64::try_from(position_sequence)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let filters_json = serde_json::to_vec(&position.filters)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = transaction
            .query_row(
                "SELECT client_session_id, position_sequence
                 FROM project_navigation WHERE singleton = 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let accepted = current.as_ref().is_none_or(|(session, sequence)| {
            session != client_session_id || stored_sequence > *sequence
        });
        if accepted {
            transaction.execute(
                "INSERT INTO project_navigation(
                    singleton, schema_version, project_id, view, unit_id, resource_id,
                    region_id, scroll_anchor_unit_id, scroll_offset_px, zoom_millionths,
                    filters_json, client_session_id, position_sequence, updated_at_ms
                 ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(singleton) DO UPDATE SET
                    schema_version = excluded.schema_version,
                    project_id = excluded.project_id,
                    view = excluded.view,
                    unit_id = excluded.unit_id,
                    resource_id = excluded.resource_id,
                    region_id = excluded.region_id,
                    scroll_anchor_unit_id = excluded.scroll_anchor_unit_id,
                    scroll_offset_px = excluded.scroll_offset_px,
                    zoom_millionths = excluded.zoom_millionths,
                    filters_json = excluded.filters_json,
                    client_session_id = excluded.client_session_id,
                    position_sequence = excluded.position_sequence,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    i64::from(position.schema_version),
                    position.project_id.as_bytes().as_slice(),
                    position.view.as_str(),
                    position.unit_id.map(|id| id.as_bytes().to_vec()),
                    position.resource_id.map(|id| id.as_bytes().to_vec()),
                    position.region_id.map(|id| id.as_bytes().to_vec()),
                    position
                        .scroll_anchor_unit_id
                        .map(|id| id.as_bytes().to_vec()),
                    position.scroll_offset_px,
                    i64::from(position.zoom_millionths),
                    filters_json,
                    client_session_id,
                    stored_sequence,
                    updated_at_ms,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(NavigationSaveReceipt {
            position_sequence,
            accepted,
        })
    }

    #[doc(hidden)]
    pub fn save_translation_with_hook<F>(
        &mut self,
        source_unit_key: &[u8; 32],
        command_id: &[u8; 32],
        text: &str,
        created_at_ms: i64,
        mut hook: F,
    ) -> rusqlite::Result<SaveReceipt>
    where
        F: FnMut(SavePoint),
    {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((revision_id, commit_sequence)) = transaction
            .query_row(
                "SELECT revision_id, commit_sequence FROM command_receipt WHERE command_id = ?1",
                [command_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
        {
            transaction.commit()?;
            return Ok(SaveReceipt {
                revision_id,
                commit_sequence,
                replayed: true,
            });
        }

        let unit_id = transaction.query_row(
            "SELECT unit_id FROM unit WHERE source_unit_key = ?1",
            [source_unit_key.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )?;
        let commit_sequence: i64 = transaction.query_row(
            "UPDATE project_state SET commit_sequence = commit_sequence + 1
             WHERE singleton = 1 RETURNING commit_sequence",
            [],
            |row| row.get(0),
        )?;
        let parent_revision_id = transaction
            .query_row(
                "SELECT revision_id FROM unit_head WHERE unit_id = ?1",
                [unit_id.as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        transaction.execute(
            "INSERT INTO translation_revision (
                unit_id, command_id, commit_sequence, parent_revision_id, text, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                unit_id.as_slice(),
                command_id.as_slice(),
                commit_sequence,
                parent_revision_id,
                text,
                created_at_ms
            ],
        )?;
        let revision_id = transaction.last_insert_rowid();
        hook(SavePoint::RevisionWritten);
        transaction.execute(
            "INSERT INTO unit_head(unit_id, revision_id)
             VALUES (?1, ?2)
             ON CONFLICT(unit_id) DO UPDATE SET revision_id = excluded.revision_id",
            params![unit_id.as_slice(), revision_id],
        )?;
        transaction.execute(
            "INSERT INTO command_receipt(command_id, revision_id, commit_sequence)
             VALUES (?1, ?2, ?3)",
            params![command_id.as_slice(), revision_id, commit_sequence],
        )?;
        transaction.execute(
            "INSERT INTO search_dirty(unit_id, dirty_sequence)
             VALUES (?1, ?2)
             ON CONFLICT(unit_id) DO UPDATE SET dirty_sequence = excluded.dirty_sequence",
            params![unit_id.as_slice(), commit_sequence],
        )?;
        hook(SavePoint::BeforeCommit);
        transaction.commit()?;
        hook(SavePoint::AfterCommit);
        Ok(SaveReceipt {
            revision_id,
            commit_sequence,
            replayed: false,
        })
    }

    pub fn restore_translation(
        &mut self,
        source_unit_key: &[u8; 32],
        command_id: &[u8; 32],
        expected_head_revision_id: i64,
        restores_revision_id: i64,
        kind: RevisionKind,
        created_at_ms: i64,
    ) -> rusqlite::Result<SaveReceipt> {
        if kind == RevisionKind::Edit {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((revision_id, commit_sequence)) = transaction
            .query_row(
                "SELECT revision_id, commit_sequence FROM command_receipt WHERE command_id = ?1",
                [command_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
        {
            transaction.commit()?;
            return Ok(SaveReceipt {
                revision_id,
                commit_sequence,
                replayed: true,
            });
        }

        let unit_id = transaction.query_row(
            "SELECT unit_id FROM unit WHERE source_unit_key = ?1",
            [source_unit_key.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )?;
        let current_head = transaction.query_row(
            "SELECT revision_id FROM unit_head WHERE unit_id = ?1",
            [unit_id.as_slice()],
            |row| row.get::<_, i64>(0),
        )?;
        if current_head != expected_head_revision_id {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let restored_text = transaction.query_row(
            "SELECT text FROM translation_revision
             WHERE revision_id = ?1 AND unit_id = ?2",
            params![restores_revision_id, unit_id.as_slice()],
            |row| row.get::<_, String>(0),
        )?;
        let commit_sequence: i64 = transaction.query_row(
            "UPDATE project_state SET commit_sequence = commit_sequence + 1
             WHERE singleton = 1 RETURNING commit_sequence",
            [],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO translation_revision(
                unit_id, command_id, commit_sequence, parent_revision_id, text,
                created_at_ms, revision_kind, restores_revision_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                unit_id.as_slice(),
                command_id.as_slice(),
                commit_sequence,
                current_head,
                restored_text,
                created_at_ms,
                kind.as_str(),
                restores_revision_id
            ],
        )?;
        let revision_id = transaction.last_insert_rowid();
        transaction.execute(
            "UPDATE unit_head SET revision_id = ?2 WHERE unit_id = ?1",
            params![unit_id.as_slice(), revision_id],
        )?;
        transaction.execute(
            "INSERT INTO command_receipt(command_id, revision_id, commit_sequence)
             VALUES (?1, ?2, ?3)",
            params![command_id.as_slice(), revision_id, commit_sequence],
        )?;
        transaction.execute(
            "INSERT INTO undo_group(
                command_id, unit_id, created_revision_id, restored_revision_id,
                expected_head_revision_id, revision_kind
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                command_id.as_slice(),
                unit_id.as_slice(),
                revision_id,
                restores_revision_id,
                expected_head_revision_id,
                kind.as_str()
            ],
        )?;
        transaction.execute(
            "INSERT INTO search_dirty(unit_id, dirty_sequence)
             VALUES (?1, ?2)
             ON CONFLICT(unit_id) DO UPDATE SET dirty_sequence = excluded.dirty_sequence",
            params![unit_id.as_slice(), commit_sequence],
        )?;
        transaction.commit()?;
        Ok(SaveReceipt {
            revision_id,
            commit_sequence,
            replayed: false,
        })
    }

    pub fn upsert_term(&mut self, request: &UpsertTermRequest) -> rusqlite::Result<()> {
        let term_id = &request.term_id;
        let source_text = request.source_text.as_str();
        let preferred_translation = request.preferred_translation.as_str();
        let variants = request.variants.as_slice();
        let notes = request.notes.as_str();
        let state = request.state.as_str();
        let timestamp_ms = request.timestamp_ms;
        if !matches!(state, "Active" | "Deprecated") {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO term(
                term_id, source_text, preferred_translation, notes, state,
                created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(term_id) DO UPDATE SET
                source_text = excluded.source_text,
                preferred_translation = excluded.preferred_translation,
                notes = excluded.notes,
                state = excluded.state,
                updated_at_ms = excluded.updated_at_ms",
            params![
                term_id.as_slice(),
                source_text,
                preferred_translation,
                notes,
                state,
                timestamp_ms
            ],
        )?;
        transaction.execute(
            "DELETE FROM term_variant WHERE term_id = ?1",
            [term_id.as_slice()],
        )?;
        {
            let mut insert = transaction
                .prepare_cached("INSERT INTO term_variant(term_id, variant) VALUES (?1, ?2)")?;
            for variant in variants {
                insert.execute(params![term_id.as_slice(), variant])?;
            }
        }
        transaction.commit()
    }

    pub fn terms(&self, include_deprecated: bool) -> rusqlite::Result<Vec<TermRecord>> {
        let where_clause = if include_deprecated {
            ""
        } else {
            "WHERE state = 'Active'"
        };
        let mut statement = self.connection.prepare(&format!(
            "SELECT term_id, source_text, preferred_translation, notes, state
             FROM term {where_clause} ORDER BY source_text, term_id"
        ))?;
        let terms = statement
            .query_map([], |row| {
                let term_id: [u8; 16] = vec_to_array(row.get(0)?)?;
                Ok(TermRecord {
                    term_id,
                    source_text: row.get(1)?,
                    preferred_translation: row.get(2)?,
                    notes: row.get(3)?,
                    state: row.get(4)?,
                    variants: self.term_variants(&term_id)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(terms)
    }

    pub fn find_terms(&self, text: &str, limit: usize) -> rusqlite::Result<Vec<TermRecord>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT DISTINCT t.term_id, t.source_text, t.preferred_translation, t.notes, t.state
             FROM term t
             LEFT JOIN term_variant v ON v.term_id = t.term_id
             WHERE t.state = 'Active'
               AND (?1 LIKE '%' || t.source_text || '%'
                    OR (v.variant IS NOT NULL AND ?1 LIKE '%' || v.variant || '%'))
             ORDER BY length(t.source_text) DESC, t.source_text
             LIMIT ?2",
        )?;
        statement
            .query_map(params![text, limit as i64], |row| {
                let term_id: [u8; 16] = vec_to_array(row.get(0)?)?;
                Ok(TermRecord {
                    term_id,
                    source_text: row.get(1)?,
                    preferred_translation: row.get(2)?,
                    notes: row.get(3)?,
                    state: row.get(4)?,
                    variants: self.term_variants(&term_id)?,
                })
            })?
            .collect()
    }

    pub fn add_annotation(
        &mut self,
        annotation_id: &[u8; 16],
        unit_id: &[u8],
        base_revision_id: Option<i64>,
        range: std::ops::Range<u64>,
        body: &str,
        created_at_ms: i64,
    ) -> rusqlite::Result<()> {
        if range.start > range.end {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.connection.execute(
            "INSERT INTO annotation(
                annotation_id, unit_id, base_revision_id, grapheme_start, grapheme_end,
                body, state, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Active', ?7, ?7)",
            params![
                annotation_id.as_slice(),
                unit_id,
                base_revision_id,
                range.start as i64,
                range.end as i64,
                body,
                created_at_ms
            ],
        )?;
        Ok(())
    }

    pub fn annotations_for_unit(&self, unit_id: &[u8]) -> rusqlite::Result<Vec<AnnotationRecord>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT a.annotation_id, a.unit_id, a.base_revision_id, h.revision_id,
                    a.grapheme_start, a.grapheme_end, a.body, a.state
             FROM annotation a
             LEFT JOIN unit_head h ON h.unit_id = a.unit_id
             WHERE a.unit_id = ?1
             ORDER BY a.created_at_ms, a.annotation_id",
        )?;
        statement
            .query_map([unit_id], |row| {
                let base_revision_id = row.get::<_, Option<i64>>(2)?;
                let current_revision_id = row.get::<_, Option<i64>>(3)?;
                Ok(AnnotationRecord {
                    annotation_id: vec_to_array(row.get(0)?)?,
                    unit_id: row.get(1)?,
                    base_revision_id,
                    current_revision_id,
                    grapheme_start: row.get::<_, i64>(4)? as u64,
                    grapheme_end: row.get::<_, i64>(5)? as u64,
                    body: row.get(6)?,
                    state: row.get(7)?,
                    stale: base_revision_id.is_some() && base_revision_id != current_revision_id,
                })
            })?
            .collect()
    }

    pub fn set_marker(
        &mut self,
        marker_id: &[u8; 16],
        unit_id: &[u8],
        kind: &str,
        label: &str,
        created_at_ms: i64,
    ) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT INTO marker(marker_id, unit_id, kind, label, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(unit_id, kind, label) DO NOTHING",
            params![marker_id.as_slice(), unit_id, kind, label, created_at_ms],
        )?;
        Ok(())
    }

    pub fn delete_marker(
        &mut self,
        unit_id: &[u8],
        kind: &str,
        label: &str,
    ) -> rusqlite::Result<usize> {
        self.connection.execute(
            "DELETE FROM marker WHERE unit_id = ?1 AND kind = ?2 AND label = ?3",
            params![unit_id, kind, label],
        )
    }

    pub fn markers_for_unit(&self, unit_id: &[u8]) -> rusqlite::Result<Vec<MarkerRecord>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT marker_id, unit_id, kind, label FROM marker
             WHERE unit_id = ?1 ORDER BY kind, label",
        )?;
        statement
            .query_map([unit_id], |row| {
                Ok(MarkerRecord {
                    marker_id: vec_to_array(row.get(0)?)?,
                    unit_id: row.get(1)?,
                    kind: row.get(2)?,
                    label: row.get(3)?,
                })
            })?
            .collect()
    }

    pub fn translation_history(
        &self,
        source_text: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<TranslationHistoryItem>> {
        let canonical = canonical_source_text(source_text);
        let mut statement = self.connection.prepare_cached(
            "SELECT u.unit_id, u.source_text, r.revision_id, r.commit_sequence, r.text
             FROM translation_revision r
             JOIN unit u ON u.unit_id = r.unit_id
             WHERE lower(trim(u.source_text)) = ?1
             ORDER BY r.commit_sequence DESC
             LIMIT ?2",
        )?;
        statement
            .query_map(params![canonical, limit as i64], |row| {
                Ok(TranslationHistoryItem {
                    unit_id: row.get(0)?,
                    source_text: row.get(1)?,
                    revision_id: row.get(2)?,
                    commit_sequence: row.get(3)?,
                    text: row.get(4)?,
                })
            })?
            .collect()
    }

    pub fn duplicate_source_groups(
        &self,
        minimum_count: usize,
        limit: usize,
    ) -> rusqlite::Result<Vec<DuplicateSourceGroup>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT lower(trim(source_text)) AS canonical, min(source_text)
             FROM unit
             WHERE local_index >= 0
             GROUP BY canonical
             HAVING count(*) >= ?1
             ORDER BY count(*) DESC, canonical
             LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![minimum_count as i64, limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut groups = Vec::with_capacity(rows.len());
        for (canonical, source_text) in rows {
            let mut units = self.connection.prepare_cached(
                "SELECT unit_id FROM unit
                 WHERE local_index >= 0 AND lower(trim(source_text)) = ?1
                 ORDER BY local_index",
            )?;
            groups.push(DuplicateSourceGroup {
                canonical_hash: hash_parts(&[b"duplicate-source-v1", canonical.as_bytes()]),
                source_text,
                unit_ids: units
                    .query_map([canonical], |row| row.get::<_, Vec<u8>>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            });
        }
        Ok(groups)
    }

    pub fn preview_replace_translations(
        &self,
        find_text: &str,
        replacement_text: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<ReplacePreviewItem>> {
        if find_text.is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let mut statement = self.connection.prepare_cached(
            "SELECT u.unit_id, u.source_unit_key, h.revision_id, r.text
             FROM unit u
             JOIN unit_head h ON h.unit_id = u.unit_id
             JOIN translation_revision r ON r.revision_id = h.revision_id
             WHERE instr(r.text, ?1) > 0
             ORDER BY u.local_index
             LIMIT ?2",
        )?;
        statement
            .query_map(params![find_text, limit as i64], |row| {
                let before_text: String = row.get(3)?;
                let occurrences = before_text.matches(find_text).count();
                Ok(ReplacePreviewItem {
                    unit_id: row.get(0)?,
                    source_unit_key: row.get(1)?,
                    expected_head_revision_id: row.get(2)?,
                    after_text: before_text.replace(find_text, replacement_text),
                    before_text,
                    occurrences,
                })
            })?
            .collect()
    }

    pub fn apply_replace_translations(
        &mut self,
        batch_id: &[u8; 32],
        find_text: &str,
        replacement_text: &str,
        expected: &[ReplacePreviewItem],
        created_at_ms: i64,
    ) -> rusqlite::Result<BatchReplaceReceipt> {
        if find_text.is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((start, end, count)) = transaction
            .query_row(
                "SELECT b.commit_sequence_start, b.commit_sequence_end, count(m.unit_id)
                 FROM translation_batch b
                 LEFT JOIN translation_batch_member m ON m.batch_id = b.batch_id
                 WHERE b.batch_id = ?1
                 GROUP BY b.batch_id",
                [batch_id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
        {
            transaction.commit()?;
            return Ok(BatchReplaceReceipt {
                affected_units: count as usize,
                commit_sequence_start: start,
                commit_sequence_end: end,
                replayed: true,
            });
        }

        let mut members = Vec::with_capacity(expected.len());
        for item in expected {
            let (current_revision_id, current_text): (i64, String) = transaction.query_row(
                "SELECT h.revision_id, r.text
                 FROM unit_head h
                 JOIN translation_revision r ON r.revision_id = h.revision_id
                 WHERE h.unit_id = ?1",
                [item.unit_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if current_revision_id != item.expected_head_revision_id
                || current_text != item.before_text
                || !current_text.contains(find_text)
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            members.push((item.unit_id.clone(), current_revision_id, current_text));
        }

        let sequence_before: i64 = transaction.query_row(
            "SELECT commit_sequence FROM project_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        let mut receipts = Vec::with_capacity(members.len());
        for (index, (unit_id, before_revision_id, before_text)) in members.iter().enumerate() {
            let commit_sequence: i64 = transaction.query_row(
                "UPDATE project_state SET commit_sequence = commit_sequence + 1
                 WHERE singleton = 1 RETURNING commit_sequence",
                [],
                |row| row.get(0),
            )?;
            let after_text = before_text.replace(find_text, replacement_text);
            let command_id = batch_member_command_id(batch_id, unit_id, index as u64);
            transaction.execute(
                "INSERT INTO translation_revision(
                    unit_id, command_id, commit_sequence, parent_revision_id, text, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    unit_id.as_slice(),
                    command_id.as_slice(),
                    commit_sequence,
                    before_revision_id,
                    after_text,
                    created_at_ms
                ],
            )?;
            let after_revision_id = transaction.last_insert_rowid();
            transaction.execute(
                "UPDATE unit_head SET revision_id = ?2
                 WHERE unit_id = ?1 AND revision_id = ?3",
                params![unit_id.as_slice(), after_revision_id, before_revision_id],
            )?;
            transaction.execute(
                "INSERT INTO search_dirty(unit_id, dirty_sequence)
                 VALUES (?1, ?2)
                 ON CONFLICT(unit_id) DO UPDATE SET dirty_sequence = excluded.dirty_sequence",
                params![unit_id.as_slice(), commit_sequence],
            )?;
            receipts.push((unit_id.clone(), *before_revision_id, after_revision_id));
        }
        let sequence_after: i64 = transaction.query_row(
            "SELECT commit_sequence FROM project_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO translation_batch(
                batch_id, find_text, replacement_text, created_at_ms,
                commit_sequence_start, commit_sequence_end
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                batch_id.as_slice(),
                find_text,
                replacement_text,
                created_at_ms,
                sequence_before + 1,
                sequence_after
            ],
        )?;
        {
            let mut insert = transaction.prepare_cached(
                "INSERT INTO translation_batch_member(
                    batch_id, unit_id, before_revision_id, after_revision_id
                 ) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (unit_id, before_revision_id, after_revision_id) in &receipts {
                insert.execute(params![
                    batch_id.as_slice(),
                    unit_id.as_slice(),
                    before_revision_id,
                    after_revision_id
                ])?;
            }
        }
        transaction.commit()?;
        Ok(BatchReplaceReceipt {
            affected_units: receipts.len(),
            commit_sequence_start: sequence_before + 1,
            commit_sequence_end: sequence_after,
            replayed: false,
        })
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

    pub fn search(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<String>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT unit_id FROM unit_search WHERE unit_search MATCH ?1 LIMIT ?2",
        )?;
        statement
            .query_map(params![query, limit as i64], |row| row.get::<_, String>(0))?
            .collect()
    }

    pub fn commit_sequence(&self) -> rusqlite::Result<i64> {
        self.connection.query_row(
            "SELECT commit_sequence FROM project_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
    }

    pub fn project_id(&self) -> rusqlite::Result<ProjectId> {
        self.connection.query_row(
            "SELECT project_id FROM project_state WHERE singleton = 1",
            [],
            |row| {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(ProjectId::from_bytes(vec_to_array(bytes)?))
            },
        )
    }

    pub fn register_object_reference(
        &mut self,
        owner_kind: &str,
        owner_id: &[u8],
        object: &ObjectRecord,
        created_at_ms: i64,
    ) -> rusqlite::Result<()> {
        let byte_length = i64::try_from(object.byte_length)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO object_record(object_hash, byte_length, media_type, created_at_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(object_hash) DO NOTHING",
            params![
                object.hash.as_slice(),
                byte_length,
                object.media_type,
                created_at_ms
            ],
        )?;
        let stored_length: i64 = transaction.query_row(
            "SELECT byte_length FROM object_record WHERE object_hash = ?1",
            [object.hash.as_slice()],
            |row| row.get(0),
        )?;
        if stored_length != byte_length {
            return Err(rusqlite::Error::InvalidQuery);
        }
        transaction.execute(
            "INSERT OR IGNORE INTO object_reference(
                owner_kind, owner_id, object_hash, media_type
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                owner_kind,
                owner_id,
                object.hash.as_slice(),
                object.media_type
            ],
        )?;
        transaction.commit()
    }

    pub fn create_task(
        &mut self,
        task_id: TaskId,
        task_kind: &str,
        priority: WorkPriority,
        created_at_ms: i64,
    ) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT INTO task_record(
                task_id, task_kind, state, priority, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, 'Pending', ?3, ?4, ?4)",
            params![
                task_id.as_bytes().as_slice(),
                task_kind,
                priority_number(priority),
                created_at_ms
            ],
        )?;
        Ok(())
    }

    pub fn object_record(&self, object_hash: &[u8; 32]) -> rusqlite::Result<ObjectRecord> {
        self.connection.query_row(
            "SELECT object_hash, byte_length, media_type
             FROM object_record WHERE object_hash = ?1",
            [object_hash.as_slice()],
            |row| {
                let byte_length: i64 = row.get(1)?;
                Ok(ObjectRecord {
                    hash: vec_to_array(row.get(0)?)?,
                    byte_length: byte_length as u64,
                    media_type: row.get(2)?,
                })
            },
        )
    }

    pub fn recover_interrupted_tasks(&self, recovered_at_ms: i64) -> rusqlite::Result<usize> {
        self.connection.execute(
            "UPDATE task_record
             SET state = 'Paused', failure_code = 'interrupted', updated_at_ms = ?1
             WHERE state = 'Running'",
            [recovered_at_ms],
        )
    }

    pub fn transition_task(
        &mut self,
        task_id: TaskId,
        next: TaskState,
        failure_code: Option<&str>,
        updated_at_ms: i64,
    ) -> rusqlite::Result<TaskRecord> {
        let current = self.task(task_id)?;
        if !current.state.can_transition_to(next) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.connection.execute(
            "UPDATE task_record SET state = ?2, failure_code = ?3, updated_at_ms = ?4
             WHERE task_id = ?1",
            params![
                task_id.as_bytes().as_slice(),
                next.as_str(),
                failure_code,
                updated_at_ms
            ],
        )?;
        self.task(task_id)
    }

    pub fn task(&self, task_id: TaskId) -> rusqlite::Result<TaskRecord> {
        self.connection.query_row(
            "SELECT task_id, task_kind, state, priority, progress_current,
                    progress_total, failure_code
             FROM task_record WHERE task_id = ?1",
            [task_id.as_bytes().as_slice()],
            |row| {
                Ok(TaskRecord {
                    task_id: vec_to_array(row.get(0)?)?,
                    task_kind: row.get(1)?,
                    state: parse_task_state(&row.get::<_, String>(2)?)?,
                    priority: parse_priority(row.get(3)?)?,
                    progress_current: row.get::<_, i64>(4)? as u64,
                    progress_total: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                    failure_code: row.get(6)?,
                })
            },
        )
    }

    pub fn record_diagnostic(
        &self,
        severity: &str,
        code: &str,
        user_message: &str,
        technical_detail: Option<&str>,
        created_at_ms: i64,
    ) -> rusqlite::Result<i64> {
        self.connection.execute(
            "INSERT INTO diagnostic_event(
                severity, code, user_message, technical_detail, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                severity,
                code,
                user_message,
                technical_detail,
                created_at_ms
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn begin_backup_pin(
        &mut self,
        lease_id: &[u8; 16],
        created_at_ms: i64,
    ) -> rusqlite::Result<BackupPin> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let commit_sequence: i64 = transaction.query_row(
            "SELECT commit_sequence FROM project_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        let object_hashes = {
            let mut statement = transaction.prepare(
                "SELECT DISTINCT object_hash FROM object_reference ORDER BY object_hash",
            )?;
            statement
                .query_map([], |row| {
                    let bytes: Vec<u8> = row.get(0)?;
                    vec_to_array(bytes)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        transaction.execute(
            "INSERT INTO backup_lease(
                lease_id, commit_sequence, state, created_at_ms
             ) VALUES (?1, ?2, 'Active', ?3)",
            params![lease_id.as_slice(), commit_sequence, created_at_ms],
        )?;
        {
            let mut insert = transaction
                .prepare_cached("INSERT INTO backup_root(lease_id, object_hash) VALUES (?1, ?2)")?;
            for hash in &object_hashes {
                insert.execute(params![lease_id.as_slice(), hash.as_slice()])?;
            }
        }
        transaction.commit()?;
        Ok(BackupPin {
            commit_sequence,
            object_hashes,
        })
    }

    pub fn finish_backup_pin(&self, lease_id: &[u8; 16], completed: bool) -> rusqlite::Result<()> {
        let state = if completed { "Completed" } else { "Abandoned" };
        let changed = self.connection.execute(
            "UPDATE backup_lease SET state = ?2
             WHERE lease_id = ?1 AND state = 'Active'",
            params![lease_id.as_slice(), state],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    pub fn reachable_object_hashes(&self) -> rusqlite::Result<BTreeSet<[u8; 32]>> {
        let mut statement = self.connection.prepare(
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
                vec_to_array(bytes)
            })?
            .collect()
    }

    pub fn save_draft(
        &self,
        unit_id: &[u8],
        base_revision_id: Option<i64>,
        client_session_id: &str,
        patch: &[u8],
        updated_at_ms: i64,
    ) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT INTO draft_session (
                unit_id, base_revision_id, client_session_id, patch, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(unit_id, client_session_id) DO UPDATE SET
                base_revision_id = excluded.base_revision_id,
                patch = excluded.patch,
                updated_at_ms = excluded.updated_at_ms",
            params![
                unit_id,
                base_revision_id,
                client_session_id,
                patch,
                updated_at_ms
            ],
        )?;
        Ok(())
    }

    pub fn checkpoint_passive(&self) -> rusqlite::Result<CheckpointResult> {
        checkpoint(&self.connection, "PASSIVE")
    }

    pub fn checkpoint_truncate(&self) -> rusqlite::Result<CheckpointResult> {
        checkpoint(&self.connection, "TRUNCATE")
    }

    pub fn begin_generation(&self, descriptor: &GenerationDescriptor) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT INTO import_generation(
                generation_id, source_snapshot_hash, adapter_id, adapter_build,
                identity_version, state, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'Building', ?6)",
            params![
                descriptor.generation_id.as_slice(),
                descriptor.source_snapshot_hash.as_slice(),
                descriptor.adapter_id,
                descriptor.adapter_build,
                descriptor.identity_version,
                descriptor.created_at_ms
            ],
        )?;
        Ok(())
    }

    pub fn append_generation_batch(
        &mut self,
        generation_id: &[u8; 16],
        batch_id: &[u8; 32],
        payload_hash: &[u8; 32],
        batch: &GenerationBatch,
    ) -> rusqlite::Result<GenerationBatchReceipt> {
        // Building generations are replayable from the immutable source object. WAL NORMAL
        // preserves transaction consistency without forcing every resumable batch to disk;
        // restoring FULL before yielding keeps translations and the final activation durable.
        self.connection
            .pragma_update(None, "synchronous", "NORMAL")?;
        let result: rusqlite::Result<GenerationBatchReceipt> = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let state: String = transaction.query_row(
                "SELECT state FROM import_generation WHERE generation_id = ?1",
                [generation_id.as_slice()],
                |row| row.get(0),
            )?;
            if state != "Building" {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let existing = transaction
                .query_row(
                    "SELECT payload_hash, item_count FROM generation_batch_receipt
                 WHERE generation_id = ?1 AND batch_id = ?2",
                    params![generation_id.as_slice(), batch_id.as_slice()],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;
            if let Some((existing_hash, item_count)) = existing {
                if existing_hash.as_slice() != payload_hash {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                transaction.commit()?;
                return Ok(GenerationBatchReceipt {
                    item_count: item_count as usize,
                    replayed: true,
                });
            }

            let mut prepared_bindings = HashMap::with_capacity(batch.bindings.len());
            let empty_candidates_hash = candidate_set_hash(b"[]");
            for binding in &batch.bindings {
                if binding.selected_unit_id.is_some() && binding.disposition != "Exact" {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                if binding.disposition == "Orphaned" {
                    if binding.selected_unit_id.is_some()
                        || binding.candidates_json != b"[]"
                        || binding.candidates_hash != empty_candidates_hash
                        || prepared_bindings
                            .insert(binding.extracted_unit_id, ("Orphaned", None))
                            .is_some()
                    {
                        return Err(rusqlite::Error::InvalidQuery);
                    }
                    continue;
                }
                let candidates = parse_canonical_candidates(&binding.candidates_json)?;
                let unique_candidates = candidates.iter().copied().collect::<BTreeSet<_>>();
                let valid_shape = unique_candidates.len() == candidates.len()
                    && match binding.disposition.as_str() {
                        "Exact" => candidates.len() == 1,
                        "Shifted" => !candidates.is_empty(),
                        "Ambiguous" => candidates.len() >= 2,
                        "Orphaned" => false,
                        _ => false,
                    };
                if !valid_shape
                    || candidate_set_hash(&binding.candidates_json) != binding.candidates_hash
                    || binding
                        .selected_unit_id
                        .is_some_and(|selected| !unique_candidates.contains(&selected))
                    || prepared_bindings
                        .insert(
                            binding.extracted_unit_id,
                            (binding.disposition.as_str(), binding.selected_unit_id),
                        )
                        .is_some()
                {
                    return Err(rusqlite::Error::InvalidQuery);
                }
            }

            {
                let mut insert_resource = transaction.prepare_cached(
                    "INSERT INTO generation_resource(
                     generation_id, resource_id, resource_key, kind, semantic_path, locator_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )?;
                for resource in &batch.resources {
                    insert_resource.execute(params![
                        generation_id.as_slice(),
                        resource.resource_id.as_slice(),
                        resource.resource_key.as_slice(),
                        resource.kind,
                        resource.semantic_path,
                        resource.locator_json
                    ])?;
                }
            }
            {
                let mut insert_edge = transaction.prepare_cached(
                    "INSERT INTO generation_edge(
                     generation_id, from_resource_id, to_resource_id, edge_kind, ordinal
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                )?;
                for edge in &batch.edges {
                    insert_edge.execute(params![
                        generation_id.as_slice(),
                        edge.from_resource_id.as_slice(),
                        edge.to_resource_id.as_slice(),
                        edge.edge_kind,
                        edge.ordinal
                    ])?;
                }
            }
            {
                let mut insert_unit = transaction.prepare_cached(
                    "INSERT INTO generation_unit(
                     generation_id, extracted_unit_id, source_unit_key, resource_id,
                     locator_json, tir_json, reading_order, unit_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?;
                for unit in &batch.units {
                    let reading_order = i64::try_from(unit.reading_order).map_err(|error| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                    })?;
                    let resolved_unit_id = match prepared_bindings.get(&unit.extracted_unit_id) {
                        Some(("Exact", Some(selected))) => Some(*selected),
                        Some(("Orphaned", None)) => Some(stable_orphan_unit_id(
                            generation_id,
                            &unit.extracted_unit_id,
                        )),
                        _ => None,
                    };
                    insert_unit.execute(params![
                        generation_id.as_slice(),
                        unit.extracted_unit_id.as_slice(),
                        unit.source_unit_key.as_slice(),
                        unit.resource_id.as_slice(),
                        unit.locator_json,
                        unit.tir_json,
                        reading_order,
                        resolved_unit_id.as_ref().map(|id| id.as_slice())
                    ])?;
                }
            }
            {
                let mut insert_binding = transaction.prepare_cached(
                    "INSERT INTO generation_binding(
                     binding_id, generation_id, extracted_unit_id, disposition,
                     selected_unit_id, policy_version, candidates_json, candidates_hash
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?;
                for binding in &batch.bindings {
                    insert_binding.execute(params![
                        binding.binding_id.as_slice(),
                        generation_id.as_slice(),
                        binding.extracted_unit_id.as_slice(),
                        binding.disposition,
                        binding.selected_unit_id.as_ref().map(|id| id.as_slice()),
                        binding.policy_version,
                        binding.candidates_json,
                        binding.candidates_hash.as_slice()
                    ])?;
                }
            }
            let item_count = batch.item_count();
            let stored_item_count = i64::try_from(item_count)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
            transaction.execute(
                "INSERT INTO generation_batch_receipt(
                generation_id, batch_id, payload_hash, item_count
             ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    generation_id.as_slice(),
                    batch_id.as_slice(),
                    payload_hash.as_slice(),
                    stored_item_count
                ],
            )?;
            transaction.commit()?;
            Ok(GenerationBatchReceipt {
                item_count,
                replayed: false,
            })
        })();
        let restore_result = self.connection.pragma_update(None, "synchronous", "FULL");
        restore_result?;
        result
    }

    pub fn seal_generation(&mut self, generation_id: &[u8; 16]) -> rusqlite::Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state: String = transaction.query_row(
            "SELECT state FROM import_generation WHERE generation_id = ?1",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        if state != "Building" {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let resources: i64 = transaction.query_row(
            "SELECT count(*) FROM generation_resource WHERE generation_id = ?1",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        let unbound_units: i64 = transaction.query_row(
            "SELECT count(*)
             FROM generation_unit unit
             WHERE unit.generation_id = ?1
               AND NOT EXISTS (
                   SELECT 1 FROM generation_binding binding
                   WHERE binding.generation_id = unit.generation_id
                     AND binding.extracted_unit_id = unit.extracted_unit_id
               )",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        let unresolved_units: i64 = transaction.query_row(
            "SELECT count(*)
             FROM generation_unit unit
             JOIN generation_binding binding
               ON binding.generation_id = unit.generation_id
              AND binding.extracted_unit_id = unit.extracted_unit_id
             WHERE unit.generation_id = ?1
               AND binding.disposition IN ('Exact', 'Shifted', 'Ambiguous')
               AND binding.selected_unit_id IS NULL",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        if resources == 0 || unbound_units != 0 || unresolved_units != 0 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let missing_selected_units: i64 = transaction.query_row(
            "SELECT count(*)
             FROM generation_binding binding
             WHERE binding.generation_id = ?1
               AND binding.selected_unit_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM unit WHERE unit_id = binding.selected_unit_id
               )",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        if missing_selected_units != 0 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let mut assignments = {
            let mut statement = transaction.prepare(
                "SELECT unit.extracted_unit_id, binding.disposition, binding.selected_unit_id
                 FROM generation_unit unit
                 JOIN generation_binding binding
                   ON binding.generation_id = unit.generation_id
                  AND binding.extracted_unit_id = unit.extracted_unit_id
                 WHERE unit.generation_id = ?1
                   AND unit.unit_id IS NULL
                 ORDER BY unit.reading_order",
            )?;
            statement
                .query_map([generation_id.as_slice()], |row| {
                    let extracted_unit_id: [u8; 16] = vec_to_array(row.get(0)?)?;
                    let disposition: String = row.get(1)?;
                    let selected: Option<Vec<u8>> = row.get(2)?;
                    let unit_id = match selected {
                        Some(bytes) => vec_to_array(bytes)?,
                        None if disposition == "Orphaned" => {
                            stable_orphan_unit_id(generation_id, &extracted_unit_id)
                        }
                        None => return Err(rusqlite::Error::InvalidQuery),
                    };
                    Ok((extracted_unit_id, unit_id))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let unique_unit_ids = assignments
            .iter()
            .map(|assignment| assignment.1)
            .collect::<BTreeSet<_>>();
        if unique_unit_ids.len() != assignments.len() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        assignments.sort_by_key(|assignment| assignment.0);
        {
            let mut update = transaction.prepare_cached(
                "UPDATE generation_unit SET unit_id = ?3
                 WHERE generation_id = ?1 AND extracted_unit_id = ?2",
            )?;
            for (extracted_unit_id, unit_id) in &assignments {
                update.execute(params![
                    generation_id.as_slice(),
                    extracted_unit_id.as_slice(),
                    unit_id.as_slice()
                ])?;
            }
        }
        let duplicate_unit_ids: i64 = transaction.query_row(
            "SELECT count(*) FROM (
                SELECT unit_id FROM generation_unit
                WHERE generation_id = ?1
                GROUP BY unit_id HAVING count(*) > 1
             )",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        if duplicate_unit_ids != 0 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let changed = transaction.execute(
            "UPDATE import_generation SET state = 'Validated'
             WHERE generation_id = ?1 AND state = 'Building'",
            [generation_id.as_slice()],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        transaction.commit()
    }

    pub fn activate_generation(
        &mut self,
        generation_id: &[u8; 16],
        activated_at_ms: i64,
    ) -> rusqlite::Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state: String = transaction.query_row(
            "SELECT state FROM import_generation WHERE generation_id = ?1",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        let resources: i64 = transaction.query_row(
            "SELECT count(*) FROM generation_resource WHERE generation_id = ?1",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        let unresolved: i64 = transaction.query_row(
            "SELECT count(*) FROM generation_unit
             WHERE generation_id = ?1 AND unit_id IS NULL",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        if state != "Validated" || resources == 0 || unresolved != 0 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        transaction.execute(
            "UPDATE unit
             SET local_index = -1000000000000 - rowid
             WHERE local_index >= 0",
            [],
        )?;
        transaction.execute(
            "INSERT INTO unit(unit_id, source_unit_key, local_index, source_text)
             SELECT unit_id, source_unit_key, reading_order, babel_tir_source_text(tir_json)
             FROM generation_unit
             WHERE generation_id = ?1
             ON CONFLICT(unit_id) DO UPDATE SET
                source_unit_key = excluded.source_unit_key,
                local_index = excluded.local_index,
                source_text = excluded.source_text",
            [generation_id.as_slice()],
        )?;
        transaction.execute("DELETE FROM unit_search", [])?;
        transaction.execute(
            "INSERT INTO unit_search(unit_id, source, translation)
             SELECT hex(u.unit_id), u.source_text, COALESCE(r.text, '')
             FROM unit u
             LEFT JOIN unit_head h ON h.unit_id = u.unit_id
             LEFT JOIN translation_revision r ON r.revision_id = h.revision_id
             WHERE u.local_index >= 0",
            [],
        )?;
        transaction.execute("DELETE FROM search_dirty", [])?;
        transaction.execute(
            "UPDATE import_generation SET state = 'Retired' WHERE state = 'Active'",
            [],
        )?;
        let changed = transaction.execute(
            "UPDATE import_generation SET state = 'Active', activated_at_ms = ?2
             WHERE generation_id = ?1 AND state = 'Validated'",
            params![generation_id.as_slice(), activated_at_ms],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        transaction.commit()
    }

    pub fn source_snapshot_descriptor(
        &self,
        generation_id: &[u8; 16],
    ) -> rusqlite::Result<SourceSnapshotDescriptor> {
        self.connection.query_row(
            "SELECT generation_id, source_snapshot_hash, adapter_id, adapter_build,
                    identity_version
             FROM import_generation WHERE generation_id = ?1",
            [generation_id.as_slice()],
            |row| {
                Ok(SourceSnapshotDescriptor {
                    generation_id: vec_to_array(row.get(0)?)?,
                    source_snapshot_hash: vec_to_array(row.get(1)?)?,
                    adapter_id: row.get(2)?,
                    adapter_build: row.get(3)?,
                    identity_version: row.get::<_, i64>(4)? as u32,
                })
            },
        )
    }

    pub fn generation_units(
        &self,
        generation_id: &[u8; 16],
    ) -> rusqlite::Result<Vec<GenerationUnitView>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT extracted_unit_id, unit_id, source_unit_key, resource_id,
                    locator_json, tir_json, reading_order
             FROM generation_unit
             WHERE generation_id = ?1
             ORDER BY reading_order",
        )?;
        statement
            .query_map([generation_id.as_slice()], |row| {
                Ok(GenerationUnitView {
                    extracted_unit_id: vec_to_array(row.get(0)?)?,
                    unit_id: vec_to_array(row.get(1)?)?,
                    source_unit_key: vec_to_array(row.get(2)?)?,
                    resource_id: vec_to_array(row.get(3)?)?,
                    locator_json: row.get(4)?,
                    tir_json: row.get(5)?,
                    reading_order: row.get::<_, i64>(6)? as u64,
                })
            })?
            .collect()
    }

    pub fn frozen_unit_snapshot(
        &self,
        generation_id: &[u8; 16],
        frozen_commit_sequence: i64,
    ) -> rusqlite::Result<Vec<FrozenUnitSnapshot>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT gu.extracted_unit_id, gu.unit_id, gu.source_unit_key, gu.resource_id,
                    gu.locator_json, gu.tir_json, gu.reading_order, revision.text
             FROM generation_unit gu
             LEFT JOIN translation_revision revision
               ON revision.revision_id = (
                    SELECT tr.revision_id
                    FROM translation_revision tr
                    WHERE tr.unit_id = gu.unit_id
                      AND tr.commit_sequence <= ?2
                    ORDER BY tr.commit_sequence DESC
                    LIMIT 1
               )
             WHERE gu.generation_id = ?1
             ORDER BY gu.reading_order",
        )?;
        statement
            .query_map(
                params![generation_id.as_slice(), frozen_commit_sequence],
                |row| {
                    let tir_json: Vec<u8> = row.get(5)?;
                    Ok(FrozenUnitSnapshot {
                        extracted_unit_id: vec_to_array(row.get(0)?)?,
                        unit_id: vec_to_array(row.get(1)?)?,
                        source_unit_key: vec_to_array(row.get(2)?)?,
                        resource_id: vec_to_array(row.get(3)?)?,
                        locator_json: row.get(4)?,
                        source_text: tir_source_text(&tir_json)?,
                        tir_json,
                        reading_order: row.get::<_, i64>(6)? as u64,
                        translation: row.get(7)?,
                    })
                },
            )?
            .collect()
    }

    pub fn generation_state(&self, generation_id: &[u8; 16]) -> rusqlite::Result<String> {
        self.connection.query_row(
            "SELECT state FROM import_generation WHERE generation_id = ?1",
            [generation_id.as_slice()],
            |row| row.get(0),
        )
    }

    pub fn unresolved_binding_count(&self, generation_id: &[u8; 16]) -> rusqlite::Result<usize> {
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM generation_binding
             WHERE generation_id = ?1
               AND disposition IN ('Shifted', 'Ambiguous')
               AND selected_unit_id IS NULL",
            [generation_id.as_slice()],
            |row| row.get(0),
        )?;
        usize::try_from(count).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })
    }

    pub fn unresolved_bindings(
        &self,
        generation_id: &[u8; 16],
    ) -> rusqlite::Result<Vec<GenerationBindingView>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT binding_id, extracted_unit_id, disposition, selected_unit_id,
                    candidates_json, candidates_hash
             FROM generation_binding
             WHERE generation_id = ?1
               AND disposition IN ('Shifted', 'Ambiguous')
               AND selected_unit_id IS NULL
             ORDER BY binding_id",
        )?;
        statement
            .query_map([generation_id.as_slice()], |row| {
                let candidates_json: Vec<u8> = row.get(4)?;
                Ok(GenerationBindingView {
                    binding_id: vec_to_array(row.get(0)?)?,
                    extracted_unit_id: vec_to_array(row.get(1)?)?,
                    disposition: row.get(2)?,
                    selected_unit_id: row
                        .get::<_, Option<Vec<u8>>>(3)?
                        .map(vec_to_array)
                        .transpose()?,
                    candidates: parse_canonical_candidates(&candidates_json)?,
                    candidates_hash: vec_to_array(row.get(5)?)?,
                })
            })?
            .collect()
    }

    pub fn active_generation(&self) -> rusqlite::Result<Option<[u8; 16]>> {
        self.connection
            .query_row(
                "SELECT generation_id FROM import_generation WHERE state = 'Active'",
                [],
                |row| vec_to_array(row.get(0)?),
            )
            .optional()
    }

    pub fn decide_binding(
        &mut self,
        command_id: &[u8; 32],
        binding_id: &[u8; 16],
        selected_unit_id: &[u8; 16],
        expected_candidate_hash: &[u8; 32],
        reason_code: &str,
        created_at_ms: i64,
    ) -> rusqlite::Result<bool> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT binding_id, selected_unit_id, candidate_set_hash, reason_code
                 FROM binding_decision WHERE command_id = ?1",
                [command_id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some(existing) = existing {
            let matches = existing.0.as_slice() == binding_id
                && existing.1.as_slice() == selected_unit_id
                && existing.2.as_slice() == expected_candidate_hash
                && existing.3 == reason_code;
            return if matches {
                transaction.commit()?;
                Ok(true)
            } else {
                Err(rusqlite::Error::InvalidQuery)
            };
        }
        let (candidates_json, stored_hash): (Vec<u8>, Vec<u8>) = transaction.query_row(
            "SELECT candidates_json, candidates_hash
             FROM generation_binding WHERE binding_id = ?1",
            [binding_id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let candidates = parse_canonical_candidates(&candidates_json)?;
        if stored_hash.as_slice() != expected_candidate_hash
            || candidate_set_hash(&candidates_json) != *expected_candidate_hash
            || !candidates.contains(selected_unit_id)
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let changed = transaction.execute(
            "UPDATE generation_binding SET selected_unit_id = ?2
             WHERE binding_id = ?1 AND selected_unit_id IS NULL",
            params![binding_id.as_slice(), selected_unit_id.as_slice()],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        transaction.execute(
            "INSERT INTO binding_decision(
                command_id, binding_id, selected_unit_id, candidate_set_hash,
                reason_code, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                command_id.as_slice(),
                binding_id.as_slice(),
                selected_unit_id.as_slice(),
                expected_candidate_hash.as_slice(),
                reason_code,
                created_at_ms
            ],
        )?;
        transaction.commit()?;
        Ok(false)
    }

    pub fn reject_binding_as_new(
        &mut self,
        command_id: &[u8; 32],
        binding_id: &[u8; 16],
        expected_candidate_hash: &[u8; 32],
        reason_code: &str,
        created_at_ms: i64,
    ) -> rusqlite::Result<bool> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT binding_id, selected_unit_id, candidate_set_hash, reason_code
                 FROM binding_decision WHERE command_id = ?1",
                [command_id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some(existing) = existing {
            let (generation_id, extracted_unit_id): (Vec<u8>, Vec<u8>) = transaction.query_row(
                "SELECT generation_id, extracted_unit_id
                 FROM generation_binding WHERE binding_id = ?1",
                [binding_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let generation_id: [u8; 16] = vec_to_array(generation_id)?;
            let extracted_unit_id: [u8; 16] = vec_to_array(extracted_unit_id)?;
            let expected_new_unit_id = stable_orphan_unit_id(&generation_id, &extracted_unit_id);
            let matches = existing.0.as_slice() == binding_id
                && existing.1.as_slice() == expected_new_unit_id
                && existing.2.as_slice() == expected_candidate_hash
                && existing.3 == reason_code;
            return if matches {
                transaction.commit()?;
                Ok(true)
            } else {
                Err(rusqlite::Error::InvalidQuery)
            };
        }
        let (generation_id, extracted_unit_id, candidates_json, stored_hash): (
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
        ) = transaction.query_row(
            "SELECT generation_id, extracted_unit_id, candidates_json, candidates_hash
             FROM generation_binding
             WHERE binding_id = ?1
               AND disposition IN ('Shifted', 'Ambiguous')
               AND selected_unit_id IS NULL",
            [binding_id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if stored_hash.as_slice() != expected_candidate_hash
            || candidate_set_hash(&candidates_json) != *expected_candidate_hash
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let generation_id: [u8; 16] = vec_to_array(generation_id)?;
        let extracted_unit_id: [u8; 16] = vec_to_array(extracted_unit_id)?;
        let new_unit_id = stable_orphan_unit_id(&generation_id, &extracted_unit_id);
        let empty_candidates = serde_json::to_vec(&Vec::<[u8; 16]>::new())
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let empty_hash = candidate_set_hash(&empty_candidates);
        let changed = transaction.execute(
            "UPDATE generation_binding
             SET disposition = 'Orphaned', candidates_json = ?2, candidates_hash = ?3
             WHERE binding_id = ?1
               AND disposition IN ('Shifted', 'Ambiguous')
               AND selected_unit_id IS NULL",
            params![
                binding_id.as_slice(),
                empty_candidates,
                empty_hash.as_slice()
            ],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        transaction.execute(
            "INSERT INTO binding_decision(
                command_id, binding_id, selected_unit_id, candidate_set_hash,
                reason_code, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                command_id.as_slice(),
                binding_id.as_slice(),
                new_unit_id.as_slice(),
                expected_candidate_hash.as_slice(),
                reason_code,
                created_at_ms
            ],
        )?;
        transaction.commit()?;
        Ok(false)
    }
}

pub fn configure_connection(connection: &Connection) -> rusqlite::Result<()> {
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "busy_timeout", 5_000)?;
    connection.pragma_update(None, "wal_autocheckpoint", 0)?;
    connection.pragma_update(None, "cache_size", -WRITER_CACHE_KIB)?;
    connection.create_scalar_function(
        "babel_tir_source_text",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let bytes = context.get::<Vec<u8>>(0)?;
            tir_source_text(&bytes)
        },
    )?;
    Ok(())
}

fn initialize_project_identity(connection: &Connection) -> rusqlite::Result<()> {
    let project_id = Uuid::new_v4();
    connection.execute(
        "INSERT OR IGNORE INTO project_state(singleton, project_id, commit_sequence)
         VALUES (1, ?1, 0)",
        [project_id.as_bytes().as_slice()],
    )?;
    Ok(())
}

fn priority_number(priority: WorkPriority) -> i64 {
    match priority {
        WorkPriority::P0Interactive => 0,
        WorkPriority::P1Visible => 1,
        WorkPriority::P2Focused => 2,
        WorkPriority::P3Background => 3,
    }
}

fn parse_priority(value: i64) -> rusqlite::Result<WorkPriority> {
    match value {
        0 => Ok(WorkPriority::P0Interactive),
        1 => Ok(WorkPriority::P1Visible),
        2 => Ok(WorkPriority::P2Focused),
        3 => Ok(WorkPriority::P3Background),
        _ => Err(rusqlite::Error::IntegralValueOutOfRange(0, value)),
    }
}

fn parse_task_state(value: &str) -> rusqlite::Result<TaskState> {
    match value {
        "Pending" => Ok(TaskState::Pending),
        "Running" => Ok(TaskState::Running),
        "Paused" => Ok(TaskState::Paused),
        "Completed" => Ok(TaskState::Completed),
        "Failed" => Ok(TaskState::Failed),
        "Cancelled" => Ok(TaskState::Cancelled),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn vec_to_array<const N: usize>(bytes: Vec<u8>) -> rusqlite::Result<[u8; N]> {
    let length = bytes.len();
    bytes
        .try_into()
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, length as i64))
}

fn parse_canonical_candidates(bytes: &[u8]) -> rusqlite::Result<Vec<[u8; 16]>> {
    let candidates: Vec<[u8; 16]> = serde_json::from_slice(bytes)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let canonical = serde_json::to_vec(&candidates)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    if canonical != bytes {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(candidates)
}

fn stable_orphan_unit_id(generation_id: &[u8; 16], extracted_unit_id: &[u8; 16]) -> [u8; 16] {
    let hash = hash_parts(&[
        b"babel-unit-orphan-v1",
        generation_id.as_slice(),
        extracted_unit_id.as_slice(),
    ]);
    hash[..16].try_into().expect("hash prefix is 16 bytes")
}

fn canonical_source_text(text: &str) -> String {
    text.trim().to_lowercase()
}

fn batch_member_command_id(batch_id: &[u8; 32], unit_id: &[u8], index: u64) -> [u8; 32] {
    hash_parts(&[
        b"translation-batch-member-v1",
        batch_id.as_slice(),
        unit_id,
        &index.to_be_bytes(),
    ])
}

fn tir_source_text(bytes: &[u8]) -> rusqlite::Result<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let tokens = value
        .get("tokens")
        .and_then(|tokens| tokens.as_array())
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let mut text = String::new();
    for token in tokens {
        if let Some(part) = token
            .get("Text")
            .and_then(|text_token| text_token.get("text"))
            .and_then(|text| text.as_str())
            .or_else(|| token.get("text").and_then(|text| text.as_str()))
        {
            text.push_str(part);
        }
    }
    Ok(text)
}

fn checkpoint(connection: &Connection, mode: &str) -> rusqlite::Result<CheckpointResult> {
    connection.query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |row| {
        Ok(CheckpointResult {
            busy: row.get(0)?,
            log_pages: row.get(1)?,
            checkpointed_pages: row.get(2)?,
        })
    })
}

fn hash_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

impl ProjectStore {
    fn term_variants(&self, term_id: &[u8; 16]) -> rusqlite::Result<Vec<String>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT variant FROM term_variant
             WHERE term_id = ?1 ORDER BY variant",
        )?;
        statement
            .query_map([term_id.as_slice()], |row| row.get::<_, String>(0))?
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn save_is_idempotent_and_search_is_deferred() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        store.seed_units(10).unwrap();
        store.rebuild_search().unwrap();
        let command = [3; 32];
        let source_key: [u8; 32] = store.page_after(3, 1).unwrap()[0]
            .source_unit_key
            .clone()
            .try_into()
            .unwrap();

        let first = store
            .save_translation(&source_key, &command, "人工译文", 1_000)
            .unwrap();
        let replay = store
            .save_translation(&source_key, &command, "不能覆盖", 1_001)
            .unwrap();
        assert_eq!(first.revision_id, replay.revision_id);
        assert!(replay.replayed);
        assert!(store.search("人工译文", 10).unwrap().is_empty());
        assert_eq!(store.flush_search_dirty(100).unwrap(), 1);
        assert_eq!(store.search("人工译文", 10).unwrap().len(), 1);
    }

    #[test]
    fn translation_aids_are_authoritative_and_recoverable() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        store.seed_units(2).unwrap();
        let unit = store.page_after(-1, 1).unwrap().remove(0);
        let unit_id = unit.unit_id.clone();
        let source_key: [u8; 32] = unit.source_unit_key.clone().try_into().unwrap();
        let save = store
            .save_translation(&source_key, &[1; 32], "Babel 译文", 1_000)
            .unwrap();

        store
            .upsert_term(&UpsertTermRequest {
                term_id: [2; 16],
                source_text: "Babel Tower".to_owned(),
                preferred_translation: "巴别塔".to_owned(),
                variants: vec!["Babel".to_owned()],
                notes: "project name".to_owned(),
                state: "Active".to_owned(),
                timestamp_ms: 1_100,
            })
            .unwrap();
        assert_eq!(
            store.find_terms("About Babel", 10).unwrap()[0].variants[0],
            "Babel"
        );

        store
            .add_annotation(
                &[3; 16],
                &unit_id,
                Some(save.revision_id),
                0..5,
                "check",
                1_200,
            )
            .unwrap();
        assert!(!store.annotations_for_unit(&unit_id).unwrap()[0].stale);
        store
            .save_translation(&source_key, &[4; 32], "Babel 新译文", 1_300)
            .unwrap();
        assert!(store.annotations_for_unit(&unit_id).unwrap()[0].stale);

        store
            .set_marker(&[5; 16], &unit_id, "status", "needs-review", 1_400)
            .unwrap();
        store
            .set_marker(&[6; 16], &unit_id, "status", "needs-review", 1_401)
            .unwrap();
        assert_eq!(store.markers_for_unit(&unit_id).unwrap().len(), 1);

        let history = store.translation_history(&unit.source_text, 10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].text, "Babel 新译文");
    }

    #[test]
    fn duplicate_groups_and_batch_replace_are_derived_and_atomic() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        store.seed_units(3).unwrap();
        store
            .connection()
            .execute(
                "UPDATE unit SET source_text = 'Repeat me'
                 WHERE local_index IN (0, 1)",
                [],
            )
            .unwrap();
        assert_eq!(
            store.duplicate_source_groups(2, 10).unwrap()[0]
                .unit_ids
                .len(),
            2
        );

        let units = store.page_after(-1, 3).unwrap();
        for (index, unit) in units.iter().enumerate() {
            let source_key: [u8; 32] = unit.source_unit_key.clone().try_into().unwrap();
            store
                .save_translation(
                    &source_key,
                    &hash_parts(&[b"save", &(index as u64).to_be_bytes()]),
                    "alpha beta alpha",
                    2_000 + index as i64,
                )
                .unwrap();
        }
        let preview = store
            .preview_replace_translations("alpha", "A", 10)
            .unwrap();
        assert_eq!(preview.len(), 3);
        assert!(preview.iter().all(|item| item.occurrences == 2));
        let receipt = store
            .apply_replace_translations(&[9; 32], "alpha", "A", &preview, 3_000)
            .unwrap();
        assert_eq!(receipt.affected_units, 3);
        assert!(!receipt.replayed);
        let replay = store
            .apply_replace_translations(&[9; 32], "alpha", "A", &preview, 3_001)
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.affected_units, 3);
        assert!(
            store.page_after(-1, 1).unwrap()[0]
                .translation
                .as_deref()
                .is_some_and(|text| text == "A beta A")
        );

        let stale_preview = preview;
        assert!(
            store
                .apply_replace_translations(&[10; 32], "A", "bad", &stale_preview, 3_100)
                .is_err()
        );
    }

    #[test]
    fn keyset_page_does_not_depend_on_offset_scanning() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        store.seed_units(200).unwrap();
        let page = store.page_after(149, 20).unwrap();
        assert_eq!(page.first().unwrap().local_index, 150);
        assert_eq!(page.last().unwrap().local_index, 169);
    }

    #[test]
    fn stable_source_key_survives_local_order_changes() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        store.seed_units(2).unwrap();
        let units = store.page_after(-1, 2).unwrap();
        let first_key: [u8; 32] = units[0].source_unit_key.clone().try_into().unwrap();

        store
            .connection()
            .execute("UPDATE unit SET local_index = 100 - local_index", [])
            .unwrap();
        store
            .save_translation(&first_key, &[7; 32], "仍属于第一单元", 1_000)
            .unwrap();

        let reordered = store.page_after(-1, 2).unwrap();
        let translated = reordered
            .iter()
            .find(|unit| unit.source_unit_key == first_key)
            .unwrap();
        assert_eq!(translated.translation.as_deref(), Some("仍属于第一单元"));
    }

    #[test]
    fn draft_does_not_advance_durable_commit_sequence() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        store.seed_units(1).unwrap();
        let unit_id = store.page_after(-1, 1).unwrap()[0].unit_id.clone();
        store
            .save_draft(&unit_id, None, "window-1", b"unconfirmed", 1_000)
            .unwrap();
        assert_eq!(store.commit_sequence().unwrap(), 0);
    }

    #[test]
    fn save_fault_boundaries_are_atomic_and_replay_recovers_lost_confirmation() {
        for point in [SavePoint::RevisionWritten, SavePoint::BeforeCommit] {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("project.sqlite3");
            let mut store = ProjectStore::open(&path).unwrap();
            store.seed_units(1).unwrap();
            let source_key = store.page_after(-1, 1).unwrap()[0]
                .source_unit_key
                .clone()
                .try_into()
                .unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                store
                    .save_translation_with_hook(
                        &source_key,
                        &[6; 32],
                        "not committed",
                        1,
                        |actual| {
                            if actual == point {
                                panic!("injected crash");
                            }
                        },
                    )
                    .unwrap();
            }));
            assert!(result.is_err());
            drop(store);
            let reopened = ProjectStore::open(&path).unwrap();
            assert_eq!(reopened.commit_sequence().unwrap(), 0);
            assert!(reopened.page_after(-1, 1).unwrap()[0].translation.is_none());
        }

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("project.sqlite3");
        let mut store = ProjectStore::open(&path).unwrap();
        store.seed_units(1).unwrap();
        let source_key = store.page_after(-1, 1).unwrap()[0]
            .source_unit_key
            .clone()
            .try_into()
            .unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store
                .save_translation_with_hook(
                    &source_key,
                    &[7; 32],
                    "durable without confirmation",
                    1,
                    |point| {
                        if point == SavePoint::AfterCommit {
                            panic!("confirmation was lost");
                        }
                    },
                )
                .unwrap();
        }));
        assert!(result.is_err());
        drop(store);

        let mut reopened = ProjectStore::open(&path).unwrap();
        let receipt = reopened
            .save_translation(&source_key, &[7; 32], "must not overwrite", 2)
            .unwrap();
        assert!(receipt.replayed);
        assert_eq!(receipt.commit_sequence, 1);
        assert_eq!(
            reopened.page_after(-1, 1).unwrap()[0]
                .translation
                .as_deref(),
            Some("durable without confirmation")
        );
    }

    #[test]
    fn object_bytes_are_immutable_while_reference_media_types_stay_independent() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        let hash = [4; 32];
        store
            .register_object_reference(
                "source",
                &[1; 16],
                &ObjectRecord {
                    hash,
                    byte_length: 12,
                    media_type: "text/plain".to_owned(),
                },
                1,
            )
            .unwrap();
        store
            .register_object_reference(
                "attachment",
                &[2; 16],
                &ObjectRecord {
                    hash,
                    byte_length: 12,
                    media_type: "application/octet-stream".to_owned(),
                },
                2,
            )
            .unwrap();

        let object_media_type: String = store
            .connection()
            .query_row(
                "SELECT media_type FROM object_record WHERE object_hash = ?1",
                [hash.as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(object_media_type, "text/plain");
        let reference_types: Vec<String> = store
            .connection()
            .prepare(
                "SELECT media_type FROM object_reference
                 WHERE object_hash = ?1 ORDER BY owner_kind DESC",
            )
            .unwrap()
            .query_map([hash.as_slice()], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            reference_types,
            vec!["text/plain", "application/octet-stream"]
        );
        assert!(
            store
                .register_object_reference(
                    "source",
                    &[3; 16],
                    &ObjectRecord {
                        hash,
                        byte_length: 13,
                        media_type: "text/plain".to_owned(),
                    },
                    3,
                )
                .is_err()
        );
    }

    #[test]
    fn opening_an_older_project_creates_a_verified_pre_migration_snapshot() {
        let temp = TempDir::new().unwrap();
        let database = temp.path().join("project.sqlite3");
        let mut legacy = Connection::open(&database).unwrap();
        schema::migrate_to(&mut legacy, 1).unwrap();
        drop(legacy);

        let store = ProjectStore::open(&database).unwrap();
        assert_eq!(
            schema::schema_version(store.connection()).unwrap(),
            schema::CURRENT_SCHEMA_VERSION
        );
        let backups: Vec<_> = fs::read_dir(temp.path().join("migration-backups"))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(backups.len(), 1);
        let backup_database = backups[0].path().join("project.sqlite3");
        let backup = Connection::open(backup_database).unwrap();
        assert_eq!(schema::schema_version(&backup).unwrap(), 1);
        assert!(backups[0].path().join("migration-backup.txt").exists());
    }

    #[test]
    fn migration_does_not_start_when_its_object_backup_is_incomplete() {
        let temp = TempDir::new().unwrap();
        let database = temp.path().join("project.sqlite3");
        let mut legacy = Connection::open(&database).unwrap();
        legacy.pragma_update(None, "foreign_keys", "ON").unwrap();
        schema::migrate_to(&mut legacy, 2).unwrap();
        legacy
            .execute(
                "INSERT INTO object_record(
                    object_hash, byte_length, media_type, created_at_ms
                 ) VALUES (?1, 7, 'text/plain', 1)",
                [[7_u8; 32].as_slice()],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO object_reference(owner_kind, owner_id, object_hash)
                 VALUES ('source', ?1, ?2)",
                params![[1_u8; 16].as_slice(), [7_u8; 32].as_slice()],
            )
            .unwrap();
        drop(legacy);

        assert!(ProjectStore::open(&database).is_err());
        let unchanged = Connection::open(database).unwrap();
        assert_eq!(schema::schema_version(&unchanged).unwrap(), 2);
    }

    fn generation_descriptor(id: [u8; 16]) -> GenerationDescriptor {
        GenerationDescriptor {
            generation_id: id,
            source_snapshot_hash: [7; 32],
            adapter_id: "mock".to_owned(),
            adapter_build: "1".to_owned(),
            identity_version: 1,
            created_at_ms: 1,
        }
    }

    fn insert_test_unit(store: &ProjectStore, unit_id: [u8; 16], source_key: [u8; 32], index: i64) {
        store
            .connection()
            .execute(
                "INSERT INTO unit(unit_id, source_unit_key, local_index, source_text)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    unit_id.as_slice(),
                    source_key.as_slice(),
                    index,
                    format!("previous source {index}")
                ],
            )
            .unwrap();
    }

    fn generation_batch(binding_id: [u8; 16]) -> GenerationBatch {
        ambiguous_generation_batch(binding_id)
    }

    fn orphan_generation_batch(binding_id: [u8; 16]) -> GenerationBatch {
        let mut batch = generation_batch(binding_id);
        batch.bindings[0].disposition = "Orphaned".to_owned();
        batch.bindings[0].candidates_json = serde_json::to_vec(&Vec::<[u8; 16]>::new()).unwrap();
        batch.bindings[0].candidates_hash = candidate_set_hash(&batch.bindings[0].candidates_json);
        batch
    }

    fn exact_generation_batch(binding_id: [u8; 16], selected_unit_id: [u8; 16]) -> GenerationBatch {
        let mut batch = generation_batch(binding_id);
        batch.units[0].source_unit_key = [6; 32];
        batch.units[0].tir_json =
            br#"{"schema_version":1,"tokens":[{"Text":{"text":"changed source","style_hint":null}}]}"#
                .to_vec();
        batch.bindings[0].disposition = "Exact".to_owned();
        batch.bindings[0].selected_unit_id = Some(selected_unit_id);
        batch.bindings[0].candidates_json = serde_json::to_vec(&vec![selected_unit_id]).unwrap();
        batch.bindings[0].candidates_hash = candidate_set_hash(&batch.bindings[0].candidates_json);
        batch
    }

    fn ambiguous_generation_batch(binding_id: [u8; 16]) -> GenerationBatch {
        let resource_id = [2; 16];
        let extracted_unit_id = [3; 16];
        let candidates_json = serde_json::to_vec(&vec![[12; 16], [14; 16]]).unwrap();
        GenerationBatch {
            resources: vec![GenerationResourceRecord {
                resource_id,
                resource_key: [4; 32],
                kind: "TextStream".to_owned(),
                semantic_path: "document/text".to_owned(),
                locator_json: br#"{"kind":"byte-span"}"#.to_vec(),
            }],
            edges: Vec::new(),
            units: vec![GenerationUnitRecord {
                extracted_unit_id,
                source_unit_key: [5; 32],
                resource_id,
                locator_json: br#"{"start":0,"end":4}"#.to_vec(),
                tir_json: br#"{"schema_version":1,"tokens":[{"Text":{"text":"source text","style_hint":null}}]}"#.to_vec(),
                reading_order: 0,
            }],
            bindings: vec![GenerationBindingRecord {
                binding_id,
                extracted_unit_id,
                disposition: "Ambiguous".to_owned(),
                selected_unit_id: None,
                policy_version: 1,
                candidates_hash: candidate_set_hash(&candidates_json),
                candidates_json,
            }],
        }
    }

    #[test]
    fn generation_batches_are_resumable_and_activation_is_atomic() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            PROJECT_PAGE_SIZE_BYTES
        );
        let generation_id = [1; 16];
        store
            .begin_generation(&generation_descriptor(generation_id))
            .unwrap();
        let batch = orphan_generation_batch([8; 16]);
        let first = store
            .append_generation_batch(&generation_id, &[9; 32], &[10; 32], &batch)
            .unwrap();
        let replay = store
            .append_generation_batch(&generation_id, &[9; 32], &[10; 32], &batch)
            .unwrap();
        assert!(!first.replayed);
        assert!(replay.replayed);
        assert_eq!(first.item_count, 3);
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(store.generation_state(&generation_id).unwrap(), "Building");

        assert!(store.activate_generation(&generation_id, 2).is_err());
        store.seal_generation(&generation_id).unwrap();
        store.activate_generation(&generation_id, 2).unwrap();
        assert_eq!(store.active_generation().unwrap(), Some(generation_id));
        let projected = store.page_after(-1, 1).unwrap();
        assert_eq!(projected[0].source_text, "source text");
        assert_eq!(store.generation_units(&generation_id).unwrap().len(), 1);
        assert!(
            store
                .append_generation_batch(&generation_id, &[11; 32], &[12; 32], &batch)
                .is_err()
        );
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );

        let incomplete_generation = [13; 16];
        store
            .begin_generation(&generation_descriptor(incomplete_generation))
            .unwrap();
        let mut incomplete = generation_batch([14; 16]);
        incomplete.bindings.clear();
        store
            .append_generation_batch(&incomplete_generation, &[15; 32], &[16; 32], &incomplete)
            .unwrap();
        assert!(store.seal_generation(&incomplete_generation).is_err());
        assert_eq!(store.active_generation().unwrap(), Some(generation_id));
    }

    #[test]
    fn unresolved_binding_blocks_seal_until_decision_selects_a_candidate() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        let generation_id = [1; 16];
        let binding_id = [8; 16];
        store
            .begin_generation(&generation_descriptor(generation_id))
            .unwrap();
        store
            .append_generation_batch(
                &generation_id,
                &[9; 32],
                &[10; 32],
                &generation_batch(binding_id),
            )
            .unwrap();
        assert!(store.seal_generation(&generation_id).is_err());
        let candidates_json = serde_json::to_vec(&vec![[12; 16], [14; 16]]).unwrap();
        let candidates_hash = candidate_set_hash(&candidates_json);
        insert_test_unit(&store, [12; 16], [12; 32], 0);
        store
            .decide_binding(
                &[11; 32],
                &binding_id,
                &[12; 16],
                &candidates_hash,
                "translator-confirmed",
                2,
            )
            .unwrap();
        store.seal_generation(&generation_id).unwrap();
        assert_eq!(
            store.generation_units(&generation_id).unwrap()[0].unit_id,
            [12; 16]
        );
    }

    #[test]
    fn exact_reimport_keeps_translation_identity_and_frozen_snapshot_is_stable() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        let first_generation = [1; 16];
        store
            .begin_generation(&generation_descriptor(first_generation))
            .unwrap();
        store
            .append_generation_batch(
                &first_generation,
                &[9; 32],
                &[10; 32],
                &orphan_generation_batch([8; 16]),
            )
            .unwrap();
        store.seal_generation(&first_generation).unwrap();
        store.activate_generation(&first_generation, 2).unwrap();
        let first_unit = store.generation_units(&first_generation).unwrap()[0].clone();
        let first_save = store
            .save_translation(&[5; 32], &[20; 32], "first translation", 3)
            .unwrap();

        let second_generation = [2; 16];
        store
            .begin_generation(&generation_descriptor(second_generation))
            .unwrap();
        store
            .append_generation_batch(
                &second_generation,
                &[11; 32],
                &[12; 32],
                &exact_generation_batch([10; 16], first_unit.unit_id),
            )
            .unwrap();
        store.seal_generation(&second_generation).unwrap();
        store.activate_generation(&second_generation, 4).unwrap();
        let projected = store.page_after(-1, 1).unwrap();
        assert_eq!(projected[0].source_unit_key, [6; 32]);
        assert_eq!(projected[0].source_text, "changed source");
        assert_eq!(
            projected[0].translation.as_deref(),
            Some("first translation")
        );

        let frozen = store
            .frozen_unit_snapshot(&second_generation, first_save.commit_sequence)
            .unwrap();
        assert_eq!(frozen[0].translation.as_deref(), Some("first translation"));
        store
            .save_translation(&[6; 32], &[21; 32], "future edit", 5)
            .unwrap();
        let still_frozen = store
            .frozen_unit_snapshot(&second_generation, first_save.commit_sequence)
            .unwrap();
        assert_eq!(
            still_frozen[0].translation.as_deref(),
            Some("first translation")
        );
        let descriptor = store
            .source_snapshot_descriptor(&second_generation)
            .unwrap();
        assert_eq!(descriptor.source_snapshot_hash, [7; 32]);
    }

    #[test]
    fn binding_decision_is_idempotent_but_cannot_change_candidates() {
        let temp = TempDir::new().unwrap();
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        let generation_id = [1; 16];
        let binding_id = [8; 16];
        store
            .begin_generation(&generation_descriptor(generation_id))
            .unwrap();
        store
            .append_generation_batch(
                &generation_id,
                &[9; 32],
                &[10; 32],
                &generation_batch(binding_id),
            )
            .unwrap();
        let candidates_json = serde_json::to_vec(&vec![[12; 16], [14; 16]]).unwrap();
        let candidates_hash = candidate_set_hash(&candidates_json);
        assert!(
            !store
                .decide_binding(
                    &[11; 32],
                    &binding_id,
                    &[12; 16],
                    &candidates_hash,
                    "translator-confirmed",
                    2,
                )
                .unwrap()
        );
        assert!(
            store
                .decide_binding(
                    &[11; 32],
                    &binding_id,
                    &[12; 16],
                    &candidates_hash,
                    "translator-confirmed",
                    2,
                )
                .unwrap()
        );
        assert!(
            store
                .decide_binding(
                    &[13; 32],
                    &binding_id,
                    &[13; 16],
                    &candidates_hash,
                    "outside-candidates",
                    3,
                )
                .is_err()
        );
    }
}
