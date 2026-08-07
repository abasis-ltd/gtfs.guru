# GTFS Guru Report

[![Crates.io](https://img.shields.io/crates/v/gtfs-guru-report.svg)](https://crates.io/crates/gtfs-guru-report)

Reporting structures for the [GTFS Guru](https://github.com/abasis-ltd/gtfs.guru)
validator: turns the notices produced by
[`gtfs-guru-core`](https://crates.io/crates/gtfs-guru-core) into a
`ValidationReport`, with JSON, HTML, and SARIF output support (the same JSON
report shape as the Java `MobilityData/gtfs-validator`, for drop-in CI
compatibility).

## Installation

```bash
cargo add gtfs-guru-report
```

## License

Apache-2.0
