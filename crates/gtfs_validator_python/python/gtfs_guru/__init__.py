"""GTFS Guru - Fast GTFS validation in Python.

Example:
    >>> import gtfs_guru
    >>> result = gtfs_guru.validate("/path/to/gtfs.zip")
    >>> print(f"Valid: {result.is_valid}, Errors: {result.error_count}")
"""

from .gtfs_guru import (
    Notice,
    ProgressInfo,
    ValidationResult,
    notice_codes,
    notice_schema,
    validate,
    validate_async,
    version,
)

__all__ = [
    "Notice",
    "ProgressInfo",
    "ValidationResult",
    "notice_codes",
    "notice_schema",
    "validate",
    "validate_async",
    "version",
]

__version__ = version()
