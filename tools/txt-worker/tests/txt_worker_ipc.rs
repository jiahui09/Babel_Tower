use std::time::Duration;

use babel_runtime::process_worker::{ProcessWorker, WorkerCancelToken, WorkerLaunch};
use serde_json::json;
use sha2::{Digest, Sha256};

#[test]
fn txt_worker_process_probes_and_extracts_preview() {
    let cancel = WorkerCancelToken::new();
    let mut launch = WorkerLaunch::new(
        env!("CARGO_BIN_EXE_babel-txt-worker"),
        b"txt-worker-test".to_vec(),
    );
    launch.handshake_timeout = Duration::from_secs(2);
    launch.request_timeout = Duration::from_secs(5);
    let mut worker = ProcessWorker::spawn(launch, &cancel).unwrap();

    let probe = worker
        .request(
            1,
            serde_json::to_vec(&json!({
                "operation": "probe",
                "bytes": [111, 110, 101, 10, 116, 119, 111]
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(probe.status, 0, "{}", probe.diagnostic);
    let probe_json: serde_json::Value = serde_json::from_slice(&probe.payload).unwrap();
    assert_eq!(probe_json["detected_media_type"], "text/plain");
    assert_eq!(probe_json["reason_code"], "strict-utf8");
    assert_eq!(probe_json["adapter_id"], "org.babel-tower.txt");
    assert_eq!(probe_json["adapter_build"], "phase3.2");
    assert_eq!(probe_json["identity_version"], 1);

    let extract = worker
        .request(
            2,
            serde_json::to_vec(&json!({
                "operation": "extract-preview",
                "bytes": [111, 110, 101, 10, 116, 119, 111],
                "limit": 2
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(extract.status, 0, "{}", extract.diagnostic);
    let extract_json: serde_json::Value = serde_json::from_slice(&extract.payload).unwrap();
    assert_eq!(extract_json["lines"], json!(["one", "two"]));
}

#[test]
fn txt_worker_loads_source_session_and_extracts_pages() {
    let cancel = WorkerCancelToken::new();
    let mut launch = WorkerLaunch::new(
        env!("CARGO_BIN_EXE_babel-txt-worker"),
        b"txt-worker-test".to_vec(),
    );
    launch.handshake_timeout = Duration::from_secs(2);
    launch.request_timeout = Duration::from_secs(5);
    let mut worker = ProcessWorker::spawn(launch, &cancel).unwrap();
    let source = b"one\ntwo\nthree\n";
    let source_hash: [u8; 32] = Sha256::digest(source).into();

    let begin = worker
        .request(
            1,
            serde_json::to_vec(&json!({
                "operation": "load-begin",
                "source_hash_hex": hex::encode(source_hash),
                "byte_length": source.len()
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(begin.status, 0, "{}", begin.diagnostic);
    let begin_json: serde_json::Value = serde_json::from_slice(&begin.payload).unwrap();
    let session_id = begin_json["session_id"].as_u64().unwrap();

    let first = worker
        .request(
            2,
            serde_json::to_vec(&json!({
                "operation": "load-chunk",
                "session_id": session_id,
                "offset": 0,
                "data_hex": hex::encode(&source[..4])
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(first.status, 0, "{}", first.diagnostic);

    let second = worker
        .request(
            3,
            serde_json::to_vec(&json!({
                "operation": "load-chunk",
                "session_id": session_id,
                "offset": 4,
                "data_hex": hex::encode(&source[4..])
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(second.status, 0, "{}", second.diagnostic);

    let finish = worker
        .request(
            4,
            serde_json::to_vec(&json!({
                "operation": "load-finish",
                "session_id": session_id
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(finish.status, 0, "{}", finish.diagnostic);

    let probe = worker
        .request(
            5,
            serde_json::to_vec(&json!({
                "operation": "probe-loaded",
                "session_id": session_id
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(probe.status, 0, "{}", probe.diagnostic);

    let generation_id = [7_u8; 16];
    let inventory = worker
        .request(
            6,
            serde_json::to_vec(&json!({
                "operation": "inventory-page",
                "session_id": session_id,
                "generation_id": generation_id,
                "cursor": null
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(inventory.status, 0, "{}", inventory.diagnostic);
    let inventory_json: serde_json::Value = serde_json::from_slice(&inventory.payload).unwrap();
    let resource_id = inventory_json["page"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|item| item.get("Node"))
        .and_then(|node| node["resource_id"].as_array())
        .map(|bytes| {
            bytes
                .iter()
                .map(|value| value.as_u64().unwrap() as u8)
                .collect::<Vec<_>>()
        })
        .unwrap();

    let extract = worker
        .request(
            7,
            serde_json::to_vec(&json!({
                "operation": "extract-page",
                "session_id": session_id,
                "generation_id": generation_id,
                "resource_id": resource_id,
                "cursor": null
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(extract.status, 0, "{}", extract.diagnostic);
    let extract_json: serde_json::Value = serde_json::from_slice(&extract.payload).unwrap();
    assert_eq!(extract_json["page"]["items"].as_array().unwrap().len(), 3);
}

#[test]
fn txt_worker_reports_malformed_request_without_crashing() {
    let cancel = WorkerCancelToken::new();
    let mut launch = WorkerLaunch::new(
        env!("CARGO_BIN_EXE_babel-txt-worker"),
        b"txt-worker-test".to_vec(),
    );
    launch.handshake_timeout = Duration::from_secs(2);
    launch.request_timeout = Duration::from_secs(5);
    let mut worker = ProcessWorker::spawn(launch, &cancel).unwrap();

    let malformed = worker.request(1, b"{".to_vec(), &cancel).unwrap();
    assert_eq!(malformed.status, 1);
    assert!(malformed.diagnostic.contains("decode TXT worker request"));

    let probe = worker
        .request(
            2,
            serde_json::to_vec(&json!({
                "operation": "probe",
                "bytes": [111, 110, 101]
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(probe.status, 0, "{}", probe.diagnostic);
}
