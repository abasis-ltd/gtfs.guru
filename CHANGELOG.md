# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `--fail-on <none|error|warning>` for CI quality gates. Reports are written
  before the process exits with status `2`.
- `--fix` and `--fix-unsafe` now write a repaired copy of the feed instead of
  printing a plan and failing. `--fix-output` picks the destination, defaulting
  to `<input>.fixed.<ext>` beside the input. The input is never modified and an
  existing output path is refused. Only the CSV records carrying an edit are
  re-serialized, so line endings, quoting, a UTF-8 BOM, and every untouched file
  survive byte for byte. A fix whose target field no longer holds the expected
  value is reported and skipped rather than applied.
- `gtfs_validator_core::fix` exposing `FixPlan` and `apply_fixes` for embedders.
- `gtfs-guru --version`.
- Prebuilt Python wheels for Linux x86_64/aarch64, macOS x86_64/arm64, and
  Windows x64, plus an sdist and post-upload PyPI verification.
- A pre-publish Python wheel smoke test covering the synchronous and
  asynchronous APIs, progress callbacks, notice conversion, and report export.
- A security policy and operator guidance.

### Changed

- Python bindings now use PyO3 0.29.
- The desktop app shares the workspace's `reqwest` 0.12 dependency instead of
  retaining a second 0.11 TLS stack.
- `[PERF]` diagnostics require `GTFS_PERF_DEBUG`.
- SARIF output identifies the tool and repository as GTFS Guru.
- Documentation now reports the current 109 validators and 189 notice codes,
  and uses the measured 4.6–6.7x benchmark results.
- `--threads` is documented as report metadata; actual parallelism is
  controlled with `RAYON_NUM_THREADS`.

### Fixed

- The browser's remote-feed fallback now uses the guarded Rust
  `/cors-proxy?url=...` endpoint instead of an unrestricted nginx forward
  proxy. It rejects private and reserved addresses on every redirect and
  limits request rate, concurrency, response size, and duration.
- IPv6 URL literals are classified directly, so public IPv6 hosts work while
  private and loopback literals remain blocked.
- `--fix` and `--fix-unsafe` no longer imply that files were changed. File
  rewriting is not implemented, so these modes print the plan and fail.
- Placeholder notice codes from test modules no longer leak into the generated
  notice schema.
- Website container images remove deployment inputs such as `nginx.conf`,
  `Dockerfile`, Compose files, and `.env` from the public document root.
- Compose deployments require explicit PostgreSQL and Umami secrets.
- Updated `anyhow`, `bytes`, `crossbeam-epoch`, `plist`/`quick-xml`,
  `quinn-proto`, `rustls-webpki`, `tar`, `time`, and `wayland-scanner`.

## [0.9.4] - 2026-02-05

- Release metadata and documentation update for 0.9.4.

[Unreleased]: https://github.com/abasis-ltd/gtfs.guru/compare/v0.9.4...HEAD
[0.9.4]: https://github.com/abasis-ltd/gtfs.guru/releases/tag/v0.9.4
