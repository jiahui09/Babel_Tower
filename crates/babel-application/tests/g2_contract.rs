use std::collections::HashMap;

use babel_adapter_host::{CapabilityRegistry, MockTextAdapter};
use babel_adapter_protocol::{
    Adapter, CancellationToken, Cursor, ExecutionContext, InventoryItem, OverlayUnit, TaskBudget,
};
use babel_domain::core::GenerationId;
use babel_resource_graph::Locator;
use babel_resource_graph::{
    BindingCandidate, BindingDisposition, BindingLedger, BindingRecord,
    RESOURCE_GRAPH_SCHEMA_VERSION, ResourceGraph,
};
use babel_storage::{
    cas,
    project::{
        GenerationBatch, GenerationBindingRecord, GenerationDescriptor, GenerationEdgeRecord,
        GenerationResourceRecord, GenerationUnitRecord, ProjectStore, candidate_set_hash,
    },
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

struct ImportedGeneration {
    id: GenerationId,
    source_hash: [u8; 32],
    source_length: u64,
    units: Vec<babel_adapter_protocol::ExtractedUnit>,
    unit_ids: HashMap<[u8; 32], [u8; 16]>,
}

fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(((*part).len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn context<'a>(
    budget: &'a TaskBudget,
    cancellation: &'a CancellationToken,
) -> ExecutionContext<'a> {
    ExecutionContext::new(budget, cancellation)
}

fn collect_inventory(
    adapter: &MockTextAdapter,
    handle: &babel_adapter_protocol::ObjectHandle,
    registry: &CapabilityRegistry,
    generation_id: GenerationId,
    context: &ExecutionContext<'_>,
) -> (
    Vec<babel_resource_graph::ResourceNode>,
    Vec<babel_resource_graph::ResourceEdge>,
) {
    let mut cursor: Option<Cursor> = None;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    loop {
        let page = adapter
            .inventory(handle, generation_id, cursor.as_ref(), registry, context)
            .unwrap();
        for item in page.items {
            match item {
                InventoryItem::Node(node) => nodes.push(node),
                InventoryItem::Edge(edge) => edges.push(edge),
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    (nodes, edges)
}

fn import_text(
    store: &mut ProjectStore,
    objects: &std::path::Path,
    staging: &std::path::Path,
    bytes: &[u8],
    previous: Option<&ImportedGeneration>,
) -> ImportedGeneration {
    let (source_hash, _, byte_length) = cas::publish_reader(objects, bytes).unwrap();
    let registry = CapabilityRegistry::new(objects, staging).unwrap();
    let handle = registry.grant_object(source_hash, byte_length).unwrap();
    let adapter = MockTextAdapter::default();
    adapter.manifest().validate().unwrap();
    let cancellation = CancellationToken::default();
    let budget = TaskBudget {
        timeout_ms: 10_000,
        maximum_bytes: 1024 * 1024,
        maximum_nodes: 100,
        page_bytes: 1024,
        page_nodes: 1,
    };
    let execution = context(&budget, &cancellation);
    assert_eq!(
        adapter
            .probe(&handle, &registry, &execution)
            .unwrap()
            .detected_media_type
            .as_deref(),
        Some("text/plain")
    );

    let generation_id = GenerationId::new();
    let (nodes, edges) = collect_inventory(&adapter, &handle, &registry, generation_id, &execution);
    ResourceGraph {
        schema_version: RESOURCE_GRAPH_SCHEMA_VERSION,
        generation_id,
        nodes: nodes.clone(),
        edges: edges.clone(),
    }
    .validate()
    .unwrap();
    store
        .begin_generation(&GenerationDescriptor {
            generation_id: *generation_id.as_bytes(),
            source_snapshot_hash: source_hash,
            adapter_id: adapter.manifest().adapter_id.clone(),
            adapter_build: adapter.manifest().adapter_build.clone(),
            identity_version: adapter.manifest().identity_version,
            created_at_ms: 1,
        })
        .unwrap();

    let resources = GenerationBatch {
        resources: nodes
            .iter()
            .map(|node| GenerationResourceRecord {
                resource_id: *node.resource_id.as_bytes(),
                resource_key: node.resource_key,
                kind: format!("{:?}", node.kind),
                semantic_path: node.semantic_path.clone(),
                locator_json: serde_json::to_vec(&node.locator).unwrap(),
            })
            .collect(),
        ..GenerationBatch::default()
    };
    let receipt = store
        .append_generation_batch(
            generation_id.as_bytes(),
            &hash(&[b"resources", generation_id.as_bytes()]),
            &hash(&[b"resources-payload", &source_hash]),
            &resources,
        )
        .unwrap();
    assert!(!receipt.replayed);
    assert!(
        store
            .append_generation_batch(
                generation_id.as_bytes(),
                &hash(&[b"resources", generation_id.as_bytes()]),
                &hash(&[b"resources-payload", &source_hash]),
                &resources,
            )
            .unwrap()
            .replayed
    );
    store
        .append_generation_batch(
            generation_id.as_bytes(),
            &hash(&[b"edges", generation_id.as_bytes()]),
            &hash(&[b"edges-payload", &source_hash]),
            &GenerationBatch {
                edges: edges
                    .iter()
                    .map(|edge| GenerationEdgeRecord {
                        from_resource_id: *edge.from.as_bytes(),
                        to_resource_id: *edge.to.as_bytes(),
                        edge_kind: format!("{:?}", edge.kind),
                        ordinal: edge.ordinal,
                    })
                    .collect(),
                ..GenerationBatch::default()
            },
        )
        .unwrap();

    let text_resource = nodes
        .iter()
        .find(|node| node.kind == babel_resource_graph::ResourceKind::TextStream)
        .unwrap()
        .resource_id;
    let mut cursor = None;
    let mut units = Vec::new();
    loop {
        let page = adapter
            .extract(
                &handle,
                generation_id,
                text_resource,
                cursor.as_ref(),
                &registry,
                &execution,
            )
            .unwrap();
        for unit in &page.items {
            unit.content.validate().unwrap();
        }
        units.extend(page.items);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    let mut ledger = BindingLedger::default();
    let mut batch = GenerationBatch::default();
    for (index, unit) in units.iter().enumerate() {
        let extracted_hash = hash(&[
            b"extracted-unit-v1",
            generation_id.as_bytes(),
            &(index as u64).to_be_bytes(),
            &unit.source_unit_key,
        ]);
        let extracted_id: [u8; 16] = extracted_hash[..16].try_into().unwrap();
        batch.units.push(GenerationUnitRecord {
            extracted_unit_id: extracted_id,
            source_unit_key: unit.source_unit_key,
            resource_id: *unit.resource_id.as_bytes(),
            locator_json: serde_json::to_vec(&unit.locator).unwrap(),
            tir_json: serde_json::to_vec(&unit.content).unwrap(),
            reading_order: index as u64,
        });
        let binding_hash = hash(&[b"binding-v1", generation_id.as_bytes(), &extracted_id]);
        let binding_id: [u8; 16] = binding_hash[..16].try_into().unwrap();
        if let Some(old) = previous.and_then(|old| old.unit_ids.get(&unit.source_unit_key)) {
            ledger
                .append_record(BindingRecord {
                    binding_id: babel_domain::core::BindingId::from_bytes(binding_id),
                    generation_id,
                    source_unit_key: unit.source_unit_key,
                    disposition: BindingDisposition::Exact,
                    candidates: vec![BindingCandidate {
                        unit_id: babel_domain::core::UnitId::from_bytes(*old),
                        score_millionths: 1_000_000,
                        structure_evidence: None,
                        neighborhood_evidence: None,
                    }],
                    selected_unit_id: Some(babel_domain::core::UnitId::from_bytes(*old)),
                    policy_version: 1,
                })
                .unwrap();
            let candidates_json = serde_json::to_vec(&vec![*old]).unwrap();
            batch.bindings.push(GenerationBindingRecord {
                binding_id,
                extracted_unit_id: extracted_id,
                disposition: "Exact".to_owned(),
                selected_unit_id: Some(*old),
                policy_version: 1,
                candidates_hash: candidate_set_hash(&candidates_json),
                candidates_json,
            });
        } else {
            ledger
                .append_record(BindingRecord {
                    binding_id: babel_domain::core::BindingId::from_bytes(binding_id),
                    generation_id,
                    source_unit_key: unit.source_unit_key,
                    disposition: BindingDisposition::Orphaned,
                    candidates: Vec::new(),
                    selected_unit_id: None,
                    policy_version: 1,
                })
                .unwrap();
            let candidates_json = serde_json::to_vec(&Vec::<[u8; 16]>::new()).unwrap();
            batch.bindings.push(GenerationBindingRecord {
                binding_id,
                extracted_unit_id: extracted_id,
                disposition: "Orphaned".to_owned(),
                selected_unit_id: None,
                policy_version: 1,
                candidates_hash: candidate_set_hash(&candidates_json),
                candidates_json,
            });
        }
    }
    store
        .append_generation_batch(
            generation_id.as_bytes(),
            &hash(&[b"units", generation_id.as_bytes()]),
            &hash(&[b"units-payload", &source_hash]),
            &batch,
        )
        .unwrap();
    store.seal_generation(generation_id.as_bytes()).unwrap();
    store
        .activate_generation(generation_id.as_bytes(), 2)
        .unwrap();
    let unit_ids = store
        .generation_units(generation_id.as_bytes())
        .unwrap()
        .into_iter()
        .map(|unit| (unit.source_unit_key, unit.unit_id))
        .collect();
    ImportedGeneration {
        id: generation_id,
        source_hash,
        source_length: byte_length,
        units,
        unit_ids,
    }
}

#[test]
fn import_reextract_bind_and_resume_frozen_export() {
    let temp = TempDir::new().unwrap();
    let objects = temp.path().join("objects");
    let staging = temp.path().join("staging");
    let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();

    let first = import_text(&mut store, &objects, &staging, b"one\ntwo\nthree\n", None);
    assert_eq!(
        store.active_generation().unwrap(),
        Some(*first.id.as_bytes())
    );
    let second = import_text(
        &mut store,
        &objects,
        &staging,
        b"two\none\nthree revised\n",
        Some(&first),
    );
    assert_eq!(
        store.generation_state(first.id.as_bytes()).unwrap(),
        "Retired"
    );
    assert_eq!(
        store.active_generation().unwrap(),
        Some(*second.id.as_bytes())
    );
    assert_eq!(
        second
            .units
            .iter()
            .filter(|unit| first.unit_ids.contains_key(&unit.source_unit_key))
            .count(),
        2
    );
    for (index, unit) in second.units.iter().enumerate() {
        store
            .save_translation(
                &unit.source_unit_key,
                &hash(&[b"translate", &unit.source_unit_key]),
                &format!("译文-{index}"),
                3 + index as i64,
            )
            .unwrap();
    }

    let registry = CapabilityRegistry::new(&objects, &staging).unwrap();
    let adapter = MockTextAdapter::default();
    let cancellation = CancellationToken::default();
    let budget = TaskBudget {
        timeout_ms: 10_000,
        maximum_bytes: 1024 * 1024,
        maximum_nodes: 100,
        page_bytes: 16,
        page_nodes: 1,
    };
    let execution = context(&budget, &cancellation);
    let frozen_commit = store.commit_sequence().unwrap();
    let frozen = store
        .frozen_unit_snapshot(second.id.as_bytes(), frozen_commit)
        .unwrap()
        .into_iter()
        .map(|unit| OverlayUnit {
            source_unit_key: unit.source_unit_key,
            source_locator: serde_json::from_slice::<Locator>(&unit.locator_json).unwrap(),
            translated_text: unit.translation.unwrap_or(unit.source_text),
        })
        .collect::<Vec<_>>();
    let source = registry
        .grant_object(second.source_hash, second.source_length)
        .unwrap();
    let plan = adapter
        .plan_export(&source, second.id, frozen_commit, &frozen, &execution)
        .unwrap();
    let candidate = registry.create_staging().unwrap();
    let first_page = adapter
        .materialize(
            &plan, &source, &frozen, &candidate, None, &registry, &execution,
        )
        .unwrap();
    assert!(!first_page.complete);

    let restarted_adapter = MockTextAdapter::default();
    let mut cursor = first_page.next_cursor;
    while let Some(resume) = cursor {
        let page = restarted_adapter
            .materialize(
                &plan,
                &source,
                &frozen,
                &candidate,
                Some(&resume),
                &registry,
                &execution,
            )
            .unwrap();
        cursor = page.next_cursor;
    }
    let report = restarted_adapter
        .verify_output(&candidate, &registry, &execution)
        .unwrap();
    assert!(report.valid);
    assert_eq!(
        report.byte_length,
        registry.staging_bytes(&candidate).unwrap().len() as u64
    );

    let mut live_overlays = frozen.clone();
    live_overlays[0].translated_text = "导出快照之后的编辑".to_owned();
    assert!(
        restarted_adapter
            .materialize(
                &plan,
                &source,
                &live_overlays,
                &candidate,
                None,
                &registry,
                &execution,
            )
            .is_err()
    );
}
