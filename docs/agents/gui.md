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

## Running inside a Wayland session

On a Linux host with Wayland, WebKitGTK's hardware-accelerated
compositing may fail on startup, and the app exits immediately with:

```text
Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display.
```

Work around it by disabling the DMA-BUF renderer, which falls back to
software compositing:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./target/debug/gtfs-guru-desktop
```

This also applies to `cargo tauri dev` (set the variable before the command).