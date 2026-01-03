# Installation

## From Source (Rust)

```bash
git clone https://github.com/user/gtfs-validator-rust
cd gtfs-validator-rust
cargo build --release
```

Binaries will be in `target/release/`:
- `gtfs_validator_cli` — command-line tool
- `gtfs_validator_web` — web server

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
