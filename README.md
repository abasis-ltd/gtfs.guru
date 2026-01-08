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
2. Download the installer for your OS:
    * 🍎 **macOS**: `gtfs-guru_0.1.1_aarch64.dmg` (Apple Silicon) or `x64.dmg` (Intel)
    * 🪟 **Windows**: `gtfs-guru_0.1.1_x64-setup.exe`
    * 🐧 **Linux**: `gtfs-guru_0.1.1_amd64.deb` or `.AppImage`
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

### 🦀 For Rust Developers (CLI)

The classic high-performance command-line interface.

**From Crates.io:**

```bash
cargo install gtfs-guru-cli
```

**Build from Source:**

```bash
git clone https://github.com/abasis-ltd/gtfs.guru
cd gtfs.guru
cargo build --release -p gtfs-guru-cli
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
* `-j, --json`: Output JSON report to stdout (useful for piping).

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
