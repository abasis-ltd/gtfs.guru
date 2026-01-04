# GTFS Validator (Rust)

High-performance GTFS feed validator written in Rust. Full compatibility with [MobilityData gtfs-validator](https://github.com/MobilityData/gtfs-validator) (Java), with identical validation rules and output format.

## Features

- **88 validation rules** — full parity with Java gtfs-validator
- **Fast** — 10-50x faster than Java version
- **Multiple interfaces** — CLI, Web API, Python bindings, Desktop App
- **Cross-platform** — macOS, Linux, Windows
- **Detailed reports** — JSON and HTML output

## Quick Start

### Option 1: Python (recommended)

```bash
pip install gtfs-validator
```

```python
import gtfs_validator

result = gtfs_validator.validate("/path/to/gtfs.zip")
print(f"Valid: {result.is_valid}, Errors: {result.error_count}")

for error in result.errors():
    print(f"{error.code}: {error.message}")
```

### Option 2: Command Line

```bash
# Build
cargo build --release -p gtfs_validator_cli

# Run
./target/release/gtfs_validator_cli \
    --input /path/to/gtfs.zip \
    --output_base /tmp/report
```

### Option 3: Web Service

```bash
cargo run --release -p gtfs_validator_web
# API available at http://localhost:3000
```

### Option 4: Desktop App

```bash
cargo tauri dev
# Or build: cargo tauri build
```

### Option 5: WebAssembly (Browser)

```bash
npm install @gtfs-validator/wasm
```

```javascript
import init, { validate_gtfs } from '@gtfs-validator/wasm';

await init();
const bytes = new Uint8Array(await file.arrayBuffer());
const result = validate_gtfs(bytes, 'US');

console.log(`Valid: ${result.is_valid}, Errors: ${result.error_count}`);
```

---

## Installation

### From Source (Rust)

```bash
git clone https://github.com/abasis-ltd/gtfs.guru
cd gtfs-validator-rust
cargo build --release
```

Binaries will be in `target/release/`:

- `gtfs_validator_cli` — command-line tool
- `gtfs_validator_web` — web server

### Desktop App (Tauri)

Prerequisites: Rust and [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites).

```bash
cargo install tauri-cli --version "^2.0.0"
cargo tauri build
```

The app bundle will be in `target/release/bundle/`.

### Python Package

```bash
# From PyPI (when published)
pip install gtfs-validator

# From source
cd crates/gtfs_validator_python
pip install maturin
maturin build --release
pip install target/wheels/gtfs_validator-*.whl
```

---

## Command Line Interface (CLI)

### Basic Usage

```bash
# Validate local file
gtfs_validator_cli --input /path/to/gtfs.zip --output_base ./report

# Validate from URL
gtfs_validator_cli --url https://example.com/gtfs.zip --output_base ./report

# With options
gtfs_validator_cli \
    --input /path/to/gtfs.zip \
    --output_base ./report \
    --country_code US \
    --date 2025-01-15 \
    --pretty
```

### CLI Options

| Option | Short | Description |
|--------|-------|-------------|
| `--input <PATH>` | `-i` | Path to GTFS zip file or directory |
| `--url <URL>` | `-u` | URL to download GTFS feed |
| `--output_base <DIR>` | `-o` | Output directory for reports (required) |
| `--country_code <CODE>` | `-c` | ISO country code (e.g., US, RU, DE) |
| `--date <DATE>` | `-d` | Validation date (YYYY-MM-DD) |
| `--pretty` | `-p` | Format JSON output |
| `--export_notices_schema` | `-n` | Export notice schema to JSON |
| `--storage_directory <DIR>` | `-s` | Save downloaded feed to directory |
| `--validation_report_name <NAME>` | `-v` | Custom name for JSON report |
| `--html_report_name <NAME>` | `-r` | Custom name for HTML report |
| `--threads <N>` | | Number of threads (default: 1) |

### Output Files

After validation, the output directory contains:

| File | Description |
|------|-------------|
| `report.json` | Detailed validation report in JSON |
| `report.html` | Human-readable HTML report |
| `system_errors.json` | System errors (if any) |

---

## Web API

### Starting the Server

```bash
cargo run --release -p gtfs_validator_web
# Server starts at http://localhost:3000
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GTFS_VALIDATOR_WEB_BASE_DIR` | `target/web_jobs` | Directory for job data |
| `GTFS_VALIDATOR_WEB_PUBLIC_BASE_URL` | `http://localhost:3000` | Public URL for links |
| `GTFS_VALIDATOR_WEB_JOB_TTL_SECONDS` | `86400` | Job expiration time (24h) |

### API Endpoints

#### Health Check

```bash
GET /healthz
# Response: "ok"
```

#### Version

```bash
GET /version
# Response: {"version": "0.1.0"}
```

#### Create Validation Job (with URL)

```bash
POST /create-job
Content-Type: application/json

{"url": "https://example.com/gtfs.zip", "countryCode": "US"}

# Response:
{"jobId": "job-123-456", "url": null}
```

#### Create Validation Job (for upload)

```bash
POST /create-job

# Response:
{"jobId": "job-123-456", "url": "http://localhost:3000/upload/job-123-456"}
```

#### Upload GTFS File

```bash
PUT /upload/{job_id}
Content-Type: application/octet-stream

<binary data>

# Response: 200 OK
```

#### Check Job Status

```bash
GET /jobs/{job_id}/status

# Response:
{
  "jobId": "job-123-456",
  "status": "success",  # or "processing", "error", "awaiting_upload"
  "error": null,
  "reportJsonUrl": "http://localhost:3000/jobs/job-123-456/report.json",
  "reportHtmlUrl": "http://localhost:3000/jobs/job-123-456/report.html"
}
```

#### Get Reports

```bash
GET /jobs/{job_id}/report.json    # JSON report
GET /jobs/{job_id}/report.html    # HTML report
GET /jobs/{job_id}/system_errors.json
```

### Python Client Example

```python
import requests
import time

BASE_URL = "http://localhost:3000"

def validate_gtfs(gtfs_url):
    # Create job
    resp = requests.post(f"{BASE_URL}/create-job",
                        json={"url": gtfs_url})
    job_id = resp.json()["jobId"]

    # Wait for completion
    while True:
        status = requests.get(f"{BASE_URL}/jobs/{job_id}/status").json()
        if status["status"] in ("success", "error"):
            break
        time.sleep(1)

    # Get report
    if status["status"] == "success":
        return requests.get(f"{BASE_URL}/jobs/{job_id}/report.json").json()
    else:
        raise Exception(status.get("error"))

report = validate_gtfs("https://example.com/gtfs.zip")
print(report["summary"]["validationResult"])
```

---

## Desktop App (Tauri)

The desktop application provides a beautiful native interface identical to the web version but running locally without a browser.

### Features

- Drag & drop GTFS files
- Native file dialogs
- Offline validation
- HTML/JSON report export

### Running in Development

```bash
cargo tauri dev
```

### Building for Release

```bash
cargo tauri build
```

The output will be an `.app` (macOS), `.exe` (Windows), or `.deb/.AppImage` (Linux) in `target/release/bundle/`.

### Download Pre-built Installers

Pre-built installers are available from [GitHub Releases](https://github.com/abasis-ltd/gtfs.guru/releases):

| Platform | Format | Description |
|----------|--------|-------------|
| **macOS** | `.dmg` | Disk image with universal binary (Intel + Apple Silicon) |
| **Windows** | `.msi` | MSI installer |
| **Windows** | `.exe` | NSIS installer (alternative) |
| **Linux** | `.deb` | Debian/Ubuntu package |
| **Linux** | `.AppImage` | Portable Linux application |

---

## Python API

### Installation

```bash
pip install gtfs-validator
```

### Basic Usage

```python
import gtfs_validator

# Validate a GTFS feed
result = gtfs_validator.validate("/path/to/gtfs.zip")

# Check results
print(f"Valid: {result.is_valid}")
print(f"Errors: {result.error_count}")
print(f"Warnings: {result.warning_count}")
print(f"Time: {result.validation_time_seconds:.2f}s")
```

### Validation Options

```python
result = gtfs_validator.validate(
    "/path/to/gtfs.zip",
    country_code="US",      # ISO country code
    date="2025-01-15"       # Validation date (YYYY-MM-DD)
)
```

### Working with Notices

```python
# Get all notices
for notice in result.notices:
    print(f"[{notice.severity}] {notice.code}: {notice.message}")

# Get only errors
for error in result.errors():
    print(f"{error.code}: {error.message}")
    if error.file:
        print(f"  Location: {error.file}:{error.row}")

# Get only warnings
for warning in result.warnings():
    print(warning.code)

# Filter by notice code
for notice in result.by_code("missing_required_field"):
    field = notice.get("fieldName")
    print(f"Missing field: {field}")
```

### Notice Object

Each notice has the following attributes:

| Attribute | Type | Description |
|-----------|------|-------------|
| `code` | `str` | Notice code (e.g., "missing_required_field") |
| `severity` | `str` | "ERROR", "WARNING", or "INFO" |
| `message` | `str` | Human-readable description |
| `file` | `str?` | GTFS filename (e.g., "stops.txt") |
| `row` | `int?` | CSV row number |
| `field` | `str?` | Field name |

Methods:

- `notice.get(key)` — Get context field by name
- `notice.context()` — Get all context as dict

### Saving Reports

```python
# Save JSON report
result.save_json("/path/to/report.json")

# Save HTML report
result.save_html("/path/to/report.html")

# Get report as dict
report = result.to_dict()
print(report["summary"]["validationResult"])

# Get report as JSON string
json_str = result.to_json()
```

### Utility Functions

```python
# Get validator version
print(gtfs_validator.version())  # "0.1.0"

# Get all notice codes
codes = gtfs_validator.notice_codes()
print(len(codes))  # 164

# Get notice schema
schema = gtfs_validator.notice_schema()
print(schema["missing_required_field"])
```

### Complete Example

```python
import gtfs_validator

def analyze_feed(path):
    """Analyze a GTFS feed and print summary."""
    result = gtfs_validator.validate(path)

    print(f"{'='*50}")
    print(f"GTFS Validation Report")
    print(f"{'='*50}")
    print(f"Feed: {path}")
    print(f"Valid: {'Yes' if result.is_valid else 'No'}")
    print(f"Time: {result.validation_time_seconds:.2f}s")
    print()
    print(f"Errors:   {result.error_count}")
    print(f"Warnings: {result.warning_count}")
    print(f"Infos:    {result.info_count}")
    print()

    if result.errors():
        print("Top Errors:")
        # Group by code
        from collections import Counter
        error_codes = Counter(e.code for e in result.errors())
        for code, count in error_codes.most_common(5):
            print(f"  {code}: {count}")

    if result.warnings():
        print("\nTop Warnings:")
        warning_codes = Counter(w.code for w in result.warnings())
        for code, count in warning_codes.most_common(5):
            print(f"  {code}: {count}")

    return result

# Usage
result = analyze_feed("/path/to/gtfs.zip")
result.save_html("report.html")
```

---

## WebAssembly (WASM)

Run the validator entirely in the browser - no server required. All validation happens locally.

### Installation

```bash
npm install @gtfs-validator/wasm
```

Or use CDN:

```html
<script type="module">
  import init, { validate_gtfs } from 'https://unpkg.com/@gtfs-validator/wasm/gtfs_validator_wasm.js';
</script>
```

### Basic Usage

```javascript
import init, { validate_gtfs, version } from '@gtfs-validator/wasm';

// Initialize once
await init();
console.log('Version:', version());

// Validate a file
const file = document.getElementById('gtfs-input').files[0];
const bytes = new Uint8Array(await file.arrayBuffer());
const result = validate_gtfs(bytes, 'US');  // country code optional

console.log('Valid:', result.is_valid);
console.log('Errors:', result.error_count);
console.log('Warnings:', result.warning_count);

// Parse detailed notices
const notices = JSON.parse(result.json);
```

### Web Worker (Non-blocking)

For large files, use the Web Worker wrapper:

```javascript
import { GtfsValidator } from '@gtfs-validator/wasm';

const validator = new GtfsValidator();
await validator.waitUntilReady();

const result = await validator.validate(file, { countryCode: 'US' });
console.log(result.isValid, result.validationTimeMs);

validator.terminate();
```

### Building WASM

```bash
# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Build
./scripts/build-wasm.sh

# Or manually
wasm-pack build crates/gtfs_validator_wasm --target web --release
```

See [docs/wasm.md](docs/wasm.md) for full documentation including bundler configs (Webpack, Vite, Next.js).

---

## Validation Rules

The validator implements **88 validation rules** covering:

### File Structure

- Required files (agency.txt, stops.txt, routes.txt, trips.txt, stop_times.txt)
- Recommended files (feed_info.txt, shapes.txt)
- File encoding (UTF-8)
- CSV parsing

### Data Integrity

- Primary key uniqueness
- Foreign key references
- Required fields
- Data types and formats

### Geographic Validation

- Coordinate ranges (latitude/longitude)
- Stop-to-shape distance
- Travel speed between stops
- Shape geometry

### Schedule Validation

- Stop time sequences
- Arrival/departure times
- Calendar validity
- Service coverage
- Overlapping frequencies

### Accessibility & Quality

- Route color contrast
- Stop naming
- Pathway connectivity
- Fare system consistency

For the full list of notices, use:

```python
import gtfs_validator
print(gtfs_validator.notice_codes())
```

---

## Project Structure

```
gtfs-validator-rust/
├── crates/
│   ├── gtfs_model/           # GTFS data types and schemas
│   ├── gtfs_validator_core/  # Validation engine (88 rules)
│   ├── gtfs_validator_report/# JSON/HTML report generation
│   ├── gtfs_validator_cli/   # Command-line interface
│   ├── gtfs_validator_web/   # REST API server (Axum)
│   ├── gtfs_validator_python/# Python bindings (PyO3)
│   ├── gtfs_validator_wasm/  # WebAssembly for browsers
│   └── gtfs_validator_gui/   # Desktop app (Tauri)
├── scripts/                  # Testing and comparison tools
├── docs/                     # Additional documentation
└── Cargo.toml               # Workspace configuration
```

---

## Development

### Building

```bash
# Build all crates
cargo build --release

# Build specific crate
cargo build --release -p gtfs_validator_cli
```

### Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p gtfs_validator_core
```

### Pre-commit Hooks

Install pre-commit hooks for automatic code quality checks before each commit:

```bash
# Install pre-commit
pip install pre-commit

# Install hooks
pre-commit install
pre-commit install --hook-type pre-push

# Run manually on all files
pre-commit run --all-files
```

Hooks include:

- **cargo fmt** — Format check on commit
- **cargo clippy** — Lint check on commit
- **cargo check** — Compile check on commit
- **cargo test** — Test run on push

### Building Python Wheels

```bash
cd crates/gtfs_validator_python
pip install maturin

# Build for current platform
maturin build --release

# Build for specific target
maturin build --release --target x86_64-apple-darwin      # macOS Intel
maturin build --release --target aarch64-apple-darwin     # macOS ARM
maturin build --release --target x86_64-pc-windows-msvc   # Windows
```

---

## Comparison with Java Version

| Feature | Java | Rust |
|---------|------|------|
| Validation rules | 74 | 88 |
| Output format | JSON, HTML | JSON, HTML (identical) |
| CLI interface | Yes | Yes (compatible) |
| Web API | Yes | Yes |
| Python bindings | No | Yes |
| WASM (browser) | No | Yes |
| Performance | Baseline | 10-50x faster |
| Memory usage | ~500MB | ~50MB |
| Binary size | ~50MB (JAR) | ~5MB (CLI), ~1.5MB (WASM) |

---

## License

Apache-2.0
