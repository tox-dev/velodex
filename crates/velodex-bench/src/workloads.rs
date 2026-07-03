//! The four workloads: installs, file throughput, a parallel CI fleet, and a request swarm.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};

use crate::packages::{FLEET_PACKAGE, STRESS_PROJECT, TOP_PACKAGES};
use crate::report::{Absent, Metric, Row, publish, robust_mean, row, row_samples, table};
use crate::servers::{Active, Server};
use crate::usage::{Cost, Usage};

/// The index of the no-proxy baseline party, `direct`.
fn baseline(servers: &[Server]) -> usize {
    servers.iter().position(|server| server.name == "direct").unwrap_or(0)
}

/// The party resource rows compare against: direct runs no server, so it cannot anchor them.
fn anchor(servers: &[Server]) -> usize {
    servers
        .iter()
        .position(|server| server.name == "velodex")
        .unwrap_or_else(|| baseline(servers))
}

/// The rows every table ends with: what the server itself burned while the workload ran.
fn cost_rows(servers: &[Server], costs: &[Option<Cost>]) -> Vec<Row> {
    let anchor = anchor(servers);
    let cpu: Vec<Option<f64>> = costs.iter().map(|cost| cost.map(|cost| cost.cpu_seconds)).collect();
    #[expect(clippy::cast_precision_loss, reason = "resident sizes fit f64 to the byte")]
    let rss: Vec<Option<f64>> = costs
        .iter()
        .map(|cost| cost.map(|cost| cost.peak_rss_bytes as f64 / 1e6))
        .collect();
    vec![
        row("server CPU", &cpu, anchor, Metric::Seconds, Absent::NoServer),
        row(
            "server peak memory",
            &rss,
            anchor,
            Metric::Amount("MB"),
            Absent::NoServer,
        ),
    ]
}

/// The install workload: every server, cold then warm, per client; best of `runs`.
///
/// # Errors
/// Returns an error when a server cannot start or an install against a healthy server fails.
pub async fn installs(servers: &[Server], clients: &[&str], runs: usize, http: &reqwest::Client) -> anyhow::Result<()> {
    prewarm_cdn()?;
    for client in clients {
        let mut cold: Vec<Vec<f64>> = vec![Vec::new(); servers.len()];
        let mut warm: Vec<Vec<f64>> = vec![Vec::new(); servers.len()];
        let mut costs: Vec<Option<Cost>> = vec![None; servers.len()];
        // Interleave server order round by round rather than finishing one server before the next,
        // so a drift in the network or the laptop's thermal state spreads across every party
        // instead of penalizing whoever the run reached last.
        for round in 1..=runs {
            for (index, server) in servers.iter().enumerate() {
                let scratch = tempfile::tempdir()?;
                let state = scratch.path().join("state");
                std::fs::create_dir(&state)?;
                let active = server.start(&state, http).await?;
                let usage = Usage::watch(active.pid());
                println!("[{client}] {} round {round}: cold", server.name);
                cold[index].push(install_once(client, &active.url, scratch.path())?);
                println!("[{client}] {} round {round}: warm", server.name);
                warm[index].push(install_once(client, &active.url, scratch.path())?);
                costs[index] = usage.finish().or_else(|| costs[index].take());
            }
        }
        let base = baseline(servers);
        let cold_cells: Vec<Option<Vec<f64>>> = cold.into_iter().map(Some).collect();
        let warm_cells: Vec<Option<Vec<f64>>> = warm.into_iter().map(Some).collect();
        let mut rows = vec![
            row_samples("cold cache", &cold_cells, base, Metric::Seconds, Absent::Failed),
            row_samples("warm cache", &warm_cells, base, Metric::Seconds, Absent::Failed),
        ];
        rows.extend(cost_rows(servers, &costs));
        publish(
            &format!("install-{client}"),
            table(
                &format!("{client}: install the top {} PyPI packages", TOP_PACKAGES.len()),
                servers,
                base,
                rows,
            ),
        )?;
    }
    Ok(())
}

/// One unmeasured direct install so `PyPI`'s CDN edge is equally warm for every party.
///
/// Without it the first party measured pays the CDN's cold-cache penalty and everyone after rides
/// the edge cache that run just warmed, biasing the comparison by run order.
fn prewarm_cdn() -> anyhow::Result<()> {
    println!("prewarming the CDN edge (unmeasured)");
    let scratch = tempfile::tempdir()?;
    install_once("uv", "https://pypi.org/simple/", scratch.path())?;
    Ok(())
}

/// Time one from-scratch install of the workload through `index_url`.
fn install_once(client: &str, index_url: &str, scratch: &Path) -> anyhow::Result<f64> {
    let workdir = tempfile::tempdir_in(scratch)?;
    let venv = workdir.path().join("venv");
    run_checked(Command::new("uv").args(["venv"]).arg(&venv))?;
    let mut command;
    if client == "uv" {
        command = Command::new("uv");
        command
            .args(["pip", "install", "--index-url", index_url])
            .args(TOP_PACKAGES)
            .env("VIRTUAL_ENV", &venv)
            .env("UV_CACHE_DIR", workdir.path().join("client-cache"));
    } else {
        run_checked(
            Command::new("uv")
                .args(["pip", "install", "--python"])
                .arg(venv.join("bin").join("python"))
                .arg("pip"),
        )?;
        command = Command::new(venv.join("bin").join("pip"));
        command
            .args(["install", "--no-cache-dir", "--disable-pip-version-check"])
            .args(["--index-url", index_url])
            .args(TOP_PACKAGES);
    }
    let start = Instant::now();
    let output = command.output().context("install client did not start")?;
    let elapsed = start.elapsed().as_secs_f64();
    if !output.status.success() {
        bail!(
            "install via {index_url} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(elapsed)
}

fn run_checked(command: &mut Command) -> anyhow::Result<()> {
    let output = command.output().context("command did not start")?;
    if !output.status.success() {
        bail!("{command:?} failed:\n{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

/// The file-transfer workload: one large wheel, cold under contention and hot at full speed.
///
/// The cold row sends four clients after the same uncached wheel at once, which is what a CI fleet
/// does to a cache the moment a new release lands: it measures whether the server fans one
/// upstream transfer out to every waiter or serializes them. The hot rows measure how fast a
/// cached wheel leaves the server, alone and under eight parallel readers.
///
/// # Errors
/// Returns an error when a server cannot start; a server failing the transfers is a table cell.
pub async fn throughput(servers: &[Server], runs: usize, http: &reqwest::Client) -> anyhow::Result<()> {
    let filename = stress_wheel_filename(http).await?;
    println!("[throughput] measuring with {filename}");
    let mut cold: Vec<Vec<f64>> = vec![Vec::new(); servers.len()];
    let mut hot1: Vec<Vec<f64>> = vec![Vec::new(); servers.len()];
    let mut hot8: Vec<Vec<f64>> = vec![Vec::new(); servers.len()];
    let mut costs: Vec<Option<Cost>> = vec![None; servers.len()];
    let mut failed = vec![false; servers.len()];
    // Interleave the parties round by round so drift spreads evenly (see the install workload).
    for _ in 0..runs {
        for (index, server) in servers.iter().enumerate() {
            if failed[index] {
                continue;
            }
            let scratch = tempfile::tempdir()?;
            let state = scratch.path().join("state");
            std::fs::create_dir(&state)?;
            let active = server.start(&state, http).await?;
            let usage = Usage::watch(active.pid());
            // A server erroring under contention is itself a result worth a table cell.
            let outcome = transfer_series(&active, &filename, http).await;
            costs[index] = usage.finish().or_else(|| costs[index].take());
            match outcome {
                Ok((cold4, single, eight)) => {
                    cold[index].push(cold4);
                    hot1[index].push(single);
                    hot8[index].push(eight);
                }
                Err(error) => {
                    println!("[throughput] {}: failed under contention ({error:#})", server.name);
                    failed[index] = true;
                }
            }
        }
    }
    let base = baseline(servers);
    let cells = |data: Vec<Vec<f64>>, fail: &[bool]| -> Vec<Option<Vec<f64>>> {
        data.into_iter()
            .enumerate()
            .map(|(index, samples)| (!fail[index] && !samples.is_empty()).then_some(samples))
            .collect()
    };
    let mut rows = vec![
        row_samples(
            "cold cache: 4 clients, one wheel",
            &cells(cold, &failed),
            base,
            Metric::Seconds,
            Absent::Failed,
        ),
        row_samples(
            "hot cache: single download",
            &cells(hot1, &failed),
            base,
            Metric::Rate("MB/s"),
            Absent::Failed,
        ),
        row_samples(
            "hot cache: 8 parallel downloads",
            &cells(hot8, &failed),
            base,
            Metric::Rate("MB/s"),
            Absent::Failed,
        ),
    ];
    rows.extend(cost_rows(servers, &costs));
    publish(
        "throughput",
        table(
            &format!("moving one large wheel ({STRESS_PROJECT}): cold under contention, hot at speed"),
            servers,
            base,
            rows,
        ),
    )
}

async fn transfer_series(active: &Active, filename: &str, http: &reqwest::Client) -> anyhow::Result<(f64, f64, f64)> {
    let url = wheel_url(&active.url, STRESS_PROJECT, filename, http).await?;
    let cold4 = parallel_downloads(&url, 4, http).await?;
    // Hot transfers are sub-second syscall benchmarks; three in-session samples feed the outer
    // trimmed mean. The cold transfer cannot repeat without resetting server state.
    let mut singles = Vec::new();
    let mut size = 0;
    for _ in 0..3 {
        let (seconds, bytes) = timed_download(&url, http).await?;
        singles.push(seconds);
        size = bytes;
    }
    let single = robust_mean(&mut singles);
    let mut hot8_walls = Vec::new();
    for _ in 0..3 {
        hot8_walls.push(parallel_downloads(&url, 8, http).await?);
    }
    let hot8_wall = robust_mean(&mut hot8_walls);
    #[expect(clippy::cast_precision_loss, reason = "wheel sizes fit f64 to the byte")]
    Ok((cold4, size as f64 / single / 1e6, 8.0 * size as f64 / hot8_wall / 1e6))
}

/// The concrete wheel every server moves, resolved once from `PyPI` so all parties match.
async fn stress_wheel_filename(http: &reqwest::Client) -> anyhow::Result<String> {
    let body = http
        .get(format!("https://pypi.org/simple/{STRESS_PROJECT}/"))
        .header("Accept", "application/vnd.pypi.simple.v1+json")
        .send()
        .await?
        .text()
        .await?;
    let page: serde_json::Value = serde_json::from_str(&body)?;
    let tags: &[&str] = if cfg!(target_os = "macos") {
        &["macosx", "arm64"]
    } else {
        &["manylinux", "x86_64"]
    };
    page["files"]
        .as_array()
        .context("simple JSON has no files")?
        .iter()
        .filter_map(|file| file["filename"].as_str())
        .rfind(|name| tags.iter().all(|tag| name.contains(tag)))
        .map(str::to_owned)
        .context("no wheel matches this platform")
}

/// Resolve `filename`'s download URL through a server's simple page, JSON or HTML alike.
async fn wheel_url(index_url: &str, project: &str, filename: &str, http: &reqwest::Client) -> anyhow::Result<String> {
    let response = http
        .get(format!("{index_url}{project}/"))
        .header("Accept", "application/vnd.pypi.simple.v1+json, text/html;q=0.5")
        .send()
        .await?
        .error_for_status()?;
    let page_url = response.url().clone();
    let json_page = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/vnd.pypi.simple.v1+json"));
    let body = response.text().await?;
    let href = if json_page {
        let page: serde_json::Value = serde_json::from_str(&body)?;
        page["files"]
            .as_array()
            .context("simple JSON has no files")?
            .iter()
            .find(|file| file["filename"].as_str() == Some(filename))
            .and_then(|file| file["url"].as_str())
            .context("wheel missing from the JSON page")?
            .to_owned()
    } else {
        html_href(&body, filename).context("wheel missing from the HTML page")?
    };
    let absolute = page_url.join(href.split('#').next().unwrap_or(&href))?;
    Ok(absolute.into())
}

/// The first `href="…"` on the page whose target mentions `filename`; no HTML parser needed for
/// the anchor-list pages every simple index serves.
fn html_href(body: &str, filename: &str) -> Option<String> {
    body.split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .find(|target| target.contains(filename))
        .map(str::to_owned)
}

/// One full download; returns wall seconds and byte count.
async fn timed_download(url: &str, http: &reqwest::Client) -> anyhow::Result<(f64, u64)> {
    let start = Instant::now();
    let mut response = http.get(url).send().await?.error_for_status()?;
    let mut total = 0u64;
    while let Some(chunk) = response.chunk().await? {
        total += chunk.len() as u64;
    }
    Ok((start.elapsed().as_secs_f64(), total))
}

/// `clients` simultaneous downloads of the same URL; returns wall seconds until all finish.
async fn parallel_downloads(url: &str, clients: usize, http: &reqwest::Client) -> anyhow::Result<f64> {
    let start = Instant::now();
    let downloads: Vec<_> = (0..clients)
        .map(|_| {
            let url = url.to_owned();
            let http = http.clone();
            tokio::spawn(async move { timed_download(&url, &http).await })
        })
        .collect();
    for download in downloads {
        download.await.expect("download task never panics")?;
    }
    Ok(start.elapsed().as_secs_f64())
}

/// The CI-fleet workload: ten venvs install polars at once, cold then warm.
///
/// Each worker gets its own empty uv cache, exactly like ten CI jobs landing on the same runner
/// pool: the server sees ten simultaneous copies of every page and wheel request.
///
/// # Errors
/// Returns an error when a server cannot start; a server failing the fleet is a table cell.
pub async fn fleet(servers: &[Server], runs: usize, http: &reqwest::Client) -> anyhow::Result<()> {
    let mut cold: Vec<Vec<f64>> = vec![Vec::new(); servers.len()];
    let mut warm: Vec<Vec<f64>> = vec![Vec::new(); servers.len()];
    let mut costs: Vec<Option<Cost>> = vec![None; servers.len()];
    let mut failed = vec![false; servers.len()];
    // Interleave the parties round by round so drift spreads evenly (see the install workload).
    for _ in 0..runs {
        for (index, server) in servers.iter().enumerate() {
            if failed[index] {
                continue;
            }
            let scratch = tempfile::tempdir()?;
            let state = scratch.path().join("state");
            std::fs::create_dir(&state)?;
            let active = server.start(&state, http).await?;
            let usage = Usage::watch(active.pid());
            // A server erroring under the fleet is itself a result worth a table cell.
            let outcome = match fleet_install(&active.url, scratch.path(), 10) {
                Ok(cold) => fleet_install(&active.url, scratch.path(), 10).map(|warm| (cold, warm)),
                Err(error) => Err(error),
            };
            costs[index] = usage.finish().or_else(|| costs[index].take());
            match outcome {
                Ok((cold_wall, warm_wall)) => {
                    cold[index].push(cold_wall);
                    warm[index].push(warm_wall);
                }
                Err(error) => {
                    println!("[fleet] {}: failed under the fleet ({error:#})", server.name);
                    failed[index] = true;
                }
            }
        }
    }
    let base = baseline(servers);
    let cells = |data: Vec<Vec<f64>>, fail: &[bool]| -> Vec<Option<Vec<f64>>> {
        data.into_iter()
            .enumerate()
            .map(|(index, samples)| (!fail[index] && !samples.is_empty()).then_some(samples))
            .collect()
    };
    let mut rows = vec![
        row_samples(
            "cold cache: 10 parallel installs",
            &cells(cold, &failed),
            base,
            Metric::Seconds,
            Absent::Failed,
        ),
        row_samples(
            "warm cache: 10 parallel installs",
            &cells(warm, &failed),
            base,
            Metric::Seconds,
            Absent::Failed,
        ),
    ];
    rows.extend(cost_rows(servers, &costs));
    publish(
        "parallel-install",
        table(
            &format!("uv: ten venvs install {FLEET_PACKAGE} at once"),
            servers,
            base,
            rows,
        ),
    )
}

/// Install the fleet package into `workers` fresh venvs at once; returns wall seconds.
fn fleet_install(index_url: &str, scratch: &Path, workers: usize) -> anyhow::Result<f64> {
    let rundir = tempfile::tempdir_in(scratch)?;
    let venvs: Vec<_> = (0..workers)
        .map(|index| rundir.path().join(format!("venv-{index}")))
        .collect();
    for venv in &venvs {
        run_checked(Command::new("uv").arg("venv").arg(venv))?;
    }
    let start = Instant::now();
    let threads: Vec<_> = venvs
        .iter()
        .map(|venv| {
            let venv = venv.clone();
            let index_url = index_url.to_owned();
            std::thread::spawn(move || {
                let output = Command::new("uv")
                    .args(["pip", "install", "--index-url", &index_url, FLEET_PACKAGE])
                    .env("VIRTUAL_ENV", &venv)
                    .env("UV_CACHE_DIR", format!("{}-cache", venv.display()))
                    .output()
                    .context("uv did not start")?;
                if !output.status.success() {
                    bail!(
                        "fleet install via {index_url} failed:\n{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                Ok(())
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("fleet worker never panics")?;
    }
    Ok(start.elapsed().as_secs_f64())
}

/// The request workload: a swarm of resolvers fetching project pages against each warm server.
///
/// # Errors
/// Returns an error when a server cannot start or its pages cannot be warmed.
pub async fn load(servers: &[Server], rates: &[f64], runs: usize, http: &reqwest::Client) -> anyhow::Result<()> {
    // measured[server][rate] holds each window's rps/p95/p99 samples and the total requests answered.
    let mut measured: Vec<Vec<RateResult>> = Vec::new();
    let mut costs: Vec<Option<Cost>> = Vec::new();
    for server in servers {
        let scratch = tempfile::tempdir()?;
        let state = scratch.path().join("state");
        std::fs::create_dir(&state)?;
        let active = server.start(&state, http).await?;
        warm_pages(&active.url, http).await?;
        let usage = Usage::watch(active.pid());
        let mut per_rate = Vec::new();
        for &rate in rates {
            println!("[load] {}: {rate:.0} req/s target", server.name);
            let mut result = RateResult::default();
            for _ in 0..runs {
                let window = swarm(&active.url, rate).await?;
                result.rps.push(window.rps);
                result.p95.push(window.p95);
                result.p99.push(window.p99);
                result.requests += window.requests;
            }
            per_rate.push(result);
        }
        costs.push(usage.finish());
        measured.push(per_rate);
    }
    let base = baseline(servers);
    let mut rows = Vec::new();
    for (index, &rate) in rates.iter().enumerate() {
        let label = format!("{rate:.0} req/s target");
        let column = |pick: fn(&RateResult) -> Vec<f64>| -> Vec<Option<Vec<f64>>> {
            measured.iter().map(|server| Some(pick(&server[index]))).collect()
        };
        rows.push(row_samples(
            &format!("{label}: achieved req/s"),
            &column(|r| r.rps.clone()),
            base,
            Metric::Rate("req/s"),
            Absent::Failed,
        ));
        rows.push(row_samples(
            &format!("{label}: p95 latency"),
            &column(|r| r.p95.clone()),
            base,
            Metric::Seconds,
            Absent::Failed,
        ));
        rows.push(row_samples(
            &format!("{label}: p99 latency"),
            &column(|r| r.p99.clone()),
            base,
            Metric::Seconds,
            Absent::Failed,
        ));
    }
    // Raw CPU seconds would reward doing nothing: the fast servers answer an order of magnitude
    // more requests inside the fixed window, so the cost row normalizes per thousand answered.
    let anchor = anchor(servers);
    #[expect(clippy::cast_precision_loss, reason = "request counts fit f64 exactly here")]
    let per_thousand: Vec<Option<f64>> = costs
        .iter()
        .zip(&measured)
        .map(|(cost, rate_results)| {
            let requests: usize = rate_results.iter().map(|result| result.requests).sum();
            cost.map(|cost| cost.cpu_seconds / (requests as f64 / 1000.0))
        })
        .collect();
    rows.push(row(
        "server CPU per 1,000 requests",
        &per_thousand,
        anchor,
        Metric::Seconds,
        Absent::NoServer,
    ));
    #[expect(clippy::cast_precision_loss, reason = "resident sizes fit f64 to the byte")]
    let rss: Vec<Option<f64>> = costs
        .iter()
        .map(|cost| cost.map(|cost| cost.peak_rss_bytes as f64 / 1e6))
        .collect();
    rows.push(row(
        "server peak memory",
        &rss,
        anchor,
        Metric::Amount("MB"),
        Absent::NoServer,
    ));
    publish(
        "load",
        table("simple-page requests under open-loop load", servers, base, rows),
    )
}

/// One swarm window's outcome.
struct Swarm {
    rps: f64,
    p95: f64,
    p99: f64,
    requests: usize,
}

async fn warm_pages(index_url: &str, http: &reqwest::Client) -> anyhow::Result<()> {
    for package in &TOP_PACKAGES[..10] {
        http.get(format!("{index_url}{package}/"))
            .header("Accept", "*/*")
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}

/// One rate's windows: the per-run rps and latency percentiles, plus the total requests answered.
#[derive(Default)]
struct RateResult {
    rps: Vec<f64>,
    p95: Vec<f64>,
    p99: Vec<f64>,
    requests: usize,
}

/// Drive an open-loop swarm at a constant target arrival rate for a fixed window, timing each
/// request against its *intended* send time rather than the moment it left the client.
///
/// A closed-loop swarm — each worker sending its next request only after the last one returns —
/// lets a stalled server throttle the offered load, so the requests that would have piled up during
/// the stall are never issued and the tail latency reads far lower than production would (Gil Tene's
/// coordinated omission). Here a bounded pool of connections pulls from a fixed schedule; when the
/// server cannot keep up, the requests fall behind their intended times and that queueing delay
/// lands in the measured latency, which is what a client under steady arrivals actually feels.
async fn swarm(index_url: &str, target_rps: f64) -> anyhow::Result<Swarm> {
    const WINDOW: Duration = Duration::from_secs(15);
    const CONNECTIONS: usize = 64;
    let start = Instant::now();
    let deadline = start + WINDOW;
    let next = Arc::new(AtomicU64::new(0));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..CONNECTIONS {
        let index_url = index_url.to_owned();
        let next = next.clone();
        tasks.spawn(async move {
            let client = reqwest::Client::builder().build().expect("client builds");
            let mut latencies = Vec::new();
            loop {
                let issued = next.fetch_add(1, Ordering::Relaxed);
                #[expect(clippy::cast_precision_loss, reason = "schedule index stays well under 2^53")]
                let intended = start + Duration::from_secs_f64(issued as f64 / target_rps);
                if intended >= deadline {
                    break;
                }
                let now = Instant::now();
                if now < intended {
                    tokio::time::sleep(intended - now).await;
                }
                let target = format!(
                    "{index_url}{}/",
                    TOP_PACKAGES[usize::try_from(issued).unwrap_or(0) % 10]
                );
                let outcome = async {
                    client
                        .get(&target)
                        .header("Accept", "*/*")
                        .send()
                        .await?
                        .error_for_status()?
                        .bytes()
                        .await
                }
                .await;
                if outcome.is_ok() {
                    latencies.push(intended.elapsed().as_secs_f64());
                }
            }
            latencies
        });
    }
    let mut latencies = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        latencies.extend(joined.expect("swarm connection never panics"));
    }
    if latencies.is_empty() {
        bail!("the swarm completed no requests");
    }
    latencies.sort_unstable_by(f64::total_cmp);
    #[expect(clippy::cast_precision_loss, reason = "request counts fit f64 exactly here")]
    let rps = latencies.len() as f64 / WINDOW.as_secs_f64();
    let quantile = |per_cent: usize| latencies[(latencies.len() * per_cent / 100).min(latencies.len() - 1)];
    Ok(Swarm {
        rps,
        p95: quantile(95),
        p99: quantile(99),
        requests: latencies.len(),
    })
}
