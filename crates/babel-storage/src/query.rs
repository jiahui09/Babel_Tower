use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use babel_domain::{
    core::{ProjectId, ResourceId, UnitId},
    workbench::{NavigationFilters, NavigationPosition, WorkspaceView},
};

use crate::project::{
    DraftDisposition, DraftRecovery, ImageRegionEditRecord, OcrCandidateCacheRecord, UnitPageItem,
};

const QUERY_CACHE_KIB: i64 = 16 * 1024;

pub struct ProjectQuery {
    connection: Connection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbenchUnitRecord {
    pub generation_id: [u8; 16],
    pub unit_id: [u8; 16],
    pub source_unit_key: [u8; 32],
    pub resource_id: [u8; 16],
    pub locator_json: Vec<u8>,
    pub tir_json: Vec<u8>,
    pub reading_order: u64,
    pub source_text: String,
    pub translation: Option<String>,
    pub translation_document_schema_version: Option<i64>,
    pub translation_document_json: Option<Vec<u8>>,
    pub revision_id: Option<i64>,
    pub revision_commit_sequence: Option<i64>,
    pub resource_kind: String,
    pub semantic_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRecord {
    pub resource_id: [u8; 16],
    pub kind: String,
    pub semantic_path: String,
    pub locator_json: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedResourceRecord {
    pub resource_id: [u8; 16],
    pub kind: String,
    pub semantic_path: String,
    pub locator_json: Vec<u8>,
    pub edge_kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageExportOverlayRecord {
    pub region_resource_id: [u8; 16],
    pub image_resource_id: [u8; 16],
    pub image_locator_json: Vec<u8>,
    pub region_locator_json: Vec<u8>,
    pub derived_object_hash: [u8; 32],
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedNavigationPosition {
    pub position: NavigationPosition,
    pub client_session_id: String,
    pub position_sequence: u64,
    pub updated_at_ms: i64,
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

    pub fn image_region_edit(
        &self,
        unit_id: &[u8; 16],
    ) -> rusqlite::Result<Option<ImageRegionEditRecord>> {
        self.connection
            .query_row(
                "SELECT revision.revision_id, revision.unit_id, revision.generation_id,
                        revision.region_resource_id, revision.corrected_source_text,
                        revision.render_parameters_json, revision.derived_object_hash,
                        revision.commit_sequence
                 FROM image_region_head head
                 JOIN image_region_revision revision ON revision.revision_id = head.revision_id
                 WHERE head.unit_id = ?1",
                [unit_id.as_slice()],
                |row| {
                    let optional_hash = row
                        .get::<_, Option<Vec<u8>>>(6)?
                        .map(vec_to_array)
                        .transpose()?;
                    Ok(ImageRegionEditRecord {
                        revision_id: row.get(0)?,
                        unit_id: vec_to_array(row.get(1)?)?,
                        generation_id: vec_to_array(row.get(2)?)?,
                        region_resource_id: vec_to_array(row.get(3)?)?,
                        corrected_source_text: row.get(4)?,
                        render_parameters_json: row.get(5)?,
                        derived_object_hash: optional_hash,
                        commit_sequence: row.get(7)?,
                    })
                },
            )
            .optional()
    }

    pub fn image_export_overlays(
        &self,
        generation_id: &[u8; 16],
        frozen_commit_sequence: i64,
    ) -> rusqlite::Result<Vec<ImageExportOverlayRecord>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT revision.region_resource_id, image.resource_id,
                    image.locator_json, region.locator_json,
                    revision.derived_object_hash,
                    COALESCE(object.media_type, 'image/png')
             FROM image_region_revision revision
             JOIN generation_resource region
               ON region.generation_id = revision.generation_id
              AND region.resource_id = revision.region_resource_id
              AND region.kind = 'ImageRegion'
             JOIN generation_edge edge
               ON edge.generation_id = revision.generation_id
              AND edge.from_resource_id = revision.region_resource_id
              AND edge.edge_kind = 'RegionOf'
             JOIN generation_resource image
               ON image.generation_id = edge.generation_id
              AND image.resource_id = edge.to_resource_id
              AND image.kind = 'Image'
             LEFT JOIN object_record object
               ON object.object_hash = revision.derived_object_hash
             WHERE revision.generation_id = ?1
               AND revision.commit_sequence <= ?2
               AND revision.revision_id = (
                    SELECT candidate.revision_id
                    FROM image_region_revision candidate
                    WHERE candidate.unit_id = revision.unit_id
                      AND candidate.generation_id = revision.generation_id
                      AND candidate.commit_sequence <= ?2
                    ORDER BY candidate.commit_sequence DESC
                    LIMIT 1
               )
               AND revision.derived_object_hash IS NOT NULL
             ORDER BY revision.region_resource_id",
        )?;
        statement
            .query_map(
                rusqlite::params![generation_id.as_slice(), frozen_commit_sequence],
                |row| {
                    Ok(ImageExportOverlayRecord {
                        region_resource_id: vec_to_array(row.get(0)?)?,
                        image_resource_id: vec_to_array(row.get(1)?)?,
                        image_locator_json: row.get(2)?,
                        region_locator_json: row.get(3)?,
                        derived_object_hash: vec_to_array(row.get(4)?)?,
                        media_type: row.get(5)?,
                    })
                },
            )?
            .collect()
    }

    pub fn ocr_candidate(
        &self,
        generation_id: &[u8; 16],
        region_resource_id: &[u8; 16],
        model_hash: &[u8; 32],
    ) -> rusqlite::Result<Option<OcrCandidateCacheRecord>> {
        self.connection
            .query_row(
                "SELECT generation_id, region_resource_id, model_hash,
                        candidate_json, created_at_ms
                 FROM image_region_ocr_cache
                 WHERE generation_id = ?1 AND region_resource_id = ?2 AND model_hash = ?3",
                params![
                    generation_id.as_slice(),
                    region_resource_id.as_slice(),
                    model_hash.as_slice()
                ],
                |row| {
                    Ok(OcrCandidateCacheRecord {
                        generation_id: vec_to_array(row.get(0)?)?,
                        region_resource_id: vec_to_array(row.get(1)?)?,
                        model_hash: vec_to_array(row.get(2)?)?,
                        candidate_json: row.get(3)?,
                        created_at_ms: row.get(4)?,
                    })
                },
            )
            .optional()
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

    pub fn navigation_position(&self) -> rusqlite::Result<Option<SavedNavigationPosition>> {
        self.connection
            .query_row(
                "SELECT schema_version, project_id, view, unit_id, resource_id, region_id,
                        scroll_anchor_unit_id, scroll_offset_px, zoom_millionths, filters_json,
                        client_session_id, position_sequence, updated_at_ms
                 FROM project_navigation WHERE singleton = 1",
                [],
                |row| {
                    let view: String = row.get(2)?;
                    let view = WorkspaceView::parse(&view)
                        .ok_or_else(|| invalid_data("unknown workspace view"))?;
                    let filters_json: Vec<u8> = row.get(9)?;
                    let filters: NavigationFilters = serde_json::from_slice(&filters_json)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                filters_json.len(),
                                rusqlite::types::Type::Blob,
                                Box::new(error),
                            )
                        })?;
                    let optional_id = |value: Option<Vec<u8>>| value.map(vec_to_array).transpose();
                    let project_id = ProjectId::from_bytes(vec_to_array(row.get(1)?)?);
                    let unit_id = optional_id(row.get(3)?)?.map(UnitId::from_bytes);
                    let resource_id = optional_id(row.get(4)?)?.map(ResourceId::from_bytes);
                    let region_id = optional_id(row.get(5)?)?.map(ResourceId::from_bytes);
                    let scroll_anchor_unit_id = optional_id(row.get(6)?)?.map(UnitId::from_bytes);
                    let position_sequence = u64::try_from(row.get::<_, i64>(11)?)
                        .map_err(|_| invalid_data("negative navigation sequence"))?;
                    Ok(SavedNavigationPosition {
                        position: NavigationPosition {
                            schema_version: row.get::<_, i64>(0)? as u32,
                            project_id,
                            view,
                            unit_id,
                            resource_id,
                            region_id,
                            scroll_anchor_unit_id,
                            scroll_offset_px: row.get(7)?,
                            zoom_millionths: row.get::<_, i64>(8)? as u32,
                            filters,
                        },
                        client_session_id: row.get(10)?,
                        position_sequence,
                        updated_at_ms: row.get(12)?,
                    })
                },
            )
            .optional()
    }

    pub fn workbench_unit(
        &self,
        unit_id: &[u8; 16],
    ) -> rusqlite::Result<Option<WorkbenchUnitRecord>> {
        self.connection
            .query_row(
                "SELECT gu.generation_id, gu.unit_id, gu.source_unit_key, gu.resource_id,
                        gu.locator_json, gu.tir_json, gu.reading_order, u.source_text,
                        revision.text, head.revision_id, revision.commit_sequence,
                        revision.document_schema_version, revision.document_json,
                        resource.kind, resource.semantic_path
                 FROM import_generation generation
                 JOIN generation_unit gu ON gu.generation_id = generation.generation_id
                 JOIN generation_resource resource
                   ON resource.generation_id = gu.generation_id
                  AND resource.resource_id = gu.resource_id
                 JOIN unit u ON u.unit_id = gu.unit_id
                 LEFT JOIN unit_head head ON head.unit_id = gu.unit_id
                 LEFT JOIN translation_revision revision
                   ON revision.revision_id = head.revision_id
                 WHERE generation.state = 'Active' AND gu.unit_id = ?1",
                [unit_id.as_slice()],
                |row| {
                    Ok(WorkbenchUnitRecord {
                        generation_id: vec_to_array(row.get(0)?)?,
                        unit_id: vec_to_array(row.get(1)?)?,
                        source_unit_key: vec_to_array(row.get(2)?)?,
                        resource_id: vec_to_array(row.get(3)?)?,
                        locator_json: row.get(4)?,
                        tir_json: row.get(5)?,
                        reading_order: row.get::<_, i64>(6)? as u64,
                        source_text: row.get(7)?,
                        translation: row.get(8)?,
                        revision_id: row.get(9)?,
                        revision_commit_sequence: row.get(10)?,
                        translation_document_schema_version: row.get(11)?,
                        translation_document_json: row.get(12)?,
                        resource_kind: row.get(13)?,
                        semantic_path: row.get(14)?,
                    })
                },
            )
            .optional()
    }

    pub fn unit_id_for_source_key(
        &self,
        source_unit_key: &[u8; 32],
    ) -> rusqlite::Result<Option<[u8; 16]>> {
        self.connection
            .query_row(
                "SELECT gu.unit_id
                 FROM import_generation generation
                 JOIN generation_unit gu ON gu.generation_id = generation.generation_id
                 WHERE generation.state = 'Active' AND gu.source_unit_key = ?1
                 LIMIT 1",
                [source_unit_key.as_slice()],
                |row| vec_to_array(row.get(0)?),
            )
            .optional()
    }

    pub fn resource_queue_after(
        &self,
        after: Option<(u64, [u8; 16])>,
        limit: usize,
    ) -> rusqlite::Result<Vec<WorkbenchUnitRecord>> {
        let after_reading_order = after
            .map(|(value, _)| i64::try_from(value).unwrap_or(i64::MAX))
            .unwrap_or(-1);
        let after_unit_id = after
            .map(|(_, unit_id)| unit_id.to_vec())
            .unwrap_or_default();
        let mut statement = self.connection.prepare_cached(
            "SELECT gu.generation_id, gu.unit_id, gu.source_unit_key, gu.resource_id,
                    gu.locator_json, gu.tir_json, gu.reading_order, u.source_text,
                    revision.text, head.revision_id, revision.commit_sequence,
                    revision.document_schema_version, revision.document_json,
                    resource.kind, resource.semantic_path
             FROM import_generation generation
             JOIN generation_unit gu ON gu.generation_id = generation.generation_id
             JOIN generation_resource resource
               ON resource.generation_id = gu.generation_id
              AND resource.resource_id = gu.resource_id
             JOIN unit u ON u.unit_id = gu.unit_id
             LEFT JOIN unit_head head ON head.unit_id = gu.unit_id
             LEFT JOIN translation_revision revision
               ON revision.revision_id = head.revision_id
             WHERE generation.state = 'Active'
               AND resource.kind = 'ImageRegion'
               AND (
                    gu.reading_order > ?1
                    OR (gu.reading_order = ?1 AND gu.unit_id > ?2)
               )
             ORDER BY gu.reading_order, gu.unit_id
             LIMIT ?3",
        )?;
        statement
            .query_map(
                params![after_reading_order, after_unit_id, limit as i64],
                |row| {
                    Ok(WorkbenchUnitRecord {
                        generation_id: vec_to_array(row.get(0)?)?,
                        unit_id: vec_to_array(row.get(1)?)?,
                        source_unit_key: vec_to_array(row.get(2)?)?,
                        resource_id: vec_to_array(row.get(3)?)?,
                        locator_json: row.get(4)?,
                        tir_json: row.get(5)?,
                        reading_order: row.get::<_, i64>(6)? as u64,
                        source_text: row.get(7)?,
                        translation: row.get(8)?,
                        revision_id: row.get(9)?,
                        revision_commit_sequence: row.get(10)?,
                        translation_document_schema_version: row.get(11)?,
                        translation_document_json: row.get(12)?,
                        resource_kind: row.get(13)?,
                        semantic_path: row.get(14)?,
                    })
                },
            )?
            .collect()
    }

    pub fn generation_resource(
        &self,
        generation_id: &[u8; 16],
        resource_id: &[u8; 16],
    ) -> rusqlite::Result<Option<ResourceRecord>> {
        self.connection
            .query_row(
                "SELECT resource_id, kind, semantic_path, locator_json
                 FROM generation_resource
                 WHERE generation_id = ?1 AND resource_id = ?2",
                params![generation_id.as_slice(), resource_id.as_slice()],
                |row| {
                    Ok(ResourceRecord {
                        resource_id: vec_to_array(row.get(0)?)?,
                        kind: row.get(1)?,
                        semantic_path: row.get(2)?,
                        locator_json: row.get(3)?,
                    })
                },
            )
            .optional()
    }

    pub fn related_resources(
        &self,
        generation_id: &[u8; 16],
        resource_id: &[u8; 16],
    ) -> rusqlite::Result<Vec<RelatedResourceRecord>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT related.resource_id, related.kind, related.semantic_path,
                    related.locator_json, edge.edge_kind
             FROM generation_edge edge
             JOIN generation_resource related
               ON related.generation_id = edge.generation_id
              AND related.resource_id = CASE
                    WHEN edge.from_resource_id = ?2 THEN edge.to_resource_id
                    ELSE edge.from_resource_id
                  END
             WHERE edge.generation_id = ?1
               AND (edge.from_resource_id = ?2 OR edge.to_resource_id = ?2)
             ORDER BY edge.ordinal, related.resource_id",
        )?;
        statement
            .query_map(
                params![generation_id.as_slice(), resource_id.as_slice()],
                |row| {
                    Ok(RelatedResourceRecord {
                        resource_id: vec_to_array(row.get(0)?)?,
                        kind: row.get(1)?,
                        semantic_path: row.get(2)?,
                        locator_json: row.get(3)?,
                        edge_kind: row.get(4)?,
                    })
                },
            )?
            .collect()
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

fn vec_to_array<const N: usize>(bytes: Vec<u8>) -> rusqlite::Result<[u8; N]> {
    bytes
        .try_into()
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, N as i64))
}

fn invalid_data(message: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.to_owned(),
        )),
    )
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
