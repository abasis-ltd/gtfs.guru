# Usage

## Command Line Interface (CLI)

### Basic Usage

```bash
# Validate local file

gtfs-guru --input /path/to/gtfs.zip --output_base ./report

# Validate from URL
gtfs-guru --url https://example.com/gtfs.zip --output_base ./report
```

### CLI Options

| Option | Short | Description |
|--------|-------|-------------|
| `--input <PATH>` | `-i` | Path to GTFS zip file or directory |
| `--url <URL>` | `-u` | URL to download GTFS feed |
| `--output_base <DIR>` | `-o` | Output directory for reports (required unless `--stdout`) |
| `--stdout` | | Write only the JSON validation report to stdout |
| `--country_code <CODE>` | `-c` | ISO country code (e.g., US, RU, DE) |
| `--date <DATE>` | `-d` | Validation date (YYYY-MM-DD) |
| `--pretty` | `-p` | Format JSON output |
| `--export_notices_schema` | `-n` | Export notice schema to JSON |
| `--storage_directory <DIR>` | `-s` | Save downloaded feed to directory |
| `--validation_report_name <NAME>` | `-v` | Custom name for JSON report |
| `--html_report_name <NAME>` | `-r` | Custom name for HTML report |
| `--system_errors_report_name <NAME>` | `-e` | Custom name for system errors report |
| `--skip_validator_update` | | Skip validator update check |
| `--validated-at <TIMESTAMP>` | | Override `validated_at` in report metadata |
| `--threads <N>` | | Number recorded in report metadata; does not size the thread pool |
| `--google_rules` | | Enable Google-specific rules |
| `--sarif <FILE>` | | Write SARIF report for CI/CD |
| `--fail-on <LEVEL>` | | `none` (default), `error`, or `warning`; exit 2 at that severity |
| `--fix-dry-run` | | List suggested auto-fixes without modifying files |
| `--fix` | | Write a repaired copy with the safe fixes applied |
| `--fix-unsafe` | | Like `--fix`, but also applies confirm-level and unsafe fixes |
| `--fix-output <PATH>` | | Destination for the repaired feed (default `<input>.fixed.<ext>`) |
| `--thorough` | | Enable thorough validation (recommended fields) |
| `--timing` | | Print timing breakdown |
| `--timing-json` | | Print timing report as JSON |
| `--version` | | Print the validator version |

### Parallelism

`--threads` is report metadata retained for Java compatibility. Set Rayon's
environment variable to control actual parallelism:

```bash
RAYON_NUM_THREADS=8 gtfs-guru -i feed.zip -o ./report
```

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Validation completed |
| `1` | The run failed (invalid arguments, unreadable input, or I/O failure) |
| `2` | The feed did not meet `--fail-on` |

Use `--fail-on error` in CI. Without it, a completed validation exits 0 even
when the feed contains validation errors.

## Web API

### Starting the Server

```bash
cargo run --release -p gtfs-guru-web
# Server starts at http://localhost:3000
```

### API Endpoints

- `GET /healthz` - Health check
- `GET /version` - Version info
- `GET /cors-proxy?url=...` - Same-origin remote feed fetch, restricted to public HTTP(S) addresses and bounded by rate, concurrency, timeout, redirect, and size limits
- `POST /create-job` - Create validation job
- `PUT /upload/{job_id}` - Upload GTFS file
- `GET /jobs/{job_id}/status` - Check status
- `GET /jobs/{job_id}/report.json` - JSON report
- `GET /jobs/{job_id}/report.html` - HTML report
