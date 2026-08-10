# GTFS Guru

[![Crates.io](https://img.shields.io/crates/v/gtfs-guru.svg)](https://crates.io/crates/gtfs-guru)

Command-line interface for the [GTFS Guru](https://github.com/abasis-ltd/gtfs.guru)
validator — a fast, native replacement for the Java
[`MobilityData/gtfs-validator`](https://github.com/MobilityData/gtfs-validator),
with no JVM startup cost.

## Installation

```bash
cargo install gtfs-guru
```

Prebuilt binaries for macOS, Windows, and Linux are also published on the
[releases page](https://github.com/abasis-ltd/gtfs.guru/releases/latest).

## Usage

```bash
gtfs-guru --input path/to/gtfs.zip
```

Run `gtfs-guru --help` for the full option list, including JSON/HTML/SARIF
report output, `--country-code`, `--date`, and `--fail-on` thresholds for CI
use, plus the `diff`, `profile`, and `explain` subcommands.

See the [main project README](https://github.com/abasis-ltd/gtfs.guru) for
the full feature set, including the desktop app, Python bindings, and Web
API.

## License

Apache-2.0
