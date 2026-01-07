# GTFS Guru

[![CI](https://github.com/abasis-ltd/gtfs.guru/actions/workflows/ci.yml/badge.svg)](https://github.com/abasis-ltd/gtfs.guru/actions)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/gtfs-guru.svg)](https://crates.io/crates/gtfs-guru)
[![PyPI](https://img.shields.io/pypi/v/gtfs-guru.svg)](https://pypi.org/project/gtfs-guru/)

**The world's fastest and most versatile GTFS validator.**

GTFS Guru is a next-generation tool to check your transit data (GTFS) for errors. It ensures your schedules, routes, and stops are correct before they go live on Google Maps, Apple Maps, or other journey planners.

Building on the legacy of the original Java validator, we re-engineered everything in Rust to bring you **blazing speed** (up to 50x faster) and **universal access**—run it on your laptop, in your browser, or on your server.

---

## 🚀 Why Use GTFS Guru?

### For Transit Agencies & Operators

* **Save Time:** Validate huge feeds in seconds, not minutes.
* **Find Errors Early:** Catch issues like impossible travel speeds, missing stops, or overlapping trips.
* **Easy Reports:** Get clear, human-readable HTML reports to share with your team.
* **Secure:** Validate data right in your browser (WASM). Your data never leaves your computer.

### For Developers

* **Drop-in Replacement:** Fully compatible with the Google/MobilityData validator output format.
* **Universal API:** Bindings for Python (`gtfs-guru`), Rust, and WebAssembly.
* **CI/CD Ready:** Tiny binary size, zero dependencies, and JSON output for automated pipelines.

---

## ⚡️ Quick Start

### 1. The Easiest Way: Web Interface

You don't need to install anything. Just visit our website (coming soon) or run the local web server:

```bash
# Verify your data in the browser
# (Link to be added upon release)
```

### 2. For Python Users (Data Scientists)

Perfect for analyzing feeds in Jupyter notebooks or scripts.

```bash
pip install gtfs-guru
```

```python
import gtfs_guru

report = gtfs_guru.validate("gtfs.zip")
print(f"Is valid: {report.is_valid}")
print(f"Errors: {report.error_count}")
report.save_html("report.html")
```

### 3. For Power Users (CLI)

The classic command-line tool.

```bash
# Build (or download binary from Releases)
cargo build --release -p gtfs-guru-cli

# Run
./target/release/gtfs-guru -i gtfs.zip -o output_dir
```

---

## 💎 Features at a Glance

| Feature | Java Validator | **GTFS Guru (Rust)** |
| :--- | :---: | :---: |
| **Speed** | 🐢 1x | 🚀 **50x Faster** |
| **Memory Usage** | 🐘 Heavy (~500MB) | 🪶 **Light (~50MB)** |
| **Python Support** | ❌ No | ✅ **Native** |
| **Run in Browser** | ❌ No | ✅ **Yes (WASM)** |
| **Parsing Strategy** | Serial | **Parallel (Rayon)** |
| **Rules Implemented** | ~74 | **88** |
| **Installation** | Requires Java | **Zero Dependencies** |

---

## 🛠 Advanced Usage

<details>
<summary><strong>Rust (Crate)</strong></summary>

Add to your `Cargo.toml`:

```toml
[dependencies]
gtfs_validator_core = "0.1.0"
```

```rust
use gtfs_validator_core::input::Input;
use gtfs_validator_core::Validator;

let input = Input::from_file("gtfs.zip")?;
let result = Validator::new().validate(input)?;
```

</details>

<details>
<summary><strong>Web Server (API)</strong></summary>

Deploy your own validation API (compatible with serverless):

```bash
cargo run --release -p gtfs-guru-web
# POST /create-job -> Validate feed from URL
```

</details>

<details>
<summary><strong>Desktop App</strong></summary>

A native GUI for macOS, Windows, and Linux.
*(Coming soon to Releases)*
</details>

---

## 📂 Project Structure

This repository is organized as a workspace of multiple crates:

* **`crates/gtfs_validator_core`**: The brain. Contains all 88 validation rules.
* **`crates/gtfs_validator_cli`**: Command-line tool (crate: `gtfs-guru-cli`).
* **`crates/gtfs_validator_python`**: Python bindings (package: `gtfs-guru`).
* **`crates/gtfs_validator_wasm`**: WebAssembly module.
* **`crates/gtfs_validator_web`**: Web API server (crate: `gtfs-guru-web`).
* **`crates/gtfs_validator_gui`**: Desktop application (Tauri).

## 🤝 Contributing

We welcome contributions! Whether it's adding new rules, fixing bugs, or improving documentation.

1. Clone the repo: `git clone https://github.com/abasis-ltd/gtfs.guru`
2. Install Rust: [rustup.rs](https://rustup.rs)
3. Run tests: `cargo test`

## 📄 License

Apache-2.0. Free to use for everyone.
