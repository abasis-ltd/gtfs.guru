# Python Bindings (gtfs_validator_python)

## Scope

- PyO3 bindings around the core validator.
- Package name: `gtfs-guru` (see `crates/gtfs_validator_python/pyproject.toml`).
- Module name: `gtfs_guru`.

## Install and Build

```bash
pip install gtfs-guru
```

```bash
cd crates/gtfs_validator_python
pip install maturin
maturin build --release
pip install target/wheels/gtfs_guru-*.whl
```

## API Surface

- Functions: `validate`, `validate_async`, `version`, `notice_codes`, `notice_schema`.
- `ValidationResult` exposes counts, notice accessors, and `save_json`/`save_html` helpers.

## Example

```python
import gtfs_guru

result = gtfs_guru.validate("/path/to/gtfs.zip")
print(result.is_valid, result.error_count)
result.save_html("report.html")
```

See `crates/gtfs_validator_python/README.md` for full API and usage details.
