# GTFS Validator Python

High-performance GTFS feed validator with Python bindings. Written in Rust, exposed via PyO3.

## Installation

```bash
pip install gtfs-validator
```

### From Source

```bash
pip install maturin
maturin build --release
pip install target/wheels/gtfs_validator-*.whl
```

## Quick Start

```python
import gtfs_validator

# Validate a GTFS feed
result = gtfs_validator.validate("/path/to/gtfs.zip")

print(f"Valid: {result.is_valid}")
print(f"Errors: {result.error_count}")
print(f"Warnings: {result.warning_count}")

# Print errors
for error in result.errors():
    print(f"{error.code}: {error.message}")
```

## API Reference

### Functions

#### `validate(path, country_code=None, date=None) -> ValidationResult`

Validate a GTFS feed.

**Parameters:**
- `path` (str): Path to GTFS zip file or directory
- `country_code` (str, optional): ISO country code (e.g., "US", "RU")
- `date` (str, optional): Validation date in YYYY-MM-DD format

**Returns:** `ValidationResult` object

**Example:**
```python
result = gtfs_validator.validate(
    "/path/to/gtfs.zip",
    country_code="US",
    date="2025-01-15"
)
```

#### `async validate_async(path, country_code=None, date=None, on_progress=None) -> ValidationResult`

Validate a GTFS feed asynchronously (non-blocking).

**Parameters:**
- `path` (str): Path to GTFS zip file or directory
- `country_code` (str, optional): ISO country code
- `date` (str, optional): Validation date in YYYY-MM-DD format
- `on_progress` (Callable[[ProgressInfo], None], optional): Callback for progress updates

**Example:**
```python
import asyncio

async def main():
    def on_progress(info):
        print(f"{info.stage}: {info.current}/{info.total}")
        
    result = await gtfs_validator.validate_async(
        "/path/to/gtfs.zip",
        on_progress=on_progress
    )
    
asyncio.run(main())
```

#### `version() -> str`

Get validator version.

```python
>>> gtfs_validator.version()
'0.1.0'
```

#### `notice_codes() -> list[str]`

Get list of all available notice codes.

```python
>>> len(gtfs_validator.notice_codes())
164
>>> gtfs_validator.notice_codes()[:3]
['attribution_without_role', 'bidirectional_exit_gate', 'block_trips_with_overlapping_stop_times']
```

#### `notice_schema() -> dict`

Get schema for all notice types with descriptions and severity levels.

```python
>>> schema = gtfs_validator.notice_schema()
>>> schema['missing_required_field']
{'severity': 'ERROR', 'description': '...'}
```

### Classes

#### `ValidationResult`

Result of GTFS validation.

**Attributes:**
| Attribute | Type | Description |
|-----------|------|-------------|
| `is_valid` | `bool` | True if no errors |
| `error_count` | `int` | Number of errors |
| `warning_count` | `int` | Number of warnings |
| `info_count` | `int` | Number of info notices |
| `validation_time_seconds` | `float` | Validation time in seconds |
| `notices` | `list[Notice]` | All validation notices |

**Methods:**

```python
# Get notices by severity
errors = result.errors()       # List[Notice]
warnings = result.warnings()   # List[Notice]
infos = result.infos()        # List[Notice]

# Filter by notice code
notices = result.by_code("missing_required_field")  # List[Notice]

# Export
result.save_json("/path/to/report.json")
result.save_html("/path/to/report.html")
json_str = result.to_json()   # str
report = result.to_dict()     # dict
```

#### `Notice`

A single validation notice.

**Attributes:**
| Attribute | Type | Description |
|-----------|------|-------------|
| `code` | `str` | Notice code (e.g., "missing_required_field") |
| `severity` | `str` | "ERROR", "WARNING", or "INFO" |
| `message` | `str` | Human-readable message |
| `file` | `str \| None` | GTFS filename |
| `row` | `int \| None` | CSV row number |
| `field` | `str \| None` | Field name |

**Methods:**

```python
# Get context field
value = notice.get("fieldName")  # Any | None

# Get all context
ctx = notice.context()  # dict[str, Any]
```

## Examples

### Basic Validation

```python
import gtfs_validator

result = gtfs_validator.validate("/path/to/gtfs.zip")

if result.is_valid:
    print("Feed is valid!")
else:
    print(f"Found {result.error_count} errors")
    for error in result.errors():
        print(f"  - {error.code}: {error.message}")
```

### Detailed Analysis

```python
from collections import Counter

result = gtfs_validator.validate("/path/to/gtfs.zip")

# Count notices by code
error_counts = Counter(e.code for e in result.errors())
for code, count in error_counts.most_common(10):
    print(f"{code}: {count}")

# Find all missing required fields
for notice in result.by_code("missing_required_field"):
    file = notice.file
    field = notice.get("fieldName")
    row = notice.row
    print(f"{file}:{row} - missing {field}")
```

### Save Reports

```python
result = gtfs_validator.validate("/path/to/gtfs.zip")

# Save JSON report (same format as Java validator)
result.save_json("report.json")

# Save HTML report
result.save_html("report.html")

# Get as Python dict
report = result.to_dict()
summary = report["summary"]
print(f"Agencies: {summary.get('agencies', [])}")
print(f"Routes: {summary.get('routes', {}).get('count', 0)}")
```

### Validation with Options

```python
# Validate for specific country (affects some rules)
result = gtfs_validator.validate(
    "/path/to/gtfs.zip",
    country_code="DE"
)

# Validate as of specific date
result = gtfs_validator.validate(
    "/path/to/gtfs.zip",
    date="2025-06-01"
)
```

## Supported Platforms

| Platform | Architecture | Python |
|----------|--------------|--------|
| macOS | ARM64 (M1/M2) | 3.8+ |
| macOS | x86_64 (Intel) | 3.8+ |
| Windows | x86_64 | 3.8+ |
| Linux | x86_64 | 3.8+ |

## Performance

Typical validation times (compared to Java validator):

| Feed Size | Java | Rust/Python |
|-----------|------|-------------|
| Small (<1MB) | ~2s | ~0.05s |
| Medium (10MB) | ~10s | ~0.5s |
| Large (100MB) | ~60s | ~3s |

## License

Apache-2.0
