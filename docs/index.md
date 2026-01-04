# GTFS.Guru

**High-performance GTFS feed validator written in Rust.**

Full compatibility with [MobilityData gtfs-validator](https://github.com/MobilityData/gtfs-validator) (Java), with identical validation rules and output format — but 10-50x faster.

## Why GTFS.Guru?

| Feature | Java Validator | GTFS.Guru |
|---------|---------------|-----------|
| **Speed** | 1x | 10-50x faster |
| **Memory** | ~500MB | ~50MB |
| **Binary size** | 50MB (JAR) | 5MB (CLI) |
| **Python bindings** | ❌ | ✅ |
| **WebAssembly** | ❌ | ✅ |
| **Desktop app** | ❌ | ✅ |

## Quick Start

=== "Python"

    ```bash
    pip install gtfs-validator
    ```

    ```python
    import gtfs_validator

    result = gtfs_validator.validate("/path/to/gtfs.zip")
    print(f"Valid: {result.is_valid}, Errors: {result.error_count}")
    ```

=== "Command Line"

    ```bash
    # Build
    cargo build --release -p gtfs_validator_cli

    # Run
    ./target/release/gtfs_validator_cli \
        --input /path/to/gtfs.zip \
        --output_base /tmp/report
    ```

=== "Web API"

    ```bash
    cargo run --release -p gtfs_validator_web
    # API at http://localhost:3000
    ```

## Features

- **88 validation rules** — full parity with Java gtfs-validator
- **Multiple interfaces** — CLI, Web API, Python bindings, Desktop App, WebAssembly
- **Cross-platform** — macOS, Linux, Windows
- **Detailed reports** — JSON and HTML output with geographic context
- **Auto-fix suggestions** — machine-applicable fixes for common issues

## Next Steps

- [Installation](installation.md) — Install via pip, cargo, or download binaries
- [CLI Usage](usage.md) — Command-line options and examples
- [Python API](python_api.md) — Python bindings documentation
- [Validation Rules](rules.md) — All 88 validation rules explained
