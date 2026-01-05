use wasm_bindgen::prelude::*;

use gtfs_guru_core::{
    default_runner, set_validation_country_code, set_validation_date, validate_bytes,
    NoticeContainer, NoticeSeverity,
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

/// Validate a GTFS ZIP file from bytes
///
/// # Arguments
/// * `zip_bytes` - The raw bytes of a GTFS ZIP file
/// * `country_code` - Optional ISO 3166-1 alpha-2 country code for country-specific validation
///
/// # Returns
/// A ValidationResult containing the JSON report and summary counts
#[wasm_bindgen]
pub fn validate_gtfs(zip_bytes: &[u8], country_code: Option<String>) -> ValidationResult {
    // Set validation context
    let _country_guard = set_validation_country_code(country_code);
    let _date_guard = set_validation_date(None); // Use current date

    // Create runner with all validators
    let runner = default_runner();

    // Run validation
    let outcome = validate_bytes(zip_bytes, &runner);

    // Count notices by severity
    let (error_count, warning_count, info_count) = count_notices(&outcome.notices);

    // Serialize notices to JSON
    let notices: Vec<_> = outcome.notices.iter().collect();
    let json = serde_json::to_string(&notices).unwrap_or_else(|_| "[]".to_string());

    ValidationResult {
        json,
        error_count,
        warning_count,
        info_count,
    }
}

/// Validate GTFS and return only the JSON report (simpler API)
#[wasm_bindgen]
pub fn validate_gtfs_json(zip_bytes: &[u8], country_code: Option<String>) -> String {
    let result = validate_gtfs(zip_bytes, country_code);
    result.json
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
