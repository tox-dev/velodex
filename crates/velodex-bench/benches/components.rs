//! Component microbenchmarks: the pure, CPU-bound work velodex does on the hot path, measured in
//! isolation so `CodSpeed`'s simulation instrument reads them deterministically on shared CI.
//!
//! These are the instrumented subset of the benchmark suite whose wall-clock half — the six-server
//! comparison — lives in this crate's binary. They call the real crate functions on representative
//! project pages (small like `flask`, medium like `requests`, large like `boto3`), so a regression
//! in parsing, transforming, serializing, versioning, or hashing shows up per commit.
#![allow(
    clippy::significant_drop_tightening,
    reason = "criterion_group! expands to a temporary the nursery lint flags"
)]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use url::Url;
use velodex_core::pypi::{Meta, ProjectDetail, parse_detail, parse_detail_html, parse_metadata, sorted_desc, to_json};
use velodex_http::stream::{PageTransformer, page_context};
use velodex_storage::blob::Digest;

/// The page shapes the benchmarks sweep, named for the real projects they resemble.
const PAGES: &[(&str, usize)] = &[("flask", 12), ("requests", 100), ("boto3", 500)];

/// A PEP 691 JSON project page with `files` entries, each carrying a sha256, requires-python, size,
/// upload time, and a PEP 658 core-metadata hash — the full shape the transformer rewrites.
fn json_page(files: usize) -> String {
    let mut out = String::from(r#"{"meta":{"api-version":"1.1"},"name":"sample","versions":["#);
    for version in 0..files / 8 {
        if version > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{version}.0.0\"");
    }
    out.push_str(r#"],"files":["#);
    for file in 0..files {
        if file > 0 {
            out.push(',');
        }
        let sha = format!("{file:064x}");
        let (version, size, day) = (file / 8, 5_000_000 + file * 137, (file % 28) + 1);
        let _ = write!(
            out,
            "{{\"filename\":\"sample-{version}.0.0-cp312-cp312-manylinux_2_17_x86_64.whl\",\
             \"url\":\"https://files.pythonhosted.org/packages/{a}/{b}/{sha}/sample.whl\",\
             \"hashes\":{{\"sha256\":\"{sha}\"}},\"requires-python\":\">=3.9\",\"size\":{size},\
             \"upload-time\":\"2026-01-{day:02}T12:00:00.000000Z\",\
             \"core-metadata\":{{\"sha256\":\"{sha}\"}}}}",
            a = &sha[0..2],
            b = &sha[2..4],
        );
    }
    out.push_str("]}");
    out
}

/// The same page as PEP 503 HTML anchors, the fallback some indexes still serve.
fn html_page(files: usize) -> String {
    let mut out = String::from("<!DOCTYPE html><html><body>\n");
    for file in 0..files {
        let sha = format!("{file:064x}");
        let version = file / 8;
        let _ = writeln!(
            out,
            "<a href=\"https://files.pythonhosted.org/packages/{sha}/sample-{version}.0.0-py3-none-any.whl#sha256={sha}\" \
             data-requires-python=\"&gt;=3.9\" data-core-metadata=\"sha256={sha}\">sample-{version}.0.0-py3-none-any.whl</a><br/>",
        );
    }
    out.push_str("</body></html>");
    out
}

/// Turn a parsed page into the owned model `to_json` serializes.
fn detail_of(json: &str) -> ProjectDetail {
    let parsed = parse_detail(json.as_bytes()).unwrap();
    ProjectDetail {
        meta: Meta::default(),
        name: parsed.name,
        versions: parsed.versions,
        files: parsed.files,
    }
}

/// Parse an upstream JSON page: the first CPU step of every cache miss.
fn bench_parse_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_detail_json");
    for &(name, files) in PAGES {
        let page = json_page(files);
        group.throughput(Throughput::Bytes(page.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &page, |b, page| {
            b.iter(|| parse_detail(black_box(page.as_bytes())).unwrap());
        });
    }
    group.finish();
}

/// Parse an upstream HTML page: the same step for indexes that only speak PEP 503.
fn bench_parse_html(c: &mut Criterion) {
    let base = Url::parse("https://pypi.org/simple/sample/").unwrap();
    let mut group = c.benchmark_group("parse_detail_html");
    for &(name, files) in PAGES {
        let page = html_page(files);
        group.throughput(Throughput::Bytes(page.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &page, |b, page| {
            b.iter(|| parse_detail_html(black_box("sample"), black_box(page), black_box(&base)));
        });
    }
    group.finish();
}

/// Serialize the local page model back to PEP 691 JSON: the last CPU step before the client.
fn bench_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("to_json");
    for &(name, files) in PAGES {
        let detail = detail_of(&json_page(files));
        group.bench_with_input(BenchmarkId::from_parameter(name), &detail, |b, detail| {
            b.iter(|| to_json(black_box(detail)));
        });
    }
    group.finish();
}

/// The streaming rewrite: velodex runs every cache-missed page through this, rewriting file URLs to
/// the local route and recording sources as bytes flow to the client. This is the defining hot path.
fn bench_transform(c: &mut Criterion) {
    let mut group = c.benchmark_group("page_transform");
    for &(name, files) in PAGES {
        let page = json_page(files);
        group.throughput(Throughput::Bytes(page.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &page, |b, page| {
            b.iter(|| {
                let mut transformer =
                    PageTransformer::new(page_context("root/pypi", Vec::new(), Vec::new(), &HashMap::new()));
                let mut sink = Vec::new();
                for chunk in black_box(page.as_bytes()).chunks(64 * 1024) {
                    sink.extend_from_slice(&transformer.push(chunk).unwrap());
                }
                black_box((sink, transformer.finish().unwrap()));
            });
        });
    }
    group.finish();
}

/// Sort a project's versions newest-first: the PEP 440 ordering applied when merging overlay layers.
fn bench_version_sort(c: &mut Criterion) {
    let versions: Vec<String> = (0..300)
        .map(|n| format!("{}.{}.{}", n / 100, (n / 10) % 10, n % 10))
        .collect();
    c.bench_function("version_sort/300", |b| {
        b.iter(|| sorted_desc(black_box(&versions)));
    });
}

/// Parse a PEP 658 core-metadata document: served on the resolver fast path so uv skips the wheel.
fn bench_metadata(c: &mut Criterion) {
    let metadata = "Metadata-Version: 2.1\nName: sample\nVersion: 1.2.3\n\
         Summary: A representative package for benchmarking\n\
         Requires-Python: >=3.9\nLicense: MIT\nAuthor: Example\n\
         Requires-Dist: requests>=2.0\nRequires-Dist: click\nRequires-Dist: pydantic>=2\n\n\
         The long description body that trails the headers and is skipped by the parser.\n"
        .to_owned();
    c.bench_function("parse_metadata", |b| {
        b.iter(|| parse_metadata(black_box(&metadata)));
    });
}

/// Hash an artifact's bytes: velodex verifies every cached blob against the promised sha256.
fn bench_digest(c: &mut Criterion) {
    let mut group = c.benchmark_group("digest");
    for size in [256_usize * 1024, 8 * 1024 * 1024] {
        let data: Vec<u8> = (0..size)
            .map(|index| u8::try_from(index % 256).expect("0..256 fits u8"))
            .collect();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}KiB", size / 1024)),
            &data,
            |b, data| {
                b.iter(|| Digest::of(black_box(data)));
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_parse_json,
    bench_parse_html,
    bench_serialize,
    bench_transform,
    bench_version_sort,
    bench_metadata,
    bench_digest,
);
criterion_main!(benches);
