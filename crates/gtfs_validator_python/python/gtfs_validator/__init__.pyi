"""GTFS Validator Python bindings - Type stubs."""

from typing import Any, Callable, Optional

class Notice:
    """A single validation notice (error, warning, or info)."""

    code: str
    """Notice code (e.g., 'missing_required_field')."""

    severity: str
    """Severity level: 'ERROR', 'WARNING', or 'INFO'."""

    message: str
    """Human-readable message describing the issue."""

    file: Optional[str]
    """GTFS filename where the issue was found."""

    row: Optional[int]
    """CSV row number (1-based) where the issue was found."""

    field: Optional[str]
    """Field name where the issue was found."""

    def get(self, key: str) -> Any:
        """Get a context field by name.

        Args:
            key: Context field name.

        Returns:
            Field value or None if not found.
        """
        ...

    def context(self) -> dict[str, Any]:
        """Get all context fields as a dictionary.

        Returns:
            Dictionary of context fields.
        """
        ...


class ValidationResult:
    """Result of GTFS validation."""

    is_valid: bool
    """True if no errors were found."""

    error_count: int
    """Number of error notices."""

    warning_count: int
    """Number of warning notices."""

    info_count: int
    """Number of info notices."""

    validation_time_seconds: float
    """Time taken for validation in seconds."""

    notices: list[Notice]
    """All validation notices."""

    def errors(self) -> list[Notice]:
        """Get only error notices.

        Returns:
            List of error notices.
        """
        ...

    def warnings(self) -> list[Notice]:
        """Get only warning notices.

        Returns:
            List of warning notices.
        """
        ...

    def infos(self) -> list[Notice]:
        """Get only info notices.

        Returns:
            List of info notices.
        """
        ...

    def by_code(self, code: str) -> list[Notice]:
        """Get notices by code.

        Args:
            code: Notice code to filter by.

        Returns:
            List of notices with the given code.
        """
        ...

    def to_json(self) -> str:
        """Get full report as JSON string.

        Returns:
            JSON string representation of the report.
        """
        ...

    def to_dict(self) -> dict[str, Any]:
        """Get full report as Python dict.

        Returns:
            Dictionary representation of the report.
        """
        ...

    def save_json(self, path: str) -> None:
        """Save JSON report to file.

        Args:
            path: File path to save the report.
        """
        ...

    def save_html(self, path: str) -> None:
        """Save HTML report to file.

        Args:
            path: File path to save the report.
        """
        ...



class ProgressInfo:
    """Progress information during validation."""
    
    stage: str
    """Current stage ('loading', 'validating', 'finalizing', 'complete')."""
    
    current: int
    """Current progress value."""
    
    total: int
    """Total expected progress value."""
    
    message: str
    """Human-readable status message."""

ProgressCallback = Callable[[ProgressInfo], None]

def validate(
    path: str,
    country_code: Optional[str] = None,
    date: Optional[str] = None,
) -> ValidationResult:
    """Validate a GTFS feed.

    Args:
        path: Path to GTFS zip file or directory.
        country_code: Optional ISO country code (e.g., 'US', 'RU').
        date: Optional validation date in YYYY-MM-DD format.

    Returns:
        ValidationResult with all notices and summary.

    Raises:
        ValueError: If the path is invalid or the feed cannot be loaded.

    Example:
        >>> import gtfs_validator
        >>> result = gtfs_validator.validate("/path/to/gtfs.zip")
        >>> print(f"Valid: {result.is_valid}")
        >>> for error in result.errors():
        ...     print(f"{error.code}: {error.message}")
    """
    ...


async def validate_async(
    path: str,
    country_code: Optional[str] = None,
    date: Optional[str] = None,
    on_progress: Optional[ProgressCallback] = None,
) -> ValidationResult:
    """Async validation with optional progress callback.
    
    Args:
        path: Path to GTFS zip file or directory.
        country_code: Optional ISO country code (e.g., 'US', 'RU').
        date: Optional validation date in YYYY-MM-DD format.
        on_progress: Optional callback function for progress updates.

    Returns:
        ValidationResult with all notices and summary.

    Example:
        async def on_progress(info: ProgressInfo):
            print(f"{info.stage}: {info.current}/{info.total}")
        
        result = await gtfs_validator.validate_async(
            "/path/to/feed.zip",
            on_progress=on_progress
        )
    """
    ...


def version() -> str:
    """Get the version of the validator.

    Returns:
        Version string.
    """
    ...


def notice_codes() -> list[str]:
    """Get list of all available notice codes.

    Returns:
        List of notice code strings.
    """
    ...


def notice_schema() -> dict[str, Any]:
    """Get schema for all notice types.

    Returns:
        Dictionary with notice code as key and schema as value.
    """
    ...
