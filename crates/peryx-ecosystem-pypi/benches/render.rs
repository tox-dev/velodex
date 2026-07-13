#![allow(
    clippy::significant_drop_tightening,
    reason = "criterion_group! expands to a temporary flagged by this nursery lint"
)]

#[path = "support/detail.rs"]
mod detail;
#[path = "support/index.rs"]
mod index;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use peryx_ecosystem_pypi::{render_detail_html, render_index_html, render_legacy_json, to_json};

use detail::project_detail;
use index::index_list;

const SMALL: usize = 3;
const LARGE: usize = 400;

fn bench_render(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("render");
    for (label, files) in [("small", SMALL), ("large", LARGE)] {
        let detail = project_detail("flask", files);
        group.bench_function(BenchmarkId::new("to_json", label), |bencher| {
            bencher.iter(|| to_json(std::hint::black_box(&detail)));
        });
        group.bench_function(BenchmarkId::new("detail_html", label), |bencher| {
            bencher.iter(|| render_detail_html(std::hint::black_box(&detail)));
        });
        group.bench_function(BenchmarkId::new("legacy_json", label), |bencher| {
            bencher.iter(|| render_legacy_json(std::hint::black_box(&detail), None));
        });
    }
    let list = index_list(LARGE);
    group.bench_function("index_html", |bencher| {
        bencher.iter(|| render_index_html(std::hint::black_box(&list)));
    });
    group.finish();
}

criterion_group!(benches, bench_render);
criterion_main!(benches);
