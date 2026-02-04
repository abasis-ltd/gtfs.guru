use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use anyhow::Context;
use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::Utc;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use include_dir::{include_dir, Dir};
use mime_guess::MimeGuess;

use gtfs_guru_core::{default_runner, validate_input, GtfsInput, NoticeContainer};
use gtfs_guru_report::{
    write_html_report, HtmlReportContext, ReportSummary, ReportSummaryContext, ValidationReport,
};

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);
static WEBSITE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../website");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let base_dir = load_base_dir();
    let public_base_url = load_public_base_url();
    tokio::fs::create_dir_all(&base_dir).await?;
    let state = AppState::new(base_dir, public_base_url);
    spawn_job_cleanup(state.clone());

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/version", get(version))
        .route("/create-job", post(create_job))
        .route("/run-validator", post(run_validator))
        .route("/error", post(error))
        .route("/upload/:job_id", put(upload_job))
        .route("/jobs/:job_id/status", get(job_status))
        .route("/jobs/:job_id/report.json", get(job_report_json))
        .route("/jobs/:job_id/report.html", get(job_report_html))
        .route("/jobs/:job_id/system_errors.json", get(job_system_errors))
        .route(
            "/jobs/:job_id/execution_result.json",
            get(job_execution_result),
        )
        .route("/sitemap.xml", get(sitemap_xml))
        .route("/", get(index_html))
        .route("/*path", get(static_file))
        .with_state(state);
    let addr = "0.0.0.0:3000";
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateJobRequest {
    country_code: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateJobResponse {
    job_id: String,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PubsubEnvelope {
    message: Option<PubsubMessage>,
}

#[derive(Debug, Deserialize)]
struct PubsubMessage {
    data: Option<String>,
}

#[derive(Debug, Serialize)]
struct VersionResponse {
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum JobStatus {
    AwaitingUpload,
    Processing,
    Success,
    Error,
}

#[derive(Debug, Clone)]
struct Job {
    id: String,
    status: JobStatus,
    country_code: Option<String>,
    input_path: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobMetadata {
    id: String,
    status: JobStatus,
    country_code: Option<String>,
    input_path: Option<String>,
    output_dir: Option<String>,
    error: Option<String>,
    created_at_millis: u128,
    updated_at_millis: u128,
}

#[derive(Clone)]
struct AppState {
    jobs: Arc<RwLock<HashMap<String, Job>>>,
    base_dir: PathBuf,
    public_base_url: String,
}

impl AppState {
    fn new(base_dir: PathBuf, public_base_url: String) -> Self {
        let jobs = load_jobs(&base_dir);
        Self {
            jobs: Arc::new(RwLock::new(jobs)),
            base_dir,
            public_base_url,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobStatusResponse {
    job_id: String,
    status: JobStatus,
    error: Option<String>,
    upload_url: Option<String>,
    report_json_url: Option<String>,
    report_html_url: Option<String>,
    system_errors_url: Option<String>,
    execution_result_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionResult {
    status: String,
    error: String,
}

async fn index_html() -> Response {
    serve_static_path("index.html")
}

async fn sitemap_xml() -> Response {
    let lastmod = Utc::now().format("%Y-%m-%d").to_string();
    let base_url = "https://gtfs.guru";

    let mut paths: Vec<String> = WEBSITE_DIR
        .files()
        .filter_map(|file| {
            let path = file.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("html") {
                return None;
            }
            let path_str = path.to_string_lossy().to_string();
            if path_str == "index.html" {
                return None;
            }
            Some(path_str)
        })
        .collect();
    paths.sort();

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    xml.push_str("  <url>\n");
    xml.push_str(&format!("    <loc>{}/</loc>\n", base_url));
    xml.push_str(&format!("    <lastmod>{}</lastmod>\n", lastmod));
    xml.push_str("  </url>\n");
    for path in paths {
        xml.push_str("  <url>\n");
        xml.push_str(&format!("    <loc>{}/{}</loc>\n", base_url, path));
        xml.push_str(&format!("    <lastmod>{}</lastmod>\n", lastmod));
        xml.push_str("  </url>\n");
    }
    xml.push_str("</urlset>");

    let mut response = Response::new(Body::from(xml));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    response
}

async fn static_file(AxumPath(path): AxumPath<String>) -> Response {
    let Some(clean_path) = sanitize_path(&path) else {
        return not_found();
    };
    if clean_path.is_empty() {
        return serve_static_path("index.html");
    }
    serve_static_path(&clean_path)
}

fn sanitize_path(path: &str) -> Option<String> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(String::new());
    }
    let mut segments = Vec::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        segments.push(segment);
    }
    Some(segments.join("/"))
}

fn serve_static_path(path: &str) -> Response {
    let Some(file) = WEBSITE_DIR.get_file(path) else {
        return not_found();
    };
    let mime = MimeGuess::from_path(path).first_or_octet_stream();
    let mut response = Response::new(Body::from(file.contents().to_owned()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str(mime.as_ref())
            .unwrap_or_else(|_| header::HeaderValue::from_static("application/octet-stream")),
    );
    response
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("Not Found"))
        .unwrap_or_else(|_| Response::new(Body::from("Not Found")))
}

async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn create_job(
    State(state): State<AppState>,
    body: Option<Json<CreateJobRequest>>,
) -> Json<CreateJobResponse> {
    let job_id = next_job_id();
    let job_dir = state.base_dir.join(&job_id);
    let _ = tokio::fs::create_dir_all(&job_dir).await;

    let country_code = body.as_ref().and_then(|value| value.country_code.clone());
    let source_url = body.as_ref().and_then(|value| value.url.clone());
    let input_path = source_url.as_ref().map(|_| job_dir.join("input.zip"));

    let status = if source_url.is_some() {
        JobStatus::Processing
    } else {
        JobStatus::AwaitingUpload
    };

    let job = Job {
        id: job_id.clone(),
        status,
        country_code,
        input_path,
        output_dir: Some(job_dir.join("output")),
        error: None,
    };
    insert_job(&state, job);

    if let Some(url) = source_url {
        spawn_job_processing(state.clone(), job_id.clone(), url);
        Json(CreateJobResponse { job_id, url: None })
    } else {
        Json(CreateJobResponse {
            job_id: job_id.clone(),
            url: Some(format!("{}/upload/{}", state.public_base_url, job_id)),
        })
    }
}

async fn run_validator(
    State(state): State<AppState>,
    Json(payload): Json<PubsubEnvelope>,
) -> StatusCode {
    let data = payload
        .message
        .and_then(|msg| msg.data)
        .and_then(decode_pubsub_data);
    let Some(job_id) = data.and_then(|name| extract_job_id(&name)) else {
        return StatusCode::BAD_REQUEST;
    };
    if !job_exists(&state, &job_id) {
        return StatusCode::NOT_FOUND;
    }
    spawn_job_processing(state.clone(), job_id, String::new());
    StatusCode::OK
}

async fn error() -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn upload_job(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
    body: axum::body::Bytes,
) -> StatusCode {
    if !job_exists(&state, &job_id) {
        return StatusCode::NOT_FOUND;
    }
    let job_dir = state.base_dir.join(&job_id);
    if tokio::fs::create_dir_all(&job_dir).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    let input_path = job_dir.join("input.zip");
    if tokio::fs::write(&input_path, body).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    update_job_input(&state, &job_id, input_path);
    spawn_job_processing(state, job_id, String::new());
    StatusCode::OK
}

async fn job_status(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<JobStatusResponse>, StatusCode> {
    let job = get_job(&state, &job_id).ok_or(StatusCode::NOT_FOUND)?;
    let base_url = state.public_base_url.trim_end_matches('/');
    let upload_url = if matches!(job.status, JobStatus::AwaitingUpload) {
        Some(format!("{}/upload/{}", base_url, job_id))
    } else {
        None
    };
    let report_json_url = Some(format!("{}/jobs/{}/report.json", base_url, job_id));
    let report_html_url = Some(format!("{}/jobs/{}/report.html", base_url, job_id));
    let system_errors_url = Some(format!("{}/jobs/{}/system_errors.json", base_url, job_id));
    let execution_result_url = Some(format!(
        "{}/jobs/{}/execution_result.json",
        base_url, job_id
    ));
    Ok(Json(JobStatusResponse {
        job_id: job.id,
        status: job.status,
        error: job.error,
        upload_url,
        report_json_url,
        report_html_url,
        system_errors_url,
        execution_result_url,
    }))
}

async fn job_report_json(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let path = job_output_path(&state, &job_id, "report.json")?;
    read_file_response(path, "application/json").await
}

async fn job_system_errors(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let path = job_output_path(&state, &job_id, "system_errors.json")?;
    read_file_response(path, "application/json").await
}

async fn job_execution_result(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let path = job_output_path(&state, &job_id, "execution_result.json")?;
    read_file_response(path, "application/json").await
}

async fn job_report_html(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let path = job_output_path(&state, &job_id, "report.html")?;
    read_file_response(path, "text/html; charset=utf-8").await
}

fn next_job_id() -> String {
    let counter = JOB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let millis = current_millis();
    format!("job-{}-{}-{}", std::process::id(), counter, millis)
}

fn load_base_dir() -> PathBuf {
    std::env::var("GTFS_VALIDATOR_WEB_BASE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/web_jobs"))
}

fn load_public_base_url() -> String {
    let fallback = "http://localhost:3000".to_string();
    match std::env::var("GTFS_VALIDATOR_WEB_PUBLIC_BASE_URL") {
        Ok(value) => value.trim_end_matches('/').to_string(),
        Err(_) => fallback,
    }
}

fn insert_job(state: &AppState, job: Job) {
    let job_id = job.id.clone();
    if let Ok(mut jobs) = state.jobs.write() {
        jobs.insert(job_id.clone(), job);
    }
    persist_job_metadata(state, job_id.as_str());
}

fn job_exists(state: &AppState, job_id: &str) -> bool {
    state
        .jobs
        .read()
        .map(|jobs| jobs.contains_key(job_id))
        .unwrap_or(false)
}

fn get_job(state: &AppState, job_id: &str) -> Option<Job> {
    state
        .jobs
        .read()
        .ok()
        .and_then(|jobs| jobs.get(job_id).cloned())
}

fn update_job_input(state: &AppState, job_id: &str, input_path: PathBuf) {
    if let Ok(mut jobs) = state.jobs.write() {
        if let Some(job) = jobs.get_mut(job_id) {
            job.input_path = Some(input_path);
        }
    }
    persist_job_metadata(state, job_id);
}

fn update_job_status(state: &AppState, job_id: &str, status: JobStatus, error: Option<String>) {
    if let Ok(mut jobs) = state.jobs.write() {
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = status;
            job.error = error;
        }
    }
    persist_job_metadata(state, job_id);
}

fn job_output_path(state: &AppState, job_id: &str, name: &str) -> Result<PathBuf, StatusCode> {
    let job = get_job(state, job_id).ok_or(StatusCode::NOT_FOUND)?;
    let output_dir = job.output_dir.ok_or(StatusCode::NOT_FOUND)?;
    Ok(output_dir.join(name))
}

async fn read_file_response(
    path: PathBuf,
    content_type: &'static str,
) -> Result<impl IntoResponse, StatusCode> {
    let data = tokio::fs::read(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(([(header::CONTENT_TYPE, content_type)], data))
}

fn spawn_job_processing(state: AppState, job_id: String, url: String) {
    tokio::spawn(async move {
        let state_for_block = state.clone();
        let job_id_for_block = job_id.clone();
        let url_for_block = url.clone();
        let result = tokio::task::spawn_blocking(move || {
            process_job(&state_for_block, &job_id_for_block, &url_for_block)
        })
        .await;

        if let Err(err) = result {
            update_job_status(
                &state,
                &job_id,
                JobStatus::Error,
                Some(format!("join error: {}", err)),
            );
        }
    });
}

fn process_job(state: &AppState, job_id: &str, url: &str) {
    update_job_status(state, job_id, JobStatus::Processing, None);
    let job = match get_job(state, job_id) {
        Some(job) => job,
        None => return,
    };
    let job_dir = state.base_dir.join(job_id);
    let input_path = if !url.is_empty() {
        let path = job_dir.join("input.zip");
        if let Err(err) = download_url_to_path(url, &path) {
            write_execution_result(&job_dir, Err(err.to_string()));
            update_job_status(state, job_id, JobStatus::Error, Some(err.to_string()));
            return;
        }
        path
    } else if let Some(input_path) = job.input_path.clone() {
        input_path
    } else {
        write_execution_result(&job_dir, Err("missing input".to_string()));
        update_job_status(
            state,
            job_id,
            JobStatus::Error,
            Some("missing input".to_string()),
        );
        return;
    };

    let output_dir = job_dir.join("output");
    if let Err(err) = std::fs::create_dir_all(&output_dir) {
        write_execution_result(&job_dir, Err(err.to_string()));
        update_job_status(state, job_id, JobStatus::Error, Some(err.to_string()));
        return;
    }

    let started_at = Instant::now();
    let input_uri = if url.is_empty() { None } else { Some(url) };
    let result = run_validation(
        &input_path,
        &output_dir,
        job.country_code.as_deref(),
        input_uri,
        started_at,
    );
    match result {
        Ok(()) => {
            write_execution_result(&job_dir, Ok(()));
            update_job_status(state, job_id, JobStatus::Success, None);
        }
        Err(err) => {
            write_execution_result(&job_dir, Err(err.to_string()));
            update_job_status(state, job_id, JobStatus::Error, Some(err.to_string()));
        }
    }
}

fn run_validation(
    input_path: &Path,
    output_dir: &Path,
    country_code: Option<&str>,
    input_uri: Option<&str>,
    started_at: Instant,
) -> anyhow::Result<()> {
    let input = GtfsInput::from_path(input_path)?;
    let runner = default_runner();
    let outcome = validate_input(&input, &runner);
    let elapsed = started_at.elapsed();
    let (validation_notices, system_errors) = if outcome.feed.is_none() {
        (NoticeContainer::new(), outcome.notices)
    } else {
        (outcome.notices, NoticeContainer::new())
    };

    let mut summary_context = ReportSummaryContext::new()
        .with_gtfs_input(input_path)
        .with_output_directory(output_dir)
        .with_validation_time_seconds(elapsed.as_secs_f64())
        .with_validator_version(env!("CARGO_PKG_VERSION"))
        .with_threads(1);
    if let Some(uri) = input_uri {
        summary_context = summary_context.with_gtfs_input_uri(uri);
    }
    if let Some(code) = country_code {
        summary_context = summary_context.with_country_code(code);
    }
    if let Some(feed) = outcome.feed.as_ref() {
        summary_context = summary_context.with_feed(feed);
    }

    let summary = ReportSummary::from_context(summary_context);
    let gtfs_source_label = input_uri
        .map(|value| value.to_string())
        .unwrap_or_else(|| input_path.display().to_string());
    let html_context = HtmlReportContext::from_summary(&summary, gtfs_source_label);
    write_html_report(
        output_dir.join("report.html"),
        &validation_notices,
        &summary,
        html_context,
    )?;
    let report = ValidationReport::from_container_with_summary(&validation_notices, summary);
    report.write_json(output_dir.join("report.json"))?;
    ValidationReport::from_container(&system_errors)
        .write_json(output_dir.join("system_errors.json"))?;
    Ok(())
}

fn write_execution_result(job_dir: &Path, result: Result<(), String>) {
    let output_dir = job_dir.join("output");
    let _ = std::fs::create_dir_all(&output_dir);
    let payload = match result {
        Ok(()) => ExecutionResult {
            status: "success".to_string(),
            error: "".to_string(),
        },
        Err(err) => ExecutionResult {
            status: "error".to_string(),
            error: err,
        },
    };
    if let Ok(json) = serde_json::to_string_pretty(&payload) {
        let _ = std::fs::write(
            output_dir.join("execution_result.json"),
            format!("{}\n", json),
        );
    }
}

fn download_url_to_path(url: &str, path: &Path) -> anyhow::Result<()> {
    let client = Client::builder()
        .user_agent(format!(
            "gtfs-validator-rust-web/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .context("build http client")?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("download gtfs from {}", url))?
        .error_for_status()
        .with_context(|| format!("download gtfs from {}", url))?;
    let mut file =
        std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    std::io::copy(&mut response, &mut file).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn decode_pubsub_data(data: String) -> Option<String> {
    if let Ok(decoded) = STANDARD.decode(data.as_bytes()) {
        if let Ok(text) = String::from_utf8(decoded) {
            if let Ok(payload) = serde_json::from_str::<HashMap<String, String>>(&text) {
                if let Some(name) = payload.get("name").cloned() {
                    return Some(name);
                }
            }
        }
    }
    if data.trim_start().starts_with('{') {
        return serde_json::from_str::<HashMap<String, String>>(&data)
            .ok()
            .and_then(|payload| payload.get("name").cloned());
    }
    Some(data)
}

fn extract_job_id(name: &str) -> Option<String> {
    let segments: Vec<&str> = name
        .split(['/', '\\'])
        .filter(|value| !value.trim().is_empty())
        .collect();
    for segment in &segments {
        if segment.starts_with("job-") {
            return Some((*segment).to_string());
        }
    }
    segments.first().map(|value| (*value).to_string())
}

fn spawn_job_cleanup(state: AppState) {
    let ttl_ms = load_job_ttl_ms();
    if ttl_ms == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            cleanup_jobs(&state, ttl_ms);
        }
    });
}

fn cleanup_jobs(state: &AppState, ttl_ms: u128) {
    let now = current_millis();
    let mut expired = Vec::new();
    if let Ok(jobs) = state.jobs.read() {
        for (job_id, job) in jobs.iter() {
            if matches!(job.status, JobStatus::Processing) {
                continue;
            }
            let meta_path = state.base_dir.join(job_id).join("job.json");
            let updated_at = read_metadata_timestamp(&meta_path).unwrap_or(now);
            if now.saturating_sub(updated_at) >= ttl_ms {
                expired.push((job_id.clone(), job.output_dir.clone()));
            }
        }
    }

    if expired.is_empty() {
        return;
    }

    if let Ok(mut jobs) = state.jobs.write() {
        for (job_id, output_dir) in &expired {
            jobs.remove(job_id);
            let job_dir = state.base_dir.join(job_id);
            let _ = std::fs::remove_dir_all(&job_dir);
            if let Some(output_dir) = output_dir.as_ref() {
                let _ = std::fs::remove_dir_all(output_dir);
            }
        }
    }
}

fn read_metadata_timestamp(path: &Path) -> Option<u128> {
    let data = std::fs::read_to_string(path).ok()?;
    let metadata: JobMetadata = serde_json::from_str(&data).ok()?;
    Some(metadata.updated_at_millis)
}

fn load_job_ttl_ms() -> u128 {
    let default_ms: u128 = 24 * 60 * 60 * 1000;
    match std::env::var("GTFS_VALIDATOR_WEB_JOB_TTL_SECONDS") {
        Ok(value) => value
            .trim()
            .parse::<u128>()
            .ok()
            .map(|seconds| seconds.saturating_mul(1000))
            .unwrap_or(default_ms),
        Err(_) => default_ms,
    }
}

fn current_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn load_jobs(base_dir: &Path) -> HashMap<String, Job> {
    let mut jobs = HashMap::new();
    let entries = match std::fs::read_dir(base_dir) {
        Ok(entries) => entries,
        Err(_) => return jobs,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let meta_path = path.join("job.json");
        let data = match std::fs::read_to_string(&meta_path) {
            Ok(data) => data,
            Err(_) => continue,
        };
        let metadata: JobMetadata = match serde_json::from_str(&data) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        let job = metadata.to_job(&path);
        jobs.insert(job.id.clone(), job);
    }

    jobs
}

fn persist_job_metadata(state: &AppState, job_id: &str) {
    let job = match get_job(state, job_id) {
        Some(job) => job,
        None => return,
    };
    let job_dir = state.base_dir.join(&job.id);
    let mut metadata = JobMetadata::from_job(&job, &job_dir);
    let meta_path = job_dir.join("job.json");
    if let Ok(data) = std::fs::read_to_string(&meta_path) {
        if let Ok(existing) = serde_json::from_str::<JobMetadata>(&data) {
            metadata.created_at_millis = existing.created_at_millis;
        }
    }
    metadata.updated_at_millis = current_millis();
    let Ok(json) = serde_json::to_string_pretty(&metadata) else {
        return;
    };
    let _ = std::fs::create_dir_all(&job_dir);
    let _ = std::fs::write(&meta_path, format!("{}\n", json));
}

impl JobMetadata {
    fn from_job(job: &Job, job_dir: &Path) -> Self {
        let now = current_millis();
        Self {
            id: job.id.clone(),
            status: job.status.clone(),
            country_code: job.country_code.clone(),
            input_path: job
                .input_path
                .as_ref()
                .map(|path| path_to_metadata(job_dir, path)),
            output_dir: job
                .output_dir
                .as_ref()
                .map(|path| path_to_metadata(job_dir, path)),
            error: job.error.clone(),
            created_at_millis: now,
            updated_at_millis: now,
        }
    }

    fn to_job(&self, job_dir: &Path) -> Job {
        let output_dir = self
            .output_dir
            .as_deref()
            .map(|path| resolve_job_path(job_dir, path))
            .or_else(|| Some(job_dir.join("output")));
        Job {
            id: self.id.clone(),
            status: self.status.clone(),
            country_code: self.country_code.clone(),
            input_path: self
                .input_path
                .as_deref()
                .map(|path| resolve_job_path(job_dir, path)),
            output_dir,
            error: self.error.clone(),
        }
    }
}

fn path_to_metadata(job_dir: &Path, path: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(job_dir) {
        relative.to_string_lossy().to_string()
    } else {
        path.to_string_lossy().to_string()
    }
}

fn resolve_job_path(job_dir: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        job_dir.join(path)
    }
}
