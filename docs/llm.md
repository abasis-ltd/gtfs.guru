# LLM Guide

Compact, copy/paste guide for analysts. Keep it short in LLM context.

## Quick Start (CLI)

Validate a local GTFS zip:

```bash
gtfs-guru --input /path/to/gtfs.zip --output_base ./report
```

Validate from URL (optional cache):

```bash
gtfs-guru --url https://example.com/gtfs.zip --storage_directory /tmp/gtfs --output_base ./report
```

Common flags:

- `--country_code US`
- `--date 2025-01-15`
- `--thorough`
- `--google_rules`
- `--pretty`

## Quick Start (Python)

```python
import gtfs_guru

result = gtfs_guru.validate(
    "/path/to/gtfs.zip",
    country_code="US",
    date="2025-01-15",
)

print(result.is_valid, result.error_count, result.warning_count)
result.save_json("report.json")
result.save_html("report.html")
```

## Web API (Minimal curl)

Start the server:

```bash
cargo run --release -p gtfs-guru-web
```

Workflow (base URL `http://localhost:3000`):

```bash
# 1) Create job
JOB_ID=$(curl -s -X POST http://localhost:3000/create-job | tr -d '\"')

# 2) Upload feed
curl -s -X PUT --data-binary @/path/to/gtfs.zip \
  http://localhost:3000/upload/$JOB_ID

# 3) Poll status
curl -s http://localhost:3000/jobs/$JOB_ID/status

# 4) Download reports
curl -s -o report.json http://localhost:3000/jobs/$JOB_ID/report.json
curl -s -o report.html http://localhost:3000/jobs/$JOB_ID/report.html
```

## Output Files

Written to `--output_base`:

- `report.json`
- `report.html`
- `system_errors.json`
- `notice_schema.json` (when `--export_notices_schema` is used)
- `report.sarif.json` (when `--sarif` is used)

## report.json (Structure)

Minimal shape (example fields only):

```json
{
  "summary": {
    "is_valid": false,
    "error_count": 3,
    "warning_count": 12,
    "info_count": 5,
    "validation_time_seconds": 0.42
  },
  "notices": [
    {
      "code": "missing_required_field",
      "severity": "ERROR",
      "message": "Missing required field",
      "file": "stops.txt",
      "row": 12,
      "field": "stop_name",
      "context": {
        "fieldName": "stop_name"
      }
    }
  ]
}
```

## Quick Filtering (Python)

```python
errors = result.errors()
by_code = [n for n in errors if n.code == "missing_required_field"]
print(len(by_code))
```

## Notice Schema (All codes + descriptions)

```bash
gtfs-guru --export_notices_schema --output_base ./report
```

Open `./report/notice_schema.json` to see all notice codes, severity, and descriptions.

## Fixes (Dry Run)

```bash
gtfs-guru --input /path/to/gtfs.zip --output_base ./report --fix-dry-run
```

Note: `--fix` and `--fix-unsafe` currently only log intended edits; files are not modified yet.
