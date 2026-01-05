use criterion::{criterion_group, criterion_main, Criterion};
use gtfs_guru_core::{GtfsFeed, GtfsInput};
use std::path::PathBuf;

fn benchmark_loading(c: &mut Criterion) {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.pop(); // crates
    d.pop(); // root
    d.push("benchmark-feeds");
    d.push("nl.zip");

    if !d.exists() {
        eprintln!("Benchmark file not found: {:?}", d);
        return;
    }

    c.bench_function("load_nl_zip_feed", |b| {
        b.iter(|| {
            let input = GtfsInput::from_path(&d).expect("Failed to create input");
            let _feed = GtfsFeed::from_reader(&input.reader()).expect("Failed to load feed");
        })
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark_loading
);
criterion_main!(benches);
