# Python API

## Installation

```bash
pip install gtfs-validator
```

## Basic Usage

```python
import gtfs_validator

# Validate a GTFS feed
result = gtfs_validator.validate("/path/to/gtfs.zip")

# Check results
print(f"Valid: {result.is_valid}")
print(f"Errors: {result.error_count}")
print(f"Warnings: {result.warning_count}")
```

## Validation Options

```python
result = gtfs_validator.validate(
    "/path/to/gtfs.zip",
    country_code="US",      # ISO country code
    date="2025-01-15"       # Validation date (YYYY-MM-DD)
)
```

## Working with Notices

```python
# Get all notices
for notice in result.notices:
    print(f"[{notice.severity}] {notice.code}: {notice.message}")

# Get only errors
for error in result.errors():
    print(f"{error.code}: {error.message}")
    if error.file:
        print(f"  Location: {error.file}:{error.row}")
```

## Notice Object

Each notice has the following attributes:

| Attribute | Type | Description |
|-----------|------|-------------|
| `code` | `str` | Notice code (e.g., "missing_required_field") |
| `severity` | `str` | "ERROR", "WARNING", or "INFO" |
| `message` | `str` | Human-readable description |
| `file` | `str?` | GTFS filename (e.g., "stops.txt") |
| `row` | `int?` | CSV row number |
| `field` | `str?` | Field name |
