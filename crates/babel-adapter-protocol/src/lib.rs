//! Versioned, bounded protocol shared by the core and built-in format adapters.

use std::{
    io::{Read, Seek, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use babel_domain::core::{GenerationId, ResourceId};
use babel_resource_graph::{Locator, ResourceEdge, ResourceKind, ResourceNode};
use babel_tir::UnitContent;
use prost::Message;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ADAPTER_PROTOCOL_MAJOR: u32 = 1;
pub const ADAPTER_PROTOCOL_MINOR: u32 = 3;
pub const DEFAULT_PAGE_BYTES: u64 = 256 * 1024;
pub const DEFAULT_PAGE_NODES: u32 = 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRange {
    pub major: u32,
    pub minimum_minor: u32,
    pub maximum_minor: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyLimits {
    pub maximum_input_bytes: u64,
    pub maximum_output_bytes: u64,
    pub maximum_nodes_per_page: u32,
    pub maximum_page_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub adapter_id: String,
    pub adapter_build: String,
    pub protocol_range: ProtocolRange,
    pub identity_version: u32,
    pub mime_types: Vec<String>,
    pub extensions: Vec<String>,
    pub resource_kinds: Vec<ResourceKind>,
    pub export_fidelity_tier: String,
    pub deterministic_stages: Vec<Operation>,
    pub safety_limits: SafetyLimits,
}

impl AdapterManifest {
    pub fn validate(&self) -> Result<(), AdapterError> {
        if self.adapter_id.is_empty() || self.adapter_build.is_empty() {
            return Err(AdapterError::InvalidManifest);
        }
        if self.protocol_range.major != ADAPTER_PROTOCOL_MAJOR
            || !supports_minor(&self.protocol_range, ADAPTER_PROTOCOL_MINOR)
        {
            return Err(AdapterError::IncompatibleProtocol);
        }
        if self.safety_limits.maximum_input_bytes == 0
            || self.safety_limits.maximum_output_bytes == 0
            || self.safety_limits.maximum_nodes_per_page == 0
            || self.safety_limits.maximum_page_bytes == 0
        {
            return Err(AdapterError::InvalidManifest);
        }
        Ok(())
    }
}

fn supports_minor(range: &ProtocolRange, minor: u32) -> bool {
    range.minimum_minor <= minor && minor <= range.maximum_minor
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityAccess {
    ReadObject,
    ReadWriteStaging,
    ReadPrivateArtifact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectHandle {
    pub capability_id: [u8; 16],
    pub object_hash: [u8; 32],
    pub access: CapabilityAccess,
    pub byte_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagingHandle {
    pub capability_id: [u8; 16],
    pub access: CapabilityAccess,
}

pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

pub trait WriteSeek: Write + Seek + Send {}
impl<T: Write + Seek + Send> WriteSeek for T {}

pub trait CapabilityIo: Send + Sync {
    fn open_object(&self, handle: &ObjectHandle) -> Result<Box<dyn ReadSeek>, AdapterError>;
    fn write_staging_at(
        &self,
        handle: &StagingHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), AdapterError>;
    fn open_staging(&self, handle: &StagingHandle) -> Result<Box<dyn ReadSeek>, AdapterError>;
    fn open_staging_writer(
        &self,
        handle: &StagingHandle,
    ) -> Result<Box<dyn WriteSeek>, AdapterError>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskBudget {
    pub timeout_ms: u64,
    pub maximum_bytes: u64,
    pub maximum_nodes: u32,
    pub page_bytes: u64,
    pub page_nodes: u32,
}

impl TaskBudget {
    pub fn bounded_page_bytes(&self, manifest: &AdapterManifest) -> u64 {
        self.page_bytes
            .max(1)
            .min(self.maximum_bytes)
            .min(manifest.safety_limits.maximum_page_bytes)
    }

    pub fn bounded_page_nodes(&self, manifest: &AdapterManifest) -> u32 {
        self.page_nodes
            .max(1)
            .min(self.maximum_nodes)
            .min(manifest.safety_limits.maximum_nodes_per_page)
    }
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub struct ExecutionContext<'a> {
    pub budget: &'a TaskBudget,
    pub cancellation: &'a CancellationToken,
    deadline: Instant,
}

impl<'a> ExecutionContext<'a> {
    pub fn new(budget: &'a TaskBudget, cancellation: &'a CancellationToken) -> Self {
        Self {
            budget,
            cancellation,
            deadline: Instant::now() + Duration::from_millis(budget.timeout_ms),
        }
    }

    #[cfg(test)]
    fn with_deadline(
        budget: &'a TaskBudget,
        cancellation: &'a CancellationToken,
        deadline: Instant,
    ) -> Self {
        Self {
            budget,
            cancellation,
            deadline,
        }
    }

    pub fn checkpoint(&self) -> Result<(), AdapterError> {
        if self.cancellation.is_cancelled() {
            return Err(AdapterError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(AdapterError::DeadlineExceeded);
        }
        if self.budget.maximum_bytes == 0
            || self.budget.maximum_nodes == 0
            || self.budget.page_bytes == 0
            || self.budget.page_nodes == 0
        {
            return Err(AdapterError::BudgetExceeded);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    Probe,
    Inventory,
    Extract,
    Validate,
    PlanExport,
    Materialize,
    VerifyOutput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor(pub Vec<u8>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Cursor>,
    pub emitted_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub confidence_millionths: u32,
    pub detected_media_type: Option<String>,
    pub reason_code: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InventoryItem {
    Node(ResourceNode),
    Edge(ResourceEdge),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractedUnit {
    pub generation_id: GenerationId,
    pub resource_id: ResourceId,
    pub source_unit_key: [u8; 32],
    pub locator: Locator,
    pub content: UnitContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OverlayUnit {
    pub source_unit_key: [u8; 32],
    pub source_locator: Locator,
    pub translated_text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageOverlay {
    pub image_resource_id: ResourceId,
    pub source_locator: Locator,
    pub region_locator: Locator,
    pub derived_object: ObjectHandle,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportPlan {
    pub plan_id: [u8; 32],
    pub generation_id: GenerationId,
    pub source_object_hash: [u8; 32],
    pub frozen_commit_sequence: i64,
    pub overlay_hash: [u8; 32],
    pub ordered_source_unit_keys: Vec<[u8; 32]>,
    pub ordered_overlay_hashes: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterializeProgress {
    pub next_cursor: Option<Cursor>,
    pub bytes_written: u64,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub valid: bool,
    pub output_hash: [u8; 32],
    pub byte_length: u64,
    pub issue_codes: Vec<String>,
}

pub trait Adapter: Send + Sync {
    fn manifest(&self) -> &AdapterManifest;
    fn probe(
        &self,
        input: &ObjectHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<ProbeResult, AdapterError>;
    fn inventory(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<InventoryItem>, AdapterError>;
    fn extract(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        resource_id: ResourceId,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<ExtractedUnit>, AdapterError>;
    fn plan_export(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        frozen_commit_sequence: i64,
        overlays: &[OverlayUnit],
        context: &ExecutionContext<'_>,
    ) -> Result<ExportPlan, AdapterError>;
    #[allow(clippy::too_many_arguments)]
    fn materialize(
        &self,
        plan: &ExportPlan,
        input: &ObjectHandle,
        overlays: &[OverlayUnit],
        staging: &StagingHandle,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<MaterializeProgress, AdapterError>;
    fn verify_output(
        &self,
        candidate: &StagingHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<VerificationReport, AdapterError>;

    fn apply_image_overlays(
        &self,
        _plan: &ExportPlan,
        _input: &ObjectHandle,
        overlays: &[ImageOverlay],
        _staging: &StagingHandle,
        _io: &dyn CapabilityIo,
        _context: &ExecutionContext<'_>,
    ) -> Result<(), AdapterError> {
        if overlays.is_empty() {
            Ok(())
        } else {
            Err(AdapterError::InvalidInput(
                "adapter does not support image member overlays".to_owned(),
            ))
        }
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct AdapterEnvelope {
    #[prost(uint64, tag = "1")]
    pub request_id: u64,
    #[prost(uint32, tag = "2")]
    pub protocol_major: u32,
    #[prost(uint32, tag = "3")]
    pub protocol_minor: u32,
    #[prost(enumeration = "WireOperation", tag = "4")]
    pub operation: i32,
    #[prost(bytes = "vec", tag = "5")]
    pub cursor: Vec<u8>,
    #[prost(bytes = "vec", tag = "6")]
    pub payload: Vec<u8>,
    #[prost(uint64, tag = "7")]
    pub maximum_response_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub enum WireOperation {
    Unspecified = 0,
    Probe = 1,
    Inventory = 2,
    Extract = 3,
    Validate = 4,
    PlanExport = 5,
    Materialize = 6,
    VerifyOutput = 7,
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("adapter manifest is invalid")]
    InvalidManifest,
    #[error("adapter protocol version is incompatible")]
    IncompatibleProtocol,
    #[error("operation was cancelled")]
    Cancelled,
    #[error("operation deadline was exceeded")]
    DeadlineExceeded,
    #[error("adapter exceeded its byte or node budget")]
    BudgetExceeded,
    #[error("capability is unknown or does not grant the requested access")]
    CapabilityDenied,
    #[error("cursor is invalid for this operation")]
    InvalidCursor,
    #[error("adapter input is invalid: {0}")]
    InvalidInput(String),
    #[error("adapter I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_budget_is_clamped_by_request_and_manifest() {
        let manifest = AdapterManifest {
            adapter_id: "mock".to_owned(),
            adapter_build: "1".to_owned(),
            protocol_range: ProtocolRange {
                major: ADAPTER_PROTOCOL_MAJOR,
                minimum_minor: 0,
                maximum_minor: 0,
            },
            identity_version: 1,
            mime_types: Vec::new(),
            extensions: Vec::new(),
            resource_kinds: Vec::new(),
            export_fidelity_tier: "A".to_owned(),
            deterministic_stages: Vec::new(),
            safety_limits: SafetyLimits {
                maximum_input_bytes: 1_000,
                maximum_output_bytes: 1_000,
                maximum_nodes_per_page: 50,
                maximum_page_bytes: 500,
            },
        };
        let budget = TaskBudget {
            timeout_ms: 10,
            maximum_bytes: 400,
            maximum_nodes: 40,
            page_bytes: 900,
            page_nodes: 90,
        };
        assert_eq!(budget.bounded_page_bytes(&manifest), 400);
        assert_eq!(budget.bounded_page_nodes(&manifest), 40);
    }

    #[test]
    fn cancellation_is_checked_at_protocol_boundaries() {
        let token = CancellationToken::default();
        token.cancel();
        let budget = TaskBudget {
            timeout_ms: 10,
            maximum_bytes: 1,
            maximum_nodes: 1,
            page_bytes: 1,
            page_nodes: 1,
        };
        let context = ExecutionContext::new(&budget, &token);
        assert!(matches!(context.checkpoint(), Err(AdapterError::Cancelled)));
    }

    #[test]
    fn deadline_uses_a_live_monotonic_clock() {
        let token = CancellationToken::default();
        let budget = TaskBudget {
            timeout_ms: 10,
            maximum_bytes: 1,
            maximum_nodes: 1,
            page_bytes: 1,
            page_nodes: 1,
        };
        let context = ExecutionContext::with_deadline(
            &budget,
            &token,
            Instant::now() - Duration::from_millis(1),
        );
        assert!(matches!(
            context.checkpoint(),
            Err(AdapterError::DeadlineExceeded)
        ));
    }

    #[test]
    fn zero_budget_fails_instead_of_returning_a_non_progressing_page() {
        let token = CancellationToken::default();
        let budget = TaskBudget {
            timeout_ms: 10,
            maximum_bytes: 1,
            maximum_nodes: 0,
            page_bytes: 1,
            page_nodes: 1,
        };
        assert!(matches!(
            ExecutionContext::new(&budget, &token).checkpoint(),
            Err(AdapterError::BudgetExceeded)
        ));
    }
}
