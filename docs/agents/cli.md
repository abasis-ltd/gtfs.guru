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

## Feed Diff

- `gtfs-guru diff old.zip new.zip` compares two local archives or directories.
- The default console report covers table presence, feed version/date range,
  agencies, routes, added/removed/renamed/moved stops, route-level trip counts,
  frequency windows, and notice-count changes.
- `--json <PATH|->` and `--markdown <PATH|->` produce CI-friendly reports.
- `--fail-on-new-errors` exits `2` when an error group's count increases.
- `--no-validation` skips all validation rules and compares only feed contents.
- Diff logic and its serializable result types live in
  `gtfs_validator_core::diff`; CLI code only loads inputs and renders output.

## Feed Profile and Explanation

- `gtfs-guru profile -i feed.zip` emits deterministic model-friendly JSON.
- `gtfs-guru explain -i feed.zip` emits an evidence-backed Markdown summary;
  add `--json` for structured output.
- Both commands accept `--url`, `--date`, `--country-code`, `--google-rules`,
  and `--thorough`.
- Facts and prose structures live in `gtfs-guru-profile`; the CLI must not
  duplicate service-calendar or issue-ranking logic.
- Service facts cover seven actual dates beginning with the analysis date and
  apply `calendar_dates.txt` additions and removals.

## Fix Flags

- Suggestions are derived in `crates/gtfs_validator_core/src/fix_suggest.rs`; each one is
  re-checked against the validator that rejected the value before it is offered.
- `--fix-dry-run` enumerates planned edits without writing anything.
- `--fix` applies safe fixes; `--fix-unsafe` also applies confirm-level and unsafe ones.
- `--fix-output` sets the destination (default `<input>.fixed.<ext>`). The input is never
  modified and an existing output path is refused.
- Safe structural fixes can reorder raw `stop_times.txt` records; orphan row
  deletion is reserved for `--fix-unsafe` and guarded by the expected foreign-key value.
- After writing, the CLI validates the repaired feed and prints resolved,
  remaining, and introduced notice counts.
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

# Compare a feed update and fail CI on newly introduced errors
./target/release/gtfs-guru diff old.zip new.zip --markdown - --fail-on-new-errors
```
