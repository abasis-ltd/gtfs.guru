# Installation

## From Source (Rust)

```bash
git clone https://github.com/abasis-ltd/gtfs.guru
cd gtfs.guru
cargo build --release
```

Binaries will be in `target/release/`:

- `gtfs-guru` — command-line tool
- `gtfs-guru-web` — web server

## Python Package

```bash
# From PyPI (when published)
pip install gtfs-validator

# From source
cd crates/gtfs_validator_python
pip install maturin
maturin build --release
pip install target/wheels/gtfs_validator-*.whl
```
