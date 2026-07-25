use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path as AxumPath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::blocking::Client;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use include_dir::{include_dir, Dir};
use mime_guess::MimeGuess;

use gtfs_guru_core::{default_runner, validate_input, GtfsInput, NoticeContainer};
use gtfs_guru_report::{
    write_html_report, HtmlReportContext, ReportSummary, ReportSummaryContext, ValidationReport,
};

static WEBSITE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/website");

/// Default cap on an uploaded/downloaded GTFS archive (bytes). Overridable via
/// `GTFS_VALIDATOR_WEB_MAX_UPLOAD_BYTES`. Large public feeds run 200+ MB.
const DEFAULT_MAX_UPLOAD_BYTES: usize = 512 * 1024 * 1024;

/// Default number of validations that may run concurrently. Overridable via
/// `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_JOBS`. Validation is CPU- and
/// memory-heavy, so this bounds load from public traffic.
const DEFAULT_MAX_CONCURRENT_JOBS: usize = 4;

/// Default cap on how many jobs may be queued or running at once (admission
/// control). Overridable via `GTFS_VALIDATOR_WEB_MAX_QUEUED_JOBS`. Without this,
/// a flood of requests would spawn unbounded tasks all waiting for a run permit.
const DEFAULT_MAX_QUEUED_JOBS: usize = 64;

/// Keep the browser proxy aligned with the WASM validator's input limit.
const DEFAULT_MAX_PROXY_BYTES: usize = 70 * 1024 * 1024;
const DEFAULT_MAX_CONCURRENT_PROXY_REQUESTS: usize = 4;
const DEFAULT_MAX_PROXY_REQUESTS_PER_MINUTE: usize = 60;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let base_dir = load_base_dir();
    let public_base_url = load_public_base_url();
    tokio::fs::create_dir_all(&base_dir).await?;
    let state = AppState::new(base_dir, public_base_url);
    spawn_job_cleanup(state.clone());

    let max_upload_bytes = state.max_upload_bytes;
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/version", get(version))
        .route("/cors-proxy", get(cors_proxy))
        .route("/create-job", post(create_job))
        .route("/run-validator", post(run_validator))
        .route("/error", post(error))
        .route(
            "/upload/:job_id",
            put(upload_job).layer(DefaultBodyLimit::max(max_upload_bytes)),
        )
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
    max_upload_bytes: usize,
    job_semaphore: Arc<Semaphore>,
    admission_semaphore: Arc<Semaphore>,
    max_proxy_bytes: usize,
    proxy_semaphore: Arc<Semaphore>,
    proxy_rate_limiter: Arc<ProxyRateLimiter>,
}

impl AppState {
    fn new(base_dir: PathBuf, public_base_url: String) -> Self {
        let jobs = load_jobs(&base_dir);
        Self {
            jobs: Arc::new(RwLock::new(jobs)),
            base_dir,
            public_base_url,
            max_upload_bytes: load_max_upload_bytes(),
            job_semaphore: Arc::new(Semaphore::new(load_max_concurrent_jobs())),
            admission_semaphore: Arc::new(Semaphore::new(load_max_queued_jobs())),
            max_proxy_bytes: load_max_proxy_bytes(),
            proxy_semaphore: Arc::new(Semaphore::new(load_max_concurrent_proxy_requests())),
            proxy_rate_limiter: Arc::new(ProxyRateLimiter::new(
                load_max_proxy_requests_per_minute(),
            )),
        }
    }
}

fn load_max_upload_bytes() -> usize {
    std::env::var("GTFS_VALIDATOR_WEB_MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_UPLOAD_BYTES)
}

fn load_max_concurrent_jobs() -> usize {
    std::env::var("GTFS_VALIDATOR_WEB_MAX_CONCURRENT_JOBS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_CONCURRENT_JOBS)
}

fn load_max_queued_jobs() -> usize {
    std::env::var("GTFS_VALIDATOR_WEB_MAX_QUEUED_JOBS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_QUEUED_JOBS)
}

fn load_max_proxy_bytes() -> usize {
    load_positive_usize(
        "GTFS_VALIDATOR_WEB_MAX_PROXY_BYTES",
        DEFAULT_MAX_PROXY_BYTES,
    )
}

fn load_max_concurrent_proxy_requests() -> usize {
    load_positive_usize(
        "GTFS_VALIDATOR_WEB_MAX_CONCURRENT_PROXY_REQUESTS",
        DEFAULT_MAX_CONCURRENT_PROXY_REQUESTS,
    )
}

fn load_max_proxy_requests_per_minute() -> usize {
    load_positive_usize(
        "GTFS_VALIDATOR_WEB_MAX_PROXY_REQUESTS_PER_MINUTE",
        DEFAULT_MAX_PROXY_REQUESTS_PER_MINUTE,
    )
}

fn load_positive_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

struct ProxyRateLimiter {
    max_requests: usize,
    requests: Mutex<VecDeque<Instant>>,
}

impl ProxyRateLimiter {
    fn new(max_requests: usize) -> Self {
        Self {
            max_requests,
            requests: Mutex::new(VecDeque::new()),
        }
    }

    fn try_acquire(&self, now: Instant) -> bool {
        let Ok(mut requests) = self.requests.lock() else {
            return false;
        };
        let cutoff = now - Duration::from_secs(60);
        while requests.front().is_some_and(|request| *request <= cutoff) {
            requests.pop_front();
        }
        if requests.len() >= self.max_requests {
            return false;
        }
        requests.push_back(now);
        true
    }
}

#[derive(Debug, Deserialize)]
struct CorsProxyQuery {
    url: String,
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
    serve_static_path("sitemap.xml")
}

async fn static_file(AxumPath(path): AxumPath<String>) -> Response {
    let Some(clean_path) = sanitize_path(&path) else {
        return not_found();
    };
    if clean_path.is_empty() {
        return serve_static_path("index.html");
    }
    if is_sensitive_static_path(&clean_path) {
        return not_found();
    }
    serve_static_path(&clean_path)
}

fn is_sensitive_static_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.eq_ignore_ascii_case("nginx.conf")
        || name.eq_ignore_ascii_case("dockerfile")
        || name.eq_ignore_ascii_case(".env")
        || name.eq_ignore_ascii_case("docker-compose.yml")
        || name.eq_ignore_ascii_case("docker-compose.yaml")
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
    let directory_index = format!("{}/index.html", path.trim_end_matches('/'));
    let (file, served_path) = if let Some(file) = WEBSITE_DIR.get_file(path) {
        (file, path)
    } else if let Some(file) = WEBSITE_DIR.get_file(&directory_index) {
        (file, directory_index.as_str())
    } else {
        return not_found();
    };
    let mime = MimeGuess::from_path(served_path).first_or_octet_stream();
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

async fn cors_proxy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CorsProxyQuery>,
) -> Response {
    if is_cross_site_browser_request(&headers) {
        return plain_text_response(
            StatusCode::FORBIDDEN,
            "cross-site proxy requests are blocked",
        );
    }
    if guard_public_url(&query.url).is_err() {
        return plain_text_response(StatusCode::BAD_REQUEST, "invalid or non-public URL");
    }
    if !state.proxy_rate_limiter.try_acquire(Instant::now()) {
        return plain_text_response(StatusCode::TOO_MANY_REQUESTS, "proxy rate limit exceeded");
    }
    let Ok(permit) = state.proxy_semaphore.clone().try_acquire_owned() else {
        return plain_text_response(StatusCode::TOO_MANY_REQUESTS, "proxy is busy");
    };

    let url = query.url;
    let max_bytes = state.max_proxy_bytes;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        download_url_to_bytes(&url, max_bytes)
    })
    .await;

    match result {
        Ok(Ok(bytes)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| {
                plain_text_response(StatusCode::INTERNAL_SERVER_ERROR, "response error")
            }),
        Ok(Err(err)) if err.to_string().contains("exceeds") => plain_text_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "remote response is too large",
        ),
        Ok(Err(_)) => plain_text_response(StatusCode::BAD_GATEWAY, "remote fetch failed"),
        Err(_) => plain_text_response(StatusCode::INTERNAL_SERVER_ERROR, "proxy worker failed"),
    }
}

fn is_cross_site_browser_request(headers: &HeaderMap) -> bool {
    headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| !value.eq_ignore_ascii_case("same-origin"))
}

fn plain_text_response(status: StatusCode, message: &'static str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(message))
        .unwrap_or_else(|_| Response::new(Body::from(message)))
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
    match try_begin_processing(&state, &job_id) {
        BeginOutcome::NotFound => StatusCode::NOT_FOUND,
        // Already claimed by another handler / a redelivered event. Ack so
        // Pub/Sub stops retrying instead of piling on duplicate work.
        BeginOutcome::AlreadyActive => StatusCode::OK,
        BeginOutcome::Started => {
            spawn_job_processing(state.clone(), job_id, String::new());
            StatusCode::OK
        }
    }
}

async fn error() -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn upload_job(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
    body: axum::body::Bytes,
) -> StatusCode {
    // Claim the job before touching input.zip so a redelivered upload or a
    // concurrent run cannot overwrite the input of an already-running job.
    match try_begin_processing(&state, &job_id) {
        BeginOutcome::NotFound => return StatusCode::NOT_FOUND,
        BeginOutcome::AlreadyActive => return StatusCode::CONFLICT,
        BeginOutcome::Started => {}
    }
    let job_dir = state.base_dir.join(&job_id);
    if tokio::fs::create_dir_all(&job_dir).await.is_err() {
        update_job_status(
            &state,
            &job_id,
            JobStatus::Error,
            Some("failed to create job directory".to_string()),
        );
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    let input_path = job_dir.join("input.zip");
    if tokio::fs::write(&input_path, body).await.is_err() {
        update_job_status(
            &state,
            &job_id,
            JobStatus::Error,
            Some("failed to persist upload".to_string()),
        );
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
    // Unguessable id: it is the only capability protecting a job's report and
    // its (unauthenticated) upload slot from other clients.
    format!("job-{}", uuid::Uuid::new_v4().simple())
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
    // Admission control: cap the number of jobs queued or running at once.
    // Without this, every request spawns a task that then waits on the run
    // semaphore, so a flood of requests piles up unbounded waiting tasks (each
    // holding memory and, for URL jobs, a pending download). Acquire the permit
    // synchronously here, before spawning, so we can shed load instead of
    // queueing without limit.
    let admission = match state.admission_semaphore.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            update_job_status(
                &state,
                &job_id,
                JobStatus::Error,
                Some("server is at capacity; please retry later".to_string()),
            );
            return;
        }
    };
    tokio::spawn(async move {
        // Held for the whole lifetime of the job so the admission count only
        // drops once this job is fully done.
        let _admission = admission;
        // Bound concurrent validations so public traffic cannot exhaust the
        // blocking thread pool / CPU / memory. The permit is held for the whole
        // download + validation and released when the blocking task returns.
        let permit = match state.job_semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return, // semaphore closed => shutting down
        };
        let state_for_block = state.clone();
        let job_id_for_block = job_id.clone();
        let url_for_block = url.clone();
        let result = tokio::task::spawn_blocking(move || {
            process_job(&state_for_block, &job_id_for_block, &url_for_block)
        })
        .await;
        drop(permit);

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

/// Result of trying to move a job into the `Processing` state.
enum BeginOutcome {
    /// The job existed and was atomically claimed for processing.
    Started,
    /// The job is already `Processing` or finished `Success`; caller must not
    /// start a second worker for it.
    AlreadyActive,
    /// No such job.
    NotFound,
}

/// Atomically transition a job into `Processing`. Only jobs waiting for input
/// or in a prior `Error` state may (re)start; this is the single point that
/// prevents two workers writing the same job's output concurrently.
fn try_begin_processing(state: &AppState, job_id: &str) -> BeginOutcome {
    let started = {
        let Ok(mut jobs) = state.jobs.write() else {
            return BeginOutcome::NotFound;
        };
        match jobs.get_mut(job_id) {
            None => return BeginOutcome::NotFound,
            Some(job) => match job.status {
                JobStatus::Processing | JobStatus::Success => false,
                JobStatus::AwaitingUpload | JobStatus::Error => {
                    job.status = JobStatus::Processing;
                    job.error = None;
                    true
                }
            },
        }
    };
    if started {
        persist_job_metadata(state, job_id);
        BeginOutcome::Started
    } else {
        BeginOutcome::AlreadyActive
    }
}

fn process_job(state: &AppState, job_id: &str, url: &str) {
    // Status is already `Processing` (claimed by the caller before spawning).
    let job = match get_job(state, job_id) {
        Some(job) => job,
        None => return,
    };
    let job_dir = state.base_dir.join(job_id);
    let input_path = if !url.is_empty() {
        let path = job_dir.join("input.zip");
        if let Err(err) = download_url_to_path(url, &path, state.max_upload_bytes) {
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

fn download_url_to_path(url: &str, path: &Path, max_bytes: usize) -> anyhow::Result<()> {
    // Scheme/host pre-check for a clear early error. This alone is not
    // sufficient: it resolves the host independently of the connection, so DNS
    // could return a public IP here and a private one at connect time (DNS
    // rebinding). The authoritative guard is the custom DNS resolver below,
    // which filters every connection (initial and each redirect hop) down to
    // public addresses only.
    guard_public_url(url)?;

    let client = build_public_http_client()?;
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("download gtfs from {}", url))?
        .error_for_status()
        .with_context(|| format!("download gtfs from {}", url))?;
    let mut file =
        std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    copy_bounded(response, &mut file, max_bytes)
        .with_context(|| format!("write {}", path.display()))
        .inspect_err(|_| {
            drop(std::fs::remove_file(path));
        })?;
    Ok(())
}

fn download_url_to_bytes(url: &str, max_bytes: usize) -> anyhow::Result<Vec<u8>> {
    guard_public_url(url)?;
    let client = build_public_http_client()?;
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("fetch {}", url))?
        .error_for_status()
        .with_context(|| format!("fetch {}", url))?;
    let initial_capacity = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or(0)
        .min(max_bytes);
    let mut bytes = Vec::with_capacity(initial_capacity);
    copy_bounded(response, &mut bytes, max_bytes)?;
    Ok(bytes)
}

fn build_public_http_client() -> anyhow::Result<Client> {
    Client::builder()
        .user_agent(format!(
            "gtfs-validator-rust-web/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .dns_resolver(Arc::new(PublicOnlyResolver))
        .connect_timeout(Duration::from_secs(10))
        // Overall cap; large public feeds can take a while to stream.
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error(std::io::Error::other("too many redirects"));
            }
            // Scheme re-check on redirects; the resolver still enforces the
            // address filter for the actual connection.
            match guard_public_url(attempt.url().as_str()) {
                Ok(()) => attempt.follow(),
                Err(_) => attempt.error(std::io::Error::other(
                    "redirect to non-public address blocked",
                )),
            }
        }))
        .build()
        .context("build http client")
}

fn copy_bounded(
    reader: impl std::io::Read,
    writer: &mut impl std::io::Write,
    max_bytes: usize,
) -> anyhow::Result<()> {
    // Read at most max_bytes (+1 to detect overflow) so a huge or endless
    // response cannot fill memory or disk.
    let limit = max_bytes as u64;
    let mut limited = std::io::Read::take(reader, limit + 1);
    let copied = std::io::copy(&mut limited, writer)?;
    if copied > limit {
        bail!("remote response exceeds {}-byte limit", max_bytes);
    }
    Ok(())
}

/// Reject URLs that would let a client make the server fetch internal
/// resources (SSRF): non-HTTP(S) schemes and any host that resolves to a
/// loopback/private/link-local/reserved address (e.g. cloud metadata).
fn guard_public_url(raw: &str) -> anyhow::Result<()> {
    let parsed = url::Url::parse(raw).with_context(|| format!("parse url {}", raw))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => bail!("unsupported url scheme: {}", other),
    }
    let port = parsed.port_or_known_default().unwrap_or(80);

    // IP literals bypass reqwest's DNS resolver. Inspect them directly; this
    // also avoids trying to resolve the bracketed form returned by host_str()
    // for an IPv6 URL.
    match parsed.host().context("url has no host")? {
        url::Host::Ipv4(addr) => {
            let ip = IpAddr::V4(addr);
            if !is_global_ip(ip) {
                bail!("refusing to fetch from non-public address {}", ip);
            }
        }
        url::Host::Ipv6(addr) => {
            let ip = IpAddr::V6(addr);
            if !is_global_ip(ip) {
                bail!("refusing to fetch from non-public address {}", ip);
            }
        }
        url::Host::Domain(host) => {
            let mut resolved_any = false;
            for addr in (host, port)
                .to_socket_addrs()
                .with_context(|| format!("resolve host {}", host))?
            {
                resolved_any = true;
                if !is_global_ip(addr.ip()) {
                    bail!("refusing to fetch from non-public address {}", addr.ip());
                }
            }
            if !resolved_any {
                bail!("host {} did not resolve to any address", host);
            }
        }
    }
    Ok(())
}

/// True only for addresses that are safe to fetch from a public server:
/// excludes loopback, private (RFC1918), CGNAT, link-local, documentation and
/// otherwise reserved ranges for both IPv4 and IPv6.
fn is_global_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || o[0] == 0
                // 100.64.0.0/10 carrier-grade NAT
                || (o[0] == 100 && (o[1] & 0xC0) == 64)
                // 192.0.0.0/24 IETF protocol assignments
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                // 198.18.0.0/15 benchmarking
                || (o[0] == 198 && (o[1] & 0xFE) == 18)
                // 240.0.0.0/4 reserved (incl. 255.255.255.255)
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return false;
            }
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_global_ip(IpAddr::V4(mapped));
            }
            let seg0 = v6.segments()[0];
            // fc00::/7 unique-local, fe80::/10 link-local
            if (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80 {
                return false;
            }
            true
        }
    }
}

/// Resolve `host` and keep only addresses that are safe to connect to from a
/// public server. Errors if the host does not resolve to any public address.
fn resolve_public_addrs(host: &str) -> std::io::Result<Vec<SocketAddr>> {
    // Port 0 is a placeholder; reqwest substitutes the real port. We only need
    // the IP addresses to make the public/private decision.
    let addrs: Vec<SocketAddr> = (host, 0u16)
        .to_socket_addrs()?
        .filter(|addr| is_global_ip(addr.ip()))
        .collect();
    if addrs.is_empty() {
        return Err(std::io::Error::other(format!(
            "host {host} did not resolve to a public address"
        )));
    }
    Ok(addrs)
}

/// A reqwest DNS resolver that never yields a private/loopback/reserved address.
/// Because reqwest resolves through this for every connection — the initial
/// request and each redirect hop — a host that resolves to an internal address
/// at connect time simply has no usable address, closing the DNS-rebinding /
/// TOCTOU gap that a one-shot pre-flight check leaves open.
struct PublicOnlyResolver;

impl Resolve for PublicOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            match resolve_public_addrs(&host) {
                Ok(addrs) => {
                    let addrs: Addrs = Box::new(addrs.into_iter());
                    Ok(addrs)
                }
                Err(err) => Err(Box::new(err) as Box<dyn std::error::Error + Send + Sync>),
            }
        })
    }
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
    let mut known_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Ok(jobs) = state.jobs.read() {
        for (job_id, job) in jobs.iter() {
            known_ids.insert(job_id.clone());
            if matches!(job.status, JobStatus::Processing) {
                continue;
            }
            let job_dir = state.base_dir.join(job_id);
            // Prefer metadata timestamp; fall back to the directory mtime, then
            // to 0 (== expired) so a job whose metadata is unreadable is still
            // reclaimed instead of living forever.
            let updated_at = read_metadata_timestamp(&job_dir.join("job.json"))
                .or_else(|| dir_mtime_millis(&job_dir))
                .unwrap_or(0);
            if now.saturating_sub(updated_at) >= ttl_ms {
                expired.push((job_id.clone(), job.output_dir.clone()));
            }
        }
    }

    if !expired.is_empty() {
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

    // Reclaim orphan directories that never made it into the in-memory map
    // (e.g. a job.json that failed to parse on load): the loop above can never
    // see them, so they would otherwise leak disk indefinitely.
    if let Ok(entries) = std::fs::read_dir(&state.base_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };
            if known_ids.contains(&name) {
                continue;
            }
            let updated_at = dir_mtime_millis(&path).unwrap_or(0);
            if now.saturating_sub(updated_at) >= ttl_ms {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

fn dir_mtime_millis(path: &Path) -> Option<u128> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    #[test]
    fn is_global_ip_rejects_internal_ipv4() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "0.0.0.0",
            "255.255.255.255",
        ] {
            assert!(
                !is_global_ip(ip.parse().unwrap()),
                "{ip} must be non-global"
            );
        }
    }

    #[test]
    fn is_global_ip_rejects_internal_ipv6() {
        assert!(!is_global_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!is_global_ip("fd00::1".parse().unwrap())); // unique-local
        assert!(!is_global_ip("fe80::1".parse().unwrap())); // link-local
        assert!(!is_global_ip("::ffff:127.0.0.1".parse().unwrap())); // mapped loopback
    }

    #[test]
    fn is_global_ip_accepts_public() {
        for ip in ["8.8.8.8", "1.1.1.1", "93.184.216.34"] {
            assert!(is_global_ip(ip.parse().unwrap()), "{ip} must be global");
        }
        assert!(is_global_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn guard_public_url_rejects_bad_scheme() {
        assert!(guard_public_url("javascript:alert(1)").is_err());
        assert!(guard_public_url("file:///etc/passwd").is_err());
        assert!(guard_public_url("ftp://example.com/x").is_err());
    }

    #[test]
    fn guard_public_url_rejects_internal_hosts() {
        assert!(guard_public_url("http://127.0.0.1/feed.zip").is_err());
        assert!(guard_public_url("http://localhost/feed.zip").is_err());
        assert!(guard_public_url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(guard_public_url("http://[::1]/feed.zip").is_err());
    }

    #[test]
    fn guard_public_url_accepts_public_ipv6_literal() {
        assert!(guard_public_url("https://[2606:4700:4700::1111]/feed.zip").is_ok());
    }

    #[test]
    fn extract_job_id_finds_uuid_segment() {
        let id = format!("job-{}", uuid::Uuid::new_v4().simple());
        assert_eq!(
            extract_job_id(&format!("{id}/input.zip")).as_deref(),
            Some(id.as_str())
        );
    }

    #[test]
    fn resolve_public_addrs_rejects_loopback_host() {
        // `localhost` resolves to loopback (127.0.0.1 / ::1) via the hosts file,
        // so the resolver must refuse it — this is the connect-time guard that
        // defeats DNS rebinding.
        assert!(resolve_public_addrs("localhost").is_err());
    }

    #[test]
    fn resolve_public_addrs_accepts_public_literal() {
        // A public IP literal parses without touching DNS, keeping this test
        // hermetic while still exercising the public/private filter.
        let addrs = resolve_public_addrs("8.8.8.8").expect("public literal must resolve");
        assert!(!addrs.is_empty());
        assert!(addrs.iter().all(|addr| is_global_ip(addr.ip())));
    }

    #[test]
    fn resolve_public_addrs_rejects_private_literal() {
        assert!(resolve_public_addrs("169.254.169.254").is_err());
        assert!(resolve_public_addrs("127.0.0.1").is_err());
    }

    #[test]
    fn copy_bounded_rejects_oversized_response() {
        let mut output = Vec::new();
        let error = copy_bounded(&b"12345"[..], &mut output, 4).unwrap_err();
        assert!(error.to_string().contains("exceeds"));
        assert_eq!(output, b"12345");
    }

    #[test]
    fn proxy_rate_limiter_enforces_sliding_window() {
        let limiter = ProxyRateLimiter::new(2);
        let start = Instant::now();
        assert!(limiter.try_acquire(start));
        assert!(limiter.try_acquire(start));
        assert!(!limiter.try_acquire(start));
        assert!(limiter.try_acquire(start + Duration::from_secs(61)));
    }

    #[test]
    fn sensitive_deployment_files_are_not_static_assets() {
        for path in [
            "nginx.conf",
            "Dockerfile",
            ".env",
            "docker-compose.yml",
            "docker-compose.yaml",
            "nested/NGINX.CONF",
        ] {
            assert!(is_sensitive_static_path(path), "{path} must be blocked");
        }
        assert!(!is_sensitive_static_path("index.html"));
    }

    #[test]
    fn browser_proxy_rejects_cross_site_requests() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "sec-fetch-site",
            header::HeaderValue::from_static("cross-site"),
        );
        assert!(is_cross_site_browser_request(&headers));
        headers.insert(
            "sec-fetch-site",
            header::HeaderValue::from_static("same-origin"),
        );
        assert!(!is_cross_site_browser_request(&headers));
        headers.remove("sec-fetch-site");
        assert!(!is_cross_site_browser_request(&headers));
    }

    #[test]
    fn static_directories_serve_their_index_page() {
        let response = serve_static_path("notices/missing_required_field/");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static("text/html"))
        );
    }
}
