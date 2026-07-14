#![allow(
    clippy::significant_drop_tightening,
    reason = "criterion_group! expands to a temporary flagged by this nursery lint"
)]

#[path = "support/detail.rs"]
mod detail;
#[path = "support/index.rs"]
mod index;
#[path = "support/metadata.rs"]
mod metadata;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use peryx_ecosystem_pypi::{
    parse_detail, parse_detail_html, parse_distribution_filename, parse_index, parse_index_html, parse_metadata,
    render_detail_html, render_index_html, to_json,
};
use url::Url;

use detail::project_detail;
use index::index_list;
use metadata::METADATA;

const SMALL: usize = 3;
const LARGE: usize = 400;

fn bench_parse(criterion: &mut Criterion) {
    let base = Url::parse("https://pypi.org/simple/flask/").unwrap();
    let mut group = criterion.benchmark_group("parse");
    for (label, files) in [("small", SMALL), ("large", LARGE)] {
        let detail = project_detail("flask", files);
        let json = to_json(&detail).into_bytes();
        let html = render_detail_html(&detail);
        group.bench_function(BenchmarkId::new("detail_json", label), |bencher| {
            bencher.iter(|| parse_detail(std::hint::black_box(&json)).unwrap());
        });
        group.bench_function(BenchmarkId::new("detail_html", label), |bencher| {
            bencher.iter(|| parse_detail_html("flask", std::hint::black_box(&html), &base).unwrap());
        });
    }
    let list = index_list(LARGE);
    let index_json = to_json(&list).into_bytes();
    let index_html = render_index_html(&list);
    group.bench_function("index_json", |bencher| {
        bencher.iter(|| parse_index(std::hint::black_box(&index_json)).unwrap());
    });
    group.bench_function("index_html", |bencher| {
        bencher.iter(|| parse_index_html(std::hint::black_box(&index_html), &base).unwrap());
    });
    group.bench_function("metadata", |bencher| {
        bencher.iter(|| parse_metadata(std::hint::black_box(METADATA)).unwrap());
    });
    group.bench_function("distribution_filename", |bencher| {
        bencher.iter(|| {
            parse_distribution_filename(std::hint::black_box(
                "flask-3.0.0-cp312-cp312-manylinux_2_17_x86_64.whl",
            ))
        });
    });
    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
