#![allow(
    clippy::significant_drop_tightening,
    reason = "criterion_group! expands to a temporary flagged by this nursery lint"
)]

#[path = "support/registry.rs"]
mod registry;

use criterion::{Criterion, criterion_group, criterion_main};

use registry::{get, runtime, seeded};

fn bench_blob_serve(criterion: &mut Criterion) {
    let runtime = runtime();
    let (_dir, app, _, blob_digest) = seeded(&runtime);
    let uri = format!("/v2/store/app/blobs/{blob_digest}");
    criterion.bench_function("oci_blob_serve", |bencher| {
        bencher.to_async(&runtime).iter(|| get(&app, &uri));
    });
}

criterion_group!(benches, bench_blob_serve);
criterion_main!(benches);
