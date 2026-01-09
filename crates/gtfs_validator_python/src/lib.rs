#![allow(clippy::useless_conversion)]
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use gtfs_guru_core::{
    default_runner, set_validation_country_code, set_validation_date, validate_input, GtfsInput,
    NoticeSeverity, ValidationNotice as RustNotice,
};
use gtfs_guru_report::{ReportSummary, ReportSummaryContext, ValidationReport};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// A single validation notice (error, warning, or info).
#[pyclass]
#[derive(Clone)]
pub struct Notice {
    #[pyo3(get)]
    pub code: String,
    #[pyo3(get)]
    pub severity: String,
    #[pyo3(get)]
    pub message: String,
    #[pyo3(get)]
    pub file: Option<String>,
    #[pyo3(get)]
    pub row: Option<u64>,
    #[pyo3(get)]
    pub field: Option<String>,
    context: HashMap<String, serde_json::Value>,
}

#[pymethods]
impl Notice {
    fn __repr__(&self) -> String {
        format!(
            "Notice(code={:?}, severity={:?}, message={:?})",
            self.code, self.severity, self.message
        )
    }

    /// Get context field by name.
    fn get(&self, key: &str) -> Option<PyObject> {
        Python::with_gil(|py| self.context.get(key).map(|v| json_to_py(py, v)))
    }

    /// Get all context as a dictionary.
    fn context(&self, py: Python<'_>) -> PyObject {
        let dict = PyDict::new_bound(py);
        for (k, v) in &self.context {
            dict.set_item(k, json_to_py(py, v)).ok();
        }
        dict.into()
    }
}

/// Result of GTFS validation.
#[pyclass]
pub struct ValidationResult {
    #[pyo3(get)]
    pub is_valid: bool,
    #[pyo3(get)]
    pub error_count: usize,
    #[pyo3(get)]
    pub warning_count: usize,
    #[pyo3(get)]
    pub info_count: usize,
    #[pyo3(get)]
    pub validation_time_seconds: f64,
    // Store raw Rust notices for lazy conversion
    raw_notices: Vec<RustNotice>,
    report_json: String,
}

#[pymethods]
impl ValidationResult {
    fn __repr__(&self) -> String {
        format!(
            "ValidationResult(is_valid={}, errors={}, warnings={}, infos={})",
            self.is_valid, self.error_count, self.warning_count, self.info_count
        )
    }

    /// Get all notices.
    #[getter]
    fn notices(&self) -> Vec<Notice> {
        self.raw_notices.iter().map(rust_notice_to_py).collect()
    }

    /// Get only error notices.
    fn errors(&self) -> Vec<Notice> {
        self.raw_notices
            .iter()
            .filter(|n| matches!(n.severity, NoticeSeverity::Error))
            .map(rust_notice_to_py)
            .collect()
    }

    /// Get only warning notices.
    fn warnings(&self) -> Vec<Notice> {
        self.raw_notices
            .iter()
            .filter(|n| matches!(n.severity, NoticeSeverity::Warning))
            .map(rust_notice_to_py)
            .collect()
    }

    /// Get only info notices.
    fn infos(&self) -> Vec<Notice> {
        self.raw_notices
            .iter()
            .filter(|n| matches!(n.severity, NoticeSeverity::Info))
            .map(rust_notice_to_py)
            .collect()
    }

    /// Get notices by code.
    fn by_code(&self, code: &str) -> Vec<Notice> {
        self.raw_notices
            .iter()
            .filter(|n| n.code == code)
            .map(rust_notice_to_py)
            .collect()
    }

    /// Get full report as JSON string.
    fn to_json(&self) -> String {
        self.report_json.clone()
    }

    /// Get full report as Python dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let value: serde_json::Value = serde_json::from_str(&self.report_json)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(json_to_py(py, &value))
    }

    /// Save HTML report to file.
    fn save_html(&self, path: &str) -> PyResult<()> {
        // For now, just save a simple HTML version
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><title>GTFS Validation Report</title></head>
<body>
<h1>GTFS Validation Report</h1>
<p>Errors: {}</p>
<p>Warnings: {}</p>
<p>Infos: {}</p>
<pre>{}</pre>
</body>
</html>"#,
            self.error_count, self.warning_count, self.info_count, self.report_json
        );
        std::fs::write(path, html).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Save JSON report to file.
    fn save_json(&self, path: &str) -> PyResult<()> {
        std::fs::write(path, &self.report_json).map_err(|e| PyValueError::new_err(e.to_string()))
    }
}

/// Validate a GTFS feed from a file path or URL.
///
/// Args:
///     path: Path to GTFS zip file or directory
///     country_code: Optional ISO country code (e.g., "US", "RU")
///     date: Optional validation date in YYYY-MM-DD format
///
/// Returns:
///     ValidationResult with all notices and summary
///
/// Example:
///     >>> import gtfs_validator
///     >>> result = gtfs_validator.validate("/path/to/gtfs.zip")
///     >>> print(f"Valid: {result.is_valid}")
///     >>> for error in result.errors():
///     ...     print(f"{error.code}: {error.message}")
#[pyfunction]
#[pyo3(signature = (path, country_code=None, date=None))]
fn validate(
    py: Python<'_>,
    path: &str,
    country_code: Option<&str>,
    date: Option<&str>,
) -> PyResult<ValidationResult> {
    run_validation(Some(py), path, country_code, date, None)
}

/// Progress information during validation.
#[pyclass]
#[derive(Clone)]
pub struct ProgressInfo {
    #[pyo3(get)]
    pub stage: String,
    #[pyo3(get)]
    pub current: usize,
    #[pyo3(get)]
    pub total: usize,
    #[pyo3(get)]
    pub message: String,
}

#[pymethods]
impl ProgressInfo {
    fn __repr__(&self) -> String {
        format!(
            "ProgressInfo(stage={:?}, current={}, total={}, message={:?})",
            self.stage, self.current, self.total, self.message
        )
    }
}

/// Internal function to run validation (used by both sync and async).
fn run_validation(
    py: Option<Python<'_>>,
    path: &str,
    country_code: Option<&str>,
    date: Option<&str>,
    progress_callback: Option<&dyn Fn(ProgressInfo)>,
) -> PyResult<ValidationResult> {
    let input_path = PathBuf::from(path);

    // Set validation context
    let _country_guard = country_code
        .filter(|c| !c.trim().is_empty() && !c.eq_ignore_ascii_case("ZZ"))
        .map(|c| set_validation_country_code(Some(c.to_string())));

    let _date_guard = date.and_then(|d| {
        chrono::NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d")
            .or_else(|_| chrono::NaiveDate::parse_from_str(d.trim(), "%Y%m%d"))
            .ok()
            .map(|parsed| set_validation_date(Some(parsed)))
    });

    // Report loading progress
    if let Some(cb) = progress_callback {
        cb(ProgressInfo {
            stage: "loading".to_string(),
            current: 0,
            total: 3,
            message: "Loading GTFS feed...".to_string(),
        });
    }

    // Load input
    let input = GtfsInput::from_path(&input_path)
        .map_err(|e| PyValueError::new_err(format!("Failed to load GTFS: {}", e)))?;

    // Report validating progress
    if let Some(cb) = progress_callback {
        cb(ProgressInfo {
            stage: "validating".to_string(),
            current: 1,
            total: 3,
            message: "Running validation rules...".to_string(),
        });
    }

    // Run validation
    let runner = default_runner();
    let started_at = Instant::now();
    let outcome = if let Some(py) = py {
        py.allow_threads(|| validate_input(&input, &runner))
    } else {
        validate_input(&input, &runner)
    };
    let elapsed = started_at.elapsed();

    // Report finalizing progress
    if let Some(cb) = progress_callback {
        cb(ProgressInfo {
            stage: "finalizing".to_string(),
            current: 2,
            total: 3,
            message: "Building validation report...".to_string(),
        });
    }

    // Count notices by severity (without full conversion)
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut info_count = 0;

    for notice in outcome.notices.iter() {
        match notice.severity {
            NoticeSeverity::Error => error_count += 1,
            NoticeSeverity::Warning => warning_count += 1,
            NoticeSeverity::Info => info_count += 1,
        }
    }

    // Collect raw notices for lazy conversion
    let raw_notices: Vec<RustNotice> = outcome.notices.iter().cloned().collect();

    // Build report JSON
    let summary_context = ReportSummaryContext::new()
        .with_gtfs_input(&input_path)
        .with_output_directory(input_path.parent().unwrap_or(&input_path))
        .with_validation_time_seconds(elapsed.as_secs_f64())
        .with_validator_version(env!("CARGO_PKG_VERSION"));

    let summary_context = if let Some(feed) = outcome.feed.as_ref() {
        summary_context.with_feed(feed)
    } else {
        summary_context
    };

    let summary = ReportSummary::from_context(summary_context);
    let report = ValidationReport::from_container_with_summary(&outcome.notices, summary);
    let report_json =
        serde_json::to_string_pretty(&report).map_err(|e| PyValueError::new_err(e.to_string()))?;

    // Report completion
    if let Some(cb) = progress_callback {
        cb(ProgressInfo {
            stage: "complete".to_string(),
            current: 3,
            total: 3,
            message: format!(
                "Validation complete: {} errors, {} warnings",
                error_count, warning_count
            ),
        });
    }

    Ok(ValidationResult {
        is_valid: error_count == 0,
        error_count,
        warning_count,
        info_count,
        validation_time_seconds: elapsed.as_secs_f64(),
        raw_notices,
        report_json,
    })
}

/// Validate a GTFS feed asynchronously.
///
/// Args:
///     path: Path to GTFS zip file or directory
///     country_code: Optional ISO country code (e.g., "US", "RU")
///     date: Optional validation date in YYYY-MM-DD format
///     on_progress: Optional callback function for progress updates
///
/// Returns:
///     Coroutine that resolves to ValidationResult
///
/// Example:
///     >>> import asyncio
///     >>> import gtfs_validator
///     >>> async def main():
///     ...     def on_progress(info):
///     ...         print(f"{info.stage}: {info.current}/{info.total}")
///     ...     result = await gtfs_validator.validate_async("/path/to/gtfs.zip", on_progress=on_progress)
///     ...     print(f"Valid: {result.is_valid}")
///     >>> asyncio.run(main())
#[pyfunction]
#[pyo3(signature = (path, country_code=None, date=None, on_progress=None))]
fn validate_async(
    py: Python<'_>,
    path: String,
    country_code: Option<String>,
    date: Option<String>,
    on_progress: Option<PyObject>,
) -> PyResult<Bound<'_, PyAny>> {
    // Clone data for the async block
    let path_clone = path.clone();
    let country_code_clone = country_code.clone();
    let date_clone = date.clone();
    let on_progress_clone = on_progress.map(Arc::new);

    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        // Run validation in a blocking task
        let result = tokio::task::spawn_blocking(move || {
            // Create progress callback wrapper
            let progress_cb: Option<Box<dyn Fn(ProgressInfo) + Send>> =
                on_progress_clone.map(|py_cb| {
                    Box::new(move |info: ProgressInfo| {
                        Python::with_gil(|py| {
                            let py_info = Py::new(py, info).ok();
                            if let Some(py_info) = py_info {
                                let _ = py_cb.call1(py, (py_info,));
                            }
                        });
                    }) as Box<dyn Fn(ProgressInfo) + Send>
                });

            run_validation(
                None,
                &path_clone,
                country_code_clone.as_deref(),
                date_clone.as_deref(),
                progress_cb.as_ref().map(|cb| {
                    let f: &dyn Fn(ProgressInfo) = &**cb;
                    f
                }),
            )
        })
        .await
        .map_err(|e| PyValueError::new_err(format!("Task join error: {}", e)))??;

        Ok(result)
    })
}

/// Get the version of the validator.
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Get list of all available notice codes.
#[pyfunction]
fn notice_codes(py: Python<'_>) -> PyObject {
    let schema = gtfs_guru_core::build_notice_schema_map();
    let list = PyList::empty_bound(py);
    for code in schema.keys() {
        list.append(code).ok();
    }
    list.into()
}

/// Get schema for all notice types.
#[pyfunction]
fn notice_schema(py: Python<'_>) -> PyResult<PyObject> {
    let schema = gtfs_guru_core::build_notice_schema_map();
    let json = serde_json::to_value(&schema).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(json_to_py(py, &json))
}

fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyObject {
    match value {
        serde_json::Value::Null => py.None(),
        serde_json::Value::Bool(b) => b.into_py(py),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py(py)
            } else if let Some(f) = n.as_f64() {
                f.into_py(py)
            } else {
                py.None()
            }
        }
        serde_json::Value::String(s) => s.into_py(py),
        serde_json::Value::Array(arr) => {
            let list = PyList::empty_bound(py);
            for item in arr {
                list.append(json_to_py(py, item)).ok();
            }
            list.into()
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in obj {
                dict.set_item(k, json_to_py(py, v)).ok();
            }
            dict.into()
        }
    }
}

/// Convert a Rust ValidationNotice to a Python Notice object.
fn rust_notice_to_py(notice: &RustNotice) -> Notice {
    let severity_str = match notice.severity {
        NoticeSeverity::Error => "ERROR",
        NoticeSeverity::Warning => "WARNING",
        NoticeSeverity::Info => "INFO",
    };

    let mut context = HashMap::new();
    for (k, v) in &notice.context {
        context.insert(k.clone(), v.clone());
    }

    Notice {
        code: notice.code.clone(),
        severity: severity_str.to_string(),
        message: notice.message.clone(),
        file: notice.file.clone(),
        row: notice.row,
        field: notice.field.clone(),
        context,
    }
}

/// GTFS Validator Python bindings.
///
/// Example:
///     >>> import gtfs_guru
///     >>> result = gtfs_guru.validate("/path/to/gtfs.zip")
///     >>> print(f"Valid: {result.is_valid}, Errors: {result.error_count}")
#[pymodule]
fn gtfs_guru(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(validate, m)?)?;
    m.add_function(wrap_pyfunction!(validate_async, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(notice_codes, m)?)?;
    m.add_function(wrap_pyfunction!(notice_schema, m)?)?;
    m.add_class::<Notice>()?;
    m.add_class::<ProgressInfo>()?;
    m.add_class::<ValidationResult>()?;
    Ok(())
}
