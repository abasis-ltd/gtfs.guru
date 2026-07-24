use wasm_bindgen::prelude::*;

use gtfs_guru_core::{
    default_runner, set_notice_group_limit, set_thorough_mode_enabled, set_validation_country_code,
    set_validation_date, validate_bytes,
};
use gtfs_guru_report::{
    generate_html_report_string, HtmlReportContext, ReportSummary, ReportSummaryContext,
};

#[cfg(feature = "console_error_panic_hook")]
pub use console_error_panic_hook::set_once as set_panic_hook;

// Multithreaded build only: exposes `initThreadPool(numThreads)` to JS. The
// worker must await it once (after `init()`) before calling `validate_gtfs`,
// otherwise the first rayon parallel iterator has no pool and panics.
#[cfg(feature = "threads")]
pub use wasm_bindgen_rayon::init_thread_pool;

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
    truncated: bool,
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

    /// True when the notice list in `json` was capped per issue type to keep
    /// memory bounded. Counts (`error_count` etc.) are always exact.
    #[wasm_bindgen(getter)]
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Coarse upper bound on the ZIP itself (sanity check before we even look
/// inside). The real gate is `MAX_UNCOMPRESSED_BYTES` below.
const MAX_FILE_SIZE_BYTES: usize = 150 * 1024 * 1024;

/// Maximum uncompressed feed size accepted for in-browser (wasm32) validation.
///
/// The ceiling is the wasm32 linear-memory limit (~4 GB), and measured peak
/// memory is ~4-5x the UNCOMPRESSED size — the ZIP size is a bad proxy in
/// both directions (measured on real feeds: Hallandstrafiken 129 MB zip /
/// 576 MB raw peaks at 2.3 GB and fits, while Île-de-France 107 MB zip /
/// 1.06 GB raw needs ~5 GB and aborts). 700 MB raw keeps peak under ~3.5 GB.
/// Entry sizes come from the ZIP central directory (no decompression), so
/// this also rejects zip bombs cheaply. Beyond the limit we bail out with a
/// clear message pointing at the desktop app / CLI rather than risk an OOM
/// that takes down the worker.
const MAX_UNCOMPRESSED_BYTES: u64 = 700 * 1024 * 1024;

/// Returns a user-facing error when the feed is too big for in-browser
/// validation, `None` when it is fine. An unreadable archive returns `None`:
/// `validate_bytes` will produce the proper "not a valid ZIP" notice.
fn feed_size_error(zip_bytes: &[u8]) -> Option<String> {
    if zip_bytes.len() > MAX_FILE_SIZE_BYTES {
        let size_mb = zip_bytes.len() as f64 / (1024.0 * 1024.0);
        return Some(format!(
            "File too large ({:.1} MB). Maximum size for browser validation is {} MB. \
             Please download the desktop application or CLI for larger feeds.",
            size_mb,
            MAX_FILE_SIZE_BYTES / (1024 * 1024),
        ));
    }

    let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)) else {
        return None;
    };
    let mut uncompressed: u64 = 0;
    for index in 0..archive.len() {
        if let Ok(entry) = archive.by_index_raw(index) {
            uncompressed = uncompressed.saturating_add(entry.size());
        }
    }
    if uncompressed > MAX_UNCOMPRESSED_BYTES {
        let raw_mb = uncompressed as f64 / (1024.0 * 1024.0);
        return Some(format!(
            "This feed unpacks to {:.0} MB of data — too large for in-browser validation \
             (limit: {} MB uncompressed, roughly the memory a browser tab can hold). \
             Please download the desktop application or CLI for feeds of any size.",
            raw_mb,
            MAX_UNCOMPRESSED_BYTES / (1024 * 1024),
        ));
    }
    None
}

/// Maximum notices stored per (code, severity) group in the browser.
///
/// Feeds with pervasive issues can emit millions of notices for the same
/// rule; each one holds several heap strings plus a context map, so an
/// unbounded list — and the flat JSON string built from it — is what
/// actually OOMs the wasm32 heap, not the feed size. Storing the first 10k
/// per issue type keeps memory bounded while the report still shows exact
/// totals (counters keep counting dropped notices).
const MAX_NOTICES_PER_CODE_AND_SEVERITY: usize = 10_000;

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
/// Throws a JavaScript error if the feed exceeds the browser size limits
/// (150 MB zipped / 700 MB uncompressed)
#[wasm_bindgen]
pub fn validate_gtfs(
    zip_bytes: &[u8],
    country_code: Option<String>,
    date: Option<String>,
) -> Result<ValidationResult, JsValue> {
    if let Some(message) = feed_size_error(zip_bytes) {
        return Err(JsValue::from_str(&message));
    }

    // Set validation context
    // We clone these for the report context later
    let report_country_code = country_code.clone();
    let report_date = date.clone();

    let _country_guard = set_validation_country_code(country_code);
    let naive_date = date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let _date_guard = set_validation_date(naive_date);
    let _thorough_guard = set_thorough_mode_enabled(false); // Default to standard mode
    let _notice_limit_guard = set_notice_group_limit(Some(MAX_NOTICES_PER_CODE_AND_SEVERITY));

    // Create runner with all validators
    let runner = default_runner();

    // Run validation (no progress handler in WASM - it runs synchronously)
    let outcome = validate_bytes(zip_bytes, &runner);

    // Exact severity totals (they include notices dropped by the group cap)
    let (errors, warnings, infos) = outcome.notices.severity_counts();
    let (error_count, warning_count, info_count) = (errors as u32, warnings as u32, infos as u32);
    let truncated = outcome.notices.is_truncated();

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
        truncated,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
    }

    fn zip_with_stored_entry(name: &str, payload: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file(name, options).unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn small_feed_passes_size_gate() {
        let bytes = zip_with_stored_entry("stops.txt", b"stop_id,stop_name\n1,A\n");
        assert_eq!(feed_size_error(&bytes), None);
    }

    #[test]
    fn dense_feed_rejected_by_uncompressed_size() {
        // Highly compressible payload: tiny zip, huge uncompressed size —
        // exactly the IDFM-style case the old zip-size cap missed.
        let payload = vec![b'a'; (MAX_UNCOMPRESSED_BYTES + 1) as usize];
        let bytes = zip_with_stored_entry("stop_times.txt", &payload);
        assert!(bytes.len() < MAX_FILE_SIZE_BYTES);
        let message = feed_size_error(&bytes).expect("must be rejected");
        assert!(message.contains("unpacks to"), "got: {message}");
    }

    #[test]
    fn non_zip_bytes_pass_gate_for_downstream_error() {
        assert_eq!(feed_size_error(b"not a zip at all"), None);
    }
}
