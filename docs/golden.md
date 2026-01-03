# Golden Parity Workflow

This repo includes helper scripts to compare Rust outputs against expected
baselines (for parity checks with Java or other references).

## Core scripts
- `scripts/compare_reports.py`: compare `report.json`, `system_errors.json`, and HTML.
- `scripts/run_golden_compare.sh` / `scripts/run_golden_compare.py`: run one case.
- `scripts/run_golden_suite.sh` / `scripts/run_golden_suite.py`: run a suite from TSV.
- `scripts/validate_golden_manifest.py`: sanity-check manifest paths.
- `scripts/update_expected_from_manifest.py`: generate expected outputs from feeds.
- `scripts/golden.py`: unified helper with `suite`, `single`, `validate`, `update`.
- `scripts/golden.sh` / `scripts/golden_single.sh`: bash wrappers for quick local runs.

## Manifest format (TSV)
Columns are tab-separated:
1. `feed_path`
2. `expected_dir`
3. `case_name` (optional)
4. `extra_json` (optional, space-separated)
5. `html_name` (optional)
6. `validator_args` (optional, space-separated)
7. `compare_flags` (optional, space-separated)

If you need quoted values or paths with spaces, use the Python suite runner.
Keep empty columns as tabs to preserve positions (e.g., `feed<TAB>expected<TAB><TAB><TAB>html`).
`validate_golden_manifest.py` warns if a line has fewer than 7 columns (use `--no-column-warn` to silence).
When updating expected on failure, the HTML name respects `--html-name` in compare flags or `COMPARE_HTML_NAME`.
Local manifest path: `scripts/golden_manifest.tsv` (gitignored); create it as needed.
Example manifest: `scripts/golden_manifest.example.tsv`.
You need to fill `feed_path` and `expected_dir` at minimum.
`expected_dir` should point to the baseline outputs (e.g., Java validator output folder).
`actual_dir` is typically a local folder like `golden_actual/`.
CI looks for `scripts/golden_manifest.tsv` by default.
The example manifest contains placeholder paths and will fail unless updated.

Example row:
```
/path/to/feed.zip    /path/to/expected/feed    feed    notice_schema.json    report.html    --date 2024-01-01 --country_code US    --strip-runtime-fields
```

Another example (custom HTML + compare flags):
```
/path/to/feed-b.zip    /path/to/expected/feed-b        custom_report.html    --date 2024-02-01    --skip-html
```

## Expected outputs layout
- `expected_dir/report.json`, `expected_dir/report.html`, `expected_dir/system_errors.json`
- Example layout: `scripts/expected_layout.example.txt`

## Common environment variables
- `GTFS_VALIDATOR_BIN`: use a prebuilt CLI instead of `cargo run`.
- `COMPARE_FLAGS`: extra flags for `compare_reports.py` (space-separated).
- `COMPARE_EXTRA_JSON`: additional JSON files to compare.
- `COMPARE_HTML_NAME`: override HTML name for compare runners.
- `GOLDEN_MANIFEST`: manifest path for `scripts/ci_golden.sh`.
- `GOLDEN_ACTUAL_DIR`: output directory for `scripts/ci_golden.sh` (default `golden_actual`).
- `UPDATE_EXPECTED_ON_FAIL`: copy actual outputs into expected on compare failure.
- `UPDATE_EXPECTED`: for wrapper scripts, refresh expected before compare.
- `UPDATE_EXPECTED_FLAGS`: flags for `update_expected_from_manifest.py` (defaults to `--overwrite`).
- `MANIFEST_VALIDATE_FLAGS`: flags for manifest validation.
- `SKIP_MANIFEST_VALIDATE`: skip validation before suite runs.
- `CASE_FILTER`: glob to select case_name entries for suite/update (e.g., `*feed-a*`).
Example env file: `scripts/golden.env.example`

## compare_reports.py options
- `--extra-json FILE` (repeatable)
- `--skip-html`
- `--html-name NAME`
- `--ignore-summary`
- `--ignore-summary-field FIELD` (repeatable)
- `--strip-runtime-fields`
- `--ignore-notice-order`
- `--ignore-input`
- `--sort-summary-arrays`
- `--ignore-validator-version`
Note: use `--ignore-notice-order` only when order differences are not meaningful for parity.
Use `--sort-summary-arrays` if summary arrays are treated as sets.

## Typical flows
Update expected then compare suite:
```
scripts/update_expected_from_manifest.py --overwrite scripts/golden_manifest.example.tsv
COMPARE_FLAGS="--strip-runtime-fields --skip-html --ignore-notice-order" \
  scripts/run_golden_suite.sh scripts/golden_manifest.example.tsv golden_actual
```

Single case (quoted paths supported):
```
scripts/golden_single.py "feed path.zip" "expected dir" "actual dir"
```

Unified helper:
```
scripts/golden.py suite scripts/golden_manifest.example.tsv golden_actual
```
Run `scripts/golden.py --help` for full usage.

## Bash vs Python runners
- Bash runners are fast and simple for paths without spaces.
- Python runners support quoted paths and more robust parsing.

## Cheatsheet
```
# Validate manifest
scripts/golden.py validate scripts/golden_manifest.example.tsv --warn-empty-expected

# Update expected
scripts/golden.py update scripts/golden_manifest.example.tsv --overwrite

# Compare with notice order ignored + sorted summary arrays
python3 scripts/compare_reports.py expected actual --ignore-notice-order --sort-summary-arrays

# Compare using default flags
scripts/compare_defaults.sh expected actual

# Run suite compare
COMPARE_FLAGS="--strip-runtime-fields --skip-html --ignore-notice-order --ignore-input" \
  scripts/golden.py suite scripts/golden_manifest.example.tsv golden_actual

# Override HTML name for compare
COMPARE_HTML_NAME="custom_report.html" \
  scripts/golden.py suite scripts/golden_manifest.example.tsv golden_actual

# Update expected + compare in one step
UPDATE_EXPECTED=1 UPDATE_EXPECTED_FLAGS="--overwrite" \
  scripts/golden.py suite scripts/golden_manifest.example.tsv golden_actual

# Run only a subset of cases
CASE_FILTER="*feed-a*" \
  scripts/golden.py suite scripts/golden_manifest.example.tsv golden_actual

# Single case (quoted paths)
scripts/golden.py single "feed path.zip" "expected dir" "actual dir"

# CI (skips if manifest missing)
scripts/ci_golden.sh
```

## CI (GitHub Actions)
An optional workflow lives at `.github/workflows/golden.yml` and runs the suite
when golden scripts/docs change. It skips if `scripts/golden_manifest.tsv` is not
present. `workflow_dispatch` accepts a custom manifest path.
`scripts/ci_golden.sh` defaults to `COMPARE_FLAGS="--strip-runtime-fields --skip-html --ignore-notice-order --ignore-input"`.

## Local output folders
- Suggested `golden_actual/` is ignored by git for local runs.
