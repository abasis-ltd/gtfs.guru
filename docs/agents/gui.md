# Desktop GUI (gtfs_validator_gui)

## Scope

- Tauri-based desktop app wrapping the Rust validator.
- UI and validation run locally; validation happens off the UI thread.

## Prerequisites

- Rust (stable)
- Tauri CLI (see https://tauri.app/v1/guides/getting-started/prerequisites)
- Node.js only if frontend tooling is needed

## Development and Build

```bash
cargo tauri dev
```

```bash
cargo tauri build
```

Release artifacts are written under `target/release/bundle/`.
