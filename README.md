# GTFS Guru 🚀

[![CI](https://github.com/abasis-ltd/gtfs.guru/actions/workflows/rust.yml/badge.svg)](https://github.com/abasis-ltd/gtfs.guru/actions)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/gtfs-guru.svg)](https://crates.io/crates/gtfs-guru)
[![PyPI](https://img.shields.io/pypi/v/gtfs-guru.svg)](https://pypi.org/project/gtfs-guru/)

**A fast, native, multi-platform GTFS validator.**

GTFS Guru is a next-generation tool to check your transit data (GTFS) for errors. It ensures your schedules, routes, and stops are correct before they go live on Google Maps, Apple Maps, or other journey planners.

> 💡 **Inspired by [MobilityData/gtfs-validator](https://github.com/MobilityData/gtfs-validator)**. We rebuilt the validation logic from the ground up in Rust to achieve blazing speed, privacy, and universal portability.

---

## 🌟 Why GTFS Guru?

1. **Fast native engine**: Parallel CSV loading and validation keep large feeds in the seconds range, with no JVM startup cost.
2. **Privacy First**: Runs locally on your machine. No need to upload sensitive or pre-release schedules to the cloud.
3. **Cross-Platform**: Available as a desktop app, command-line tool, Python library, Web API, and WebAssembly module.
4. **CI & Integrations**: JSON/HTML/SARIF reports, notice schema export, URL validation, and timing breakdowns.
5. **Deep Coverage**: 110 validators, Google-specific rules, and an optional `--thorough` mode.

| Feature | Java Validator | **GTFS Guru (Rust)** |
| :--- | :---: | :---: |
| **Speed** | JVM startup + Java pipeline | Native binary; see benchmarks below |
| **Memory** | 🐘 Heavy (JVM) | 🪶 **Light (Native)** |
| **Platform** | Java Runtime Required | **Standalone Binary** |
| **Python** | ❌ Wrapper only | ✅ **Native (`pip install`)** |
| **Web** | ❌ Server-side only | ✅ **Browser-native (WASM)** |
| **CI Output** | ❌ | ✅ **SARIF + JSON/HTML** |

---

## 📌 Versions

* Current engine/CLI/report/model/python/wasm/web crate versions: **`v1.0.0`**
* Desktop app releases are tagged on GitHub; download the latest for your OS.

---

## 📥 Installation

### 👨‍💼 For Non-Developers (Desktop App)

The easiest way to validate feeds without using the command line.

1. Go to the [**Releases Page**](https://github.com/abasis-ltd/gtfs.guru/releases/latest).
2. Download the installer for your OS (these links always point to the latest release):
    * 🍎 **macOS (DMG)**: [`gtfs-guru-macos.dmg`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-macos.dmg)
    * 🪟 **Windows (x64)**: [`gtfs-guru-windows-x64.msi`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-windows-x64.msi) or [`gtfs-guru-windows-x64-setup.exe`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-windows-x64-setup.exe)
    * 🐧 **Linux (Debian)**: [`gtfs-guru-linux-amd64.deb`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-amd64.deb)
    * 🐧 **Linux (AppImage)**: [`gtfs-guru-linux-amd64.AppImage`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-amd64.AppImage)
3. Run the installer and launch the app. Drag and drop your `gtfs.zip` file to validate!

### 🐍 For Python Developers (Data Science)

Perfect for checking data integrity within Jupyter Notebooks or ETL pipelines (Python 3.8+).

```bash
pip install gtfs-guru
```

```python
import gtfs_guru

# Validate a feed and return a rich report object
report = gtfs_guru.validate("path/to/gtfs.zip")

print(f"Valid: {report.is_valid}")
print(f"Notices: {len(report.notices)}")

# Export results
report.save_html("validation_report.html")
report.save_json("report.json")
```

### 🧰 For CLI Users (Prebuilt Binaries)

Download the latest CLI for your platform:

* 🍎 **macOS (arm64)**: [`gtfs-guru-macos-arm64.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-macos-arm64.tar.gz)
* 🍎 **macOS (x86_64)**: [`gtfs-guru-macos-x86_64.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-macos-x86_64.tar.gz)
* 🐧 **Linux (x86_64, glibc/gnu)**: [`gtfs-guru-linux-x86_64.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-x86_64.tar.gz)
* 🐧 **Linux (x86_64, musl)**: [`gtfs-guru-linux-x86_64-musl.tar.gz`](https://github.com/abasis-ltd/gtfs.guru/releases/latest/download/gtfs-guru-linux-x86_64-musl.tar.gz)
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
* `GTFS_GURU_LINUX_FLAVOR=gnu|musl` (x86_64 Linux only)
* `GTFS_GURU_VERSION=v1.0.0`

**CI examples (GitHub Actions):**

Use the action — it installs a checksum-verified binary, runs the validation,
sends SARIF to the Security tab, and fails the job on a bad feed:

```yaml
jobs:
  validate:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write
    steps:
      - uses: actions/checkout@v4
      - uses: abasis-ltd/gtfs.guru/action@v1
        with:
          feed: feed.zip
          fail-on: error
```

Full input/output reference: [`action/README.md`](action/README.md).

Or drive the CLI yourself:

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
        run: gtfs-guru -i feed.zip -o out --fail-on error
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
    - gtfs-guru -i feed.zip -o out --fail-on error
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

Validate from a URL (with an optional download cache):

```bash
gtfs-guru -u https://example.com/gtfs.zip -s /tmp/gtfs -o ./output_report
```

Compare a feed update, including routes, stops, trip/frequency aggregates, and
validation regressions:

```bash
gtfs-guru diff old.zip new.zip
gtfs-guru diff old.zip new.zip --json diff.json --markdown diff.md --fail-on-new-errors
```

The diff command exits with status `2` under `--fail-on-new-errors` when the new
feed adds error occurrences. Use `--no-validation` for a faster structural-only
comparison.

Generate deterministic facts or a human-readable explanation:

```bash
gtfs-guru profile -i feed.zip --date 2026-07-27 --pretty
gtfs-guru explain -i feed.zip --date 2026-07-27
gtfs-guru explain -i feed.zip --json --pretty
```

The profile includes entity counts, route types, completeness facts, seven
actual service dates with calendar exceptions applied, and grouped validation
issues. The explanation is derived from the same profile, so every statement
can be checked without sending the feed to an LLM provider.

### MCP server

Build the read-only MCP server. The default stdio transport is suitable for a
local Claude/ChatGPT-compatible MCP host:

```bash
cargo build --release -p gtfs-guru-mcp
./target/release/gtfs-guru-mcp --allow-dir /path/to/feeds
```

It exposes `validate_gtfs`, `explain_gtfs`, and `get_notice_details`. Validation
responses include exact grouped totals plus up to three concrete examples per
code/severity group with available file, row, field, context, and suggested
fixes. Local file access is restricted to configured roots. Public URL
downloads are disabled by default and can be explicitly enabled with
`--allow-url`.

After an in-browser validation, gtfs.guru also renders a local “What ChatGPT
receives” preview from the same report. It demonstrates the MCP payload without
uploading the feed or calling an AI provider.

For a remote MCP client, start authenticated stateless Streamable HTTP:

```bash
export GTFS_GURU_MCP_BEARER_TOKEN="$(openssl rand -hex 32)"
./target/release/gtfs-guru-mcp \
  --transport http \
  --bind 127.0.0.1:3000 \
  --allow-dir /path/to/feeds \
  --allow-url
```

The MCP endpoint is `/mcp`; `/healthz` is unauthenticated. Put public
deployments behind TLS and pass each externally visible hostname with
`--allowed-host`. HTTP defaults to 60 authenticated requests per rolling minute,
four concurrent validations, and 64 KiB request bodies.

Default outputs in the report directory:
* `report.json`
* `report.html`
* `system_errors.json`

Optional outputs:
* `--sarif report.sarif.json`
* `--badge badge.json` (a shields.io endpoint descriptor for a README badge)
* `--export-notices-schema` (writes `notice_schema.json`)

**Options (highlights):**
* `-i, --input <FILE>`: Path to GTFS zip file or directory.
* `-u, --url <URL>`: Validate a remote GTFS zip.
* `-s, --storage_directory <DIR>`: Save downloaded feeds when using `--url`.
* `-o, --output_base <DIR>`: Directory to save reports.
* `--stdout`: Write only the JSON validation report to stdout instead of report files.
* `--skip_validator_update`: Skip the online validator update check.
* `--threads <N>`: Thread count recorded in the generated report.
* `--google-rules`: Enable Google-specific rules.
* `--thorough`: Enable recommended-field checks.
* `--sarif <FILE>`: Write SARIF report for CI.
* `--timing` / `--timing-json`: Print timing breakdowns.
* `--fail-on <none|error|warning>`: Exit with status 2 when the report reaches that severity. Reports are still written.
* `--badge <FILE>` / `--badge-svg <FILE>`: Write a status badge. See [Status badges](docs/usage.md#status-badges).

`--fix-dry-run` lists suggested edits without touching anything. `--fix` applies the safe ones and `--fix-unsafe` applies all of them, writing a repaired copy to `--fix-output` (default: `<input>.fixed.<ext>` beside the input). The input is never modified and an existing output path is refused. Field repairs rewrite only their CSV records; sorting moves the original raw records, and every untouched file is copied byte for byte.

Safe fixes cover whitespace, colors (`#FF0000`, `0F0`), dates written with separators, times missing their seconds, URLs missing a scheme, `mailto:`-wrapped emails, and canonical `stop_times.txt` ordering. Decimal commas need confirmation; deleting rows whose foreign key points at a missing parent is available only under `--fix-unsafe`. Syntactic replacements are re-validated before they are offered, ambiguous values (`01-05-2026`, `1,500`) get no suggestion, and the repaired feed is automatically validated again with resolved/remaining/introduced totals.

Exit codes: `0` validation completed, `1` the run failed, `2` the feed did not meet `--fail-on`.

See the [LLM Guide](docs/llm.md) for a compact, copy/paste reference.

## ⚡ Performance

Benchmarks below were run on an Apple M3 Pro with warm page cache. Each tool validates the feed end-to-end and writes its normal report files; stdout/stderr were redirected to `/dev/null` so terminal progress logging does not dominate the measurement.

Setup:

* `gtfs.guru` was built with `cargo build --release -p gtfs-guru` and run with `RAYON_NUM_THREADS=8`, `--threads 8`, and `--skip_validator_update`.
* `mecatran/gtfsvtor` was v1.0.3, run on OpenJDK 21 with `--numThreads 8` and `GTFSVTOR_OPTS=-Xmx6G`.
* The canonical `MobilityData/gtfs-validator` was v8.0.1, run on OpenJDK 21 with `--threads 8`, `--skip_validator_update`, and `-Xmx6G`.

| Feed | Size | `gtfs.guru` | `gtfsvtor` 1.0.3 | canonical `gtfs-validator` 8.0.1 |
| :--- | ---: | ---: | ---: | ---: |
| MBTA Boston | 38 MB zip, 295 MB uncompressed, 5.4M `stop_times.txt` rows | **2.32 s** (n=5) | 6.13 s (n=3) | 10.60 s (n=3) |
| OVapi NL 2026-06-09 | 198 MB zip, 1.27 GB uncompressed, 16.0M `stop_times.txt` rows | **9.75 s** (n=5) | 21.66 s (n=3) | 65.18 s (n=3) |

On these feeds `gtfs.guru` is about 2.2-2.6x faster than `gtfsvtor`, and about 4.6-6.7x faster than the canonical Java validator. The comparison is wall-clock time for each tool doing its own full validation pipeline; rule sets and report formats differ, so this is not a per-rule apples-to-apples benchmark.

Reproduce:

```bash
curl -sL -o /tmp/mbta.zip https://cdn.mbta.com/MBTA_GTFS.zip
curl -L -o /tmp/NL-20260609.gtfs.zip https://gtfs.ovapi.nl/nl/NL-20260609.gtfs.zip

RAYON_NUM_THREADS=8 gtfs-guru \
  -i /tmp/NL-20260609.gtfs.zip \
  -o /tmp/gtfs-guru-nl \
  --skip_validator_update \
  --threads 8

java -Xmx6G -jar gtfs-validator-8.0.1-cli.jar \
  -i /tmp/NL-20260609.gtfs.zip \
  -o /tmp/gtfs-validator-nl \
  --skip_validator_update \
  --threads 8

GTFSVTOR_OPTS=-Xmx6G gtfsvtor \
  --numThreads 8 \
  --htmlOutput /tmp/gtfsvtor-nl.html \
  --jsonOutput /tmp/gtfsvtor-nl.json \
  /tmp/NL-20260609.gtfs.zip
```

---

## 📂 Project Structure

This monorepo houses the entire ecosystem. `gtfs_validator_core` is the
validation engine; every front-end (CLI, web, GUI, WASM, Python) is a thin
wrapper around it and `gtfs_validator_report`:

```mermaid
graph LR
    model["gtfs_model<br/>shared GTFS types"] --> core["gtfs_validator_core<br/>110 validators"]
    core --> report["gtfs_validator_report<br/>JSON · HTML · SARIF"]
    report --> cli["gtfs_validator_cli<br/>gtfs-guru binary"]
    report --> web["gtfs_validator_web<br/>Axum API server"]
    report --> gui["gtfs_validator_gui<br/>Tauri desktop app"]
    report --> wasm["gtfs_validator_wasm<br/>browser bindings"]
    report --> python["gtfs_validator_python<br/>PyO3 bindings"]
```

* **`crates/gtfs_model`**: Shared GTFS data model types.
* **`crates/gtfs_validator_core`**: The validation engine (110 validators).
* **`crates/gtfs_validator_report`**: Report generation (JSON/HTML/SARIF).
* **`crates/gtfs_validator_cli`**: CLI tool implementation.
* **`crates/gtfs_validator_web`**: Web API service.
* **`crates/gtfs_validator_gui`**: Desktop application (Tauri).
* **`crates/gtfs_validator_python`**: Python bindings (via PyO3/Maturin).
* **`crates/gtfs_validator_wasm`**: WebAssembly bindings for browser usage.

## 🤝 Contributing

We welcome contributions! Whether it's adding new rules, fixing bugs, or improving documentation.

1. Clone the repo: `git clone https://github.com/abasis-ltd/gtfs.guru`
2. Install Rust: [rustup.rs](https://rustup.rs)
3. On Linux, install the system dependencies: see [`docs/system-dependencies.md`](docs/system-dependencies.md)
4. Run tests: `cargo test --workspace`

## 📄 License

Apache-2.0. Free to use for everyone.
