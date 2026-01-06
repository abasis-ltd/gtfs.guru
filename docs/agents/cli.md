# CLI (gtfs-guru-cli)

## Scope

- Binary name: `gtfs-guru` (crate `gtfs-guru-cli`).
- Clap-based interface for local file or URL validation.

## Inputs

- `--input /path/to/gtfs.zip` or `--url https://...` (mutually exclusive).
- `--storage-directory /tmp/gtfs` keeps downloaded feeds when using `--url`.

## Outputs

- `--output /path/to/output` is required.
- Default outputs include `report.json`, `report.html`, and `system_errors.json`.
- `--sarif report.sarif.json` adds SARIF output for CI tooling.
- `--export-notices-schema` writes `notice_schema.json` into the output directory.

## Validation Options

- `--country-code`, `--date-for-validation`, `--google-rules`, `--thorough`.
- `--threads` controls the reported thread count in the summary.
- Run `gtfs-guru --help` for the full flag list.

## Fix Flags

- `--fix-dry-run`, `--fix`, and `--fix-unsafe` enumerate fixable issues and planned changes.
- Current implementation reports the intended edits rather than modifying files.

## Examples

```bash
# Local file
./target/release/gtfs-guru -i feeds/nl.zip -o out

# URL input with cached download
./target/release/gtfs-guru -u https://example.com/gtfs.zip -s /tmp/gtfs -o out
```
