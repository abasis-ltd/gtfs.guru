# GTFS Guru Model

[![Crates.io](https://img.shields.io/crates/v/gtfs-guru-model.svg)](https://crates.io/crates/gtfs-guru-model)

Data models for GTFS (General Transit Feed Specification): the shared types
(`RouteType`, `GtfsDate`, `ExceptionType`, `ServiceAvailability`, `StringId`,
...) used across [GTFS Guru](https://github.com/abasis-ltd/gtfs.guru), a fast
native GTFS validator written in Rust.

This crate has no validation logic of its own — see
[`gtfs-guru-core`](https://crates.io/crates/gtfs-guru-core) for that. It
exists so the model types can be shared between the core validator, the
report/profile crates, and downstream tools without pulling in the rest of
the validation engine.

## Installation

```bash
cargo add gtfs-guru-model
```

## License

Apache-2.0
