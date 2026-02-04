# GTFS Guru 🚀

[![CI](https://github.com/abasis-ltd/gtfs.guru/actions/workflows/rust.yml/badge.svg)](https://github.com/abasis-ltd/gtfs.guru/actions)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/gtfs-guru.svg)](https://crates.io/crates/gtfs-guru)
[![PyPI](https://img.shields.io/pypi/v/gtfs-guru.svg)](https://pypi.org/project/gtfs-guru/)

**The world's fastest and most versatile GTFS validator.**

GTFS Guru is a next-generation tool to check your transit data (GTFS) for errors. It ensures your schedules, routes, and stops are correct before they go live on Google Maps, Apple Maps, or other journey planners.

> 💡 **Inspired by [MobilityData/gtfs-validator](https://github.com/MobilityData/gtfs-validator)**. We rebuilt the validation logic from the ground up in Rust to achieve blazing speed, privacy, and universal portability.

---

## 🌟 Why GTFS Guru?

1. **Unmatched Speed**: Validates large feeds in milliseconds, not minutes. Typically **50x-100x faster** than the reference Java validator.
2. **Privacy First**: Runs locally on your machine. No need to upload sensitive or pre-release schedules to the cloud.
3. **Cross-Platform**: Available as a desktop app, command-line tool, Python library, and WebAssembly module.
4. **Developer Friendly**: integrate directly into your data pipelines with native Python (`pip`) and Rust (`cargo`) bindings.

| Feature | Java Validator | **GTFS Guru (Rust)** |
| :--- | :---: | :---: |
| **Speed** | 🐢 ~1.5s / feed | 🚀 **~0.01s / feed** |
| **Memory** | 🐘 Heavy (JVM) | 🪶 **Light (Native)** |
| **Platform** | Java Runtime Required | **Standalone Binary** |
| **Python** | ❌ Wrapper only | ✅ **Native (`pip install`)** |
| **Web** | ❌ Server-side only | ✅ **Browser-native (WASM)** |

---

## 📥 Installation

### 👨‍💼 For Non-Developers (Desktop App)

The easiest way to validate feeds without using the command line.

1. Go to the [**Releases Page**](https://github.com/abasis-ltd/gtfs.guru/releases/latest).
2. Download the installer for your OS (these links always point to the latest release):
    * 🍎 **macOS (Universal)**: [`gtfs-guru-macos.dmg`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-macos.dmg)
    * 🪟 **Windows (x64)**: [`gtfs-guru-windows-x64-setup.exe`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-windows-x64-setup.exe)
    * 🐧 **Linux (Debian)**: [`gtfs-guru-linux-amd64.deb`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-amd64.deb)
    * 🐧 **Linux (AppImage)**: [`gtfs-guru-linux-amd64.AppImage`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-amd64.AppImage)
3. Run the installer and launch the app. Drag and drop your `gtfs.zip` file to validate!

### 🐍 For Python Developers (Data Science)

Perfect for checking data integrity within Jupyter Notebooks or ETL pipelines.

```bash
pip install gtfs-guru
```

```python
import gtfs_guru

# Validate a feed return a rich report object
report = gtfs_guru.validate("path/to/gtfs.zip")

print(f"Valid: {report.is_valid}")
print(f"Notices: {len(report.notices)}")

# Export results
report.save_html("validation_report.html")
report.save_json("report.json")
```

### 🧰 For CLI Users (Prebuilt Binaries)

Download the latest CLI for your platform:

* 🍎 **macOS (Universal)**: [`gtfs-guru-macos-universal.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-macos-universal.tar.gz)
* 🍎 **macOS (arm64)**: [`gtfs-guru-macos-arm64.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-macos-arm64.tar.gz)
* 🍎 **macOS (x86_64)**: [`gtfs-guru-macos-x86_64.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-macos-x86_64.tar.gz)
* 🐧 **Linux (x86_64, glibc)**: [`gtfs-guru-linux-x86_64.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-x86_64.tar.gz)
* 🐧 **Linux (x86_64, musl)**: [`gtfs-guru-linux-x86_64-musl.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-x86_64-musl.tar.gz)
* 🐧 **Linux (x86_64, static alias)**: [`gtfs-guru-linux-x86_64-static.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-x86_64-static.tar.gz)
* 🐧 **Linux (arm64)**: [`gtfs-guru-linux-aarch64.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-aarch64.tar.gz)
* 🪟 **Windows (x64)**: [`gtfs-guru-windows-x64.zip`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-windows-x64.zip)

**One-liner (macOS/Linux):**

```bash
curl -fsSL https://raw.githubusercontent.com/abasis-ltd/gtfs.guru/main/scripts/install.sh | bash
```

**One-liner (Windows PowerShell):**

```powershell
iwr -useb https://raw.githubusercontent.com/abasis-ltd/gtfs.guru/main/scripts/install.ps1 | iex
```

Optional env vars:
* `INSTALL_DIR=/custom/bin`
* `GTFS_GURU_LINUX_FLAVOR=musl` (x86_64 Linux only)
* `GTFS_GURU_VERSION=v0.9.0`

**CI examples (GitHub Actions):**

```yaml
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install gtfs-guru
        run: |
          curl -fsSL https://raw.githubusercontent.com/abasis-ltd/gtfs.guru/main/scripts/install.sh | bash
          echo "$HOME/.local/bin" >> $GITHUB_PATH
      - name: Run validation
        run: gtfs-guru -i feed.zip -o out
```

**CI examples (GitLab CI):**

```yaml
validate:
  image: ubuntu:22.04
  before_script:
    - apt-get update && apt-get install -y ca-certificates curl
    - curl -fsSL https://raw.githubusercontent.com/abasis-ltd/gtfs.guru/main/scripts/install.sh | bash
    - export PATH="$HOME/.local/bin:$PATH"
  script:
    - gtfs-guru -i feed.zip -o out
```

### 🦀 For Rust Developers (CLI)

The classic high-performance command-line interface.

**From Crates.io:**

```bash
cargo install gtfs-guru
```

**Build from Source:**

```bash
git clone https://github.com/abasis-ltd/gtfs.guru
cd gtfs.guru
cargo build --release -p gtfs-guru
```

---

## ⚡ Usage (CLI)

Validate a feed and output the report to a directory:

```bash
gtfs-guru -i /path/to/gtfs.zip -o ./output_report
```

**Options:**
* `-i, --input <FILE>`: Path to GTFS zip file or directory.
* `-o, --output <DIR>`: Directory to save HTML/JSON reports.

See the [LLM Guide](docs/llm.md) for a compact, copy/paste reference.

---

## 📂 Project Structure

This monorepo houses the entire ecosystem:

* **`crates/gtfs_validator_core`**: The validation engine (88+ rules).
* **`crates/gtfs_validator_cli`**: CLI tool implementation.
* **`crates/gtfs_validator_gui`**: Desktop application (Tauri).
* **`crates/gtfs_validator_python`**: Python bindings (via PyO3/Maturin).
* **`crates/gtfs_validator_wasm`**: WebAssembly bindings for browser usage.

## 🤝 Contributing

We welcome contributions! Whether it's adding new rules, fixing bugs, or improving documentation.

1. Clone the repo: `git clone https://github.com/abasis-ltd/gtfs.guru`
2. Install Rust: [rustup.rs](https://rustup.rs)
3. Run tests: `cargo test --workspace`

## 📄 License

Apache-2.0. Free to use for everyone.
