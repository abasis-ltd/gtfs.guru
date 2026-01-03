# GTFS Validator (Rust)

High-performance GTFS feed validator written in Rust. Full compatibility with [MobilityData gtfs-validator](https://github.com/MobilityData/gtfs-validator) (Java), with identical validation rules and output format.

## Features

- **88 validation rules** — full parity with Java gtfs-validator
- **Fast** — 10-50x faster than Java version
- **Multiple interfaces** — CLI, Web API, Python bindings
- **Cross-platform** — macOS, Linux, Windows
- **Detailed reports** — JSON and HTML output

## Quick Start

### Python (recommended)

```bash
pip install gtfs-validator
```

```python
import gtfs_validator

result = gtfs_validator.validate("/path/to/gtfs.zip")
print(f"Valid: {result.is_valid}, Errors: {result.error_count}")
```

### Command Line

```bash
# Build
cargo build --release -p gtfs_validator_cli

# Run
./target/release/gtfs_validator_cli \
    --input /path/to/gtfs.zip \
    --output_base /tmp/report
```
