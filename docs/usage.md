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
| `--badge <PATH>` | | Write a shields.io endpoint descriptor for a README badge |
| `--badge-svg <PATH>` | | Write a self-contained SVG badge |
| `--badge-label <TEXT>` | | Left-hand badge text (default `GTFS`) |
| `--fix-dry-run` | | List suggested auto-fixes without modifying files |
| `--fix` | | Write a repaired copy with the safe fixes applied |
| `--fix-unsafe` | | Like `--fix`, but also applies confirm-level and unsafe fixes |
| `--fix-output <PATH>` | | Destination for the repaired feed (default `<input>.fixed.<ext>`) |
| `--thorough` | | Enable thorough validation (recommended fields) |
| `--timing` | | Print timing breakdown |
| `--timing-json` | | Print timing report as JSON |
| `--version` | | Print the validator version |

`--fix` writes a new feed and never changes the input. It safely normalizes
supported field values, trims declared GTFS fields, and sorts `stop_times.txt`
by trip and `stop_sequence`. `--fix-unsafe` can additionally delete rows whose
foreign key references a missing parent. After writing the copy, the CLI
validates it again and reports resolved, remaining, and introduced notices.

### Which standard a report answers for

Every JSON report's `summary` states the upstream revisions the build was
aligned with, alongside the validator's own version:

```bash
gtfs-guru -i feed.zip --stdout | jq '.summary | {validatorVersion, specRevision, canonicalBaseline}'
```

```json
{
  "validatorVersion": "1.0.0",
  "specRevision": "google/transit@3215f98f26615f1b925dca1bf2205311b747e308",
  "canonicalBaseline": "MobilityData/gtfs-validator@v8.0.1"
}
```

`specRevision` is the GTFS specification commit, and `canonicalBaseline` the
release of the canonical Java validator, that this build was checked against.
Both are extensions to the canonical report schema. `gtfs-guru spec-surface`
prints the files, fields, enum values, and notice codes that follow from them.

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

### Status badges

`--badge` writes a [shields.io endpoint][shields-endpoint] descriptor describing
the run:

```bash
gtfs-guru -i feed.zip -o ./report --fail-on none --badge badge/gtfs.json
```

```json
{
  "schemaVersion": 1,
  "label": "GTFS",
  "message": "0 errors, 3 warnings",
  "color": "yellow"
}
```

Publish that file (a `gh-pages` branch, an object store, anywhere reachable) and
reference it from a README:

```markdown
![GTFS](https://img.shields.io/endpoint?url=https://example.org/badge/gtfs.json)
```

The message is `valid` on a clean feed, `0 errors, N warnings` when only
warnings remain, and `N errors` otherwise; the colour follows. `--badge-svg`
writes a self-contained SVG for places that cannot reach shields.io, and
`--badge-label` replaces the `GTFS` on the left with, say, a feed name.

Pair it with `--fail-on none` when the badge is the point: a workflow that
aborts on the first error never gets to write one.

Both paths are taken as given rather than resolved against `--output_base`,
so a badge can be written straight into the directory a workflow publishes,
and they work with `--stdout` too.

[shields-endpoint]: https://shields.io/badges/endpoint-badge

### GitHub Actions

The repository ships a composite action that installs the binary, runs it,
uploads SARIF to code scanning, and fails the job on a bad feed:

```yaml
- uses: actions/checkout@v4
- uses: abasis-ltd/gtfs.guru/action@v1
  with:
    feed: feed.zip
    fail-on: error
```

See [`action/README.md`](https://github.com/abasis-ltd/gtfs.guru/blob/main/action/README.md)
for every input, the outputs it
sets, and the badge-publishing recipe.

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
