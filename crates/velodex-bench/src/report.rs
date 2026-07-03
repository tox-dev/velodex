//! The TOML report zola renders: one file, one table per workload, merged across partial runs.

use std::path::PathBuf;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::servers::Server;

/// Best-in-row to worst-in-row tint names the site's stylesheet colors green through red.
const LADDER: &[&str] = &["faster", "par", "mild", "slow", "veryslow", "worst"];

/// The tint scale never compresses below an 8x span, so a near-parity row reads green throughout.
const MIN_SPAN: f64 = 2.079_441_541_679_835_9; // ln 8

/// One comparison table.
#[derive(Serialize, Deserialize)]
pub struct Table {
    pub label: String,
    pub baseline: String,
    pub parties: Vec<Party>,
    pub rows: Vec<Row>,
}

#[derive(Serialize, Deserialize)]
pub struct Party {
    pub name: String,
    pub url: String,
}

#[derive(Serialize, Deserialize)]
pub struct Row {
    pub name: String,
    pub cells: Vec<Cell>,
}

#[derive(Serialize, Deserialize)]
pub struct Cell {
    pub text: String,
    pub ratio: String,
    pub tint: String,
    /// Run-to-run coefficient of variation across the kept samples, e.g. `±4%`; empty for a
    /// derived or absent cell that has no spread of its own.
    #[serde(default)]
    pub spread: String,
}

/// What a `None` measurement means for a row, and how its cell renders.
#[derive(Clone, Copy)]
pub enum Absent {
    /// The party ran the workload and failed it.
    Failed,
    /// The party has nothing to measure (direct runs no server).
    NoServer,
}

/// How a row's numbers read.
#[derive(Clone, Copy)]
pub enum Metric {
    /// Wall-clock seconds; lower is better.
    Seconds,
    /// A rate in the named unit; higher is better.
    Rate(&'static str),
    /// A quantity in the named unit; lower is better.
    Amount(&'static str),
}

/// Format one row from scalar values: readable value, ratio against the baseline, and a tint. Used
/// for derived rows (a ratio, a normalized cost) that carry no per-cell spread.
///
/// The baseline is the no-proxy `direct` measurement where present, so every other cell reads as
/// the overhead (or win) a server adds on top of talking to the upstream itself. A `None` marks a
/// party without a number; `absent` says whether that is a failure (red) or a non-entity (plain).
///
/// # Panics
/// Never in practice: the caller always measures the baseline party.
pub fn row(name: &str, values: &[Option<f64>], baseline: usize, metric: Metric, absent: Absent) -> Row {
    let spreads = vec![String::new(); values.len()];
    build_row(name, values, &spreads, baseline, metric, absent)
}

/// Format one row from the raw samples per party: the cell reports the trimmed mean and the
/// run-to-run coefficient of variation beside it, so a reader sees not just the figure but how much
/// it wandered between runs.
///
/// # Panics
/// Never in practice: the caller always measures the baseline party.
pub fn row_samples(name: &str, per_party: &[Option<Vec<f64>>], baseline: usize, metric: Metric, absent: Absent) -> Row {
    let mut values = Vec::with_capacity(per_party.len());
    let mut spreads = Vec::with_capacity(per_party.len());
    for samples in per_party {
        match samples {
            Some(samples) if !samples.is_empty() => {
                let (mean, cv) = robust_stats(&mut samples.clone());
                values.push(Some(mean));
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "cv percent is small"
                )]
                spreads.push(format!("±{}%", (cv * 100.0).round() as u64));
            }
            _ => {
                values.push(None);
                spreads.push(String::new());
            }
        }
    }
    build_row(name, &values, &spreads, baseline, metric, absent)
}

fn build_row(
    name: &str,
    values: &[Option<f64>],
    spreads: &[String],
    baseline: usize,
    metric: Metric,
    absent: Absent,
) -> Row {
    let reference = values[baseline].expect("the baseline party always has a number");
    let higher_is_better = matches!(metric, Metric::Rate(_));
    let cost = |value: f64| if higher_is_better { 1.0 / value } else { value };
    let finite: Vec<f64> = values.iter().flatten().map(|&value| cost(value)).collect();
    let best = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let worst = finite.iter().copied().fold(0.0f64, f64::max);
    let span = (worst / best).ln().max(MIN_SPAN);
    let cells = values
        .iter()
        .zip(spreads)
        .map(|(value, spread)| {
            value.map_or_else(
                || absent_cell(absent),
                |value| {
                    let position = (cost(value) / best).ln() / span;
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_precision_loss,
                        clippy::cast_sign_loss,
                        reason = "position is a small non-negative ladder fraction"
                    )]
                    let index = ((position * LADDER.len() as f64) as usize).min(LADDER.len() - 1);
                    Cell {
                        text: format_value(value, metric),
                        ratio: format!("{:.2}x", value / reference),
                        tint: LADDER[index].to_owned(),
                        spread: spread.clone(),
                    }
                },
            )
        })
        .collect();
    Row {
        name: name.to_owned(),
        cells,
    }
}

const fn absent_kinds(absent: Absent) -> (&'static str, &'static str) {
    match absent {
        Absent::Failed => ("error", "worst"),
        Absent::NoServer => ("no server", "none"),
    }
}

fn absent_cell(absent: Absent) -> Cell {
    let (text, tint) = absent_kinds(absent);
    Cell {
        text: text.to_owned(),
        ratio: "n/a".to_owned(),
        tint: tint.to_owned(),
        spread: String::new(),
    }
}

/// Assemble a table over the run's parties.
pub fn table(label: &str, servers: &[Server], baseline: usize, rows: Vec<Row>) -> Table {
    Table {
        label: label.to_owned(),
        baseline: servers[baseline].name.to_owned(),
        parties: servers
            .iter()
            .map(|server| Party {
                name: server.name.to_owned(),
                url: server.homepage.to_owned(),
            })
            .collect(),
        rows,
    }
}

/// Merge `name` into the report on disk, keeping tables other runs produced.
///
/// # Errors
/// Returns an error when the existing report cannot be parsed or the new one cannot be written.
pub fn publish(name: &str, table: Table) -> anyhow::Result<()> {
    let path = report_path();
    let mut report: toml::Table = match std::fs::read_to_string(&path) {
        Ok(existing) => existing.parse().context("existing report is not valid TOML")?,
        Err(_) => toml::Table::new(),
    };
    let tables = report
        .entry("tables")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .context("`tables` is not a TOML table")?;
    tables.insert(name.to_owned(), toml::Value::try_from(table)?);
    std::fs::create_dir_all(path.parent().expect("the report lives under site/data"))?;
    std::fs::write(&path, toml::to_string_pretty(&report)?)?;
    println!("updated {} [{name}]", path.display());
    Ok(())
}

/// Write the run's machine-and-toolchain manifest to the report's top-level `[meta]` table, kept
/// alongside the workload tables so the docs can show what produced the numbers.
///
/// # Errors
/// Returns an error when the existing report cannot be parsed or the new one cannot be written.
pub fn publish_meta(entries: &[(&str, String)]) -> anyhow::Result<()> {
    let path = report_path();
    let mut report: toml::Table = match std::fs::read_to_string(&path) {
        Ok(existing) => existing.parse().context("existing report is not valid TOML")?,
        Err(_) => toml::Table::new(),
    };
    let meta: toml::Table = entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), toml::Value::String(value.clone())))
        .collect();
    report.insert("meta".to_owned(), toml::Value::Table(meta));
    std::fs::create_dir_all(path.parent().expect("the report lives under site/data"))?;
    std::fs::write(&path, toml::to_string_pretty(&report)?)?;
    Ok(())
}

fn format_value(value: f64, metric: Metric) -> String {
    match metric {
        Metric::Seconds => format_seconds(value),
        Metric::Rate(unit) | Metric::Amount(unit) => format!("{} {unit}", thousands(value)),
    }
}

fn format_seconds(seconds: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "wall clocks are non-negative and far below u64::MAX minutes"
    )]
    if seconds >= 60.0 {
        format!("{}m {:04.1}s", (seconds / 60.0) as u64, seconds % 60.0)
    } else if seconds >= 1.0 {
        format!("{seconds:.1} s")
    } else if seconds >= 0.0005 {
        format!("{:.0} ms", seconds * 1000.0)
    } else {
        // Sub-half-millisecond work is real work: never print a bare zero.
        "<1 ms".to_owned()
    }
}

/// Round to a whole number with `,` thousands separators so large rates stay readable.
fn thousands(value: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "benchmark magnitudes are non-negative and far below u64::MAX"
    )]
    let whole = value.round() as u64;
    let digits = whole.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// Where the report lives: zola loads it relative to the site root.
pub fn report_path() -> PathBuf {
    repo_root().join("site").join("data").join("bench").join("report.toml")
}

/// Mean with both extremes dropped: one bad sample on either side cannot move the figure.
///
/// # Panics
/// Never in practice: every measurement loop collects at least one sample.
pub fn robust_mean(samples: &mut [f64]) -> f64 {
    robust_stats(samples).0
}

/// The trimmed mean and the coefficient of variation of the kept samples (standard deviation over
/// mean), the run-to-run spread a single figure hides.
///
/// # Panics
/// Never in practice: every measurement loop collects at least one sample.
pub fn robust_stats(samples: &mut [f64]) -> (f64, f64) {
    assert!(!samples.is_empty(), "a measurement always produces samples");
    samples.sort_unstable_by(f64::total_cmp);
    let trim = usize::from(samples.len() >= 3);
    let kept = &samples[trim..samples.len() - trim];
    #[expect(clippy::cast_precision_loss, reason = "sample counts are single digits")]
    let count = kept.len() as f64;
    let mean = kept.iter().sum::<f64>() / count;
    let cv = if kept.len() < 2 || mean == 0.0 {
        0.0
    } else {
        let variance = kept.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / count;
        variance.sqrt() / mean
    };
    (mean, cv)
}

/// The repository checkout root.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the crate lives two levels under the repository root")
        .to_path_buf()
}
