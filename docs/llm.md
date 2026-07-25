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

Compare two feed versions:

```bash
gtfs-guru diff old.zip new.zip
gtfs-guru diff old.zip new.zip --json diff.json --markdown diff.md --fail-on-new-errors
```

The diff covers agencies, routes, stops, route-level trip/frequency aggregates,
and validation notice deltas. Add `--no-validation` for structural comparison
only.

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

Badges are written to the path given, not into `--output_base`:

- `--badge badge.json` — shields.io endpoint descriptor
- `--badge-svg badge.svg` — self-contained SVG

## CI

```bash
gtfs-guru -i feed.zip -o ./report --fail-on error   # exit 2 on any error
```

GitHub Actions:

```yaml
- uses: actions/checkout@v4
- uses: abasis-ltd/gtfs.guru/action@v1
  with:
    feed: feed.zip
    fail-on: error
```

Exit codes: `0` completed, `1` the run failed, `2` the feed did not meet
`--fail-on`.

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

## Fixes

Preview the edits without changing anything:

```bash
gtfs-guru --input /path/to/gtfs.zip --output_base ./report --fix-dry-run
```

Write a repaired copy:

```bash
gtfs-guru --input /path/to/gtfs.zip --output_base ./report --fix --fix-output ./gtfs.fixed.zip
```

### What gets fixed

| Notice | Repair | Level |
| --- | --- | --- |
| `invalid_color` | `#FF0000` / `0xFF0000` → `FF0000`, shorthand `0F0` → `00FF00` | safe |
| `invalid_date` | `2026-01-05`, `2026/1/5` → `20260105` | safe |
| `invalid_time` | `9:05` → `9:05:00`, drop an all-zero fraction | safe |
| `invalid_url`, `u_r_i_syntax_error` | add the missing `https://` scheme | safe |
| `invalid_email` | strip a `mailto:` prefix or angle brackets | safe |
| `leading_or_trailing_whitespaces` | trim declared GTFS fields | safe |
| `unsorted_stop_times` | group trips and sort by `stop_sequence` without changing row values | safe |
| `invalid_float` | decimal comma → decimal point (`1,5` → `1.5`) | confirm |
| `invalid_integer` | drop a redundant fraction (`12.0` → `12`) | confirm |
| `foreign_key_violation` | delete a child row that references a missing parent | unsafe |
| `translation_foreign_key_violation` | delete a translation for a missing record | unsafe |

Every syntactic replacement is fed back through the validator that rejected the
value before it is offered. Ambiguous inputs are left alone: `01-05-2026`
(day-first or month-first?), `1,500` (1.5 or 1500?), and casing warnings like
`mixed_case_recommended_field` get no suggestion at all. Structural repairs are
guarded against stale input, and the complete output is validated again.

### Notes

- The input is never modified. Without `--fix-output` the copy lands at
  `<input>.fixed.<ext>`; an existing output path is refused rather than
  overwritten.
- `--fix` applies safe fixes only; `--fix-unsafe` also applies confirm-level and
  unsafe ones.
- Field edits rewrite only their records. Sorting moves the original raw
  records, preserving line endings and quoting; every other file is unchanged.
- A fix whose field no longer holds the expected value is reported and skipped.
- The output is immediately validated again. The CLI prints how many notices
  were resolved, remain, or appeared after repair.
- A few fix-carrying rules (`u_r_i_syntax_error`) only run under `--thorough`.
