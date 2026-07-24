use serde::Serialize;
use wasm_bindgen::prelude::*;

use gtfs_guru_core::{
    default_runner, set_thorough_mode_enabled, set_validation_country_code, set_validation_date,
    validate_bytes_reader_with_timing, GtfsBytesReader, NoticeContainer, NoticeSeverity,
    TimingCollector,
};
use gtfs_guru_report::{
    generate_html_report_string, HtmlReportContext, ReportSummary, ReportSummaryContext,
};

#[cfg(feature = "threads")]
pub use wasm_bindgen_rayon::init_thread_pool;

#[cfg(feature = "console_error_panic_hook")]
pub use console_error_panic_hook::set_once as set_panic_hook;

/// Initialize the WASM module (call once on page load)
#[wasm_bindgen]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// Get the validator version
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Validation result returned to JavaScript
#[wasm_bindgen]
pub struct ValidationResult {
    json: String,
    html: String,
    timings_json: String,
    error_count: u32,
    warning_count: u32,
    info_count: u32,
}

#[wasm_bindgen]
impl ValidationResult {
    /// Get the full validation report as JSON
    #[wasm_bindgen(getter)]
    pub fn json(&self) -> String {
        self.json.clone()
    }

    /// Get the full validation report as HTML
    #[wasm_bindgen(getter)]
    pub fn html(&self) -> String {
        self.html.clone()
    }

    /// Get the loading and per-validator timing breakdown as JSON.
    #[wasm_bindgen(getter)]
    pub fn timings_json(&self) -> String {
        self.timings_json.clone()
    }

    /// Move the JSON report into JavaScript without cloning it in Rust.
    pub fn take_json(&mut self) -> String {
        std::mem::take(&mut self.json)
    }

    /// Move the HTML report into JavaScript without cloning it in Rust.
    pub fn take_html(&mut self) -> String {
        std::mem::take(&mut self.html)
    }

    /// Move the timing report into JavaScript without cloning it in Rust.
    pub fn take_timings_json(&mut self) -> String {
        std::mem::take(&mut self.timings_json)
    }

    /// Get the number of errors
    #[wasm_bindgen(getter)]
    pub fn error_count(&self) -> u32 {
        self.error_count
    }

    /// Get the number of warnings
    #[wasm_bindgen(getter)]
    pub fn warning_count(&self) -> u32 {
        self.warning_count
    }

    /// Get the number of info notices
    #[wasm_bindgen(getter)]
    pub fn info_count(&self) -> u32 {
        self.info_count
    }

    /// Check if validation passed (no errors)
    #[wasm_bindgen(getter)]
    pub fn is_valid(&self) -> bool {
        self.error_count == 0
    }
}

/// Maximum ZIP size accepted for in-browser (wasm32) validation.
///
/// Compressed size alone is not a reliable memory predictor, so validation also
/// checks the total declared uncompressed size below.
const MAX_FILE_SIZE_BYTES: usize = 70 * 1024 * 1024;

/// Maximum total size of root-level files declared in the ZIP central
/// directory. This remains a defense-in-depth check because compressed size
/// alone does not reliably predict the memory needed while parsing.
const MAX_UNCOMPRESSED_SIZE_BYTES: u64 = 512 * 1024 * 1024;

/// Validate a GTFS ZIP file from bytes
///
/// # Arguments
/// * `zip_bytes` - The raw bytes of a GTFS ZIP file
/// * `country_code` - Optional ISO 3166-1 alpha-2 country code for country-specific validation
/// * `date` - Optional validation date in YYYY-MM-DD format
///
/// # Returns
/// A ValidationResult containing the JSON report and summary counts
///
/// # Errors
/// Throws a JavaScript error if the ZIP exceeds 70 MB compressed or 512 MB
/// uncompressed
#[wasm_bindgen]
pub fn validate_gtfs(
    zip_bytes: &[u8],
    country_code: Option<String>,
    date: Option<String>,
) -> Result<ValidationResult, JsValue> {
    // Check file size limit
    if zip_bytes.len() > MAX_FILE_SIZE_BYTES {
        let size_mb = zip_bytes.len() as f64 / (1024.0 * 1024.0);
        return Err(JsValue::from_str(&format!(
            "File too large ({:.1} MB). Maximum size for browser validation is 70 MB. \
             Please download the desktop application or CLI for larger feeds.",
            size_mb
        )));
    }

    // Keep this reader for validation after inspecting the central directory,
    // avoiding a second copy of the input in the WASM heap.
    let reader = GtfsBytesReader::from_slice(zip_bytes);
    if let Ok(files) = reader.get_files_with_sizes() {
        let uncompressed_bytes = files.values().copied().fold(0u64, u64::saturating_add);
        if uncompressed_bytes > MAX_UNCOMPRESSED_SIZE_BYTES {
            let size_mb = uncompressed_bytes as f64 / (1024.0 * 1024.0);
            return Err(JsValue::from_str(&format!(
                "Feed expands to {:.1} MB. Maximum uncompressed size for browser validation is 512 MB. \
                 Please download the desktop application or CLI for larger feeds.",
                size_mb
            )));
        }
    }

    // Set validation context
    // We clone these for the report context later
    let report_country_code = country_code.clone();
    let report_date = date.clone();

    let _country_guard = set_validation_country_code(country_code);
    let naive_date = date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let _date_guard = set_validation_date(naive_date);
    let _thorough_guard = set_thorough_mode_enabled(false); // Default to standard mode

    // Create runner with all validators
    let runner = default_runner();
    let timing = TimingCollector::new();

    // Run validation (no progress handler in WASM - it runs synchronously)
    let outcome = validate_bytes_reader_with_timing(&reader, &runner, &timing);
    let timings_json = timing.summary().to_json().to_string();

    // Count notices by severity
    let (error_count, warning_count, info_count) = count_notices(&outcome.notices);

    // Encode notices to JSON
    let notices_vec: Vec<_> = outcome
        .notices
        .iter()
        .map(|notice| WasmNotice {
            notice,
            total_notices: outcome.notices.count_for(&notice.code, notice.severity),
        })
        .collect();
    let json = serde_json::to_string(&notices_vec).unwrap_or_else(|_| "[]".to_string());

    // Generate HTML Report
    let mut summary_context = ReportSummaryContext::new().with_validator_version(version());

    if let Some(cc) = report_country_code {
        summary_context = summary_context.with_country_code(cc);
    }
    if let Some(d) = report_date {
        summary_context = summary_context.with_date_for_validation(d);
    }

    if let Some(feed) = &outcome.feed {
        summary_context = summary_context.with_feed(feed);
    }

    let summary = ReportSummary::from_context(summary_context);
    let html_context = HtmlReportContext::from_summary(&summary, "Uploaded File");
    let html = generate_html_report_string(&outcome.notices, &summary, html_context);

    Ok(ValidationResult {
        json,
        html,
        timings_json,
        error_count,
        warning_count,
        info_count,
    })
}

/// Validate GTFS and return only the JSON report (simpler API)
#[wasm_bindgen]
pub fn validate_gtfs_json(
    zip_bytes: &[u8],
    country_code: Option<String>,
    date: Option<String>,
) -> Result<String, JsValue> {
    let mut result = validate_gtfs(zip_bytes, country_code, date)?;
    Ok(result.take_json())
}

fn count_notices(notices: &NoticeContainer) -> (u32, u32, u32) {
    let count = |severity| u32::try_from(notices.count_by_severity(severity)).unwrap_or(u32::MAX);
    (
        count(NoticeSeverity::Error),
        count(NoticeSeverity::Warning),
        count(NoticeSeverity::Info),
    )
}

#[derive(Serialize)]
struct WasmNotice<'a> {
    #[serde(flatten)]
    notice: &'a gtfs_guru_core::ValidationNotice,
    #[serde(rename = "totalNotices")]
    total_notices: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
    }
}
