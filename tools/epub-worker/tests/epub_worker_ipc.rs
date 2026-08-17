use std::{
    io::{Cursor, Write},
    time::Duration,
};

use babel_runtime::process_worker::{ProcessWorker, WorkerCancelToken, WorkerLaunch};
use serde_json::json;
use sha2::{Digest, Sha256};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn epub_fixture() -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    for (name, bytes, method) in [
        (
            "mimetype",
            b"application/epub+zip".as_slice(),
            CompressionMethod::Stored,
        ),
        (
            "META-INF/container.xml",
            br#"<container><rootfiles><rootfile full-path="EPUB/package.opf"/></rootfiles></container>"#.as_slice(),
            CompressionMethod::Deflated,
        ),
        (
            "EPUB/package.opf",
            br#"<package version="3.0"><manifest><item id="c1" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="chapter2.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#.as_slice(),
            CompressionMethod::Deflated,
        ),
        (
            "EPUB/chapter.xhtml",
            br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Title</h1><p>First paragraph.</p><p>Second paragraph.</p></body></html>"#.as_slice(),
            CompressionMethod::Deflated,
        ),
        (
            "EPUB/nav.xhtml",
            br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><nav><a href="chapter.xhtml">Chapter</a><a href="chapter2.xhtml">Chapter 2</a></nav></body></html>"#.as_slice(),
            CompressionMethod::Deflated,
        ),
        (
            "EPUB/chapter2.xhtml",
            br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Another chapter.</p></body></html>"#.as_slice(),
            CompressionMethod::Deflated,
        ),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default().compression_method(method))
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn epub_worker_process_probes_and_extracts_preview() {
    let cancel = WorkerCancelToken::new();
    let mut launch = WorkerLaunch::new(
        env!("CARGO_BIN_EXE_babel-epub-worker"),
        b"epub-worker-test".to_vec(),
    );
    launch.handshake_timeout = Duration::from_secs(2);
    launch.request_timeout = Duration::from_secs(5);
    let mut worker = ProcessWorker::spawn(launch, &cancel).unwrap();

    let source_bytes = epub_fixture();
    let probe = worker
        .request(
            1,
            serde_json::to_vec(&json!({
                "operation": "probe",
                "bytes": source_bytes
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(probe.status, 0, "{}", probe.diagnostic);
    let probe_json: serde_json::Value = serde_json::from_slice(&probe.payload).unwrap();
    assert_eq!(probe_json["detected_media_type"], "application/epub+zip");
    assert_eq!(probe_json["adapter_id"], "org.babel-tower.epub");
    assert_eq!(probe_json["adapter_build"], "phase5.0");
    assert_eq!(probe_json["identity_version"], 1);

    let extract = worker
        .request(
            2,
            serde_json::to_vec(&json!({
                "operation": "extract-preview",
                "bytes": source_bytes,
                "limit": 2
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(extract.status, 0, "{}", extract.diagnostic);
    let extract_json: serde_json::Value = serde_json::from_slice(&extract.payload).unwrap();
    assert!(extract_json["lines"].as_array().unwrap().len() <= 2);
    assert!(
        extract_json["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str().is_some_and(|line| line.contains("Title")))
    );
}

#[test]
fn epub_worker_loads_source_session_and_extracts_pages() {
    let cancel = WorkerCancelToken::new();
    let mut launch = WorkerLaunch::new(
        env!("CARGO_BIN_EXE_babel-epub-worker"),
        b"epub-worker-test".to_vec(),
    );
    launch.handshake_timeout = Duration::from_secs(2);
    launch.request_timeout = Duration::from_secs(5);
    let mut worker = ProcessWorker::spawn(launch, &cancel).unwrap();
    let source = epub_fixture();
    let source_hash: [u8; 32] = Sha256::digest(&source).into();

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
    assert_eq!(begin_json["max_chunk_bytes"], 1024 * 1024);

    for (request_id, (offset, bytes)) in [(0_u64, &source[..8]), (8_u64, &source[8..])]
        .into_iter()
        .enumerate()
    {
        let response = worker
            .request(
                request_id as u64 + 2,
                serde_json::to_vec(&json!({
                    "operation": "load-chunk",
                    "session_id": session_id,
                    "offset": offset,
                    "data_hex": hex::encode(bytes)
                }))
                .unwrap(),
                &cancel,
            )
            .unwrap();
        assert_eq!(response.status, 0, "{}", response.diagnostic);
    }

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

    let generation_id = [7_u8; 16];
    let inventory = worker
        .request(
            5,
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
    let resource_id_for_kind = |kind: &str| {
        inventory_json["page"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item.get("Node"))
            .find(|node| node["kind"].as_str() == Some(kind))
            .and_then(|node| node["resource_id"].as_array())
            .map(|bytes| {
                bytes
                    .iter()
                    .map(|value| value.as_u64().unwrap() as u8)
                    .collect::<Vec<_>>()
            })
            .unwrap()
    };
    let resource_ids_for_kind = |kind: &str| {
        inventory_json["page"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item.get("Node"))
            .filter(|node| node["kind"].as_str() == Some(kind))
            .filter_map(|node| node["resource_id"].as_array())
            .map(|bytes| {
                bytes
                    .iter()
                    .map(|value| value.as_u64().unwrap() as u8)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };

    let document_resource_id = resource_id_for_kind("Document");
    let rejected = worker
        .request(
            6,
            serde_json::to_vec(&json!({
                "operation": "extract-page",
                "session_id": session_id,
                "generation_id": generation_id,
                "resource_id": document_resource_id,
                "cursor": null
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_ne!(rejected.status, 0);

    let resource_ids = resource_ids_for_kind("TextStream");
    assert_eq!(resource_ids.len(), 2);
    for (index, resource_id) in resource_ids.into_iter().enumerate() {
        let extract = worker
            .request(
                7 + index as u64,
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
        assert!(!extract_json["page"]["items"].as_array().unwrap().is_empty());
        assert!(extract_json["worker_peak_rss_kib"].as_u64().is_some());
    }
}

#[test]
fn epub_worker_reports_malformed_request_without_crashing() {
    let cancel = WorkerCancelToken::new();
    let mut launch = WorkerLaunch::new(
        env!("CARGO_BIN_EXE_babel-epub-worker"),
        b"epub-worker-test".to_vec(),
    );
    launch.handshake_timeout = Duration::from_secs(2);
    launch.request_timeout = Duration::from_secs(5);
    let mut worker = ProcessWorker::spawn(launch, &cancel).unwrap();

    let malformed = worker.request(1, b"{".to_vec(), &cancel).unwrap();
    assert_eq!(malformed.status, 1);
    assert!(malformed.diagnostic.contains("decode EPUB worker request"));

    let probe = worker
        .request(
            2,
            serde_json::to_vec(&json!({
                "operation": "probe",
                "bytes": [35, 32, 84, 105, 116, 108, 101]
            }))
            .unwrap(),
            &cancel,
        )
        .unwrap();
    assert_eq!(probe.status, 0, "{}", probe.diagnostic);
}
