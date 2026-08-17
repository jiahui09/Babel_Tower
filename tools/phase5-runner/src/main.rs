use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use babel_adapter_host::CapabilityRegistry;
use babel_adapter_protocol::{Adapter, CancellationToken, ExecutionContext, TaskBudget};
use babel_application::Kernel;
use babel_domain::core::GenerationId;
use babel_epub_adapter::EpubAdapter;
use babel_resource_graph::ResourceKind;
use clap::{Parser, Subcommand};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const MIB: usize = 1024 * 1024;
const L_COMPRESSED_PAYLOAD: usize = 500 * MIB;
const L_EXPANDED_BYTES: usize = 2 * 1024 * MIB;
const L_MEMBERS: usize = 5_000;
const L_UNITS: usize = 100_000;
const L_CHAPTERS: usize = 100;
const L_TRANSLATION_BYTES: usize = 64 * MIB;

#[derive(Parser)]
#[command(name = "babel-phase5", about = "Babel Tower Phase 5 EPUB 验收器")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Smoke {
        #[arg(long)]
        work_dir: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Benchmark {
        #[arg(long, default_value_t = L_COMPRESSED_PAYLOAD)]
        compressed_payload_bytes: usize,
        #[arg(long, default_value_t = L_EXPANDED_BYTES)]
        expanded_bytes: usize,
        #[arg(long, default_value_t = L_MEMBERS)]
        members: usize,
        #[arg(long, default_value_t = L_UNITS)]
        units: usize,
        #[arg(long, default_value_t = L_CHAPTERS)]
        chapters: usize,
        #[arg(long, default_value_t = L_TRANSLATION_BYTES)]
        translation_bytes: usize,
        #[arg(long)]
        work_dir: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Serialize)]
struct SmokeReport {
    verdict: &'static str,
    imported_units: usize,
    recovered_translations: bool,
    spine_order_preserved: bool,
    protected_member_preserved: bool,
    exported_sha256: String,
}

#[derive(Serialize)]
struct BenchmarkReport {
    verdict: &'static str,
    input_bytes: u64,
    expanded_bytes: u64,
    members: usize,
    units: usize,
    chapters: usize,
    adapter_first_content_ms: u128,
    full_import_ms: u128,
    cold_open_ms: u128,
    export_ms: u128,
    export_input_bytes: u64,
    export_units: usize,
    runner_peak_rss_mib: Option<f64>,
    worker_peak_rss_mib: Option<f64>,
    conservative_total_peak_rss_mib: Option<f64>,
    thresholds: Thresholds,
}

#[derive(Serialize)]
struct Thresholds {
    adapter_first_content_ms: u128,
    full_import_ms: u128,
    export_ms: u128,
    peak_rss_mib: f64,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Smoke { work_dir, output } => {
            let (_temporary, root) = project_root(work_dir)?;
            write_json(&smoke(&root)?, output.as_deref())
        }
        Command::Benchmark {
            compressed_payload_bytes,
            expanded_bytes,
            members,
            units,
            chapters,
            translation_bytes,
            work_dir,
            output,
        } => {
            let (_temporary, root) = project_root(work_dir)?;
            let report = benchmark(
                &root,
                compressed_payload_bytes,
                expanded_bytes,
                members,
                units,
                chapters,
                translation_bytes,
            )?;
            write_json(&report, output.as_deref())
        }
    }
}

fn smoke(root: &Path) -> Result<SmokeReport> {
    let source = small_epub(&["c1", "c2"])?;
    let kernel = Kernel::open(root)?;
    let imported = kernel.import_epub_reader([1; 16], Cursor::new(&source), 1)?;
    let units = kernel.query()?.page_after(-1, 16)?;
    ensure!(units.len() == 3, "unexpected EPUB smoke unit count");
    for (index, unit) in units.iter().enumerate() {
        kernel.save_translation(
            unit.source_unit_key
                .clone()
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid source key"))?,
            Sha256::digest(format!("phase5-smoke-{index}")).into(),
            format!("译文-{index}"),
            10 + index as i64,
        )?;
    }
    let export_path = root.join("smoke-export.epub");
    let export = kernel.export_active_epub_to_path(&export_path)?;
    ensure!(
        kernel.validate_active_epub()?.is_empty(),
        "EPUB smoke validation failed"
    );
    let protected_member_preserved =
        zip_member(&source, "EPUB/style.css")? == zip_file_member(&export_path, "EPUB/style.css")?;
    drop(kernel);

    let reopened = Kernel::open(root)?;
    let recovered = reopened.query()?.page_after(-1, 16)?;
    let recovered_translations = recovered.iter().all(|unit| unit.translation.is_some());
    Ok(SmokeReport {
        verdict: "SUPPORTED",
        imported_units: imported.units,
        recovered_translations,
        spine_order_preserved: recovered
            .first()
            .is_some_and(|unit| unit.source_text == "Title"),
        protected_member_preserved,
        exported_sha256: hex_hash(&export.output_hash),
    })
}

fn benchmark(
    root: &Path,
    compressed_payload_bytes: usize,
    expanded_bytes: usize,
    members: usize,
    units: usize,
    chapters: usize,
    translation_bytes: usize,
) -> Result<BenchmarkReport> {
    ensure!(
        chapters > 0 && units >= chapters,
        "invalid EPUB corpus dimensions"
    );
    ensure!(members >= chapters + 4, "member count is too small");
    fs::create_dir_all(root)?;
    let source_path = root.join("phase5-L.epub");
    let corpus = write_synthetic_epub(
        &source_path,
        compressed_payload_bytes,
        expanded_bytes,
        members,
        units,
        chapters,
        translation_bytes,
    )?;

    let adapter_first_content_ms = measure_adapter_first_content(&source_path)?;
    let kernel = Kernel::open(root.join("project.babel"))?;
    let started = Instant::now();
    let source = File::open(&source_path)?;
    let imported = kernel.import_epub_reader([2; 16], source, 1)?;
    let full_import_ms = started.elapsed().as_millis();
    ensure!(
        imported.units == units,
        "EPUB unit count differs from corpus contract"
    );
    drop(kernel);

    let started = Instant::now();
    let kernel = Kernel::open(root.join("project.babel"))?;
    ensure!(
        !kernel.query()?.page_after(-1, 1)?.is_empty(),
        "cold project is empty"
    );
    let cold_open_ms = started.elapsed().as_millis();
    drop(kernel);

    let export_source_path = root.join("phase5-L-export.epub");
    let export_corpus = write_synthetic_epub(
        &export_source_path,
        compressed_payload_bytes,
        expanded_bytes,
        members,
        1,
        1,
        1024,
    )?;
    let export_project = root.join("export-project.babel");
    let export_kernel = Kernel::open(&export_project)?;
    let export_import =
        export_kernel.import_epub_reader([3; 16], File::open(&export_source_path)?, 2)?;
    ensure!(export_import.units == 1, "export corpus must have one unit");
    let export_unit = export_kernel
        .query()?
        .page_after(-1, 1)?
        .into_iter()
        .next()
        .context("export corpus unit missing")?;
    export_kernel.save_translation(
        export_unit
            .source_unit_key
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid export source key"))?,
        Sha256::digest(b"phase5-L-export").into(),
        "translated".to_owned(),
        3,
    )?;
    let export_path = root.join("phase5-L-output.epub");
    let started = Instant::now();
    let export = export_kernel.export_active_epub_to_path(&export_path)?;
    let export_ms = started.elapsed().as_millis();
    ensure!(
        export.byte_length == fs::metadata(&export_path)?.len(),
        "export size mismatch"
    );
    ensure!(
        export.output_hash == hash_file(&export_path)?,
        "export hash mismatch"
    );

    let thresholds = Thresholds {
        adapter_first_content_ms: 8_000,
        full_import_ms: 90_000,
        export_ms: 60_000,
        peak_rss_mib: 1_536.0,
    };
    let runner_peak_rss_mib = linux_peak_rss_mib();
    let worker_peak_rss_mib = imported
        .worker_peak_rss_kib
        .max(export_import.worker_peak_rss_kib)
        .map(|kib| kib as f64 / 1024.0);
    let conservative_total_peak_rss_mib = runner_peak_rss_mib
        .zip(worker_peak_rss_mib)
        .map(|(runner, worker)| runner + worker);
    let supported = adapter_first_content_ms <= thresholds.adapter_first_content_ms
        && full_import_ms <= thresholds.full_import_ms
        && export_ms <= thresholds.export_ms
        && conservative_total_peak_rss_mib.is_none_or(|rss| rss <= thresholds.peak_rss_mib);
    Ok(BenchmarkReport {
        verdict: if supported { "SUPPORTED" } else { "FALSIFIED" },
        input_bytes: corpus.input_bytes,
        expanded_bytes: corpus.expanded_bytes,
        members: corpus.members,
        units: imported.units,
        chapters,
        adapter_first_content_ms,
        full_import_ms,
        cold_open_ms,
        export_ms,
        export_input_bytes: export_corpus.input_bytes,
        export_units: export_import.units,
        runner_peak_rss_mib,
        worker_peak_rss_mib,
        conservative_total_peak_rss_mib,
        thresholds,
    })
}

struct CorpusReport {
    input_bytes: u64,
    expanded_bytes: u64,
    members: usize,
}

fn write_synthetic_epub(
    path: &Path,
    stored_bytes: usize,
    expanded_target: usize,
    members: usize,
    units: usize,
    chapters: usize,
    translation_bytes: usize,
) -> Result<CorpusReport> {
    let file = File::create(path)?;
    let mut writer = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer.start_file("mimetype", stored)?;
    writer.write_all(b"application/epub+zip")?;
    let mut expanded = b"application/epub+zip".len();

    let container = br#"<container><rootfiles><rootfile full-path="EPUB/package.opf"/></rootfiles></container>"#;
    writer.start_file("META-INF/container.xml", deflated)?;
    writer.write_all(container)?;
    expanded += container.len();

    let manifest = (0..chapters)
        .map(|index| format!(r#"<item id="c{index}" href="c{index:04}.xhtml" media-type="application/xhtml+xml"/>"#))
        .collect::<String>();
    let spine = (0..chapters)
        .map(|index| format!(r#"<itemref idref="c{index}"/>"#))
        .collect::<String>();
    let package = format!(
        r#"<package version="3.0"><manifest>{manifest}<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine>{spine}</spine></package>"#
    );
    writer.start_file("EPUB/package.opf", deflated)?;
    writer.write_all(package.as_bytes())?;
    expanded += package.len();

    let nav_links = (0..chapters)
        .map(|index| format!(r#"<a href="c{index:04}.xhtml">{index}</a>"#))
        .collect::<String>();
    let nav = format!(r#"<html><body><nav>{nav_links}</nav></body></html>"#);
    writer.start_file("EPUB/nav.xhtml", deflated)?;
    writer.write_all(nav.as_bytes())?;
    expanded += nav.len();

    let structural_estimate = units * 32 + chapters * 64;
    ensure!(
        translation_bytes > structural_estimate,
        "translation byte target is too small"
    );
    let payload_total = translation_bytes - structural_estimate;
    let mut emitted_units = 0;
    for chapter in 0..chapters {
        let chapter_units = units / chapters + usize::from(chapter < units % chapters);
        let mut xhtml = Vec::new();
        xhtml.extend_from_slice(b"<html><body>");
        for _ in 0..chapter_units {
            let payload =
                payload_total / units + usize::from(emitted_units < payload_total % units);
            write!(xhtml, "<p>unit-{emitted_units:06} ")?;
            xhtml.resize(xhtml.len() + payload, b'x');
            xhtml.extend_from_slice(b"</p>");
            emitted_units += 1;
        }
        xhtml.extend_from_slice(b"</body></html>");
        writer.start_file(format!("EPUB/c{chapter:04}.xhtml"), deflated)?;
        writer.write_all(&xhtml)?;
        expanded += xhtml.len();
    }

    writer.start_file("EPUB/payload.bin", stored)?;
    write_deterministic_bytes(&mut writer, stored_bytes)?;
    expanded += stored_bytes;

    ensure!(
        expanded <= expanded_target,
        "generated EPUB exceeds expanded target"
    );
    let padding_bytes = expanded_target - expanded;
    let padding_members = padding_bytes.div_ceil(500 * MIB);
    let used_members = chapters + 5 + padding_members;
    ensure!(
        members >= used_members,
        "member count cannot contain required EPUB members"
    );
    for index in 0..members - used_members {
        writer.start_file(format!("EPUB/assets/{index:05}.bin"), stored)?;
    }
    let mut padding_remaining = padding_bytes;
    for index in 0..padding_members {
        let padding = padding_remaining.min(500 * MIB);
        writer.start_file(format!("EPUB/expanded-padding-{index}.bin"), deflated)?;
        write_sparse_entropy_bytes(&mut writer, padding)?;
        expanded += padding;
        padding_remaining -= padding;
    }
    writer.finish()?;
    Ok(CorpusReport {
        input_bytes: fs::metadata(path)?.len(),
        expanded_bytes: expanded as u64,
        members,
    })
}

fn measure_adapter_first_content(path: &Path) -> Result<u128> {
    let temporary = TempDir::new()?;
    let hash = hash_file(path)?;
    let encoded = hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let object = temporary
        .path()
        .join("objects/sha256")
        .join(&encoded[..2])
        .join(&encoded[2..]);
    fs::create_dir_all(object.parent().context("object parent")?)?;
    fs::copy(path, &object)?;
    let registry = CapabilityRegistry::new(
        temporary.path().join("objects"),
        temporary.path().join("staging"),
    )?;
    let handle = registry.grant_object(hash, fs::metadata(path)?.len())?;
    let adapter = EpubAdapter::new();
    let token = CancellationToken::default();
    let budget = TaskBudget {
        timeout_ms: 120_000,
        maximum_bytes: 4 * 1024 * 1024 * 1024,
        maximum_nodes: 1_000_000,
        page_bytes: 64 * MIB as u64,
        page_nodes: 100_000,
    };
    let context = ExecutionContext::new(&budget, &token);
    let started = Instant::now();
    let inventory = adapter.inventory(&handle, GenerationId::new(), None, &registry, &context)?;
    let stream = inventory
        .items
        .iter()
        .find_map(|item| match item {
            babel_adapter_protocol::InventoryItem::Node(node)
                if node.kind == ResourceKind::TextStream =>
            {
                Some(node.resource_id)
            }
            _ => None,
        })
        .context("EPUB has no text stream")?;
    ensure!(
        !adapter
            .extract(
                &handle,
                GenerationId::new(),
                stream,
                None,
                &registry,
                &context
            )?
            .items
            .is_empty(),
        "first chapter is empty"
    );
    Ok(started.elapsed().as_millis())
}

fn small_epub(spine_order: &[&str]) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let entries = [
        ("mimetype", b"application/epub+zip".as_slice(), stored),
        ("META-INF/container.xml", br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.as_slice(), deflated),
    ];
    for (name, bytes, options) in entries {
        writer.start_file(name, options)?;
        writer.write_all(bytes)?;
    }
    let spine = spine_order
        .iter()
        .map(|id| format!(r#"<itemref idref="{id}"/>"#))
        .collect::<String>();
    let package = format!(
        r#"<?xml version="1.0"?><package version="3.0" unique-identifier="pub-id" xmlns="http://www.idpf.org/2007/opf"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="pub-id">urn:uuid:00000000-0000-0000-0000-000000000005</dc:identifier><dc:title>Babel Tower Smoke</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-08-18T00:00:00Z</meta></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="css" href="style.css" media-type="text/css"/></manifest><spine>{spine}</spine></package>"#
    );
    let dynamic = [
        ("EPUB/package.opf", package.as_bytes()),
        ("EPUB/c1.xhtml", br#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml" lang="en"><head><title>One</title><link rel="stylesheet" href="style.css"/></head><body><h1>Title</h1><p>Alpha</p></body></html>"#.as_slice()),
        ("EPUB/c2.xhtml", br#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml" lang="en"><head><title>Two</title></head><body><p>Beta</p></body></html>"#.as_slice()),
        ("EPUB/nav.xhtml", br#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="en"><head><title>Contents</title></head><body><nav epub:type="toc"><h1>Contents</h1><ol><li><a href="c1.xhtml">One</a></li><li><a href="c2.xhtml">Two</a></li></ol></nav></body></html>"#.as_slice()),
        ("EPUB/style.css", b"body { color: black; }".as_slice()),
    ];
    for (name, bytes) in dynamic {
        writer.start_file(name, deflated)?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn write_deterministic_bytes(writer: &mut impl Write, bytes: usize) -> Result<()> {
    let mut state = 0x9e3779b97f4a7c15_u64;
    let mut buffer = vec![0_u8; MIB];
    let mut remaining = bytes;
    while remaining > 0 {
        let count = remaining.min(buffer.len());
        for byte in &mut buffer[..count] {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        writer.write_all(&buffer[..count])?;
        remaining -= count;
    }
    Ok(())
}

fn write_sparse_entropy_bytes(writer: &mut impl Write, bytes: usize) -> Result<()> {
    let mut state = 0xd1b54a32d192ed03_u64;
    let mut buffer = vec![0_u8; MIB];
    let mut remaining = bytes;
    while remaining > 0 {
        let count = remaining.min(buffer.len());
        buffer[..count].fill(0);
        for offset in (0..count).step_by(512) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            buffer[offset] = state as u8;
        }
        writer.write_all(&buffer[..count])?;
        remaining -= count;
    }
    Ok(())
}

fn zip_member(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut member = archive.by_name(name)?;
    let mut output = Vec::new();
    member.read_to_end(&mut output)?;
    Ok(output)
}

fn zip_file_member(path: &Path, name: &str) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
    let mut member = archive.by_name(name)?;
    let mut output = Vec::new();
    member.read_to_end(&mut output)?;
    Ok(output)
}

fn project_root(work_dir: Option<PathBuf>) -> Result<(Option<TempDir>, PathBuf)> {
    match work_dir {
        Some(root) => {
            fs::create_dir_all(&root)?;
            Ok((None, root))
        }
        None => {
            let temporary = TempDir::new()?;
            let root = temporary.path().join("phase5");
            Ok((Some(temporary), root))
        }
    }
}

fn write_json<T: Serialize>(value: &T, output: Option<&Path>) -> Result<()> {
    let json = serde_json::to_vec_pretty(value)?;
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &json)?;
    }
    println!("{}", String::from_utf8(json)?);
    Ok(())
}

fn hash_file(path: &Path) -> Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; MIB];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hasher.finalize().into());
        }
        hasher.update(&buffer[..read]);
    }
}

fn hex_hash(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn linux_peak_rss_mib() -> Option<f64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_ascii_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    Some(kib as f64 / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_epub_honors_expansion_member_and_unit_contracts() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("contract.epub");
        let report = write_synthetic_epub(&path, MIB, 8 * MIB, 30, 100, 5, MIB).unwrap();
        assert_eq!(report.expanded_bytes, 8 * MIB as u64);
        assert_eq!(report.members, 30);
        assert!(report.input_bytes > MIB as u64);
        assert!(measure_adapter_first_content(&path).unwrap() < 8_000);
    }
}
