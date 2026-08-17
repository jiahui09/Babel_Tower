//! Versioned, format-neutral resource graph and binding contracts.

use std::collections::{HashMap, HashSet, hash_map::Entry};

use babel_domain::core::{BindingId, GenerationId, ResourceId, UnitId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const RESOURCE_GRAPH_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    Container,
    Document,
    TextStream,
    Image,
    ImageRegion,
    Font,
    Stylesheet,
    BinaryAttachment,
    AudioTrack,
    VideoTrack,
    SubtitleTrack,
    TimelineRegion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Contains,
    References,
    ReadingOrderAfter,
    DerivedFrom,
    RegionOf,
    AlternateOf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Locator {
    ByteSpan {
        object_hash: [u8; 32],
        start: u64,
        end: u64,
    },
    ArchiveMemberByteSpan {
        object_hash: [u8; 32],
        member_path: String,
        start: u64,
        end: u64,
    },
    StructuralPath {
        resource_id: ResourceId,
        path_segments: Vec<String>,
        attribute: Option<String>,
    },
    TextRange {
        resource_id: ResourceId,
        node_key: String,
        grapheme_start: u64,
        grapheme_end: u64,
    },
    SpatialRegion {
        resource_id: ResourceId,
        polygon: Vec<[f32; 2]>,
        coordinate_space: String,
    },
    TemporalRange {
        resource_id: ResourceId,
        start_ns: u64,
        end_ns: u64,
    },
    FrameRange {
        resource_id: ResourceId,
        start_frame: u64,
        end_frame: u64,
        timebase_numerator: u32,
        timebase_denominator: u32,
    },
    OpaqueAdapter {
        adapter_id: String,
        schema_version: u32,
        bytes_hash: [u8; 32],
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceNode {
    pub resource_id: ResourceId,
    pub resource_key: [u8; 32],
    pub kind: ResourceKind,
    pub semantic_path: String,
    pub locator: Locator,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEdge {
    pub from: ResourceId,
    pub to: ResourceId,
    pub kind: EdgeKind,
    pub ordinal: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceGraph {
    pub schema_version: u32,
    pub generation_id: GenerationId,
    pub nodes: Vec<ResourceNode>,
    pub edges: Vec<ResourceEdge>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GraphError {
    #[error("unsupported resource graph schema version {0}")]
    UnsupportedSchema(u32),
    #[error("resource id is duplicated")]
    DuplicateResourceId,
    #[error("resource key is duplicated")]
    DuplicateResourceKey,
    #[error("resource semantic path cannot be empty")]
    EmptySemanticPath,
    #[error("resource edge references a missing endpoint")]
    MissingEndpoint,
    #[error("{0:?} edges contain a cycle")]
    Cycle(EdgeKind),
    #[error("byte span or range has an inverted boundary")]
    InvertedRange,
    #[error("spatial region must contain at least three finite points")]
    InvalidPolygon,
    #[error("image region must point to an image through RegionOf")]
    InvalidRegionOwner,
    #[error("image region must have exactly one RegionOf owner")]
    MissingOrDuplicateRegionOwner,
    #[error("frame range timebase denominator must be nonzero")]
    InvalidTimebase,
}

impl ResourceGraph {
    pub fn validate(&self) -> Result<(), GraphError> {
        if self.schema_version != RESOURCE_GRAPH_SCHEMA_VERSION {
            return Err(GraphError::UnsupportedSchema(self.schema_version));
        }
        let mut ids = HashSet::with_capacity(self.nodes.len());
        let mut keys = HashSet::with_capacity(self.nodes.len());
        let mut kinds = HashMap::with_capacity(self.nodes.len());
        for node in &self.nodes {
            if !ids.insert(node.resource_id) {
                return Err(GraphError::DuplicateResourceId);
            }
            if !keys.insert(node.resource_key) {
                return Err(GraphError::DuplicateResourceKey);
            }
            if node.semantic_path.is_empty() {
                return Err(GraphError::EmptySemanticPath);
            }
            validate_locator(&node.locator)?;
            kinds.insert(node.resource_id, node.kind);
        }
        for node in &self.nodes {
            if locator_resource(&node.locator)
                .is_some_and(|resource_id| !ids.contains(&resource_id))
            {
                return Err(GraphError::MissingEndpoint);
            }
        }
        let mut region_owners = HashMap::<ResourceId, usize>::new();
        for edge in &self.edges {
            if !ids.contains(&edge.from) || !ids.contains(&edge.to) {
                return Err(GraphError::MissingEndpoint);
            }
            if edge.kind == EdgeKind::RegionOf
                && (kinds[&edge.from] != ResourceKind::ImageRegion
                    || kinds[&edge.to] != ResourceKind::Image)
            {
                return Err(GraphError::InvalidRegionOwner);
            }
            if edge.kind == EdgeKind::RegionOf {
                *region_owners.entry(edge.from).or_default() += 1;
            }
        }
        if kinds.iter().any(|(id, kind)| {
            *kind == ResourceKind::ImageRegion && region_owners.get(id).copied() != Some(1)
        }) {
            return Err(GraphError::MissingOrDuplicateRegionOwner);
        }
        for kind in [EdgeKind::Contains, EdgeKind::ReadingOrderAfter] {
            if has_cycle(&ids, &self.edges, kind) {
                return Err(GraphError::Cycle(kind));
            }
        }
        Ok(())
    }
}

fn locator_resource(locator: &Locator) -> Option<ResourceId> {
    match locator {
        Locator::StructuralPath { resource_id, .. }
        | Locator::TextRange { resource_id, .. }
        | Locator::SpatialRegion { resource_id, .. }
        | Locator::TemporalRange { resource_id, .. }
        | Locator::FrameRange { resource_id, .. } => Some(*resource_id),
        Locator::ByteSpan { .. }
        | Locator::ArchiveMemberByteSpan { .. }
        | Locator::OpaqueAdapter { .. } => None,
    }
}

pub fn resource_key(
    source_snapshot: &[u8; 32],
    adapter_id: &str,
    identity_version: u32,
    semantic_path: &str,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(source_snapshot);
    hasher.update((adapter_id.len() as u64).to_be_bytes());
    hasher.update(adapter_id.as_bytes());
    hasher.update(identity_version.to_be_bytes());
    hasher.update((semantic_path.len() as u64).to_be_bytes());
    hasher.update(semantic_path.as_bytes());
    hasher.finalize().into()
}

fn validate_locator(locator: &Locator) -> Result<(), GraphError> {
    match locator {
        Locator::ByteSpan { start, end, .. }
        | Locator::ArchiveMemberByteSpan { start, end, .. }
        | Locator::TemporalRange {
            start_ns: start,
            end_ns: end,
            ..
        }
        | Locator::FrameRange {
            start_frame: start,
            end_frame: end,
            ..
        } if start > end => Err(GraphError::InvertedRange),
        Locator::TextRange {
            grapheme_start,
            grapheme_end,
            ..
        } if grapheme_start > grapheme_end => Err(GraphError::InvertedRange),
        Locator::FrameRange {
            timebase_denominator: 0,
            ..
        } => Err(GraphError::InvalidTimebase),
        Locator::SpatialRegion { polygon, .. }
            if polygon.len() < 3
                || polygon
                    .iter()
                    .flatten()
                    .any(|coordinate| !coordinate.is_finite()) =>
        {
            Err(GraphError::InvalidPolygon)
        }
        _ => Ok(()),
    }
}

fn has_cycle(ids: &HashSet<ResourceId>, edges: &[ResourceEdge], kind: EdgeKind) -> bool {
    let mut incoming = ids
        .iter()
        .map(|id| (*id, 0_usize))
        .collect::<HashMap<_, _>>();
    let mut outgoing: HashMap<ResourceId, Vec<ResourceId>> = HashMap::new();
    for edge in edges.iter().filter(|edge| edge.kind == kind) {
        outgoing.entry(edge.from).or_default().push(edge.to);
        *incoming.get_mut(&edge.to).expect("validated endpoint") += 1;
    }
    let mut ready = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect::<Vec<_>>();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        for next in outgoing.get(&id).into_iter().flatten() {
            let count = incoming.get_mut(next).expect("validated endpoint");
            *count -= 1;
            if *count == 0 {
                ready.push(*next);
            }
        }
    }
    visited != ids.len()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingDisposition {
    Exact,
    Shifted,
    Ambiguous,
    Orphaned,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingCandidate {
    pub unit_id: UnitId,
    pub score_millionths: u32,
    pub structure_evidence: Option<[u8; 32]>,
    pub neighborhood_evidence: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingRecord {
    pub binding_id: BindingId,
    pub generation_id: GenerationId,
    pub source_unit_key: [u8; 32],
    pub disposition: BindingDisposition,
    pub candidates: Vec<BindingCandidate>,
    pub selected_unit_id: Option<UnitId>,
    pub policy_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingDecision {
    pub command_id: [u8; 32],
    pub binding_id: BindingId,
    pub selected_unit_id: UnitId,
    pub candidate_unit_ids: Vec<UnitId>,
    pub reason_code: String,
}

#[derive(Default)]
pub struct BindingLedger {
    records: HashMap<BindingId, BindingRecord>,
    decisions: HashMap<[u8; 32], BindingDecision>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BindingError {
    #[error("binding record already exists")]
    DuplicateBinding,
    #[error("binding decision command already exists with different content")]
    CommandConflict,
    #[error("binding record does not exist")]
    MissingBinding,
    #[error("binding decision selected a unit outside the recorded candidate set")]
    CandidateMismatch,
    #[error("automatic inheritance is only allowed for Exact bindings")]
    UnsafeAutomaticInheritance,
    #[error("binding candidate set does not match its disposition")]
    InvalidCandidateSet,
}

impl BindingLedger {
    pub fn append_record(&mut self, record: BindingRecord) -> Result<(), BindingError> {
        if record.selected_unit_id.is_some() && record.disposition != BindingDisposition::Exact {
            return Err(BindingError::UnsafeAutomaticInheritance);
        }
        let candidate_ids = record
            .candidates
            .iter()
            .map(|candidate| candidate.unit_id)
            .collect::<HashSet<_>>();
        let candidate_shape_is_valid = candidate_ids.len() == record.candidates.len()
            && record
                .candidates
                .iter()
                .all(|candidate| candidate.score_millionths <= 1_000_000)
            && match record.disposition {
                BindingDisposition::Exact => record.candidates.len() == 1,
                BindingDisposition::Shifted => !record.candidates.is_empty(),
                BindingDisposition::Ambiguous => record.candidates.len() >= 2,
                BindingDisposition::Orphaned => record.candidates.is_empty(),
            };
        if !candidate_shape_is_valid {
            return Err(BindingError::InvalidCandidateSet);
        }
        if record
            .selected_unit_id
            .is_some_and(|selected| !candidate_ids.contains(&selected))
        {
            return Err(BindingError::CandidateMismatch);
        }
        match self.records.entry(record.binding_id) {
            Entry::Vacant(entry) => {
                entry.insert(record);
                Ok(())
            }
            Entry::Occupied(_) => Err(BindingError::DuplicateBinding),
        }
    }

    pub fn decide(&mut self, decision: BindingDecision) -> Result<(), BindingError> {
        if let Some(existing) = self.decisions.get(&decision.command_id) {
            return if existing == &decision {
                Ok(())
            } else {
                Err(BindingError::CommandConflict)
            };
        }
        let record = self
            .records
            .get_mut(&decision.binding_id)
            .ok_or(BindingError::MissingBinding)?;
        let recorded = record
            .candidates
            .iter()
            .map(|candidate| candidate.unit_id)
            .collect::<HashSet<_>>();
        let declared = decision
            .candidate_unit_ids
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        if recorded != declared || !recorded.contains(&decision.selected_unit_id) {
            return Err(BindingError::CandidateMismatch);
        }
        record.selected_unit_id = Some(decision.selected_unit_id);
        self.decisions.insert(decision.command_id, decision);
        Ok(())
    }

    pub fn record(&self, id: BindingId) -> Option<&BindingRecord> {
        self.records.get(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: ResourceId, kind: ResourceKind, path: &str) -> ResourceNode {
        ResourceNode {
            resource_id: id,
            resource_key: resource_key(&[1; 32], "mock", 1, path),
            kind,
            semantic_path: path.to_owned(),
            locator: Locator::ByteSpan {
                object_hash: [1; 32],
                start: 0,
                end: 1,
            },
        }
    }

    #[test]
    fn graph_rejects_cycles_and_invalid_region_ownership() {
        let first = ResourceId::new();
        let second = ResourceId::new();
        let graph = ResourceGraph {
            schema_version: RESOURCE_GRAPH_SCHEMA_VERSION,
            generation_id: GenerationId::new(),
            nodes: vec![
                node(first, ResourceKind::Document, "a"),
                node(second, ResourceKind::Image, "b"),
            ],
            edges: vec![
                ResourceEdge {
                    from: first,
                    to: second,
                    kind: EdgeKind::Contains,
                    ordinal: 0,
                },
                ResourceEdge {
                    from: second,
                    to: first,
                    kind: EdgeKind::Contains,
                    ordinal: 0,
                },
            ],
        };
        assert_eq!(graph.validate(), Err(GraphError::Cycle(EdgeKind::Contains)));
    }

    #[test]
    fn graph_rejects_a_locator_that_targets_a_missing_resource() {
        let resource_id = ResourceId::new();
        let graph = ResourceGraph {
            schema_version: RESOURCE_GRAPH_SCHEMA_VERSION,
            generation_id: GenerationId::new(),
            nodes: vec![ResourceNode {
                resource_id,
                resource_key: resource_key(&[1; 32], "mock", 1, "text"),
                kind: ResourceKind::TextStream,
                semantic_path: "text".to_owned(),
                locator: Locator::TextRange {
                    resource_id: ResourceId::new(),
                    node_key: "paragraph".to_owned(),
                    grapheme_start: 0,
                    grapheme_end: 1,
                },
            }],
            edges: Vec::new(),
        };
        assert_eq!(graph.validate(), Err(GraphError::MissingEndpoint));
    }

    #[test]
    fn image_region_requires_one_owner_and_frame_timebase_is_nonzero() {
        let image = ResourceId::new();
        let region = ResourceId::new();
        let graph = ResourceGraph {
            schema_version: RESOURCE_GRAPH_SCHEMA_VERSION,
            generation_id: GenerationId::new(),
            nodes: vec![
                node(image, ResourceKind::Image, "image"),
                ResourceNode {
                    resource_id: region,
                    resource_key: resource_key(&[1; 32], "mock", 1, "region"),
                    kind: ResourceKind::ImageRegion,
                    semantic_path: "region".to_owned(),
                    locator: Locator::SpatialRegion {
                        resource_id: image,
                        polygon: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                        coordinate_space: "pixel".to_owned(),
                    },
                },
            ],
            edges: Vec::new(),
        };
        assert_eq!(
            graph.validate(),
            Err(GraphError::MissingOrDuplicateRegionOwner)
        );

        assert_eq!(
            validate_locator(&Locator::FrameRange {
                resource_id: image,
                start_frame: 0,
                end_frame: 1,
                timebase_numerator: 1,
                timebase_denominator: 0,
            }),
            Err(GraphError::InvalidTimebase)
        );

        assert_eq!(
            validate_locator(&Locator::ArchiveMemberByteSpan {
                object_hash: [7; 32],
                member_path: "EPUB/chapter.xhtml".to_owned(),
                start: 9,
                end: 4,
            }),
            Err(GraphError::InvertedRange)
        );
    }

    #[test]
    fn ambiguous_binding_requires_an_explicit_candidate_decision() {
        let binding_id = BindingId::new();
        let first = UnitId::new();
        let second = UnitId::new();
        let mut ledger = BindingLedger::default();
        ledger
            .append_record(BindingRecord {
                binding_id,
                generation_id: GenerationId::new(),
                source_unit_key: [2; 32],
                disposition: BindingDisposition::Ambiguous,
                candidates: vec![first, second]
                    .into_iter()
                    .map(|unit_id| BindingCandidate {
                        unit_id,
                        score_millionths: 900_000,
                        structure_evidence: None,
                        neighborhood_evidence: None,
                    })
                    .collect(),
                selected_unit_id: None,
                policy_version: 1,
            })
            .unwrap();
        ledger
            .decide(BindingDecision {
                command_id: [3; 32],
                binding_id,
                selected_unit_id: second,
                candidate_unit_ids: vec![first, second],
                reason_code: "translator-confirmed".to_owned(),
            })
            .unwrap();
        assert_eq!(
            ledger.record(binding_id).unwrap().selected_unit_id,
            Some(second)
        );
    }

    #[test]
    fn duplicate_binding_does_not_replace_the_original_record() {
        let binding_id = BindingId::new();
        let original_key = [2; 32];
        let mut ledger = BindingLedger::default();
        let original = BindingRecord {
            binding_id,
            generation_id: GenerationId::new(),
            source_unit_key: original_key,
            disposition: BindingDisposition::Orphaned,
            candidates: Vec::new(),
            selected_unit_id: None,
            policy_version: 1,
        };
        ledger.append_record(original.clone()).unwrap();

        let mut replacement = original;
        replacement.source_unit_key = [9; 32];
        assert_eq!(
            ledger.append_record(replacement),
            Err(BindingError::DuplicateBinding)
        );
        assert_eq!(
            ledger.record(binding_id).unwrap().source_unit_key,
            original_key
        );
    }

    #[test]
    fn exact_binding_cannot_select_a_unit_outside_its_candidates() {
        let mut ledger = BindingLedger::default();
        let candidate = UnitId::new();
        assert_eq!(
            ledger.append_record(BindingRecord {
                binding_id: BindingId::new(),
                generation_id: GenerationId::new(),
                source_unit_key: [4; 32],
                disposition: BindingDisposition::Exact,
                candidates: vec![BindingCandidate {
                    unit_id: candidate,
                    score_millionths: 1_000_000,
                    structure_evidence: None,
                    neighborhood_evidence: None,
                }],
                selected_unit_id: Some(UnitId::new()),
                policy_version: 1,
            }),
            Err(BindingError::CandidateMismatch)
        );
    }
}
