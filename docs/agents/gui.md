# Desktop GUI (gtfs_validator_gui)

## Scope

- Tauri-based desktop app wrapping the Rust validator.
- UI and validation run locally; validation happens off the UI thread.

## Prerequisites

- Rust (stable)
- Tauri CLI (`cargo install tauri-cli`)
- On Linux, GTK/WebKit system packages – see `docs/system-dependencies.md`
- Node.js only if frontend tooling is needed

## Development and Build

```bash
cargo tauri dev
```

```bash
cargo tauri build
```

Release artifacts are written under `target/release/bundle/`.
