use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use babel_application::Kernel;
use clap::{Parser, Subcommand};
use hdrhistogram::Histogram;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const S_LINES: usize = 100_000;
const S_LINE_BYTES: usize = 100;

#[derive(Parser)]
#[command(
    name = "babel-phase3",
    about = "Babel Tower Phase 3 TXT 纵向闭环验收器"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Smoke {
        #[arg(long)]
        work_dir: Option<PathBuf>,
    },
    Benchmark {
        #[arg(long, default_value_t = S_LINES)]
        lines: usize,
        #[arg(long, default_value_t = S_LINE_BYTES)]
        line_bytes: usize,
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
    search_hits: usize,
    recovered_translation: bool,
    output_sha256: String,
    preserved_mixed_newlines: bool,
}

#[derive(Serialize)]
struct BenchmarkReport {
    verdict: &'static str,
    input_bytes: usize,
    units: usize,
    import_ms: u128,
    cold_open_ms: u128,
    peak_rss_mib: Option<f64>,
    page_p95_ms: f64,
    save_p95_ms: f64,
    search_p95_ms: f64,
    thresholds: Thresholds,
}

#[derive(Serialize)]
struct Thresholds {
    cold_open_ms: u128,
    peak_rss_mib: f64,
    page_p95_ms: f64,
    save_p95_ms: f64,
    search_p95_ms: f64,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Smoke { work_dir } => {
            let (_temporary, root) = project_root(work_dir)?;
            write_json(&smoke(&root)?, None)
        }
        Command::Benchmark {
            lines,
            line_bytes,
            work_dir,
            output,
        } => {
            let (_temporary, root) = project_root(work_dir)?;
            write_json(&benchmark(&root, lines, line_bytes)?, output.as_deref())
        }
    }
}

fn smoke(root: &Path) -> Result<SmokeReport> {
    let input = b"one\r\ntwo\nthree\rfour";
    let kernel = Kernel::open(root)?;
    let imported = kernel.import_txt_reader([1; 16], Cursor::new(input), 1)?;
    ensure!(imported.units == 4, "TXT smoke should import four units");
    let units = kernel.query()?.page_after(-1, 16)?;
    let translations = ["一", "二", "三", "四"];
    for (index, unit) in units.iter().enumerate() {
        let key: [u8; 32] = unit
            .source_unit_key
            .clone()
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid source unit key"))?;
        kernel.save_translation(
            key,
            command_id(index),
            translations[index].to_owned(),
            10 + index as i64,
        )?;
    }
    ensure!(
        kernel.validate_active_txt()?.is_empty(),
        "translated smoke project must validate"
    );
    let search_hits = kernel.search("三".to_owned(), 10)?.len();
    ensure!(
        search_hits == 1,
        "translation search must find the saved unit"
    );
    let exported = kernel.export_active_txt()?;
    let expected = "一\r\n二\n三\r四".as_bytes();
    ensure!(
        exported.bytes == expected,
        "mixed source newlines must be preserved"
    );
    drop(kernel);

    let reopened = Kernel::open(root)?;
    let recovered = reopened.query()?.page_after(-1, 16)?;
    let recovered_translation = recovered
        .get(2)
        .and_then(|unit| unit.translation.as_deref())
        == Some("三");
    ensure!(
        recovered_translation,
        "durable translation must survive reopen"
    );
    Ok(SmokeReport {
        verdict: "SUPPORTED",
        imported_units: imported.units,
        search_hits,
        recovered_translation,
        output_sha256: hex_digest(&exported.bytes),
        preserved_mixed_newlines: true,
    })
}

fn benchmark(root: &Path, lines: usize, line_bytes: usize) -> Result<BenchmarkReport> {
    ensure!(
        lines > 0 && line_bytes >= 16,
        "benchmark corpus dimensions are invalid"
    );
    let corpus = SyntheticTxt::new(lines, line_bytes);
    let input_bytes = corpus.total_bytes();
    let kernel = Kernel::open(root)?;
    let started = Instant::now();
    let imported = kernel.import_txt_reader([2; 16], corpus, 1)?;
    let import_ms = started.elapsed().as_millis();
    ensure!(
        imported.units == lines,
        "imported unit count differs from corpus"
    );
    drop(kernel);

    let started = Instant::now();
    let kernel = Kernel::open(root)?;
    ensure!(
        !kernel.query()?.page_after(-1, 1)?.is_empty(),
        "cold-opened project must expose its first unit"
    );
    let cold_open_ms = started.elapsed().as_millis();

    let mut page_latency = Histogram::<u64>::new(3)?;
    let mut after = -1_i64;
    for _ in 0..100 {
        let started = Instant::now();
        let page = kernel.query()?.page_after(after, 100)?;
        page_latency.record(started.elapsed().as_micros() as u64)?;
        if page.is_empty() {
            after = -1;
        } else {
            after = page.last().expect("nonempty page").local_index;
        }
    }

    let first = kernel.query()?.page_after(-1, 1)?.remove(0);
    let source_key: [u8; 32] = first
        .source_unit_key
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid source unit key"))?;
    let mut save_latency = Histogram::<u64>::new(3)?;
    for index in 0..100 {
        let started = Instant::now();
        kernel.save_translation(
            source_key,
            command_id(index + 1_000),
            format!("基准译文 {index}"),
            100 + index as i64,
        )?;
        save_latency.record(started.elapsed().as_micros() as u64)?;
    }
    let mut search_latency = Histogram::<u64>::new(3)?;
    for _ in 0..20 {
        let started = Instant::now();
        let hits = kernel.search("基准译文".to_owned(), 20)?;
        ensure!(
            !hits.is_empty(),
            "search must observe flushed translation revisions"
        );
        search_latency.record(started.elapsed().as_micros() as u64)?;
    }

    let thresholds = Thresholds {
        cold_open_ms: 3_000,
        peak_rss_mib: 400.0,
        page_p95_ms: 50.0,
        save_p95_ms: 300.0,
        search_p95_ms: 150.0,
    };
    let page_p95_ms = micros_to_ms(page_latency.value_at_quantile(0.95));
    let save_p95_ms = micros_to_ms(save_latency.value_at_quantile(0.95));
    let search_p95_ms = micros_to_ms(search_latency.value_at_quantile(0.95));
    let peak_rss_mib = linux_peak_rss_mib();
    let supported = cold_open_ms <= thresholds.cold_open_ms
        && peak_rss_mib.is_none_or(|rss| rss <= thresholds.peak_rss_mib)
        && page_p95_ms <= thresholds.page_p95_ms
        && save_p95_ms <= thresholds.save_p95_ms
        && search_p95_ms <= thresholds.search_p95_ms;
    Ok(BenchmarkReport {
        verdict: if supported { "SUPPORTED" } else { "FALSIFIED" },
        input_bytes,
        units: imported.units,
        import_ms,
        cold_open_ms,
        peak_rss_mib,
        page_p95_ms,
        save_p95_ms,
        search_p95_ms,
        thresholds,
    })
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

struct SyntheticTxt {
    lines: usize,
    line_bytes: usize,
    position: usize,
    current: Vec<u8>,
}

impl SyntheticTxt {
    fn new(lines: usize, line_bytes: usize) -> Self {
        Self {
            lines,
            line_bytes,
            position: 0,
            current: Vec::new(),
        }
    }

    fn total_bytes(&self) -> usize {
        self.lines * self.line_bytes
    }

    fn fill_line(&mut self, line: usize) {
        self.current.clear();
        self.current
            .extend_from_slice(format!("{line:08}: Babel Tower offline TXT corpus ").as_bytes());
        self.current.resize(self.line_bytes - 1, b'x');
        self.current.push(b'\n');
    }
}

impl Read for SyntheticTxt {
    fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.total_bytes() || target.is_empty() {
            return Ok(0);
        }
        let mut written = 0;
        while written < target.len() && self.position < self.total_bytes() {
            let line = self.position / self.line_bytes;
            let offset = self.position % self.line_bytes;
            if offset == 0 || self.current.is_empty() {
                self.fill_line(line);
            }
            let count = (target.len() - written).min(self.line_bytes - offset);
            target[written..written + count].copy_from_slice(&self.current[offset..offset + count]);
            self.position += count;
            written += count;
        }
        Ok(written)
    }
}

fn project_root(work_dir: Option<PathBuf>) -> Result<(Option<TempDir>, PathBuf)> {
    match work_dir {
        Some(path) => {
            fs::create_dir_all(&path)?;
            Ok((None, path))
        }
        None => {
            let temporary = TempDir::new()?;
            let path = temporary.path().to_owned();
            Ok((Some(temporary), path))
        }
    }
}

fn command_id(index: usize) -> [u8; 32] {
    Sha256::digest(format!("phase3-command-{index}")).into()
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn micros_to_ms(micros: u64) -> f64 {
    micros as f64 / 1_000.0
}

fn write_json<T: Serialize>(value: &T, output: Option<&Path>) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &bytes).with_context(|| format!("write {}", path.display()))?;
    } else {
        println!("{}", String::from_utf8(bytes)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn benchmark_report_uses_cold_open_as_the_s_corpus_gate() {
        let temp = TempDir::new().unwrap();
        let report = benchmark(temp.path(), 8, 32).unwrap();
        let value = serde_json::to_value(&report).unwrap();

        assert!(value.get("cold_open_ms").is_some());
        assert!(value["thresholds"].get("cold_open_ms").is_some());
        assert!(value["thresholds"].get("import_ms").is_none());
    }

    #[test]
    fn smoke_report_records_reopen_recovery_and_newline_preservation() {
        let temp = TempDir::new().unwrap();
        let report = smoke(temp.path()).unwrap();
        let value: Value = serde_json::to_value(&report).unwrap();

        assert_eq!(value["verdict"], "SUPPORTED");
        assert_eq!(value["recovered_translation"], true);
        assert_eq!(value["preserved_mixed_newlines"], true);
    }
}
