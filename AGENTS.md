# Repository Guidelines

## Overview

- A Rust implementation of the canonical Java `MobilityData/gtfs-validator`.
- Workspace crates live in `crates/`; core logic is in `gtfs_validator_core`, deterministic feed facts live in `gtfs_validator_profile`, and CLI, MCP, web, WASM, Python, and GUI are adapters.
- Benchmark inputs and the Java baseline live in `benchmark-feeds/` (`gtfs-validator.jar`).

## System Dependencies (Linux)

See `docs/system-dependencies.md` for the package list.

## Essential Commands

- `cargo build` (workspace build)
- `cargo build --release -p gtfs-guru` (CLI binary)
- `cargo run --release -p gtfs-guru-web` (local API server)
- `cargo test` or `cargo test -p gtfs-guru-core`
- `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings`
- Golden suite: build the release CLI first and hand it to the runner, so it
  does not fall back to `cargo run` and build the 13-16 GB debug tree:
  ```
  cargo build --release -p gtfs-guru
  GTFS_VALIDATOR_BIN=./target/release/gtfs-guru scripts/ci_golden.sh
  ```
  Omitting it on a near-full disk fails the build and the suite reports
  `validator failed for <case>`, which looks like a validator bug instead.
  See `docs/golden.md`.

## Detailed Guides

- Core validator and rules: `docs/agents/core-validator.md`
- CLI usage and outputs: `docs/agents/cli.md`
- Web API service: `docs/agents/web-api.md`
- Feed profiles, explanations, and MCP: `docs/agents/profile-mcp.md`
- WASM builds: `docs/agents/wasm.md`
- Python bindings: `docs/agents/python.md`
- Desktop GUI (Tauri): `docs/agents/gui.md`
- Benchmarks and profiling: `docs/agents/benchmarks.md`

## Contribution Basics

- Branch off `main`; do not push directly.
- Commit messages: imperative, present tense, <=72 chars.
- Run `cargo check` and `cargo test --all` before PRs; keep CI green.
