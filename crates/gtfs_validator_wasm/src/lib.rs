use wasm_bindgen::prelude::*;

use gtfs_guru_core::{
    default_runner, set_thorough_mode_enabled, set_validation_country_code, set_validation_date,
    validate_bytes, NoticeContainer, NoticeSeverity,
};
use gtfs_guru_report::{
    generate_html_report_string, HtmlReportContext, ReportSummary, ReportSummaryContext,
};

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

/// Maximum file size for WASM validation (50 MB)
/// Larger files may cause memory issues in the browser.
const MAX_FILE_SIZE_BYTES: usize = 50 * 1024 * 1024;

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
/// Throws a JavaScript error if the file exceeds 50 MB
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
            "File too large ({:.1} MB). Maximum size for browser validation is 50 MB. \
             Please download the desktop application for larger feeds.",
            size_mb
        )));
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

    // Run validation (no progress handler in WASM - it runs synchronously)
    let outcome = validate_bytes(zip_bytes, &runner);

    // Count notices by severity
    let (error_count, warning_count, info_count) = count_notices(&outcome.notices);

    // Encode notices to JSON
    let notices_vec: Vec<_> = outcome.notices.iter().collect();
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
    let result = validate_gtfs(zip_bytes, country_code, date)?;
    Ok(result.json)
}

fn count_notices(notices: &NoticeContainer) -> (u32, u32, u32) {
    let mut errors = 0u32;
    let mut warnings = 0u32;
    let mut infos = 0u32;

    for notice in notices.iter() {
        match notice.severity {
            NoticeSeverity::Error => errors += 1,
            NoticeSeverity::Warning => warnings += 1,
            NoticeSeverity::Info => infos += 1,
        }
    }

    (errors, warnings, infos)
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
