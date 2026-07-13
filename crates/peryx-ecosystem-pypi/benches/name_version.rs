#![allow(
    clippy::significant_drop_tightening,
    reason = "criterion_group! expands to a temporary flagged by this nursery lint"
)]

use criterion::{Criterion, criterion_group, criterion_main};
use peryx_ecosystem_pypi::{normalize_name, parse_version, parse_version_specifiers, sorted_desc};

const LARGE: usize = 400;

fn bench_name_version(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("name_version");
    group.bench_function("normalize_name", |bencher| {
        bencher.iter(|| normalize_name(std::hint::black_box("Flask.Extension_Name")));
    });
    group.bench_function("parse_version", |bencher| {
        bencher.iter(|| parse_version(std::hint::black_box("3.0.1.post2")));
    });
    group.bench_function("parse_version_specifiers", |bencher| {
        bencher.iter(|| parse_version_specifiers(std::hint::black_box(">=3.8,<4.0,!=3.9.1")));
    });
    let versions: Vec<String> = (0..LARGE)
        .map(|index| format!("{}.{}.{}", index / 100, (index / 10) % 10, index % 10))
        .collect();
    group.bench_function("sorted_desc", |bencher| {
        bencher.iter(|| sorted_desc(std::hint::black_box(&versions)));
    });
    group.finish();
}

criterion_group!(benches, bench_name_version);
criterion_main!(benches);
