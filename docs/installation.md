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

## Prebuilt binaries

Download the CLI for your platform from the
[latest release](https://github.com/abasis-ltd/gtfs.guru/releases/latest), or use
the installer:

```bash
curl -fsSL https://raw.githubusercontent.com/abasis-ltd/gtfs.guru/main/scripts/install.sh | bash
```

```powershell
iwr -useb https://raw.githubusercontent.com/abasis-ltd/gtfs.guru/main/scripts/install.ps1 | iex
```

## Python Package

```bash
pip install gtfs-guru

# From source
cd crates/gtfs_validator_python
pip install maturin
maturin build --release
pip install target/wheels/gtfs_guru-*.whl
```
