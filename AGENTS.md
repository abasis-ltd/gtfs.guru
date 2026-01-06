# Repository Guidelines

## Overview

- Rust implementation of the Java validator at `../gtfs-validator` (`/Users/akimov/Documents/GitHub/gtfs-validator`).
- Workspace crates live in `crates/`; core logic is in `gtfs_validator_core` with CLI, web, WASM, Python, and GUI front-ends.
- Benchmark inputs and the Java baseline live in `benchmark-feeds/` (`gtfs-validator.jar`).

## Essential Commands

- `cargo build` (workspace build)
- `cargo build --release -p gtfs-guru-cli` (CLI binary)
- `cargo run --release -p gtfs-guru-web` (local API server)
- `cargo test` or `cargo test -p gtfs_validator_core`
- `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings`

## Detailed Guides

- Core validator and rules: `docs/agents/core-validator.md`
- CLI usage and outputs: `docs/agents/cli.md`
- Web API service: `docs/agents/web-api.md`
- WASM builds: `docs/agents/wasm.md`
- Python bindings: `docs/agents/python.md`
- Desktop GUI (Tauri): `docs/agents/gui.md`
- Benchmarks and profiling: `docs/agents/benchmarks.md`

## Contribution Basics

- Branch off `main`; do not push directly.
- Commit messages: imperative, present tense, <=72 chars.
- Run `cargo check` and `cargo test --all` before PRs; keep CI green.
