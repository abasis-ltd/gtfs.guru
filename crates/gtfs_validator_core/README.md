# GTFS Guru Core

[![Crates.io](https://img.shields.io/crates/v/gtfs-guru-core.svg)](https://crates.io/crates/gtfs-guru-core)

Core validation logic for [GTFS Guru](https://github.com/abasis-ltd/gtfs.guru)
— a fast, native, multi-platform GTFS validator. This crate loads and parses
a GTFS feed (`GtfsFeed`, `GtfsInput`), runs its validators, and produces
`ValidationNotice`s through the notice/`NoticeContainer` machinery, with an
optional `parallel` feature (enabled by default) for multi-threaded loading
and validation via `rayon`.

This is the engine used by the [`gtfs-guru`](https://crates.io/crates/gtfs-guru)
CLI, the desktop app, the Python bindings, and the WebAssembly build; use it
directly if you want to embed GTFS validation in your own Rust project.

## Installation

```bash
cargo add gtfs-guru-core
```

## License

Apache-2.0
