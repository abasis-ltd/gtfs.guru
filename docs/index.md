# GTFS.Guru

**High-performance GTFS feed validator written in Rust.**

Closely compatible with [MobilityData gtfs-validator](https://github.com/MobilityData/gtfs-validator) (Java), with the same report shape and familiar notice codes. The rule sets are not identical.

## Why GTFS.Guru?

| Feature | Java Validator | GTFS.Guru |
|---------|---------------|-----------|
| **Speed** | 1x | 4.6–6.7x faster on published benchmarks |
| **Startup** | JVM warm-up | Native binary, no runtime |
| **Python bindings** | ❌ | ✅ |
| **WebAssembly** | ❌ | ✅ |
| **Parsing Strategy** | Serial | Parallel (Rayon) |
| **Desktop app** | ❌ | ✅ |

## Quick Start

=== "Python"

    ```bash
    pip install gtfs-guru
    ```

    ```python
    import gtfs_guru

    result = gtfs_guru.validate("/path/to/gtfs.zip")
    print(f"Valid: {result.is_valid}, Errors: {result.error_count}")
    ```

=== "Command Line"

    ```bash
    # Build
    cargo build --release -p gtfs-guru

    # Run
    ./target/release/gtfs-guru \
        --input /path/to/gtfs.zip \
        --output_base /tmp/report
    ```

=== "Web API"

    ```bash
    cargo run --release -p gtfs-guru-web
    # API at http://localhost:3000
    ```

## Features

- **110 validation rules** — broad coverage including Fares v2, Flex and Pathways
- **Multiple interfaces** — CLI, Web API, Python bindings, Desktop App, WebAssembly
- **Cross-platform** — macOS, Linux, Windows
- **Detailed reports** — JSON, HTML and SARIF output with geographic context
- **Auto-fix** — `--fix-dry-run` lists suggested edits, `--fix` writes a repaired copy of the feed
- **Robust CSV Parsing** — handles spaces in headers and other common format issues

## Next Steps

- [Installation](installation.md) — Install via pip, cargo, or download binaries
- [CLI Usage](usage.md) — Command-line options and examples
- [Python API](python_api.md) — Python bindings documentation
- [Validation Rules](rules.md) — Notice codes emitted by the 110 validators
