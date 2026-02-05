# Benchmarks and Profiling

## Inputs and Baseline

- Feeds and baseline jar live in `benchmark-feeds/`.
- Java baseline: `benchmark-feeds/gtfs-validator.jar`.
- Keep output directories alongside inputs (see `benchmark-feeds/output-java-*` and `benchmark-feeds/output-guru-*`).

## Running Benchmarks

- Build the Rust CLI:

```bash
cargo build --release -p gtfs-guru
```

- Run the Rust validator on a feed and write to a dedicated output dir:

```bash
./target/release/gtfs-guru -i benchmark-feeds/nl.zip -o benchmark-feeds/output-guru-nl
```

- Run the Java validator from the `../gtfs-validator` repo with the same input and output naming.
- Record wall time, commit hash, and machine details for comparisons.

## Comparing Outputs

- Use `scripts/compare_reports.py` to diff report outputs:

```bash
python3 scripts/compare_reports.py \
  benchmark-feeds/output-java-nl \
  benchmark-feeds/output-guru-nl \
  --strip-runtime-fields --ignore-notice-order
```

## Profiling

- Profile the Rust CLI when it regresses vs Java; use release builds and the same feed.
- Linux example (if `perf` is installed):

```bash
perf record --call-graph dwarf -- \
  ./target/release/gtfs-guru -i benchmark-feeds/nl.zip -o benchmark-feeds/output-guru-nl
perf report
```

- For flamegraphs, `cargo flamegraph` is supported when installed.
