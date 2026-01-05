//! GTFS Validator Desktop GUI - Tauri Library
//!
//! This module provides Tauri commands for GTFS validation.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::State;

use gtfs_guru_core::{
    default_runner, set_validation_country_code, validate_input, GtfsInput, NoticeSeverity,
};
use gtfs_guru_report::{
    write_html_report, HtmlReportContext, ReportSummary, ReportSummaryContext, ValidationReport,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoError {
    pub stop_name: String,
    pub stop_lat: f64,
    pub stop_lon: f64,
    pub match_lat: f64,
    pub match_lon: f64,
    pub shape_path: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub success: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub validation_time_secs: f64,
    pub html_report_path: Option<String>,
    pub json_report_path: Option<String>,
    pub error_message: Option<String>,
    pub geo_errors: Vec<GeoError>,
}

#[derive(Default)]
pub struct AppState {
    pub last_output_dir: Mutex<Option<PathBuf>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![validate_gtfs, get_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn validate_gtfs(
    path: Option<String>,
    url: Option<String>,
    country_code: Option<String>,
    state: State<'_, AppState>,
) -> Result<ValidationResult, String> {
    if path.is_none() && url.is_none() {
        return Err("Either path or url must be provided".to_string());
    }

    // Run validation in blocking task
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_validation(path, url, country_code.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?;

    let result = result?;

    // Store output dir for later
    if let Some(ref html_path) = result.html_report_path {
        if let Some(parent) = PathBuf::from(html_path).parent() {
            *state.last_output_dir.lock().unwrap() = Some(parent.to_path_buf());
        }
    }

    Ok(result)
}

#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn run_validation(
    path: Option<String>,
    url: Option<String>,
    country_code: Option<&str>,
) -> Result<ValidationResult, String> {
    // Determine input path (download if URL)
    let (input_path, _download_cleanup) = if let Some(url_str) = url {
        if url_str.trim().is_empty() {
            return Err("URL cannot be empty".to_string());
        }
        let temp_dir = std::env::temp_dir();
        let file_name = format!("gtfs_download_{}.zip", uuid::Uuid::new_v4());
        let download_path = temp_dir.join(&file_name);

        download_url_to_path(&url_str, &download_path).map_err(|e| e.to_string())?;
        (download_path, true)
    } else if let Some(p) = path {
        (PathBuf::from(p), false)
    } else {
        return Err("No input provided".to_string());
    };

    let input = GtfsInput::from_path(&input_path).map_err(|e| e.to_string())?;

    let started_at = Instant::now();

    // Set country code if provided
    let _guard = country_code.map(|c| set_validation_country_code(Some(c.to_string())));

    let runner = default_runner();
    let outcome = validate_input(&input, &runner);
    let elapsed = started_at.elapsed();

    // Create output directory
    let output_dir = std::env::temp_dir().join(format!(
        "gtfs_validation_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    ));
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    // Generate reports
    let mut summary_context = ReportSummaryContext::new()
        .with_gtfs_input(&input_path)
        .with_output_directory(&output_dir)
        .with_validation_time_seconds(elapsed.as_secs_f64())
        .with_validator_version(env!("CARGO_PKG_VERSION"))
        .with_threads(1);

    if let Some(code) = country_code {
        summary_context = summary_context.with_country_code(code);
    }
    if let Some(feed) = outcome.feed.as_ref() {
        summary_context = summary_context.with_feed(feed);
    }

    let summary = ReportSummary::from_context(summary_context);
    let gtfs_source_label = input_path.display().to_string();
    let html_context = HtmlReportContext::from_summary(&summary, gtfs_source_label);

    let html_path = output_dir.join("report.html");
    let json_path = output_dir.join("report.json");

    write_html_report(&html_path, &outcome.notices, &summary, html_context)
        .map_err(|e| e.to_string())?;

    let report = ValidationReport::from_container_with_summary(&outcome.notices, summary);
    report.write_json(&json_path).map_err(|e| e.to_string())?;

    // Count notices and extract geographic errors
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut info_count = 0;
    let mut geo_errors = Vec::new();

    for notice in outcome.notices.iter() {
        match notice.severity {
            NoticeSeverity::Error => error_count += 1,
            NoticeSeverity::Warning => warning_count += 1,
            NoticeSeverity::Info => info_count += 1,
        }

        // Extract geographic errors for map display
        if notice.code == "stop_too_far_from_shape"
            || notice.code == "stop_too_far_from_shape_using_user_distance"
        {
            if let Some(geo_error) = extract_geo_error(&notice.context) {
                geo_errors.push(geo_error);
            }
        }
    }

    Ok(ValidationResult {
        success: true,
        error_count,
        warning_count,
        info_count,
        validation_time_secs: elapsed.as_secs_f64(),
        html_report_path: Some(html_path.to_string_lossy().to_string()),
        json_report_path: Some(json_path.to_string_lossy().to_string()),
        error_message: None,
        geo_errors,
    })
}

fn extract_geo_error(
    context: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Option<GeoError> {
    let stop_location = context.get("stopLocation")?.as_array()?;
    let match_location = context.get("match")?.as_array()?;

    if stop_location.len() < 2 || match_location.len() < 2 {
        return None;
    }

    let stop_lat = stop_location[0].as_f64()?;
    let stop_lon = stop_location[1].as_f64()?;
    let match_lat = match_location[0].as_f64()?;
    let match_lon = match_location[1].as_f64()?;
    let stop_name = context
        .get("stopName")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    // Extract shape path if available
    let shape_path = context
        .get("shapePath")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|pt| {
                    let arr = pt.as_array()?;
                    if arr.len() >= 2 {
                        Some([arr[0].as_f64()?, arr[1].as_f64()?])
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Some(GeoError {
        stop_name,
        stop_lat,
        stop_lon,
        match_lat,
        match_lon,
        shape_path,
    })
}

fn download_url_to_path(url: &str, path: &std::path::Path) -> Result<(), anyhow::Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("gtfs-validator-gui/{}", env!("CARGO_PKG_VERSION")))
        .build()?;

    let mut response = client.get(url).send()?.error_for_status()?;

    let mut file = std::fs::File::create(path)?;
    std::io::copy(&mut response, &mut file)?;
    Ok(())
}
