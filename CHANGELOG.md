# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0] - 2026-07-25

First stable release. The CLI, report, core, model, Python, WASM, and web
crates share the 1.0.0 version and their public APIs are now covered by
semantic versioning.

### Added

- `--fail-on <none|error|warning>` for CI quality gates. Reports are written
  before the process exits with status `2`.
- `--fix` and `--fix-unsafe` write a repaired copy of the feed. `--fix-output`
  picks the destination, defaulting to `<input>.fixed.<ext>` beside the input.
  The input is never modified and an existing output path is refused. Only the
  CSV records carrying an edit are re-serialized, so line endings, quoting, a
  UTF-8 BOM, and every untouched file survive byte for byte. A fix whose target
  field no longer holds the expected value is reported and skipped rather than
  applied.
- `gtfs_validator_core::fix` exposing `FixPlan` and `apply_fixes` for embedders.
- `gtfs-guru --version`.
- `service_never_active`, which flags calendar.txt rows with no active weekday
  and no added dates in calendar_dates.txt.
- `feed_version` in the Feed Info section of the JSON and HTML reports.
- Multithreaded in-browser validation: a wasm-threads build backed by a rayon
  Web Worker pool, served to cross-origin-isolated pages with an automatic
  fallback to the single-threaded package.
- In-browser validation of large feeds, with the work moved into a Web Worker
  so an out-of-memory failure kills the worker instead of the tab, and a
  main-thread fallback when the worker cannot load.
- An interactive map showing the stops a notice refers to.
- Prebuilt Python wheels for Linux x86_64/aarch64, macOS x86_64/arm64, and
  Windows x64, plus an sdist and post-upload PyPI verification.
- A pre-publish Python wheel smoke test covering the synchronous and
  asynchronous APIs, progress callbacks, notice conversion, and report export.
- A security policy and operator guidance.

### Changed

- Notices are capped per (code, severity) group in memory while the reported
  totals stay exact, so feeds with pervasive issues no longer exhaust the
  browser heap. Reports mark a capped list as truncated.
- In-browser validation is gated on the uncompressed feed size declared in the
  ZIP central directory (150 MB compressed / 700 MB uncompressed) rather than
  the compressed size alone, which predicts peak memory poorly in both
  directions.
- Python bindings now use PyO3 0.29.
- The desktop app shares the workspace's `reqwest` 0.12 dependency instead of
  retaining a second 0.11 TLS stack.
- `[PERF]` diagnostics require `GTFS_PERF_DEBUG`.
- SARIF output identifies the tool and repository as GTFS Guru.
- Documentation reports the current 110 validators and 190 notice codes, and
  uses the measured 4.6–6.7x benchmark results.
- `--threads` is documented as report metadata; actual parallelism is
  controlled with `RAYON_NUM_THREADS`.
- Parallel CSV loading is faster, and the hottest rules were optimized.
- Website polish: mobile navigation, scroll-reveal that degrades without
  JavaScript, clearer size-limit copy, and a cache-buster for site scripts.
- Tagging a release now rebuilds and deploys the website after the GitHub
  Release, crates.io, PyPI, and npm jobs succeed.
- Release binaries ship without debug symbols. Use the `profiling` profile
  (`cargo build --profile profiling`) when a flamegraph needs them.

### Fixed

- The browser's remote-feed fallback now uses the guarded Rust
  `/cors-proxy?url=...` endpoint instead of an unrestricted nginx forward
  proxy. It rejects private and reserved addresses on every redirect and
  limits request rate, concurrency, response size, and duration.
- IPv6 URL literals are classified directly, so public IPv6 hosts work while
  private and loopback literals remain blocked.
- Zip decompression is capped per member and across the archive.
- tzdb backward-compatibility link names (for example `Europe/Nicosia`,
  `US/Eastern`) are accepted, matching Java's `ZoneId`.
- Translation foreign-key lookups are indexed instead of scanned, which fixed a
  quadratic single-threaded hang on feeds with many translations.
- `duplicate_route_name` matches the canonical validator for extended route
  types.
- Service windows account for dates added through calendar_dates.txt.
- Placeholder notice codes from test modules no longer leak into the generated
  notice schema.
- Website container images remove deployment inputs such as `nginx.conf`,
  `Dockerfile`, Compose files, and `.env` from the public document root.
- Compose deployments require explicit PostgreSQL and Umami secrets.
- The Python package version is read from Cargo metadata, so a release can no
  longer publish wheels carrying the previous version.
- Updated `anyhow`, `bytes`, `crossbeam-epoch`, `plist`/`quick-xml`,
  `quinn-proto`, `rustls-webpki`, `tar`, `time`, and `wayland-scanner`.

## [0.9.4] - 2026-02-05

- Release metadata and documentation update for 0.9.4.

[Unreleased]: https://github.com/abasis-ltd/gtfs.guru/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/abasis-ltd/gtfs.guru/compare/v0.9.4...v1.0.0
[0.9.4]: https://github.com/abasis-ltd/gtfs.guru/releases/tag/v0.9.4
