use anyhow::{Context, Result, bail, ensure};
use babel_domain::identity::{BindingKind, SourceUnit, rebind};
use babel_runtime::{dag, ipc};
use babel_storage::{
    project::{ProjectStore, configure_connection},
    recovery::{self, CrashPoint},
};
use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tempfile::TempDir;
use walkdir::WalkDir;

const DEFAULT_UNITS: usize = 100_000;
const DEFAULT_SAVES: usize = 1_000;
const PACKAGE_SCOPE: &str = "phase3-txt-vertical-slice";

#[derive(Parser)]
#[command(name = "babel-phase0", about = "Babel Tower Phase 0 架构证伪运行器")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    All {
        #[arg(long, default_value_t=DEFAULT_UNITS)]
        units: usize,
        #[arg(long, default_value_t=DEFAULT_SAVES)]
        saves: usize,
        #[arg(long)]
        release_dir: Option<PathBuf>,
        #[arg(long)]
        work_dir: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Sqlite {
        #[arg(long, default_value_t=DEFAULT_UNITS)]
        units: usize,
        #[arg(long, default_value_t=DEFAULT_SAVES)]
        saves: usize,
        #[arg(long)]
        work_dir: Option<PathBuf>,
    },
    Identity {
        #[arg(long, default_value_t=DEFAULT_UNITS)]
        units: usize,
        #[arg(long, default_value_t = 100)]
        duplicate_pairs: usize,
    },
    DagIpc {
        #[arg(long, default_value_t = 2_000)]
        artifacts: usize,
        #[arg(long, default_value_t = 1_000)]
        roundtrips: usize,
        #[arg(long, default_value_t = 4_096)]
        payload_bytes: usize,
    },
    Recovery,
    PackageClosure {
        #[arg(long)]
        release_dir: PathBuf,
    },
    #[command(hide = true)]
    RecoveryChild {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        export_id: i64,
        #[arg(long, value_enum)]
        crash_point: CrashPointArg,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum CrashPointArg {
    Preparing,
    CandidateWrite,
    CandidateSync,
    PublishIntent,
    FinalRename,
    Published,
}
impl CrashPointArg {
    fn point(self) -> CrashPoint {
        match self {
            Self::Preparing => CrashPoint::AfterPreparing,
            Self::CandidateWrite => CrashPoint::AfterCandidateWrite,
            Self::CandidateSync => CrashPoint::AfterCandidateSync,
            Self::PublishIntent => CrashPoint::AfterPublishIntent,
            Self::FinalRename => CrashPoint::AfterFinalRename,
            Self::Published => CrashPoint::AfterPublished,
        }
    }
}

#[derive(Serialize)]
struct Phase0Report {
    generated_at_epoch_ms: u128,
    platform: String,
    sqlite: SqliteReport,
    identity: IdentityReport,
    dag_ipc: DagIpcReport,
    recovery: RecoveryReport,
    package_closure: PackageClosureReport,
}
#[derive(Serialize)]
struct Latencies {
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    raw_us: Vec<u64>,
}
#[derive(Serialize)]
struct SqliteReport {
    verdict: &'static str,
    units: usize,
    saves: usize,
    sqlite_version: String,
    database_parent: String,
    pragmas: BTreeMap<String, String>,
    import_ms: u128,
    initial_fts_rebuild_ms: u128,
    save: Latencies,
    keyset_page: Latencies,
    fts_search: Latencies,
    deferred_search_flush_ms: u128,
    deferred_search_rows: usize,
    active_reader_checkpoint: babel_storage::project::CheckpointResult,
    wal_bytes_with_reader: u64,
    final_checkpoint: babel_storage::project::CheckpointResult,
    wal_bytes_after_reader: u64,
    integrity_check: String,
    thresholds: BTreeMap<&'static str, &'static str>,
}
#[derive(Serialize)]
struct IdentityReport {
    verdict: &'static str,
    units: usize,
    exact: usize,
    shifted: usize,
    ambiguous: usize,
    orphaned: usize,
    false_bindings: usize,
    duplicate_pairs: usize,
    duplicate_auto_bindings: usize,
    meaningful_whitespace_auto_bindings: usize,
    elapsed_ms: u128,
}
#[derive(Serialize)]
struct DagIpcReport {
    verdict: &'static str,
    persistent_artifacts: usize,
    dependency_edges: usize,
    dag_elapsed_ms: u128,
    downstream_blocked_before_upstream_ready: bool,
    stale_fence_rejected: bool,
    ready_after_reopen: bool,
    ipc_transport: &'static str,
    ipc_protocol: String,
    ipc_roundtrips: usize,
    ipc_payload_bytes: usize,
    ipc_elapsed_ms: f64,
    ipc_average_us: f64,
    frame_limit_bytes: usize,
}
#[derive(Serialize)]
struct RecoveryCase {
    crash_point: String,
    child_exit_code: Option<i32>,
    recovered_state: String,
    output_present: bool,
    output_hash_valid: bool,
}
#[derive(Serialize)]
struct RecoveryReport {
    verdict: &'static str,
    cases: Vec<RecoveryCase>,
    corrupted_published_output_rejected: bool,
}
#[derive(Debug, Deserialize)]
struct ReleaseManifest {
    schema_version: u32,
    scope: String,
    runner_kind: Option<String>,
    platform: String,
    artifact: String,
    artifact_sha256: String,
    bundled_components: Vec<String>,
    packaged_dependencies: Vec<String>,
    allowed_external_dependencies: Vec<String>,
    static_dependencies: Vec<String>,
    runtime_dependencies: Vec<String>,
    clean_image: CleanImageEvidence,
    windows: Option<WindowsEvidence>,
}
#[derive(Debug, Deserialize)]
struct CleanImageEvidence {
    performed: bool,
    network_blocked_before_install: bool,
    installed: bool,
    launched: bool,
    component_probes: Vec<String>,
    network_attempts: u64,
}
#[derive(Debug, Deserialize)]
struct WindowsEvidence {
    webview_install_mode: String,
}
#[derive(Serialize)]
struct PackagePlatformReport {
    platform: String,
    verdict: &'static str,
    artifact: Option<String>,
    artifact_hash_valid: bool,
    unknown_dependencies: Vec<String>,
    missing_components: Vec<String>,
    clean_image_verified: bool,
    reasons: Vec<String>,
}
#[derive(Serialize)]
struct PackageClosureReport {
    verdict: &'static str,
    scope: &'static str,
    production_bundle_verdict: &'static str,
    production_bundle_reasons: Vec<&'static str>,
    release_dir: Option<String>,
    platforms: Vec<PackagePlatformReport>,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::All {
            units,
            saves,
            release_dir,
            work_dir,
            output,
        } => write_json(
            &Phase0Report {
                generated_at_epoch_ms: now_ms(),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
                sqlite: sqlite_probe(units, saves, work_dir.as_deref())?,
                identity: identity_probe(units, 100),
                dag_ipc: dag_ipc_probe(2000, 1000, 4096)?,
                recovery: recovery_probe()?,
                package_closure: package_closure(release_dir.as_deref())?,
            },
            output.as_deref(),
        )?,
        Command::Sqlite {
            units,
            saves,
            work_dir,
        } => write_json(&sqlite_probe(units, saves, work_dir.as_deref())?, None)?,
        Command::Identity {
            units,
            duplicate_pairs,
        } => write_json(&identity_probe(units, duplicate_pairs), None)?,
        Command::DagIpc {
            artifacts,
            roundtrips,
            payload_bytes,
        } => write_json(&dag_ipc_probe(artifacts, roundtrips, payload_bytes)?, None)?,
        Command::Recovery => write_json(&recovery_probe()?, None)?,
        Command::PackageClosure { release_dir } => {
            write_json(&package_closure(Some(&release_dir))?, None)?
        }
        Command::RecoveryChild {
            root,
            export_id,
            crash_point,
        } => {
            recovery::run_export_with_hook(
                &root,
                export_id,
                b"Babel Tower exported translation",
                |point| {
                    if point == crash_point.point() {
                        std::process::exit(77)
                    }
                },
            )?;
            bail!("recovery child did not terminate")
        }
    }
    Ok(())
}

fn sqlite_probe(units: usize, saves: usize, work_dir: Option<&Path>) -> Result<SqliteReport> {
    ensure!(units > 0 && saves > 0, "units and saves must be positive");
    let temp = if let Some(work_dir) = work_dir {
        fs::create_dir_all(work_dir)?;
        TempDir::new_in(work_dir)?
    } else {
        TempDir::new()?
    };
    let db = temp.path().join("project.sqlite3");
    let wal = temp.path().join("project.sqlite3-wal");
    let mut store = ProjectStore::open(&db)?;
    let sqlite_version = store
        .connection()
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let pragmas = read_pragmas(store.connection())?;
    let t = Instant::now();
    store.seed_units(units)?;
    let import_ms = t.elapsed().as_millis();
    let t = Instant::now();
    store.rebuild_search()?;
    let initial_fts_rebuild_ms = t.elapsed().as_millis();
    let mut page = Vec::new();
    for i in 0..100 {
        let t = Instant::now();
        let _ = store.page_after(((i * 997) % units) as i64 - 1, 80)?;
        page.push(us(t));
    }
    let mut search = Vec::new();
    for _ in 0..100 {
        let t = Instant::now();
        let _ = store.search("Babel", 50)?;
        search.push(us(t));
    }
    store.checkpoint_truncate()?;
    let source_keys = store
        .page_after(-1, units)?
        .into_iter()
        .map(|unit| <[u8; 32]>::try_from(unit.source_unit_key))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| anyhow::anyhow!("stored source_unit_key must be 32 bytes"))?;
    let reader = Connection::open(&db)?;
    configure_connection(&reader)?;
    reader.execute_batch("BEGIN")?;
    let _: i64 = reader.query_row("SELECT count(*) FROM unit", [], |row| row.get(0))?;
    let mut saves_us = Vec::with_capacity(saves);
    for i in 0..saves {
        let id: [u8; 32] = Sha256::digest(format!("save-{i}")).into();
        let t = Instant::now();
        store.save_translation(
            &source_keys[i % units],
            &id,
            &format!("人工译文 {i}"),
            i as i64,
        )?;
        saves_us.push(us(t));
    }
    let active = store.checkpoint_passive()?;
    let wal_with = file_size_or_zero(&wal)?;
    reader.execute_batch("COMMIT")?;
    drop(reader);
    let final_cp = store.checkpoint_truncate()?;
    let wal_after = file_size_or_zero(&wal)?;
    let t = Instant::now();
    let deferred = store.flush_search_dirty(saves.max(1))?;
    let flush_ms = t.elapsed().as_millis();
    let integrity: String = store
        .connection()
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    let save = latencies(saves_us);
    let keyset_page = latencies(page);
    let fts_search = latencies(search);
    let pass = save.p99_ms <= 20.0
        && keyset_page.p95_ms <= 50.0
        && fts_search.p95_ms <= 150.0
        && integrity == "ok"
        && final_cp.busy == 0
        && wal_after == 0;
    Ok(SqliteReport {
        verdict: verdict(pass),
        units,
        saves,
        sqlite_version,
        database_parent: temp.path().display().to_string(),
        pragmas,
        import_ms,
        initial_fts_rebuild_ms,
        save,
        keyset_page,
        fts_search,
        deferred_search_flush_ms: flush_ms,
        deferred_search_rows: deferred,
        active_reader_checkpoint: active,
        wal_bytes_with_reader: wal_with,
        final_checkpoint: final_cp,
        wal_bytes_after_reader: wal_after,
        integrity_check: integrity,
        thresholds: BTreeMap::from([
            ("durable_save_p99", "<= 20 ms"),
            ("keyset_page_p95", "<= 50 ms"),
            ("fts_search_p95", "<= 150 ms"),
            ("integrity_check", "ok"),
            ("post_reader_wal", "0 bytes after TRUNCATE checkpoint"),
        ]),
    })
}

fn identity_probe(units: usize, duplicate_pairs: usize) -> IdentityReport {
    let old = (0..units)
        .map(|i| unit(i, i, &format!("固定语料第 {i} 条")))
        .collect::<Vec<_>>();
    let new = (0..units)
        .rev()
        .map(|i| unit(i, units - i, &format!("固定语料第 {i} 条")))
        .collect::<Vec<_>>();
    let expected = old
        .iter()
        .map(|u| (u.source_key, u.normalized_text.clone()))
        .collect::<HashMap<_, _>>();
    let new_by_text = new
        .iter()
        .map(|u| (u.normalized_text.clone(), u.source_key))
        .collect::<HashMap<_, _>>();
    let t = Instant::now();
    let bindings = rebind(&old, &new);
    let mut count = [0; 4];
    let mut false_bindings = 0;
    for b in &bindings {
        count[idx(b.kind)] += 1;
        if let Some(bound) = b.new_source_key {
            let text = expected.get(&b.old_source_key).unwrap();
            if new_by_text.get(text) != Some(&bound) {
                false_bindings += 1;
            }
        }
    }
    let duplicate_old = (0..duplicate_pairs * 2)
        .map(|i| unit(i, i, &format!("重复句 {}", i / 2)))
        .collect::<Vec<_>>();
    let duplicate_new = (0..duplicate_pairs * 2)
        .map(|i| unit(i, i + 10000, &format!("重复句 {}", i / 2)))
        .collect::<Vec<_>>();
    let duplicate_auto_bindings = rebind(&duplicate_old, &duplicate_new)
        .iter()
        .filter(|b| b.new_source_key.is_some())
        .count();
    let whitespace_old = [unit(0, 0, "第一行  \n第二行")];
    let whitespace_new = [unit(0, 0, "第一行\n第二行")];
    let meaningful_whitespace_auto_bindings = rebind(&whitespace_old, &whitespace_new)
        .iter()
        .filter(|binding| binding.new_source_key.is_some())
        .count();
    let pass = false_bindings == 0
        && duplicate_auto_bindings == 0
        && meaningful_whitespace_auto_bindings == 0
        && count[0] + count[1] == units;
    IdentityReport {
        verdict: verdict(pass),
        units,
        exact: count[0],
        shifted: count[1],
        ambiguous: count[2],
        orphaned: count[3],
        false_bindings,
        duplicate_pairs,
        duplicate_auto_bindings,
        meaningful_whitespace_auto_bindings,
        elapsed_ms: t.elapsed().as_millis(),
    }
}
fn unit(_content_index: usize, path_index: usize, text: &str) -> SourceUnit {
    SourceUnit::new(
        "markdown",
        "chapter.md",
        vec![format!("p-{path_index}")],
        text,
        None,
        None,
    )
}
fn idx(k: BindingKind) -> usize {
    match k {
        BindingKind::Exact => 0,
        BindingKind::Shifted => 1,
        BindingKind::Ambiguous => 2,
        BindingKind::Orphaned => 3,
    }
}

fn dag_ipc_probe(artifacts: usize, roundtrips: usize, payload: usize) -> Result<DagIpcReport> {
    let temp = TempDir::new()?;
    let db = temp.path().join("dag.sqlite3");
    let mut c = Connection::open(&db)?;
    dag::initialize(&c)?;
    let keys = (0..artifacts)
        .map(|i| <[u8; 32]>::from(Sha256::digest(format!("artifact-{i}"))))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        dag::register(&c, key, index as i64)?;
        if index > 0 {
            dag::add_dependency(&c, key, &keys[index - 1])?;
        }
    }
    let downstream_blocked_before_upstream_ready = artifacts < 2
        || matches!(
            dag::claim(&mut c, keys.last().unwrap(), "early-worker", 0, 10)?,
            dag::Claim::Blocked { .. }
        );
    let t = Instant::now();
    for (i, key) in keys.iter().enumerate() {
        let out: [u8; 32] = Sha256::digest(format!("output-{i}")).into();
        let dag::Claim::Acquired { fencing_token } =
            dag::claim(&mut c, key, "worker-a", i as i64, 10000)?
        else {
            bail!("fresh claim failed")
        };
        dag::publish(&c, key, "worker-a", fencing_token, &out, i as i64 + 1)?;
    }
    let dag_ms = t.elapsed().as_millis();
    let key = [9; 32];
    let old = match dag::claim(&mut c, &key, "old", 0, 1)? {
        dag::Claim::Acquired { fencing_token } => fencing_token,
        _ => bail!("old claim failed"),
    };
    let new = match dag::claim(&mut c, &key, "new", 2, 10)? {
        dag::Claim::Acquired { fencing_token } => fencing_token,
        _ => bail!("new claim failed"),
    };
    let stale = dag::publish(&c, &key, "old", old, &[1; 32], 3).is_err();
    dag::publish(&c, &key, "new", new, &[2; 32], 4)?;
    drop(c);
    let mut reopened = Connection::open(&db)?;
    dag::initialize(&reopened)?;
    let ready = matches!(
        dag::claim(&mut reopened, &key, "reader", 5, 10)?,
        dag::Claim::Ready { .. }
    );
    let ipc_time = ipc::run_local_probe(roundtrips, payload)?;
    let pass =
        stale && ready && downstream_blocked_before_upstream_ready && ipc_time.as_secs() < 10;
    Ok(DagIpcReport {
        verdict: verdict(pass),
        persistent_artifacts: artifacts,
        dependency_edges: artifacts.saturating_sub(1),
        dag_elapsed_ms: dag_ms,
        downstream_blocked_before_upstream_ready,
        stale_fence_rejected: stale,
        ready_after_reopen: ready,
        ipc_transport: if cfg!(windows) {
            "Windows named pipe"
        } else {
            "Unix namespaced local socket"
        },
        ipc_protocol: format!("protobuf {}.{}", ipc::PROTOCOL_MAJOR, ipc::PROTOCOL_MINOR),
        ipc_roundtrips: roundtrips,
        ipc_payload_bytes: payload,
        ipc_elapsed_ms: ipc_time.as_secs_f64() * 1000.0,
        ipc_average_us: ipc_time.as_secs_f64() * 1_000_000.0 / roundtrips.max(1) as f64,
        frame_limit_bytes: ipc::MAX_FRAME_BYTES,
    })
}

fn recovery_probe() -> Result<RecoveryReport> {
    let exe = std::env::current_exe()?;
    let points = [
        CrashPointArg::Preparing,
        CrashPointArg::CandidateWrite,
        CrashPointArg::CandidateSync,
        CrashPointArg::PublishIntent,
        CrashPointArg::FinalRename,
        CrashPointArg::Published,
    ];
    let mut cases = Vec::new();
    for (i, p) in points.into_iter().enumerate() {
        let temp = TempDir::new()?;
        let id = i as i64 + 1;
        let status = ProcessCommand::new(&exe)
            .args(["recovery-child", "--root"])
            .arg(temp.path())
            .args([
                "--export-id",
                &id.to_string(),
                "--crash-point",
                point_name(p),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        let outcome = recovery::recover(temp.path(), id)?;
        let output = temp.path().join("exports").join(format!("{id}.bin"));
        let present = output.exists();
        let valid = present && fs::read(&output)? == b"Babel Tower exported translation";
        cases.push(RecoveryCase {
            crash_point: point_name(p).to_owned(),
            child_exit_code: status.code(),
            recovered_state: outcome.final_state,
            output_present: present,
            output_hash_valid: valid,
        });
    }
    let corrupt = TempDir::new()?;
    recovery::run_export_with_hook(corrupt.path(), 99, b"good", |_| {})?;
    fs::write(corrupt.path().join("exports/99.bin"), b"corrupt")?;
    let rejected = recovery::recover(corrupt.path(), 99).is_err();
    let pass = cases.iter().all(|c| {
        c.child_exit_code == Some(77)
            && match c.crash_point.as_str() {
                "preparing" | "candidate-write" | "candidate-sync" => {
                    c.recovered_state == "CancelledAfterCrash" && !c.output_present
                }
                _ => c.recovered_state == "Published" && c.output_hash_valid,
            }
    }) && rejected;
    Ok(RecoveryReport {
        verdict: verdict(pass),
        cases,
        corrupted_published_output_rejected: rejected,
    })
}
fn point_name(p: CrashPointArg) -> &'static str {
    match p {
        CrashPointArg::Preparing => "preparing",
        CrashPointArg::CandidateWrite => "candidate-write",
        CrashPointArg::CandidateSync => "candidate-sync",
        CrashPointArg::PublishIntent => "publish-intent",
        CrashPointArg::FinalRename => "final-rename",
        CrashPointArg::Published => "published",
    }
}

fn package_closure(dir: Option<&Path>) -> Result<PackageClosureReport> {
    let Some(dir) = dir else {
        return Ok(PackageClosureReport {
            verdict: "BLOCKED",
            scope: PACKAGE_SCOPE,
            production_bundle_verdict: "FALSIFIED",
            production_bundle_reasons: production_bundle_reasons(),
            release_dir: None,
            platforms: Vec::new(),
        });
    };
    let mut platforms = Vec::new();
    for e in WalkDir::new(dir).max_depth(2) {
        let e = e?;
        if e.file_name() == "release-manifest.json" {
            platforms.push(verify_manifest(e.path())?);
        }
    }
    for (platform, prefix) in [
        ("linux-arch-x86_64", "linux"),
        ("windows-x86_64", "windows"),
    ] {
        if !platforms
            .iter()
            .any(|report| report.platform.starts_with(prefix))
        {
            platforms.push(PackagePlatformReport {
                platform: platform.to_owned(),
                verdict: "FALSIFIED",
                artifact: None,
                artifact_hash_valid: false,
                unknown_dependencies: Vec::new(),
                missing_components: Vec::new(),
                clean_image_verified: false,
                reasons: vec!["release manifest or packaged artifact is missing".to_owned()],
            });
        }
    }
    let linux = platforms
        .iter()
        .any(|p| p.platform.starts_with("linux") && p.verdict == "SUPPORTED");
    let windows = platforms.iter().any(|p| {
        p.platform.starts_with("windows") && matches!(p.verdict, "SUPPORTED" | "BUILT_UNVERIFIED")
    });
    Ok(PackageClosureReport {
        verdict: verdict(linux && windows),
        scope: PACKAGE_SCOPE,
        production_bundle_verdict: "FALSIFIED",
        production_bundle_reasons: production_bundle_reasons(),
        release_dir: Some(dir.display().to_string()),
        platforms,
    })
}
fn verify_manifest(path: &Path) -> Result<PackagePlatformReport> {
    let m: ReleaseManifest = serde_json::from_slice(&fs::read(path)?)?;
    let base = path.parent().context("manifest parent")?;
    let artifact = base.join(&m.artifact);
    let mut reasons = Vec::new();
    if m.schema_version != 1 {
        reasons.push("manifest schema_version must be 1".into());
    }
    if m.scope != PACKAGE_SCOPE && m.scope != "phase0-architecture-probe" {
        reasons.push("manifest scope is not supported by this package gate".into());
    }
    let hash_ok = artifact.is_file() && sha256_file(&artifact)? == m.artifact_sha256;
    if !hash_ok {
        reasons.push("artifact missing or SHA-256 mismatch".into());
    }
    let req: &[&str] = if m.platform.starts_with("linux") {
        if m.scope == PACKAGE_SCOPE {
            &[
                "core-service",
                "txt-worker",
                "txt-adapter",
                "licenses",
                "sbom",
            ]
        } else {
            &["core-service", "licenses", "sbom"]
        }
    } else if m.platform.starts_with("windows") {
        if m.scope == PACKAGE_SCOPE {
            &["core-service", "txt-worker", "txt-adapter"]
        } else {
            &["core-service"]
        }
    } else {
        reasons.push("manifest platform is unsupported".into());
        &[]
    };
    let missing = req
        .iter()
        .filter(|n| !m.bundled_components.iter().any(|x| x == **n))
        .map(|n| (*n).to_owned())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        reasons.push("required bundled components are missing".into());
    }
    let known = m
        .packaged_dependencies
        .iter()
        .chain(&m.allowed_external_dependencies)
        .collect::<std::collections::HashSet<_>>();
    let unknown = m
        .static_dependencies
        .iter()
        .chain(&m.runtime_dependencies)
        .filter(|d| !known.contains(d))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        reasons.push("dependency closure contains undeclared dependencies".into());
    }
    let probes = req.iter().all(|required| {
        *required == "licenses"
            || *required == "sbom"
            || m.clean_image
                .component_probes
                .iter()
                .any(|probe| probe == required)
    });
    let clean = m.clean_image.performed
        && m.clean_image.network_blocked_before_install
        && m.clean_image.installed
        && m.clean_image.launched
        && probes
        && m.clean_image.network_attempts == 0;
    let windows_cross_build =
        m.platform.starts_with("windows") && m.runner_kind.as_deref() == Some("linux-cross-build");
    if !clean && !windows_cross_build {
        reasons.push("clean-image offline install/launch/probe evidence is incomplete".into());
    }
    if m.platform.starts_with("windows") && !windows_cross_build {
        if m.runner_kind.as_deref() != Some("windows-native") {
            reasons.push("Windows validation requires a native Windows runner".into());
        }
        match m
            .windows
            .as_ref()
            .map(|evidence| evidence.webview_install_mode.as_str())
        {
            Some("not-applicable-phase0" | "not-applicable-phase3-txt" | "offlineInstaller") => {}
            _ => reasons.push("Windows validation evidence is missing or invalid".into()),
        }
    }
    let manifest_verdict = if reasons.is_empty() && windows_cross_build {
        "BUILT_UNVERIFIED"
    } else {
        verdict(reasons.is_empty())
    };
    Ok(PackagePlatformReport {
        platform: m.platform,
        verdict: manifest_verdict,
        artifact: Some(artifact.display().to_string()),
        artifact_hash_valid: hash_ok,
        unknown_dependencies: unknown,
        missing_components: missing,
        clean_image_verified: clean,
        reasons,
    })
}

fn read_pragmas(c: &Connection) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for n in [
        "journal_mode",
        "synchronous",
        "foreign_keys",
        "wal_autocheckpoint",
    ] {
        let value: rusqlite::types::Value =
            c.query_row(&format!("PRAGMA {n}"), [], |r| r.get(0))?;
        let v = match value {
            rusqlite::types::Value::Null => "null".to_owned(),
            rusqlite::types::Value::Integer(value) => value.to_string(),
            rusqlite::types::Value::Real(value) => value.to_string(),
            rusqlite::types::Value::Text(value) => value,
            rusqlite::types::Value::Blob(value) => hex::encode(value),
        };
        out.insert(n.into(), v);
    }
    Ok(out)
}
fn latencies(mut raw: Vec<u64>) -> Latencies {
    raw.sort_unstable();
    Latencies {
        samples: raw.len(),
        p50_ms: pct(&raw, 0.5),
        p95_ms: pct(&raw, 0.95),
        p99_ms: pct(&raw, 0.99),
        max_ms: raw.last().copied().unwrap_or(0) as f64 / 1000.0,
        raw_us: raw,
    }
}
fn pct(v: &[u64], q: f64) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v[((v.len() - 1) as f64 * q).ceil() as usize] as f64 / 1000.
    }
}
fn us(t: Instant) -> u64 {
    t.elapsed().as_micros().min(u64::MAX as u128) as u64
}
fn file_size_or_zero(path: &Path) -> Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}
fn verdict(ok: bool) -> &'static str {
    if ok { "SUPPORTED" } else { "FALSIFIED" }
}
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn sha256_file(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn production_bundle_reasons() -> Vec<&'static str> {
    vec![
        "Phase 3 TXT slice packages do not contain the production desktop shell",
        "Markdown, EPUB, OCR, and resource-image workers are not bundled",
        "OCR runtime and model are not bundled",
        "native Windows offline installation has not been validated",
    ]
}
fn write_json<T: Serialize>(v: &T, path: Option<&Path>) -> Result<()> {
    let s = serde_json::to_string_pretty(v)?;
    if let Some(p) = path {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(p, format!("{s}\n"))?;
    } else {
        println!("{s}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn phase0_gate_accepts_publish_only_windows_without_promoting_production() {
        let temp = TempDir::new().unwrap();
        let linux_dir = temp.path().join("arch");
        let windows_dir = temp.path().join("windows");
        fs::create_dir_all(&linux_dir).unwrap();
        fs::create_dir_all(&windows_dir).unwrap();
        fs::write(linux_dir.join("probe.pkg.tar.zst"), b"arch package").unwrap();
        fs::write(windows_dir.join("probe.exe"), b"windows installer").unwrap();

        let linux_hash = sha256_file(&linux_dir.join("probe.pkg.tar.zst")).unwrap();
        let windows_hash = sha256_file(&windows_dir.join("probe.exe")).unwrap();
        let clean_linux = json!({
            "performed": true,
            "network_blocked_before_install": true,
            "installed": true,
            "launched": true,
            "component_probes": ["core-service", "txt-worker", "txt-adapter"],
            "network_attempts": 0
        });
        fs::write(
            linux_dir.join("release-manifest.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "scope": "phase3-txt-vertical-slice",
                "platform": "linux-arch-x86_64",
                "artifact": "probe.pkg.tar.zst",
                "artifact_sha256": linux_hash,
                "bundled_components": ["core-service", "txt-worker", "txt-adapter", "licenses", "sbom"],
                "packaged_dependencies": [],
                "allowed_external_dependencies": ["libc.so.6"],
                "static_dependencies": [],
                "runtime_dependencies": ["libc.so.6"],
                "clean_image": clean_linux
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            windows_dir.join("release-manifest.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "scope": "phase3-txt-vertical-slice",
                "runner_kind": "linux-cross-build",
                "platform": "windows-x86_64",
                "artifact": "probe.exe",
                "artifact_sha256": windows_hash,
                "bundled_components": ["core-service", "txt-worker", "txt-adapter"],
                "packaged_dependencies": [],
                "allowed_external_dependencies": ["KERNEL32.dll"],
                "static_dependencies": [],
                "runtime_dependencies": ["KERNEL32.dll"],
                "clean_image": {
                    "performed": false,
                    "network_blocked_before_install": false,
                    "installed": false,
                    "launched": false,
                    "component_probes": [],
                    "network_attempts": 0
                },
                "windows": {"webview_install_mode": "not-applicable-phase3-txt"}
            }))
            .unwrap(),
        )
        .unwrap();

        let report = package_closure(Some(temp.path())).unwrap();
        assert_eq!(report.verdict, "SUPPORTED");
        assert_eq!(report.production_bundle_verdict, "FALSIFIED");
        assert!(report.platforms.iter().any(|platform| {
            platform.platform.starts_with("linux") && platform.verdict == "SUPPORTED"
        }));
        assert!(report.platforms.iter().any(|platform| {
            platform.platform.starts_with("windows") && platform.verdict == "BUILT_UNVERIFIED"
        }));
    }

    #[test]
    fn package_closure_reports_each_missing_required_platform() {
        let temp = TempDir::new().unwrap();
        let report = package_closure(Some(temp.path())).unwrap();
        assert_eq!(report.verdict, "FALSIFIED");
        assert_eq!(report.platforms.len(), 2);
        assert!(report.platforms.iter().all(|platform| {
            platform.verdict == "FALSIFIED"
                && platform.artifact.is_none()
                && platform.reasons == ["release manifest or packaged artifact is missing"]
        }));
    }

    #[test]
    fn windows_native_manifest_requires_clean_image_evidence() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("probe.exe"), b"windows installer").unwrap();
        let windows_hash = sha256_file(&temp.path().join("probe.exe")).unwrap();
        fs::write(
            temp.path().join("release-manifest.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "scope": "phase3-txt-vertical-slice",
                "runner_kind": "windows-native",
                "platform": "windows-x86_64",
                "artifact": "probe.exe",
                "artifact_sha256": windows_hash,
                "bundled_components": ["core-service", "txt-worker", "txt-adapter"],
                "packaged_dependencies": [],
                "allowed_external_dependencies": ["KERNEL32.dll"],
                "static_dependencies": [],
                "runtime_dependencies": ["KERNEL32.dll"],
                "clean_image": {
                    "performed": false,
                    "network_blocked_before_install": false,
                    "installed": false,
                    "launched": false,
                    "component_probes": [],
                    "network_attempts": 0
                },
                "windows": {"webview_install_mode": "not-applicable-phase3-txt"}
            }))
            .unwrap(),
        )
        .unwrap();

        let report = verify_manifest(&temp.path().join("release-manifest.json")).unwrap();
        assert_eq!(report.verdict, "FALSIFIED");
        assert!(report.reasons.iter().any(|reason| {
            reason == "clean-image offline install/launch/probe evidence is incomplete"
        }));
    }

    #[test]
    fn linux_phase3_manifest_requires_txt_worker_probe() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("probe.pkg.tar.zst"), b"arch package").unwrap();
        let linux_hash = sha256_file(&temp.path().join("probe.pkg.tar.zst")).unwrap();
        fs::write(
            temp.path().join("release-manifest.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "scope": "phase3-txt-vertical-slice",
                "platform": "linux-arch-x86_64",
                "artifact": "probe.pkg.tar.zst",
                "artifact_sha256": linux_hash,
                "bundled_components": ["core-service", "txt-worker", "txt-adapter", "licenses", "sbom"],
                "packaged_dependencies": [],
                "allowed_external_dependencies": ["libc.so.6"],
                "static_dependencies": [],
                "runtime_dependencies": ["libc.so.6"],
                "clean_image": {
                    "performed": true,
                    "network_blocked_before_install": true,
                    "installed": true,
                    "launched": true,
                    "component_probes": ["core-service", "txt-adapter"],
                    "network_attempts": 0
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let report = verify_manifest(&temp.path().join("release-manifest.json")).unwrap();
        assert_eq!(report.verdict, "FALSIFIED");
        assert!(report.reasons.iter().any(|reason| {
            reason == "clean-image offline install/launch/probe evidence is incomplete"
        }));
    }
}
