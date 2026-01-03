# GTFS Validator Desktop GUI

A beautiful cross-platform desktop application for GTFS validation, built with Rust and Tauri.

## Features

- **Modern UI**: Clean interface using Inter font and smooth animations
- **Fast**: Validates feeds in a separate thread without blocking UI
- **Native**: Runs as a standalone application on macOS, Windows, and Linux
- **Secure**: All validation happens locally on your machine

## Development

### Prerequisites

- Rust (latest stable)
- Node.js (optional, for frontend tools if needed)
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites)

### Run in Development Mode

```bash
cargo tauri dev
```

### Build for Release

```bash
cargo tauri build
```

 The output binary/installer will be located in `../../target/release/bundle/`.
