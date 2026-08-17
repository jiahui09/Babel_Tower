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

const M_BYTES: usize = 100 * 1024 * 1024;
const M_UNITS: usize = 250_000;
const M_IMAGES: usize = 2_000;

#[derive(Parser)]
#[command(name = "babel-phase4", about = "Babel Tower Phase 4 Markdown 验收器")]
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
        #[arg(long, default_value_t = M_BYTES)]
        bytes: usize,
        #[arg(long, default_value_t = M_UNITS)]
        units: usize,
        #[arg(long, default_value_t = M_IMAGES)]
        images: usize,
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
    image_references_preserved: bool,
    protected_structure_preserved: bool,
    recovered_translation: bool,
    output_sha256: String,
}

#[derive(Serialize)]
struct BenchmarkReport {
    verdict: &'static str,
    input_bytes: usize,
    units: usize,
    image_references: usize,
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
    full_import_ms: u128,
    cold_open_ms: u128,
    idle_rss_mib: f64,
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
            bytes,
            units,
            images,
            work_dir,
            output,
        } => {
            let (_temporary, root) = project_root(work_dir)?;
            write_json(&benchmark(&root, bytes, units, images)?, output.as_deref())
        }
    }
}

fn smoke(root: &Path) -> Result<SmokeReport> {
    let input = b"# Title\n\nHello **world** and [site](https://example.test).\n\n![cover](images/cover.png)\n";
    let kernel = Kernel::open(root)?;
    let imported = kernel.import_markdown_reader([1; 16], Cursor::new(input), 1)?;
    let units = kernel.query()?.page_after(-1, 32)?;
    for (index, unit) in units.iter().enumerate() {
        let translated = match unit.source_text.as_str() {
            "Title" => "标题",
            "Hello " => "你好 ",
            "world" => "世界",
            " and " => " 和 ",
            "site" => "站点",
            "cover" => "封面",
            other => other,
        };
        kernel.save_translation(
            unit.source_unit_key
                .clone()
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid source key"))?,
            command_id(index),
            translated.to_owned(),
            10 + index as i64,
        )?;
    }
    ensure!(
        kernel.validate_active_markdown()?.is_empty(),
        "translated Markdown must validate"
    );
    let exported = kernel.export_active_markdown()?;
    let expected =
        "# 标题\n\n你好 **世界** 和 [站点](https://example.test).\n\n![封面](images/cover.png)\n";
    ensure!(
        exported.bytes == expected.as_bytes(),
        "unexpected Markdown export"
    );
    let protected_structure_preserved =
        expected.contains("**世界**") && expected.contains("[站点](https://example.test)");
    let image_references_preserved = expected.contains("(images/cover.png)");
    drop(kernel);

    let reopened = Kernel::open(root)?;
    let recovered = reopened.query()?.page_after(-1, 32)?;
    let recovered_translation = recovered
        .iter()
        .any(|unit| unit.translation.as_deref() == Some("世界"));
    ensure!(recovered_translation, "translation did not survive reopen");
    Ok(SmokeReport {
        verdict: "SUPPORTED",
        imported_units: imported.units,
        image_references_preserved,
        protected_structure_preserved,
        recovered_translation,
        output_sha256: hex_digest(&exported.bytes),
    })
}

fn benchmark(root: &Path, bytes: usize, units: usize, images: usize) -> Result<BenchmarkReport> {
    ensure!(units > 0 && images <= units, "invalid M corpus dimensions");
    let corpus = SyntheticMarkdown::new(bytes, units, images)?;
    let input_bytes = corpus.total_bytes();
    let kernel = Kernel::open(root)?;
    let started = Instant::now();
    let imported = kernel.import_markdown_reader([2; 16], corpus, 1)?;
    let import_ms = started.elapsed().as_millis();
    ensure!(
        imported.units == units,
        "Markdown unit count differs from M corpus"
    );
    drop(kernel);

    let started = Instant::now();
    let kernel = Kernel::open(root)?;
    ensure!(
        !kernel.query()?.page_after(-1, 1)?.is_empty(),
        "cold project is empty"
    );
    let cold_open_ms = started.elapsed().as_millis();

    let mut page_latency = Histogram::<u64>::new(3)?;
    let mut after = -1_i64;
    for _ in 0..100 {
        let started = Instant::now();
        let page = kernel.query()?.page_after(after, 100)?;
        page_latency.record(started.elapsed().as_micros() as u64)?;
        after = page.last().map_or(-1, |item| item.local_index);
    }

    let first = kernel.query()?.page_after(-1, 1)?.remove(0);
    let source_key = first
        .source_unit_key
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid source key"))?;
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
        ensure!(
            !kernel.search("基准译文".to_owned(), 20)?.is_empty(),
            "search did not observe the saved revision"
        );
        search_latency.record(started.elapsed().as_micros() as u64)?;
    }

    let thresholds = Thresholds {
        full_import_ms: 20_000,
        cold_open_ms: 3_000,
        idle_rss_mib: 400.0,
        page_p95_ms: 50.0,
        save_p95_ms: 300.0,
        search_p95_ms: 150.0,
    };
    let page_p95_ms = micros_to_ms(page_latency.value_at_quantile(0.95));
    let save_p95_ms = micros_to_ms(save_latency.value_at_quantile(0.95));
    let search_p95_ms = micros_to_ms(search_latency.value_at_quantile(0.95));
    let peak_rss_mib = linux_peak_rss_mib();
    let supported = import_ms <= thresholds.full_import_ms
        && cold_open_ms <= thresholds.cold_open_ms
        && peak_rss_mib.is_none_or(|rss| rss <= thresholds.idle_rss_mib)
        && page_p95_ms <= thresholds.page_p95_ms
        && save_p95_ms <= thresholds.save_p95_ms
        && search_p95_ms <= thresholds.search_p95_ms;
    Ok(BenchmarkReport {
        verdict: if supported { "SUPPORTED" } else { "FALSIFIED" },
        input_bytes,
        units: imported.units,
        image_references: images,
        import_ms,
        cold_open_ms,
        peak_rss_mib,
        page_p95_ms,
        save_p95_ms,
        search_p95_ms,
        thresholds,
    })
}

struct SyntheticMarkdown {
    target_bytes: usize,
    units: usize,
    images: usize,
    next_unit: usize,
    emitted: usize,
    pending: Cursor<Vec<u8>>,
    overhead: usize,
}

impl SyntheticMarkdown {
    fn new(target_bytes: usize, units: usize, images: usize) -> Result<Self> {
        let overhead = (0..units)
            .map(|index| unit_overhead(index, units, images))
            .sum::<usize>();
        ensure!(
            target_bytes >= overhead,
            "target bytes are smaller than corpus syntax"
        );
        Ok(Self {
            target_bytes,
            units,
            images,
            next_unit: 0,
            emitted: 0,
            pending: Cursor::new(Vec::new()),
            overhead,
        })
    }

    const fn total_bytes(&self) -> usize {
        self.target_bytes
    }

    fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if self.next_unit >= self.units {
            return None;
        }
        let index = self.next_unit;
        self.next_unit += 1;
        let payload_total = self.target_bytes - self.overhead;
        let payload = payload_total / self.units + usize::from(index < payload_total % self.units);
        let mut chunk = format!("unit-{index:06} ").into_bytes();
        chunk.resize(chunk.len() + payload, b'x');
        if has_image(index, self.units, self.images) {
            let image = image_ordinal(index, self.units, self.images);
            chunk.extend_from_slice(format!(" ![](images/{image:04}.png)").as_bytes());
        }
        chunk.extend_from_slice(b"\n\n");
        Some(chunk)
    }
}

impl Read for SyntheticMarkdown {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let read = self.pending.read(output)?;
            if read != 0 {
                self.emitted += read;
                return Ok(read);
            }
            let Some(chunk) = self.next_chunk() else {
                debug_assert_eq!(self.emitted, self.target_bytes);
                return Ok(0);
            };
            self.pending = Cursor::new(chunk);
        }
    }
}

fn unit_overhead(index: usize, units: usize, images: usize) -> usize {
    let mut bytes = format!("unit-{index:06} ").len() + 2;
    if has_image(index, units, images) {
        bytes += format!(
            " ![](images/{:04}.png)",
            image_ordinal(index, units, images)
        )
        .len();
    }
    bytes
}

fn has_image(index: usize, _units: usize, images: usize) -> bool {
    index < images
}

fn image_ordinal(index: usize, _units: usize, _images: usize) -> usize {
    index
}

fn project_root(work_dir: Option<PathBuf>) -> Result<(Option<TempDir>, PathBuf)> {
    match work_dir {
        Some(root) => {
            fs::create_dir_all(&root).context("create benchmark work directory")?;
            Ok((None, root))
        }
        None => {
            let temporary = TempDir::new().context("create temporary project")?;
            let root = temporary.path().join("phase4.babel");
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

fn command_id(index: usize) -> [u8; 32] {
    Sha256::digest(format!("phase4-command-{index}")).into()
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn micros_to_ms(value: u64) -> f64 {
    value as f64 / 1_000.0
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
    fn synthetic_corpus_has_exact_size_and_expected_markdown_units() {
        let mut corpus = SyntheticMarkdown::new(64 * 1024, 100, 10).unwrap();
        let mut bytes = Vec::new();
        corpus.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes.len(), 64 * 1024);
        assert_eq!(
            bytes.windows(4).filter(|window| *window == b"![](").count(),
            10
        );
        assert_eq!(
            bytes.windows(2).filter(|window| *window == b"\n\n").count(),
            100
        );
    }
}
