# CLI (gtfs-guru)

## Scope

- Binary name: `gtfs-guru` (crate `gtfs-guru`).
- Clap-based interface for local file or URL validation.

## Inputs

- `--input /path/to/gtfs.zip` or `--url https://...` (mutually exclusive).
- `--storage-directory /tmp/gtfs` keeps downloaded feeds when using `--url`.

## Outputs

- `--output_base /path/to/output` is required unless `--stdout` is used.
- `--stdout` writes only the JSON validation report to standard output.
- Default outputs include `report.json`, `report.html`, and `system_errors.json`.
- `--sarif report.sarif.json` adds SARIF output for CI tooling.
- `--export-notices-schema` writes `notice_schema.json` into the output directory.
- `--badge <PATH>` / `--badge-svg <PATH>` write a status badge. Unlike the reports,
  these paths are used verbatim instead of being joined to `--output_base`, and they
  are written under `--stdout` as well. `--badge-label` overrides the left-hand text.
  Rendering lives in `crates/gtfs_validator_report/src/badge.rs`.

## Validation Options

- `--country-code`, `--date-for-validation`, `--google-rules`, `--thorough`.
- `--threads` controls the reported thread count in the summary.
- Run `gtfs-guru --help` for the full flag list.

## Fix Flags

- `--fix-dry-run` enumerates planned edits without writing anything.
- `--fix` applies safe fixes; `--fix-unsafe` also applies confirm-level and unsafe ones.
- `--fix-output` sets the destination (default `<input>.fixed.<ext>`). The input is never
  modified and an existing output path is refused.
- Fix modes cannot be combined with `--stdout`; clap rejects the command instead
  of silently skipping the requested repair or preview.
- Planning and rewriting live in `crates/gtfs_validator_core/src/fix.rs`
  (`FixPlan`, `apply_fixes`); the CLI only formats the plan and reports conflicts.

## Examples

```bash
# Local file
./target/release/gtfs-guru -i feeds/nl.zip -o out

# Machine-readable JSON without creating report files
./target/release/gtfs-guru -i feeds/nl.zip --stdout | jq '.summary'

# URL input with cached download
./target/release/gtfs-guru -u https://example.com/gtfs.zip -s /tmp/gtfs -o out
```
