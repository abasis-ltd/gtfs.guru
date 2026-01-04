//! WASM integration tests
//!
//! Run with: wasm-pack test --node
//! Or for browser: wasm-pack test --headless --chrome

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use gtfs_validator_wasm::{init, validate_gtfs, validate_gtfs_json, version};

#[wasm_bindgen_test]
fn test_init() {
    // Should not panic
    init();
}

#[wasm_bindgen_test]
fn test_version() {
    let v = version();
    assert!(!v.is_empty(), "Version should not be empty");
    assert!(v.contains('.'), "Version should contain a dot");
}

#[wasm_bindgen_test]
fn test_validate_empty_bytes() {
    let result = validate_gtfs(&[], None);
    // Empty bytes should produce errors
    assert!(
        result.error_count() > 0,
        "Empty input should produce errors"
    );
    assert!(!result.is_valid(), "Empty input should not be valid");
}

#[wasm_bindgen_test]
fn test_validate_invalid_zip() {
    let invalid_data = vec![0u8; 100];
    let result = validate_gtfs(&invalid_data, None);
    // Invalid ZIP should produce errors
    assert!(
        result.error_count() > 0,
        "Invalid ZIP should produce errors"
    );
    assert!(!result.is_valid(), "Invalid ZIP should not be valid");
}

#[wasm_bindgen_test]
fn test_validate_with_country_code() {
    // Should handle country code without panicking
    let result = validate_gtfs(&[], Some("US".to_string()));
    assert!(result.error_count() > 0);
}

#[wasm_bindgen_test]
fn test_validate_json_returns_valid_json() {
    let json = validate_gtfs_json(&[], None);
    // Should be valid JSON (at minimum an empty array or error object)
    assert!(
        json.starts_with('[') || json.starts_with('{'),
        "Should return valid JSON"
    );
}

#[wasm_bindgen_test]
fn test_validation_result_accessors() {
    let result = validate_gtfs(&[], None);

    // All accessors should work without panic
    let _ = result.json();
    let _ = result.error_count();
    let _ = result.warning_count();
    let _ = result.info_count();
    let _ = result.is_valid();
}

#[wasm_bindgen_test]
fn test_json_parseable() {
    let result = validate_gtfs(&[], None);
    let json = result.json();

    // JSON should be parseable
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
    assert!(parsed.is_ok(), "JSON should be parseable: {}", json);
}
