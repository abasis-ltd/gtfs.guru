#![forbid(unsafe_code)]
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use axum::{
    body::Body,
    body::Bytes,
    extract::{ConnectInfo, Path as AxumPath, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use futures_util::StreamExt;
use include_dir::{include_dir, Dir};
use mime_guess::MimeGuess;
use reqwest::blocking::Client;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use gtfs_guru_core::{default_runner, validate_input, GtfsInput, NoticeContainer};
use gtfs_guru_report::{
    write_html_report, HtmlReportContext, ReportSummary, ReportSummaryContext, ValidationReport,
};

// The repo-root website/ is the single source of truth: it is what the static
// deployment serves, and it is embedded here so the axum binary serves the same
// bytes. This crate is `publish = false` precisely because `cargo package` would
// not carry a directory from outside the crate root.
static WEBSITE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../website");

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

/// Concurrent upload streams that may write to disk at once. Overridable via
/// `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_UPLOADS`. Distinct from validation
/// concurrency: this bounds how many request bodies we will accept, not how
/// many feeds we will parse.
const DEFAULT_MAX_CONCURRENT_UPLOADS: usize = 4;

/// How long a validation may run before cleanup marks it failed. Measured from
/// when validation actually starts, not from when the job was claimed, so time
/// spent queueing for a run permit or uploading does not count. Overridable via
/// `GTFS_VALIDATOR_WEB_PROCESSING_TIMEOUT_SECONDS`.
const DEFAULT_PROCESSING_TIMEOUT_SECS: u128 = 30 * 60;

/// How long a job may wait for its upload before cleanup drops it. Overridable
/// via `GTFS_VALIDATOR_WEB_PENDING_UPLOAD_TTL_SECONDS`. Pending jobs count
/// against `GTFS_VALIDATOR_WEB_MAX_QUEUED_JOBS`; with only the 24 h job TTL a
/// burst of bodiless `POST /create-job` calls would lock uploads for a day.
const DEFAULT_PENDING_UPLOAD_TTL_SECS: u64 = 15 * 60;

/// How often the cleanup sweep runs. The sweep only holds the jobs lock to
/// decide; the filesystem work runs on the blocking pool.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// How long an upload may stall between body chunks before it is abandoned.
/// Overridable via `GTFS_VALIDATOR_WEB_UPLOAD_IDLE_TIMEOUT_SECONDS`. A client
/// that opens `PUT /upload/:id` and never finishes the body would otherwise
/// hold its upload and admission permits forever: cleanup can drop the job from
/// the map, but it cannot release a permit owned by a live handler.
const DEFAULT_UPLOAD_IDLE_TIMEOUT_SECS: u64 = 60;

/// Ceiling on the whole upload, so a client that trickles one byte per idle
/// window cannot hold a permit indefinitely either. Overridable via
/// `GTFS_VALIDATOR_WEB_UPLOAD_TIMEOUT_SECONDS`.
const DEFAULT_UPLOAD_TIMEOUT_SECS: u64 = 30 * 60;

/// Global create-job requests accepted per minute.
const DEFAULT_MAX_CREATE_JOB_REQUESTS_PER_MINUTE: usize = 60;
/// Create-job requests accepted per minute from one client address.
const DEFAULT_MAX_CREATE_JOB_REQUESTS_PER_MINUTE_PER_CLIENT: usize = 10;

/// Keep the browser proxy aligned with the WASM validator's input limit.
const DEFAULT_MAX_PROXY_BYTES: usize = 70 * 1024 * 1024;
const DEFAULT_MAX_CONCURRENT_PROXY_REQUESTS: usize = 4;
const DEFAULT_MAX_PROXY_REQUESTS_PER_MINUTE: usize = 60;
const DEFAULT_MAX_PROXY_REQUESTS_PER_MINUTE_PER_CLIENT: usize = 10;
const DEFAULT_MAX_CONCURRENT_PROXY_REQUESTS_PER_CLIENT: usize = 2;
/// Wall-clock ceiling on one proxy fetch. Overridable via
/// `GTFS_VALIDATOR_WEB_PROXY_TIMEOUT_SECONDS`. reqwest's blocking timeout
/// applies per read, so without an overall deadline four slow-drip upstreams
/// could hold every proxy permit for as long as they keep dripping.
const DEFAULT_PROXY_TIMEOUT_SECS: u64 = 120;
/// Per-operation timeout (connect plus response headers, then each body read)
/// for proxy fetches. The overall deadline is checked between reads, so a
/// fetch ends at most this long after `DEFAULT_PROXY_TIMEOUT_SECS`.
const PROXY_IO_TIMEOUT: Duration = Duration::from_secs(30);
/// The same pair for server-side job downloads (`POST /create-job` with a
/// `url`), which hold an admission and a run permit while they stream.
const JOB_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const JOB_DOWNLOAD_IO_TIMEOUT: Duration = Duration::from_secs(60);

/// Peers whose `X-Forwarded-For` is believed, as comma-separated addresses or
/// CIDR ranges. Overridable via `GTFS_VALIDATOR_WEB_TRUSTED_PROXIES`; an empty
/// value trusts no one.
///
/// Loopback covers a proxy on the same host as a bare process. Docker's
/// default bridge and Compose pools (172.16.0.0/12, 192.168.0.0/16) are
/// included because production runs in a container published on
/// `127.0.0.1:3000`: the host's Caddy then reaches it through docker-proxy and
/// the service sees the bridge gateway, not loopback. Trusting only loopback
/// there would key every visitor to the gateway address and turn the
/// per-client limits into a much smaller global one. Internet peers are never
/// trusted; a deployment that exposes the port directly to a LAN in those
/// ranges should narrow this.
const DEFAULT_TRUSTED_PROXIES: &str = "127.0.0.0/8,::1,172.16.0.0/12,192.168.0.0/16";

/// Sliding window shared by every rate limiter here.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
/// Per-client limiter size at which idle entries are swept out.
const CLIENT_LIMITER_PRUNE_THRESHOLD: usize = 4096;

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
        .route("/cors-proxy", get(cors_proxy))
        .route("/create-job", post(create_job))
        .route("/run-validator", post(run_validator))
        .route("/error", post(error))
        // No `DefaultBodyLimit` here: it is honoured only by extractors that
        // call `into_limited_body` (`Bytes`, `Json`, `Form`), and `upload_job`
        // takes the raw `Request` so it can stream to disk. The cap is enforced
        // by the Content-Length pre-check and by `stream_body_to_file`.
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
    // Connect info carries the TCP peer, which decides whether a request's
    // `X-Forwarded-For` is believed when keying the per-client rate limits.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

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
    /// The commit the binary was built from, when the build stamped one
    /// (`GTFS_GURU_BUILD_COMMIT`, set by the Dockerfile). The web deploy reads
    /// it back from the live site to confirm the swap actually happened.
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
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
    /// Kept in memory so cleanup decides from the map alone instead of
    /// re-reading every `job.json` under the jobs lock.
    created_at_millis: u128,
    updated_at_millis: u128,
    /// True while a `JobLease` owns the job: an upload handler or a worker may
    /// still write its directory. Cleanup never removes such a job and nobody
    /// else may claim it. Runtime only; a restart clears it (see `load_jobs`).
    active: bool,
    /// When validation actually began, after any queueing for a run permit.
    /// The processing timeout is measured from here.
    processing_started: Option<Instant>,
}

impl Job {
    fn new(
        id: String,
        status: JobStatus,
        country_code: Option<String>,
        input_path: Option<PathBuf>,
        output_dir: Option<PathBuf>,
    ) -> Self {
        let now = current_millis();
        Self {
            id,
            status,
            country_code,
            input_path,
            output_dir,
            error: None,
            created_at_millis: now,
            updated_at_millis: now,
            active: false,
            processing_started: None,
        }
    }

    fn touch(&mut self) {
        self.updated_at_millis = current_millis();
    }
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
    max_queued_jobs: usize,
    job_semaphore: Arc<Semaphore>,
    admission_semaphore: Arc<Semaphore>,
    upload_semaphore: Arc<Semaphore>,
    max_proxy_bytes: usize,
    proxy_semaphore: Arc<Semaphore>,
    proxy_client_slots: Arc<ClientSlots>,
    proxy_rate_limiter: Arc<RateLimits>,
    proxy_timeout: Duration,
    job_create_rate_limiter: Arc<RateLimits>,
    trusted_proxies: Arc<Vec<IpNetwork>>,
    pubsub_token: Option<String>,
    processing_timeout_ms: u128,
    pending_upload_ttl_ms: u128,
    upload_idle_timeout: Duration,
    upload_timeout: Duration,
}

impl AppState {
    fn new(base_dir: PathBuf, public_base_url: String) -> Self {
        let pending_upload_ttl_ms = load_pending_upload_ttl_ms();
        let jobs = load_jobs(&base_dir, pending_upload_ttl_ms, current_millis());
        let max_queued_jobs = load_max_queued_jobs();
        Self {
            jobs: Arc::new(RwLock::new(jobs)),
            base_dir,
            public_base_url,
            max_upload_bytes: load_max_upload_bytes(),
            max_queued_jobs,
            job_semaphore: Arc::new(Semaphore::new(load_max_concurrent_jobs())),
            admission_semaphore: Arc::new(Semaphore::new(max_queued_jobs)),
            upload_semaphore: Arc::new(Semaphore::new(load_max_concurrent_uploads())),
            max_proxy_bytes: load_max_proxy_bytes(),
            proxy_semaphore: Arc::new(Semaphore::new(load_max_concurrent_proxy_requests())),
            proxy_client_slots: Arc::new(ClientSlots::new(load_positive_usize(
                "GTFS_VALIDATOR_WEB_MAX_CONCURRENT_PROXY_REQUESTS_PER_CLIENT",
                DEFAULT_MAX_CONCURRENT_PROXY_REQUESTS_PER_CLIENT,
            ))),
            proxy_rate_limiter: Arc::new(RateLimits::new(
                load_max_proxy_requests_per_minute(),
                load_positive_usize(
                    "GTFS_VALIDATOR_WEB_MAX_PROXY_REQUESTS_PER_MINUTE_PER_CLIENT",
                    DEFAULT_MAX_PROXY_REQUESTS_PER_MINUTE_PER_CLIENT,
                ),
            )),
            proxy_timeout: load_timeout_secs(
                "GTFS_VALIDATOR_WEB_PROXY_TIMEOUT_SECONDS",
                DEFAULT_PROXY_TIMEOUT_SECS,
            ),
            job_create_rate_limiter: Arc::new(RateLimits::new(
                load_max_create_job_requests_per_minute(),
                load_positive_usize(
                    "GTFS_VALIDATOR_WEB_MAX_CREATE_JOB_REQUESTS_PER_MINUTE_PER_CLIENT",
                    DEFAULT_MAX_CREATE_JOB_REQUESTS_PER_MINUTE_PER_CLIENT,
                ),
            )),
            trusted_proxies: Arc::new(load_trusted_proxies()),
            pubsub_token: load_pubsub_token(),
            processing_timeout_ms: load_processing_timeout_ms(),
            pending_upload_ttl_ms,
            upload_idle_timeout: load_upload_idle_timeout(),
            upload_timeout: load_upload_timeout(),
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

fn load_max_concurrent_uploads() -> usize {
    load_positive_usize(
        "GTFS_VALIDATOR_WEB_MAX_CONCURRENT_UPLOADS",
        DEFAULT_MAX_CONCURRENT_UPLOADS,
    )
}

fn load_max_create_job_requests_per_minute() -> usize {
    load_positive_usize(
        "GTFS_VALIDATOR_WEB_MAX_CREATE_JOB_REQUESTS_PER_MINUTE",
        DEFAULT_MAX_CREATE_JOB_REQUESTS_PER_MINUTE,
    )
}

fn load_pubsub_token() -> Option<String> {
    std::env::var("GTFS_VALIDATOR_WEB_PUBSUB_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn load_processing_timeout_ms() -> u128 {
    let default_ms = DEFAULT_PROCESSING_TIMEOUT_SECS.saturating_mul(1000);
    match std::env::var("GTFS_VALIDATOR_WEB_PROCESSING_TIMEOUT_SECONDS") {
        Ok(value) => value
            .trim()
            .parse::<u128>()
            .ok()
            .filter(|seconds| *seconds > 0)
            .map(|seconds| seconds.saturating_mul(1000))
            .unwrap_or(default_ms),
        Err(_) => default_ms,
    }
}

fn load_pending_upload_ttl_ms() -> u128 {
    load_timeout_secs(
        "GTFS_VALIDATOR_WEB_PENDING_UPLOAD_TTL_SECONDS",
        DEFAULT_PENDING_UPLOAD_TTL_SECS,
    )
    .as_millis()
}

fn load_trusted_proxies() -> Vec<IpNetwork> {
    let raw = std::env::var("GTFS_VALIDATOR_WEB_TRUSTED_PROXIES")
        .unwrap_or_else(|_| DEFAULT_TRUSTED_PROXIES.to_string());
    parse_trusted_proxies(&raw)
}

fn parse_trusted_proxies(raw: &str) -> Vec<IpNetwork> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let network = IpNetwork::parse(entry);
            if network.is_none() {
                tracing::warn!("ignoring invalid trusted proxy entry {:?}", entry);
            }
            network
        })
        .collect()
}

fn load_upload_idle_timeout() -> Duration {
    load_timeout_secs(
        "GTFS_VALIDATOR_WEB_UPLOAD_IDLE_TIMEOUT_SECONDS",
        DEFAULT_UPLOAD_IDLE_TIMEOUT_SECS,
    )
}

fn load_upload_timeout() -> Duration {
    load_timeout_secs(
        "GTFS_VALIDATOR_WEB_UPLOAD_TIMEOUT_SECONDS",
        DEFAULT_UPLOAD_TIMEOUT_SECS,
    )
}

/// Read a positive timeout in seconds. Unset, unparsable or zero all fall back
/// to the default: an upload without any deadline is what leaks the permit, so
/// there is deliberately no way to turn these off.
fn load_timeout_secs(name: &str, default_secs: u64) -> Duration {
    let seconds = std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_secs);
    Duration::from_secs(seconds)
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
        admit_in_window(&mut requests, now, self.max_requests)
    }
}

/// Drop timestamps that left the window, then record `now` if there is room.
fn admit_in_window(requests: &mut VecDeque<Instant>, now: Instant, max_requests: usize) -> bool {
    // `checked_sub`: an `Instant` less than a window after the clock's origin
    // has nothing older to expire, and plain subtraction would panic.
    if let Some(cutoff) = now.checked_sub(RATE_LIMIT_WINDOW) {
        while requests.front().is_some_and(|request| *request <= cutoff) {
            requests.pop_front();
        }
    }
    if requests.len() >= max_requests {
        return false;
    }
    requests.push_back(now);
    true
}

/// A sliding window per client address, so one client cannot spend a budget
/// meant for everyone.
struct ClientRateLimiter {
    max_requests: usize,
    clients: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl ClientRateLimiter {
    fn new(max_requests: usize) -> Self {
        Self {
            max_requests,
            clients: Mutex::new(HashMap::new()),
        }
    }

    fn try_acquire(&self, client: IpAddr, now: Instant) -> bool {
        let Ok(mut clients) = self.clients.lock() else {
            return false;
        };
        if clients.len() >= CLIENT_LIMITER_PRUNE_THRESHOLD {
            // Bound memory by forgetting clients with nothing left in the
            // window; what remains is at most one window's worth of traffic.
            let cutoff = now.checked_sub(RATE_LIMIT_WINDOW);
            clients.retain(|_, requests| {
                requests
                    .back()
                    .is_some_and(|last| cutoff.is_none_or(|cutoff| *last > cutoff))
            });
        }
        admit_in_window(clients.entry(client).or_default(), now, self.max_requests)
    }
}

/// A per-client window in front of a global one. The per-client check runs
/// first and only what it admits is counted globally: were it the other way
/// round, a client hammering the endpoint would use up the global budget with
/// requests that are refused anyway, and lock everyone else out.
struct RateLimits {
    global: ProxyRateLimiter,
    per_client: ClientRateLimiter,
}

impl RateLimits {
    fn new(global: usize, per_client: usize) -> Self {
        Self {
            global: ProxyRateLimiter::new(global),
            per_client: ClientRateLimiter::new(per_client),
        }
    }

    fn try_acquire(&self, client: IpAddr, now: Instant) -> bool {
        self.per_client.try_acquire(client, now) && self.global.try_acquire(now)
    }
}

/// Caps how many requests one client may have in flight at once, so a single
/// client cannot hold every global permit with slow requests.
struct ClientSlots {
    max_per_client: usize,
    in_flight: Mutex<HashMap<IpAddr, usize>>,
}

/// One in-flight request; frees its slot when dropped.
struct ClientSlot {
    slots: Arc<ClientSlots>,
    client: IpAddr,
}

impl ClientSlots {
    fn new(max_per_client: usize) -> Self {
        Self {
            max_per_client,
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    fn try_acquire(self: &Arc<Self>, client: IpAddr) -> Option<ClientSlot> {
        let mut in_flight = self.in_flight.lock().ok()?;
        let count = in_flight.entry(client).or_insert(0);
        if *count >= self.max_per_client {
            return None;
        }
        *count += 1;
        Some(ClientSlot {
            slots: Arc::clone(self),
            client,
        })
    }
}

impl Drop for ClientSlot {
    fn drop(&mut self) {
        if let Ok(mut in_flight) = self.slots.in_flight.lock() {
            if let Some(count) = in_flight.get_mut(&self.client) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    in_flight.remove(&self.client);
                }
            }
        }
    }
}

/// An address or CIDR range, for `GTFS_VALIDATOR_WEB_TRUSTED_PROXIES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IpNetwork {
    network: IpAddr,
    prefix: u8,
}

impl IpNetwork {
    fn parse(raw: &str) -> Option<Self> {
        let (addr, prefix) = match raw.split_once('/') {
            Some((addr, prefix)) => (addr.trim(), Some(prefix.trim().parse::<u8>().ok()?)),
            None => (raw.trim(), None),
        };
        let addr = addr.parse::<IpAddr>().ok()?.to_canonical();
        let max_prefix = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(max_prefix);
        if prefix > max_prefix {
            return None;
        }
        Some(Self {
            network: mask_ip(addr, prefix),
            prefix,
        })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        let ip = ip.to_canonical();
        ip.is_ipv4() == self.network.is_ipv4() && mask_ip(ip, self.prefix) == self.network
    }
}

fn mask_ip(ip: IpAddr, prefix: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => {
            let mask = u32::MAX
                .checked_shl(32 - u32::from(prefix.min(32)))
                .unwrap_or(0);
            IpAddr::V4((u32::from(v4) & mask).into())
        }
        IpAddr::V6(v6) => {
            let mask = u128::MAX
                .checked_shl(128 - u32::from(prefix.min(128)))
                .unwrap_or(0);
            IpAddr::V6((u128::from(v6) & mask).into())
        }
    }
}

/// The client address a request is attributed to.
///
/// `X-Forwarded-For` is read only when the TCP peer is a trusted proxy: from
/// anyone else it is attacker-controlled and would let a client pick a fresh
/// identity per request. The list is walked from the right, skipping trusted
/// hops, so a client cannot prepend a spoofed entry in front of the address
/// our own proxy appended.
fn client_ip(peer: IpAddr, headers: &HeaderMap, trusted: &[IpNetwork]) -> IpAddr {
    let is_trusted = |ip: IpAddr| trusted.iter().any(|network| network.contains(ip));
    let mut client = peer.to_canonical();
    if !is_trusted(client) {
        return client;
    }
    let hops: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .collect();
    for hop in hops.iter().rev() {
        let Some(ip) = parse_forwarded_ip(hop) else {
            // Garbage in the chain: keep the last address we can vouch for.
            break;
        };
        client = ip;
        if !is_trusted(client) {
            break;
        }
    }
    client
}

fn parse_forwarded_ip(hop: &str) -> Option<IpAddr> {
    let hop = hop.trim();
    hop.parse::<IpAddr>()
        .or_else(|_| hop.parse::<SocketAddr>().map(|addr| addr.ip()))
        .ok()
        .map(|ip| ip.to_canonical())
}

/// Key for the per-client limits. An IPv6 client usually controls a whole /64,
/// so keying on the full address would let it rotate through fresh identities.
fn rate_limit_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V4(v4) => IpAddr::V4(v4),
        v6 @ IpAddr::V6(_) => mask_ip(v6, 64),
    }
}

impl AppState {
    fn client_key(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        rate_limit_key(client_ip(peer.ip(), headers, &self.trusted_proxies))
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
    // `include_dir!` embeds whatever sits in website/ at compile time, including
    // files Git never sees: a developer's `.DS_Store`, an editor swap file, a
    // stray `.env`. The site tracks no dotfiles, so refusing every dotted
    // segment costs nothing and does not depend on remembering to extend the
    // list below.
    if path.split('/').any(|segment| segment.starts_with('.')) {
        return true;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    name.eq_ignore_ascii_case("nginx.conf")
        || name.eq_ignore_ascii_case("dockerfile")
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
        commit: std::env::var("GTFS_GURU_BUILD_COMMIT")
            .ok()
            .filter(|commit| !commit.is_empty()),
    })
}

async fn cors_proxy(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<CorsProxyQuery>,
) -> Response {
    if is_cross_site_browser_request(&headers) {
        return plain_text_response(
            StatusCode::FORBIDDEN,
            "cross-site proxy requests are blocked",
        );
    }
    let client = state.client_key(peer, &headers);
    if !state.proxy_rate_limiter.try_acquire(client, Instant::now()) {
        return plain_text_response(StatusCode::TOO_MANY_REQUESTS, "proxy rate limit exceeded");
    }
    let Some(client_slot) = state.proxy_client_slots.try_acquire(client) else {
        return plain_text_response(
            StatusCode::TOO_MANY_REQUESTS,
            "too many proxy requests in flight from this client",
        );
    };
    let Ok(permit) = state.proxy_semaphore.clone().try_acquire_owned() else {
        return plain_text_response(StatusCode::TOO_MANY_REQUESTS, "proxy is busy");
    };

    let url = query.url;
    let max_bytes = state.max_proxy_bytes;
    let timeout = state.proxy_timeout;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _client_slot = client_slot;
        // The guard resolves the host with a blocking lookup, so it runs here
        // rather than on an async worker, and only once the rate limiter has
        // admitted the request: a nameserver that never answers would
        // otherwise stall the runtime without counting against any limit.
        guard_public_url(&url).map_err(|_| None)?;
        download_url_to_bytes(&url, max_bytes, timeout).map_err(Some)
    })
    .await;

    match result {
        Ok(Err(None)) => plain_text_response(StatusCode::BAD_REQUEST, "invalid or non-public URL"),
        Ok(Ok(bytes)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| {
                plain_text_response(StatusCode::INTERNAL_SERVER_ERROR, "response error")
            }),
        Ok(Err(Some(err))) if err.to_string().contains("exceeds") => plain_text_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "remote response is too large",
        ),
        Ok(Err(Some(err))) if is_deadline_error(&err) => {
            plain_text_response(StatusCode::GATEWAY_TIMEOUT, "remote fetch took too long")
        }
        Ok(Err(Some(_))) => plain_text_response(StatusCode::BAD_GATEWAY, "remote fetch failed"),
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
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // A page on another site can POST here without a CORS preflight (a
    // `no-cors` fetch or a plain form), which would let it spend its visitors'
    // browsers on our pending-upload slots.
    if is_cross_site_browser_request(&headers) {
        return plain_text_response(StatusCode::FORBIDDEN, "cross-site job creation is blocked");
    }
    let body = match parse_create_job_body(&headers, &body) {
        Ok(body) => body,
        Err((status, message)) => return plain_text_response(status, message),
    };
    let client = state.client_key(peer, &headers);
    if !state
        .job_create_rate_limiter
        .try_acquire(client, Instant::now())
    {
        return plain_text_response(
            StatusCode::TOO_MANY_REQUESTS,
            "job creation rate limit exceeded",
        );
    }
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

    let mut job = Job::new(
        job_id.clone(),
        status,
        country_code,
        input_path,
        Some(job_dir.join("output")),
    );
    // A URL job is born owned by its worker, so it is never unowned while
    // `Processing`.
    job.active = source_url.is_some();
    // Counting and inserting under one write lock: two concurrent creates
    // cannot both observe the last free slot and both take it.
    if !insert_job_within_pending_cap(&state, job) {
        let _ = tokio::fs::remove_dir_all(&job_dir).await;
        return plain_text_response(StatusCode::TOO_MANY_REQUESTS, "too many pending uploads");
    }

    if let Some(url) = source_url {
        let lease = JobLease::adopt(state.clone(), job_id.clone(), LeaseStage::Processing);
        spawn_job_processing(state.clone(), job_id.clone(), url, lease);
        Json(CreateJobResponse { job_id, url: None }).into_response()
    } else {
        Json(CreateJobResponse {
            job_id: job_id.clone(),
            url: Some(format!("{}/upload/{}", state.public_base_url, job_id)),
        })
        .into_response()
    }
}

/// An empty body means "no options". Anything else must be JSON: a body sent
/// as `text/plain` or a form is what a cross-site page can send without a
/// preflight, and it used to be silently accepted as an option-less job.
fn parse_create_job_body(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<Option<CreateJobRequest>, (StatusCode, &'static str)> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    if !has_json_content_type(headers) {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "create-job expects an application/json body",
        ));
    }
    serde_json::from_slice(body)
        .map(Some)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid create-job body"))
}

fn has_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(|essence| essence.trim().to_ascii_lowercase())
        .is_some_and(|essence| essence == "application/json" || essence.ends_with("+json"))
}

async fn run_validator(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PubsubEnvelope>,
) -> StatusCode {
    if !pubsub_token_matches(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let data = payload
        .message
        .and_then(|msg| msg.data)
        .and_then(decode_pubsub_data);
    let Some(job_id) = data.and_then(|name| extract_job_id(&name)) else {
        return StatusCode::BAD_REQUEST;
    };
    match try_begin_processing(&state, &job_id, LeaseStage::Processing) {
        BeginOutcome::NotFound => StatusCode::NOT_FOUND,
        // Already claimed by another handler / a redelivered event. Ack so
        // Pub/Sub stops retrying instead of piling on duplicate work.
        BeginOutcome::AlreadyActive => StatusCode::OK,
        BeginOutcome::Started(lease) => {
            spawn_job_processing(state.clone(), job_id, String::new(), *lease);
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
    request: Request,
) -> StatusCode {
    // A declared length over the cap is refused before the job is claimed, so a
    // client that announces an oversized body neither burns an id nor uploads.
    if declared_length_over_cap(request.headers(), state.max_upload_bytes) {
        return StatusCode::PAYLOAD_TOO_LARGE;
    }
    // Claim the job before touching the body so a missing or already-running
    // id does not force the process to buffer hundreds of megabytes first.
    //
    // The lease is also the drop guard: if the client disconnects, hyper drops
    // this future mid-stream and none of the error arms below run. Dropping the
    // lease then puts the job back to `AwaitingUpload` and removes the partial
    // `input.zip`, instead of leaving it `Processing` (409 on every retry).
    let lease = match try_begin_processing(&state, &job_id, LeaseStage::Upload) {
        BeginOutcome::NotFound => return StatusCode::NOT_FOUND,
        BeginOutcome::AlreadyActive => return StatusCode::CONFLICT,
        BeginOutcome::Started(lease) => *lease,
    };
    let admission = match state.admission_semaphore.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            update_job_status(
                &state,
                &job_id,
                JobStatus::Error,
                Some("server is at capacity; please retry later".to_string()),
            );
            return StatusCode::TOO_MANY_REQUESTS;
        }
    };
    let upload_permit = match state.upload_semaphore.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            drop(admission);
            update_job_status(
                &state,
                &job_id,
                JobStatus::Error,
                Some("server is at capacity; please retry later".to_string()),
            );
            return StatusCode::TOO_MANY_REQUESTS;
        }
    };
    let job_dir = state.base_dir.join(&job_id);
    if tokio::fs::create_dir_all(&job_dir).await.is_err() {
        drop(upload_permit);
        drop(admission);
        update_job_status(
            &state,
            &job_id,
            JobStatus::Error,
            Some("failed to create job directory".to_string()),
        );
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    let input_path = job_dir.join("input.zip");
    let max_upload_bytes = state.max_upload_bytes;
    match stream_body_to_file(
        request.into_body(),
        &input_path,
        max_upload_bytes,
        state.upload_idle_timeout,
        state.upload_timeout,
    )
    .await
    {
        Ok(()) => {}
        Err(StreamBodyError::TooLarge) => {
            drop(upload_permit);
            drop(admission);
            update_job_status(
                &state,
                &job_id,
                JobStatus::Error,
                Some("upload exceeds the configured size limit".to_string()),
            );
            return StatusCode::PAYLOAD_TOO_LARGE;
        }
        Err(StreamBodyError::Timeout) => {
            drop(upload_permit);
            drop(admission);
            update_job_status(
                &state,
                &job_id,
                JobStatus::Error,
                Some("upload timed out".to_string()),
            );
            return StatusCode::REQUEST_TIMEOUT;
        }
        Err(StreamBodyError::Io) => {
            drop(upload_permit);
            drop(admission);
            update_job_status(
                &state,
                &job_id,
                JobStatus::Error,
                Some("failed to persist upload".to_string()),
            );
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    }
    drop(upload_permit);
    update_job_input(&state, &job_id, input_path);
    spawn_job_processing_admitted(
        state,
        job_id,
        String::new(),
        admission,
        lease.into_processing(),
    );
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
            job.touch();
        }
    }
    persist_job_metadata(state, job_id);
}

fn update_job_status(state: &AppState, job_id: &str, status: JobStatus, error: Option<String>) {
    if let Ok(mut jobs) = state.jobs.write() {
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = status;
            job.error = error;
            job.touch();
        }
    }
    persist_job_metadata(state, job_id);
}

/// Set the final status only if the job is still `Processing`. Cleanup may have
/// marked a runaway validation as timed out in the meantime; the worker's late
/// verdict must not overwrite that.
fn set_status_if_processing(
    state: &AppState,
    job_id: &str,
    status: JobStatus,
    error: Option<String>,
) -> bool {
    let updated = {
        let Ok(mut jobs) = state.jobs.write() else {
            return false;
        };
        match jobs.get_mut(job_id) {
            Some(job) if matches!(job.status, JobStatus::Processing) => {
                job.status = status;
                job.error = error;
                job.processing_started = None;
                job.touch();
                true
            }
            _ => false,
        }
    };
    if updated {
        persist_job_metadata(state, job_id);
    }
    updated
}

fn job_is_processing(state: &AppState, job_id: &str) -> bool {
    get_job(state, job_id).is_some_and(|job| matches!(job.status, JobStatus::Processing))
}

fn mark_processing_started(state: &AppState, job_id: &str) {
    if let Ok(mut jobs) = state.jobs.write() {
        if let Some(job) = jobs.get_mut(job_id) {
            job.processing_started = Some(Instant::now());
        }
    }
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

fn spawn_job_processing(state: AppState, job_id: String, url: String, lease: JobLease) {
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
    spawn_job_processing_admitted(state, job_id, url, admission, lease);
}

fn spawn_job_processing_admitted(
    state: AppState,
    job_id: String,
    url: String,
    admission: tokio::sync::OwnedSemaphorePermit,
    lease: JobLease,
) {
    tokio::spawn(async move {
        // Held for the whole lifetime of the job so the admission count only
        // drops once this job is fully done.
        let _admission = admission;
        // Bound concurrent validations so public traffic cannot exhaust the
        // blocking thread pool / CPU / memory. The permit is held for the whole
        // download + validation and released when the blocking task returns.
        let permit = match state.job_semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                update_job_status(
                    &state,
                    &job_id,
                    JobStatus::Error,
                    Some("server is shutting down".to_string()),
                );
                return;
            }
        };
        // The processing timeout starts now, not at claim time: queueing for
        // the run permit and the upload before it are not validation.
        mark_processing_started(&state, &job_id);
        let state_for_block = state.clone();
        let job_id_for_block = job_id.clone();
        let url_for_block = url.clone();
        let result = tokio::task::spawn_blocking(move || {
            // The lease rides with the blocking work, not with this task: a
            // validation cannot be cancelled, so the job stays owned (and safe
            // from cleanup) until the work has really stopped writing to its
            // directory. A panic drops it too, which marks the job failed.
            let _lease = lease;
            process_job(&state_for_block, &job_id_for_block, &url_for_block)
        })
        .await;
        drop(permit);

        if let Err(err) = result {
            tracing::error!("validation worker for {} failed: {}", job_id, err);
        }
    });
}

#[derive(Debug, PartialEq, Eq)]
enum StreamBodyError {
    TooLarge,
    Io,
    /// The client stopped sending, or kept sending for too long. Distinct from
    /// `Io` because it is the deadline, not the peer, that ended the upload.
    Timeout,
}

/// Stream a request body to `path`, bounded in size *and* in time.
///
/// Both deadlines exist so the handler always returns: it owns the upload and
/// admission permits, and nothing else can release them. `idle_timeout` bounds
/// the wait for the next chunk, `total_timeout` the whole transfer.
async fn stream_body_to_file(
    body: Body,
    path: &Path,
    max_bytes: usize,
    idle_timeout: Duration,
    total_timeout: Duration,
) -> Result<(), StreamBodyError> {
    let result =
        stream_body_to_file_inner(body, path, max_bytes, idle_timeout, total_timeout).await;
    if result.is_err() {
        // One place to drop the partial file, whichever deadline or error ended
        // the transfer.
        let _ = tokio::fs::remove_file(path).await;
    }
    result
}

async fn stream_body_to_file_inner(
    body: Body,
    path: &Path,
    max_bytes: usize,
    idle_timeout: Duration,
    total_timeout: Duration,
) -> Result<(), StreamBodyError> {
    let deadline = tokio::time::Instant::now() + total_timeout;
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|_| StreamBodyError::Io)?;
    let mut written = 0usize;
    let mut stream = body.into_data_stream();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(StreamBodyError::Timeout);
        }
        let next = match tokio::time::timeout(idle_timeout.min(remaining), stream.next()).await {
            Ok(next) => next,
            Err(_) => return Err(StreamBodyError::Timeout),
        };
        let Some(chunk) = next else { break };
        let data = chunk.map_err(|_| StreamBodyError::Io)?;
        if written.saturating_add(data.len()) > max_bytes {
            return Err(StreamBodyError::TooLarge);
        }
        written += data.len();
        if file.write_all(&data).await.is_err() {
            return Err(StreamBodyError::Io);
        }
    }
    if file.flush().await.is_err() {
        return Err(StreamBodyError::Io);
    }
    Ok(())
}

/// Insert `job` unless the pending-upload cap is already reached, counting and
/// inserting under a single write lock. Returns false when the job was refused.
///
/// This cap is separate from `admission_semaphore`: a job awaiting its upload
/// holds no admission permit, so the two limits bound different things and both
/// use `GTFS_VALIDATOR_WEB_MAX_QUEUED_JOBS` as their size.
fn insert_job_within_pending_cap(state: &AppState, job: Job) -> bool {
    let job_id = job.id.clone();
    let inserted = {
        let Ok(mut jobs) = state.jobs.write() else {
            return false;
        };
        let pending = jobs
            .values()
            .filter(|existing| matches!(existing.status, JobStatus::AwaitingUpload))
            .count();
        if matches!(job.status, JobStatus::AwaitingUpload) && pending >= state.max_queued_jobs {
            false
        } else {
            jobs.insert(job_id.clone(), job);
            true
        }
    };
    if inserted {
        persist_job_metadata(state, job_id.as_str());
    }
    inserted
}

/// True when the request declares a body larger than the cap. A missing or
/// unparsable `Content-Length` is not a rejection: the streaming write enforces
/// the cap regardless of what the client declared.
fn declared_length_over_cap(headers: &HeaderMap, max_bytes: usize) -> bool {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .is_some_and(|declared| declared > max_bytes as u64)
}

fn pubsub_token_matches(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(expected) = state.pubsub_token.as_deref() else {
        return false;
    };
    extract_pubsub_token(headers).is_some_and(|value| constant_time_eq(value, expected))
}

/// Compare two secrets without an early return, so response time does not leak
/// how many leading bytes a guess got right. Length still differs observably,
/// which is not sensitive for a fixed-length deployment token.
fn constant_time_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

fn extract_pubsub_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-pubsub-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

/// Result of trying to move a job into the `Processing` state.
enum BeginOutcome {
    /// The job existed and was atomically claimed for processing.
    Started(Box<JobLease>),
    /// The job is already `Processing` or finished `Success`; caller must not
    /// start a second worker for it.
    AlreadyActive,
    /// No such job.
    NotFound,
}

/// Atomically transition a job into `Processing`. Only jobs waiting for input
/// or in a prior `Error` state may (re)start; this is the single point that
/// prevents two workers writing the same job's output concurrently. A job
/// still owned by a lease is never restarted, even once cleanup has marked its
/// runaway validation as timed out: that worker is still writing.
fn try_begin_processing(state: &AppState, job_id: &str, stage: LeaseStage) -> BeginOutcome {
    let started = {
        let Ok(mut jobs) = state.jobs.write() else {
            return BeginOutcome::NotFound;
        };
        match jobs.get_mut(job_id) {
            None => return BeginOutcome::NotFound,
            Some(job) if job.active => false,
            Some(job) => match job.status {
                JobStatus::Processing | JobStatus::Success => false,
                JobStatus::AwaitingUpload | JobStatus::Error => {
                    job.status = JobStatus::Processing;
                    job.error = None;
                    job.active = true;
                    job.processing_started = None;
                    job.touch();
                    true
                }
            },
        }
    };
    if started {
        persist_job_metadata(state, job_id);
        BeginOutcome::Started(Box::new(JobLease::adopt(
            state.clone(),
            job_id.to_string(),
            stage,
        )))
    } else {
        BeginOutcome::AlreadyActive
    }
}

/// Who holds a job, which decides how it is handed back if the holder goes
/// away without recording an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeaseStage {
    /// An upload handler is streaming the body. Abandoned: the job goes back to
    /// `AwaitingUpload` and the partial `input.zip` is removed.
    Upload,
    /// A worker is queued for, or running, validation. Abandoned: `Error`.
    Processing,
}

/// Ownership of a job while it is `Processing` (the job's `active` flag).
///
/// While a lease is alive cleanup never removes the job, so its directory
/// cannot vanish under a writer and a late write cannot resurrect a removed
/// job. Dropping the lease is what releases it -- including when the owning
/// future is dropped (client disconnect) or the worker panics.
struct JobLease {
    state: AppState,
    job_id: String,
    stage: LeaseStage,
}

impl JobLease {
    /// Wrap a job whose `active` flag the caller has just set under the lock.
    fn adopt(state: AppState, job_id: String, stage: LeaseStage) -> Self {
        Self {
            state,
            job_id,
            stage,
        }
    }

    /// The upload finished; the same ownership continues into validation.
    fn into_processing(mut self) -> Self {
        self.stage = LeaseStage::Processing;
        self
    }
}

impl Drop for JobLease {
    fn drop(&mut self) {
        release_job(&self.state, &self.job_id, self.stage);
    }
}

/// Hand a job back. An owner that went away without an outcome left it
/// `Processing`; that is undone first, while the job is still owned, and only
/// then is `active` cleared -- so cleanup can never remove the directory
/// between the reset and its filesystem writes.
fn release_job(state: &AppState, job_id: &str, stage: LeaseStage) {
    let abandoned = {
        let Ok(mut jobs) = state.jobs.write() else {
            return;
        };
        let Some(job) = jobs.get_mut(job_id) else {
            return;
        };
        let abandoned = matches!(job.status, JobStatus::Processing);
        if abandoned {
            match stage {
                LeaseStage::Upload => {
                    job.status = JobStatus::AwaitingUpload;
                    job.error = None;
                    job.input_path = None;
                }
                LeaseStage::Processing => {
                    job.status = JobStatus::Error;
                    job.error = Some("validation was interrupted".to_string());
                }
            }
            job.processing_started = None;
            job.touch();
        }
        abandoned
    };
    if abandoned {
        let job_dir = state.base_dir.join(job_id);
        if stage == LeaseStage::Upload {
            let _ = std::fs::remove_file(job_dir.join("input.zip"));
        } else {
            write_execution_result(&job_dir, Err("validation was interrupted".to_string()));
        }
        persist_job_metadata(state, job_id);
    }
    if let Ok(mut jobs) = state.jobs.write() {
        if let Some(job) = jobs.get_mut(job_id) {
            job.active = false;
        }
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
            discard_job_input(state, job_id, &job_dir, &path);
            finish_job(state, &job_dir, job_id, Err(err.to_string()));
            return;
        }
        path
    } else if let Some(input_path) = job.input_path.clone() {
        input_path
    } else {
        finish_job(state, &job_dir, job_id, Err("missing input".to_string()));
        return;
    };

    let output_dir = job_dir.join("output");
    let result = std::fs::create_dir_all(&output_dir)
        .map_err(anyhow::Error::from)
        .and_then(|()| {
            let input_uri = if url.is_empty() { None } else { Some(url) };
            run_validation(
                &input_path,
                &output_dir,
                job.country_code.as_deref(),
                input_uri,
                Instant::now(),
            )
        });
    // Only the reports are served, so the archive -- the bulk of a job's disk
    // use -- goes as soon as validation is done rather than living out the job
    // TTL. A retry uploads a fresh one.
    discard_job_input(state, job_id, &job_dir, &input_path);
    finish_job(
        state,
        &job_dir,
        job_id,
        result.map_err(|err| err.to_string()),
    );
}

/// Record a worker's verdict, unless cleanup already marked the job as timed
/// out: that verdict stands, together with the execution result it wrote.
fn finish_job(state: &AppState, job_dir: &Path, job_id: &str, result: Result<(), String>) {
    if !job_is_processing(state, job_id) {
        return;
    }
    write_execution_result(job_dir, result.clone());
    let (status, error) = match result {
        Ok(()) => (JobStatus::Success, None),
        Err(err) => (JobStatus::Error, Some(err)),
    };
    set_status_if_processing(state, job_id, status, error);
}

/// Delete a job's input archive and forget its path. Only a file inside the
/// job's own directory is removed; a path from elsewhere is left alone.
fn discard_job_input(state: &AppState, job_id: &str, job_dir: &Path, input_path: &Path) {
    if input_path.starts_with(job_dir) {
        let _ = std::fs::remove_file(input_path);
    }
    if let Ok(mut jobs) = state.jobs.write() {
        if let Some(job) = jobs.get_mut(job_id) {
            job.input_path = None;
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
    // The country only reached the report summary before, so the report claimed
    // a country the rules never saw: country-dependent checks (phone numbers
    // among them) read it from the validation context, exactly as the CLI sets
    // it. Held across the whole validation, and dropped with this function --
    // the context is thread-local and this runs on a blocking pool thread.
    let _country_guard = normalized_country_code(country_code)
        .map(|code| gtfs_guru_core::set_validation_country_code(Some(code)));
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

/// Trim and reject the "unknown country" placeholder, matching how the CLI
/// decides whether a `--country-code` is worth setting.
fn normalized_country_code(country_code: Option<&str>) -> Option<String> {
    country_code
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("ZZ"))
        .map(str::to_string)
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

    let deadline = Instant::now() + JOB_DOWNLOAD_TIMEOUT;
    let client = build_public_http_client(JOB_DOWNLOAD_IO_TIMEOUT)?;
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("download gtfs from {}", url))?
        .error_for_status()
        .with_context(|| format!("download gtfs from {}", url))?;
    let mut file =
        std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    copy_bounded(
        DeadlineReader::new(response, deadline),
        &mut file,
        max_bytes,
    )
    .with_context(|| format!("write {}", path.display()))
    .inspect_err(|_| {
        drop(std::fs::remove_file(path));
    })?;
    Ok(())
}

fn download_url_to_bytes(
    url: &str,
    max_bytes: usize,
    timeout: Duration,
) -> anyhow::Result<Vec<u8>> {
    guard_public_url(url)?;
    let deadline = Instant::now() + timeout;
    let client = build_public_http_client(PROXY_IO_TIMEOUT.min(timeout))?;
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
    copy_bounded(
        DeadlineReader::new(response, deadline),
        &mut bytes,
        max_bytes,
    )?;
    Ok(bytes)
}

const DEADLINE_MESSAGE: &str = "remote transfer exceeded its time limit";

/// Fails reads once `deadline` has passed. reqwest's blocking timeout bounds
/// each read separately, so an upstream that drips a byte just inside it would
/// otherwise keep a transfer (and the permits behind it) alive indefinitely.
/// A read already in progress still ends by the per-read timeout.
struct DeadlineReader<R> {
    inner: R,
    deadline: Instant,
}

impl<R> DeadlineReader<R> {
    fn new(inner: R, deadline: Instant) -> Self {
        Self { inner, deadline }
    }
}

impl<R: std::io::Read> std::io::Read for DeadlineReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if Instant::now() >= self.deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                DEADLINE_MESSAGE,
            ));
        }
        self.inner.read(buf)
    }
}

fn is_deadline_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::TimedOut)
            || cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(reqwest::Error::is_timeout)
    })
}

/// `io_timeout` bounds each blocking operation (connect plus response headers,
/// then every body read); callers add an overall deadline with `DeadlineReader`.
fn build_public_http_client(io_timeout: Duration) -> anyhow::Result<Client> {
    Client::builder()
        .user_agent(format!(
            "gtfs-validator-rust-web/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .dns_resolver(Arc::new(PublicOnlyResolver))
        .connect_timeout(Duration::from_secs(10))
        .timeout(io_timeout)
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
                || v4.is_multicast()
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
            let seg = v6.segments();
            // Deprecated IPv4-compatible ::a.b.c.d reaches the IPv4 host.
            if seg[..6] == [0; 6] {
                let [a, b] = seg[6].to_be_bytes();
                let [c, d] = seg[7].to_be_bytes();
                return is_global_ip(IpAddr::V4(std::net::Ipv4Addr::new(a, b, c, d)));
            }
            let seg0 = seg[0];
            // fc00::/7 unique-local, fe80::/10 link-local, fec0::/10
            // site-local, ff00::/8 multicast
            if (seg0 & 0xfe00) == 0xfc00
                || (seg0 & 0xffc0) == 0xfe80
                || (seg0 & 0xffc0) == 0xfec0
                || (seg0 & 0xff00) == 0xff00
            {
                return false;
            }
            // NAT64 64:ff9b::/96 and 64:ff9b:1::/48, 6to4 2002::/16, Teredo
            // 2001::/32 and documentation 2001:db8::/32 all translate to or
            // embed an arbitrary IPv4 address, or are not routable.
            !((seg0 == 0x0064 && seg[1] == 0xff9b)
                || seg0 == 0x2002
                || (seg0 == 0x2001 && (seg[1] == 0 || seg[1] == 0x0db8)))
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
    // Runs even with the job TTL disabled (0): pending uploads and runaway
    // validations have their own deadlines.
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            cleanup_jobs(&state, ttl_ms).await;
        }
    });
}

/// What one cleanup pass decided, computed from the in-memory map alone.
#[derive(Debug, Default)]
struct CleanupPlan {
    /// Directories of jobs already dropped from the map.
    remove_dirs: Vec<PathBuf>,
    /// Owned jobs whose validation ran past the processing timeout and were
    /// just marked failed; their metadata still has to be written.
    timed_out: Vec<String>,
    /// Every job still in the map, so the orphan sweep leaves them alone.
    known_ids: HashSet<String>,
}

/// One cleanup pass. Deciding happens under the jobs lock and touches no disk;
/// the filesystem work (metadata writes, `remove_dir_all`, the orphan scan)
/// then runs on the blocking pool with the lock released, so neither the
/// runtime's worker threads nor every other request wait on it.
async fn cleanup_jobs(state: &AppState, ttl_ms: u128) {
    let plan = plan_cleanup(state, ttl_ms, current_millis(), Instant::now());
    let state = state.clone();
    let _ = tokio::task::spawn_blocking(move || apply_cleanup(&state, plan, ttl_ms)).await;
}

fn plan_cleanup(state: &AppState, ttl_ms: u128, now_millis: u128, now: Instant) -> CleanupPlan {
    let mut plan = CleanupPlan::default();
    let Ok(mut jobs) = state.jobs.write() else {
        return plan;
    };
    jobs.retain(|job_id, job| {
        let job_dir = state.base_dir.join(job_id);
        if job.active {
            // Owned by a live upload or worker, which may still be writing the
            // job directory: never removed here. A validation past its deadline
            // is marked failed instead; the worker cannot be cancelled, so the
            // job is reclaimed by the TTL only after it has let go.
            let overdue = matches!(job.status, JobStatus::Processing)
                && job.processing_started.is_some_and(|started| {
                    now.saturating_duration_since(started).as_millis()
                        >= state.processing_timeout_ms
                });
            if overdue {
                job.status = JobStatus::Error;
                job.error = Some(processing_timeout_message(state));
                job.processing_started = None;
                job.updated_at_millis = now_millis;
                plan.timed_out.push(job_id.clone());
            }
            plan.known_ids.insert(job_id.clone());
            return true;
        }
        let age = now_millis.saturating_sub(job.updated_at_millis);
        let expired = match job.status {
            // Measured from creation, so a client that keeps starting and
            // abandoning uploads cannot keep the pending slot alive.
            JobStatus::AwaitingUpload => {
                now_millis.saturating_sub(job.created_at_millis) >= state.pending_upload_ttl_ms
                    || (ttl_ms > 0 && age >= ttl_ms)
            }
            // `Processing` with no owner should not exist (leases and
            // `load_jobs` both resolve it); reclaim it rather than keep it.
            JobStatus::Processing => age >= state.processing_timeout_ms,
            JobStatus::Success | JobStatus::Error => ttl_ms > 0 && age >= ttl_ms,
        };
        if !expired {
            plan.known_ids.insert(job_id.clone());
            return true;
        }
        if let Some(output_dir) = job.output_dir.as_ref() {
            if !output_dir.starts_with(&job_dir) {
                plan.remove_dirs.push(output_dir.clone());
            }
        }
        plan.remove_dirs.push(job_dir);
        false
    });
    plan
}

fn processing_timeout_message(state: &AppState) -> String {
    format!(
        "validation timed out after {} seconds",
        state.processing_timeout_ms / 1000
    )
}

fn apply_cleanup(state: &AppState, plan: CleanupPlan, ttl_ms: u128) {
    for job_id in &plan.timed_out {
        // Still owned by its worker, so the directory is still there.
        write_execution_result(
            &state.base_dir.join(job_id),
            Err(processing_timeout_message(state)),
        );
        persist_job_metadata(state, job_id);
    }
    for dir in &plan.remove_dirs {
        let _ = std::fs::remove_dir_all(dir);
    }
    if ttl_ms == 0 {
        return;
    }
    // Reclaim orphan directories that never made it into the in-memory map
    // (e.g. a job.json that failed to parse on load): the pass above can never
    // see them, so they would otherwise leak disk indefinitely. A job created
    // after the snapshot is not in `known_ids`, but its directory is fresh.
    let now = current_millis();
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
            if plan.known_ids.contains(&name) {
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

/// Read jobs back after a restart. No lease survives a restart, so a job left
/// `Processing` is resolved here instead of blocking re-uploads with 409 until
/// cleanup reclaims it: an upload that never completed goes back to
/// `AwaitingUpload`, a validation that was cut off becomes an error. Pending
/// jobs past their TTL are dropped outright.
fn load_jobs(
    base_dir: &Path,
    pending_upload_ttl_ms: u128,
    now_millis: u128,
) -> HashMap<String, Job> {
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
        let mut metadata: JobMetadata = match serde_json::from_str(&data) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if matches!(metadata.status, JobStatus::Processing) {
            // `input_path` is set up front for URL jobs and after a finished
            // upload for upload jobs; unset means the upload never completed.
            let upload_completed = metadata.input_path.is_some();
            if upload_completed {
                metadata.status = JobStatus::Error;
                metadata.error = Some("interrupted by restart".to_string());
                write_execution_result(&path, Err("interrupted by restart".to_string()));
            } else {
                metadata.status = JobStatus::AwaitingUpload;
                metadata.error = None;
            }
            // Either way the archive is stale or partial; a retry uploads anew.
            let _ = std::fs::remove_file(path.join("input.zip"));
            metadata.input_path = None;
            metadata.updated_at_millis = now_millis;
            write_job_metadata(&path, &metadata);
        }
        if matches!(metadata.status, JobStatus::AwaitingUpload)
            && now_millis.saturating_sub(metadata.created_at_millis) >= pending_upload_ttl_ms
        {
            let _ = std::fs::remove_dir_all(&path);
            continue;
        }
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
    write_job_metadata(&job_dir, &JobMetadata::from_job(&job, &job_dir));
}

/// Write `job.json`. Deliberately does not create the directory: a write that
/// races a cleanup that just removed the job must fail rather than resurrect a
/// half-empty job directory that the next restart would load again.
fn write_job_metadata(job_dir: &Path, metadata: &JobMetadata) {
    let Ok(json) = serde_json::to_string_pretty(metadata) else {
        return;
    };
    let _ = std::fs::write(job_dir.join("job.json"), format!("{}\n", json));
}

impl JobMetadata {
    fn from_job(job: &Job, job_dir: &Path) -> Self {
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
            created_at_millis: job.created_at_millis,
            updated_at_millis: job.updated_at_millis,
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
            created_at_millis: self.created_at_millis,
            updated_at_millis: self.updated_at_millis,
            active: false,
            processing_started: None,
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

    /// The country the caller sent must reach the rules, not only the report
    /// summary: country-dependent checks are silently skipped without it, so
    /// the API would report a clean feed the CLI flags.
    #[test]
    fn the_requested_country_code_reaches_country_dependent_rules() {
        let feed_dir = std::env::temp_dir().join("gtfs_web_country_code_feed");
        std::fs::remove_dir_all(&feed_dir).ok();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-gtfs-feeds/base-valid");
        std::fs::create_dir_all(&feed_dir).expect("create feed dir");
        for entry in std::fs::read_dir(&source).expect("read fixture") {
            let entry = entry.expect("entry");
            std::fs::copy(entry.path(), feed_dir.join(entry.file_name())).expect("copy");
        }
        let agency = std::fs::read_to_string(feed_dir.join("agency.txt")).expect("read agency");
        std::fs::write(
            feed_dir.join("agency.txt"),
            agency.replace("+1-555-555-5555", "not-a-phone"),
        )
        .expect("write agency");

        let phone_notices = |country: Option<&str>| {
            let output = feed_dir.join(match country {
                Some(_) => "out-country",
                None => "out-plain",
            });
            std::fs::create_dir_all(&output).expect("create output");
            run_validation(&feed_dir, &output, country, None, Instant::now()).expect("validate");
            std::fs::read_to_string(output.join("report.json")).expect("read report")
        };

        assert!(phone_notices(Some("US")).contains("invalid_phone_number"));
        assert!(!phone_notices(None).contains("invalid_phone_number"));

        std::fs::remove_dir_all(&feed_dir).ok();
    }

    /// A client that opens the upload and then goes quiet must not keep the
    /// handler -- and with it the upload and admission permits -- alive: those
    /// permits are what a later upload needs to avoid a 429.
    #[tokio::test]
    async fn a_stalled_upload_gives_up_and_removes_the_partial_file() {
        let dir = std::env::temp_dir().join("gtfs_web_upload_idle_timeout");
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("input.zip");
        let body = Body::from_stream(futures_util::stream::pending::<
            Result<axum::body::Bytes, std::io::Error>,
        >());

        let started = Instant::now();
        let error = stream_body_to_file(
            body,
            &path,
            1024,
            Duration::from_millis(50),
            Duration::from_secs(30),
        )
        .await
        .expect_err("a body that never arrives must not block forever");

        assert_eq!(error, StreamBodyError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!path.exists(), "the partial file must be removed");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A trickle that keeps resetting the idle timer still hits the ceiling.
    #[tokio::test]
    async fn a_trickling_upload_hits_the_total_deadline() {
        let dir = std::env::temp_dir().join("gtfs_web_upload_total_timeout");
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("input.zip");
        let body = Body::from_stream(futures_util::stream::unfold((), |()| async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            Some((
                Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"x")),
                (),
            ))
        }));

        let error = stream_body_to_file(
            body,
            &path,
            1024 * 1024,
            Duration::from_secs(30),
            Duration::from_millis(100),
        )
        .await
        .expect_err("an endless trickle must not block forever");

        assert_eq!(error, StreamBodyError::Timeout);
        assert!(!path.exists(), "the partial file must be removed");
        std::fs::remove_dir_all(&dir).ok();
    }

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
    fn guard_public_url_rejects_ipv6_forms_of_internal_ipv4() {
        for url in [
            "http://[::ffff:169.254.169.254]/feed.zip",
            "http://[::127.0.0.1]/feed.zip",
            "http://[64:ff9b::a9fe:a9fe]/feed.zip",
            "http://[2002:7f00:1::]/feed.zip",
            "http://[fec0::1]/feed.zip",
            "http://[ff02::1]/feed.zip",
            "http://224.0.0.1/feed.zip",
        ] {
            assert!(guard_public_url(url).is_err(), "{url} must be rejected");
        }
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
            // Untracked files that include_dir! would still embed.
            ".DS_Store",
            "pkg/.DS_Store",
            ".git/config",
            "notices/.index.html.swp",
        ] {
            assert!(is_sensitive_static_path(path), "{path} must be blocked");
        }
        assert!(!is_sensitive_static_path("index.html"));
        assert!(!is_sensitive_static_path("notices/unknown_file/index.html"));
        assert!(!is_sensitive_static_path("pkg/gtfs_guru_wasm_bg.wasm"));
    }

    #[test]
    fn embedded_validator_assets_are_present() {
        for path in [
            "pkg/gtfs_guru_wasm.js",
            "pkg/gtfs_guru_wasm_bg.wasm",
            "pkg/worker.js",
            "pkg-mt/gtfs_guru_wasm.js",
            "pkg-mt/gtfs_guru_wasm_bg.wasm",
            "pkg-mt/worker-mt.js",
        ] {
            assert!(WEBSITE_DIR.get_file(path).is_some(), "missing {path}");
        }
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

    #[test]
    fn pubsub_token_is_read_from_custom_header_or_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-pubsub-token", header::HeaderValue::from_static("abc"));
        assert_eq!(extract_pubsub_token(&headers), Some("abc"));

        headers.remove("x-pubsub-token");
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_static("Bearer secret"),
        );
        assert_eq!(extract_pubsub_token(&headers), Some("secret"));

        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_static("Basic nope"),
        );
        assert_eq!(extract_pubsub_token(&headers), None);
    }

    #[test]
    fn constant_time_eq_matches_string_equality() {
        assert!(constant_time_eq("secret", "secret"));
        assert!(!constant_time_eq("secret", "secreT"));
        assert!(!constant_time_eq("secret", "secrets"));
        assert!(!constant_time_eq("", "s"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn declared_length_over_cap_only_rejects_a_declared_overflow() {
        let mut headers = HeaderMap::new();
        assert!(!declared_length_over_cap(&headers, 10));

        headers.insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from_static("10"),
        );
        assert!(!declared_length_over_cap(&headers, 10));

        headers.insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from_static("11"),
        );
        assert!(declared_length_over_cap(&headers, 10));

        // Chunked uploads declare nothing; the streaming write is the guard.
        headers.insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from_static("not-a-number"),
        );
        assert!(!declared_length_over_cap(&headers, 10));
    }

    #[test]
    fn stream_size_check_rejects_a_chunk_past_the_cap() {
        let written = 4usize;
        let incoming = 2usize;
        let max_bytes = 5usize;
        assert!(written.saturating_add(incoming) > max_bytes);
        assert!(written.saturating_add(1) <= max_bytes);
    }

    fn test_state(name: &str) -> AppState {
        let dir =
            std::env::temp_dir().join(format!("gtfs_web_{name}_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).expect("create base dir");
        AppState::new(dir, "http://localhost:3000".to_string())
    }

    fn add_job(state: &AppState, status: JobStatus) -> String {
        let job_id = next_job_id();
        let job_dir = state.base_dir.join(&job_id);
        std::fs::create_dir_all(&job_dir).expect("create job dir");
        let job = Job::new(
            job_id.clone(),
            status,
            None,
            None,
            Some(job_dir.join("output")),
        );
        assert!(insert_job_within_pending_cap(state, job));
        job_id
    }

    fn edit_job(state: &AppState, job_id: &str, edit: impl FnOnce(&mut Job)) {
        let mut jobs = state.jobs.write().expect("jobs lock");
        edit(jobs.get_mut(job_id).expect("job exists"));
    }

    fn peer() -> ConnectInfo<SocketAddr> {
        ConnectInfo("203.0.113.7:40000".parse().expect("socket addr"))
    }

    // --- Pending uploads -------------------------------------------------

    /// A job nobody uploads to gives its pending slot back after the short
    /// pending TTL, not after the 24 h job TTL.
    #[test]
    fn a_pending_upload_expires_after_its_own_ttl() {
        let mut state = test_state("pending_ttl");
        state.pending_upload_ttl_ms = 1_000;
        let pending = add_job(&state, JobStatus::AwaitingUpload);
        let finished = add_job(&state, JobStatus::Success);
        let created = get_job(&state, &pending).expect("job").created_at_millis;
        let day = 24 * 60 * 60 * 1000;

        let plan = plan_cleanup(&state, day, created + 500, Instant::now());
        assert!(plan.remove_dirs.is_empty());
        assert!(get_job(&state, &pending).is_some());

        let plan = plan_cleanup(&state, day, created + 1_000, Instant::now());
        assert_eq!(plan.remove_dirs, vec![state.base_dir.join(&pending)]);
        assert!(get_job(&state, &pending).is_none());
        assert!(
            get_job(&state, &finished).is_some(),
            "only pending jobs use it"
        );
        std::fs::remove_dir_all(&state.base_dir).ok();
    }

    #[tokio::test]
    async fn create_job_refuses_cross_site_and_non_json_requests() {
        let state = test_state("create_job_guard");
        let create = |headers: HeaderMap, body: &'static [u8]| {
            create_job(
                State(state.clone()),
                peer(),
                headers,
                Bytes::from_static(body),
            )
        };

        let mut cross_site = HeaderMap::new();
        cross_site.insert(
            "sec-fetch-site",
            header::HeaderValue::from_static("cross-site"),
        );
        assert_eq!(
            create(cross_site, b"").await.status(),
            StatusCode::FORBIDDEN
        );

        // What a `no-cors` fetch or a form can send without a preflight.
        let mut text = HeaderMap::new();
        text.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/plain"),
        );
        assert_eq!(
            create(text, b"{}").await.status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );

        let mut json = HeaderMap::new();
        json.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        assert_eq!(
            create(json.clone(), b"{not json").await.status(),
            StatusCode::BAD_REQUEST
        );
        assert!(state.jobs.read().expect("jobs").is_empty());

        // `curl -X POST` with no body, and the UI's JSON body, still work.
        assert_eq!(create(HeaderMap::new(), b"").await.status(), StatusCode::OK);
        assert_eq!(
            create(json, br#"{"countryCode":"US"}"#).await.status(),
            StatusCode::OK
        );
        assert_eq!(state.jobs.read().expect("jobs").len(), 2);
        std::fs::remove_dir_all(&state.base_dir).ok();
    }

    // --- Input retention --------------------------------------------------

    /// Only the reports are served, so the uploaded archive goes once
    /// validation has finished, whatever its outcome.
    #[test]
    fn validation_discards_the_input_archive() {
        let state = test_state("discard_input");
        let job_id = add_job(&state, JobStatus::AwaitingUpload);
        let input = state.base_dir.join(&job_id).join("input.zip");
        std::fs::write(&input, b"not a zip").expect("write input");
        update_job_input(&state, &job_id, input.clone());
        let BeginOutcome::Started(lease) =
            try_begin_processing(&state, &job_id, LeaseStage::Processing)
        else {
            panic!("job must be claimable");
        };

        process_job(&state, &job_id, "");
        drop(lease);

        let job = get_job(&state, &job_id).expect("job");
        assert!(!input.exists(), "the archive must be deleted");
        assert!(job.input_path.is_none());
        assert!(!matches!(job.status, JobStatus::Processing));
        assert!(!job.active);
        assert!(state
            .base_dir
            .join(&job_id)
            .join("output/execution_result.json")
            .exists());
        std::fs::remove_dir_all(&state.base_dir).ok();
    }

    // --- Per-client limits ------------------------------------------------

    #[test]
    fn forwarded_for_is_only_believed_from_trusted_peers() {
        let trusted = parse_trusted_proxies(DEFAULT_TRUSTED_PROXIES);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            header::HeaderValue::from_static("198.51.100.9"),
        );
        let loopback: IpAddr = "127.0.0.1".parse().unwrap();
        let stranger: IpAddr = "203.0.113.7".parse().unwrap();

        assert_eq!(
            client_ip(loopback, &headers, &trusted),
            "198.51.100.9".parse::<IpAddr>().unwrap()
        );
        assert_eq!(client_ip(stranger, &headers, &trusted), stranger);
        assert_eq!(client_ip(loopback, &HeaderMap::new(), &trusted), loopback);
        // Production: the host's Caddy reaches the container via the Docker
        // bridge gateway, not loopback.
        let docker_gateway: IpAddr = "172.17.0.1".parse().unwrap();
        assert_eq!(
            client_ip(docker_gateway, &headers, &trusted),
            "198.51.100.9".parse::<IpAddr>().unwrap()
        );
        // Nobody trusted: the header is ignored even from loopback.
        assert_eq!(client_ip(loopback, &headers, &[]), loopback);
        // A v4-mapped loopback peer is still loopback.
        let mapped: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert_eq!(
            client_ip(mapped, &headers, &trusted),
            "198.51.100.9".parse::<IpAddr>().unwrap()
        );
    }

    /// A client can put anything at the front of the header; only what our own
    /// proxy appended on the right is believed.
    #[test]
    fn forwarded_for_is_read_from_the_right() {
        let trusted = parse_trusted_proxies("127.0.0.1, 10.0.0.0/8");
        let loopback: IpAddr = "127.0.0.1".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            header::HeaderValue::from_static("1.2.3.4, 198.51.100.9, 10.1.2.3"),
        );
        assert_eq!(
            client_ip(loopback, &headers, &trusted),
            "198.51.100.9".parse::<IpAddr>().unwrap()
        );

        headers.insert(
            "x-forwarded-for",
            header::HeaderValue::from_static("1.2.3.4, garbage"),
        );
        assert_eq!(client_ip(loopback, &headers, &trusted), loopback);
    }

    #[test]
    fn trusted_proxy_networks_parse_and_match() {
        let networks = parse_trusted_proxies("127.0.0.0/8, ::1, fd00::/8, nonsense, 10.0.0.0/33");
        assert_eq!(networks.len(), 3, "invalid entries are skipped");
        let contains = |ip: &str| {
            let ip: IpAddr = ip.parse().unwrap();
            networks.iter().any(|network| network.contains(ip))
        };
        assert!(contains("127.5.6.7"));
        assert!(contains("::1"));
        assert!(contains("fd12::1"));
        assert!(!contains("128.0.0.1"));
        assert!(!contains("::2"));
        assert!(parse_trusted_proxies("").is_empty());
        assert!(IpNetwork::parse("0.0.0.0/0")
            .expect("any")
            .contains("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn ipv6_clients_are_keyed_by_their_64() {
        let a: IpAddr = "2001:db8:1:2::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:ffff::9".parse().unwrap();
        let c: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(rate_limit_key(a), rate_limit_key(b));
        assert_ne!(rate_limit_key(a), rate_limit_key(c));
        let v4: IpAddr = "198.51.100.9".parse().unwrap();
        assert_eq!(rate_limit_key(v4), v4);
    }

    /// Requests a client is refused do not count against the global budget, so
    /// one noisy client cannot lock everyone else out.
    #[test]
    fn one_client_cannot_spend_the_global_budget() {
        let limits = RateLimits::new(3, 2);
        let now = Instant::now();
        let noisy: IpAddr = "198.51.100.1".parse().unwrap();
        let quiet: IpAddr = "198.51.100.2".parse().unwrap();
        let admitted = (0..50).filter(|_| limits.try_acquire(noisy, now)).count();
        assert_eq!(admitted, 2);
        assert!(limits.try_acquire(quiet, now));
        // The global cap still holds across clients.
        let third: IpAddr = "198.51.100.3".parse().unwrap();
        assert!(!limits.try_acquire(third, now));
        let later = now + Duration::from_secs(61);
        assert!(limits.try_acquire(noisy, later));
    }

    #[test]
    fn client_slots_cap_in_flight_requests_per_client() {
        let slots = Arc::new(ClientSlots::new(2));
        let client: IpAddr = "198.51.100.1".parse().unwrap();
        let other: IpAddr = "198.51.100.2".parse().unwrap();
        let first = slots.try_acquire(client).expect("first");
        let _second = slots.try_acquire(client).expect("second");
        assert!(slots.try_acquire(client).is_none());
        assert!(slots.try_acquire(other).is_some());
        drop(first);
        assert!(slots.try_acquire(client).is_some());
    }

    /// Each read arrives well inside any per-read timeout, but the transfer as
    /// a whole must still end at the deadline.
    #[test]
    fn a_slow_drip_hits_the_overall_deadline() {
        struct Drip;
        impl std::io::Read for Drip {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                std::thread::sleep(Duration::from_millis(5));
                buf[0] = b'x';
                Ok(1)
            }
        }
        let started = Instant::now();
        let reader = DeadlineReader::new(Drip, started + Duration::from_millis(50));
        let error = copy_bounded(reader, &mut Vec::new(), usize::MAX - 1)
            .expect_err("an endless drip must stop");
        assert!(is_deadline_error(&error), "{error:#}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    // --- Processing timeout -----------------------------------------------

    /// A validation past its deadline is reported as failed, but its job and
    /// directory stay until the worker lets go, and the worker's late verdict
    /// does not overwrite the timeout.
    #[test]
    fn a_runaway_validation_is_failed_but_not_removed_while_running() {
        let mut state = test_state("processing_timeout");
        state.processing_timeout_ms = 1_000;
        let job_id = add_job(&state, JobStatus::AwaitingUpload);
        let job_dir = state.base_dir.join(&job_id);
        let BeginOutcome::Started(lease) =
            try_begin_processing(&state, &job_id, LeaseStage::Processing)
        else {
            panic!("job must be claimable");
        };
        edit_job(&state, &job_id, |job| {
            job.processing_started = Instant::now().checked_sub(Duration::from_secs(5));
        });

        let plan = plan_cleanup(&state, 1, current_millis() + 10_000, Instant::now());
        assert!(
            plan.remove_dirs.is_empty(),
            "a running job is never removed"
        );
        assert_eq!(plan.timed_out, vec![job_id.clone()]);
        apply_cleanup(&state, plan, 0);
        let job = get_job(&state, &job_id).expect("job kept");
        assert!(matches!(job.status, JobStatus::Error));
        assert!(job.error.as_deref().unwrap_or("").contains("timed out"));
        assert!(job_dir.exists());
        // Nobody may restart it while the old worker still writes.
        assert!(matches!(
            try_begin_processing(&state, &job_id, LeaseStage::Upload),
            BeginOutcome::AlreadyActive
        ));

        finish_job(&state, &job_dir, &job_id, Ok(()));
        let job = get_job(&state, &job_id).expect("job kept");
        assert!(
            matches!(job.status, JobStatus::Error),
            "timeout verdict stands"
        );
        let result = std::fs::read_to_string(job_dir.join("output/execution_result.json"))
            .expect("execution result");
        assert!(result.contains("timed out"));

        drop(lease);
        assert!(!get_job(&state, &job_id).expect("job").active);
        let plan = plan_cleanup(&state, 1, current_millis() + 20_000, Instant::now());
        assert_eq!(plan.remove_dirs, vec![job_dir]);
        std::fs::remove_dir_all(&state.base_dir).ok();
    }

    /// Time spent queueing for a run permit is not processing time: a job
    /// claimed long ago that has not started validating is left alone.
    #[test]
    fn a_queued_job_does_not_time_out_before_validation_starts() {
        let mut state = test_state("queued_not_timed_out");
        state.processing_timeout_ms = 1_000;
        let job_id = add_job(&state, JobStatus::AwaitingUpload);
        let BeginOutcome::Started(_lease) =
            try_begin_processing(&state, &job_id, LeaseStage::Processing)
        else {
            panic!("job must be claimable");
        };
        edit_job(&state, &job_id, |job| {
            job.created_at_millis = 0;
            job.updated_at_millis = 0;
        });

        let plan = plan_cleanup(&state, 1, current_millis(), Instant::now());
        assert!(plan.remove_dirs.is_empty());
        assert!(plan.timed_out.is_empty());
        let job = get_job(&state, &job_id).expect("job kept");
        assert!(matches!(job.status, JobStatus::Processing));
        std::fs::remove_dir_all(&state.base_dir).ok();
    }

    // --- Restart and cancellation -----------------------------------------

    #[test]
    fn restart_resolves_jobs_left_processing() {
        let base = std::env::temp_dir().join(format!(
            "gtfs_web_restart_{}",
            uuid::Uuid::new_v4().simple()
        ));
        let now = current_millis();
        let write = |status: JobStatus, input: Option<&str>, created: u128| {
            let job_id = next_job_id();
            let job_dir = base.join(&job_id);
            std::fs::create_dir_all(&job_dir).expect("job dir");
            std::fs::write(job_dir.join("input.zip"), b"partial").expect("input");
            let metadata = JobMetadata {
                id: job_id.clone(),
                status,
                country_code: None,
                input_path: input.map(str::to_string),
                output_dir: Some("output".to_string()),
                error: None,
                created_at_millis: created,
                updated_at_millis: created,
            };
            write_job_metadata(&job_dir, &metadata);
            job_id
        };
        let mid_upload = write(JobStatus::Processing, None, now);
        let mid_validation = write(JobStatus::Processing, Some("input.zip"), now);
        let stale_pending = write(JobStatus::AwaitingUpload, None, now - 10_000);
        let done = write(JobStatus::Success, None, now - 10_000);

        let jobs = load_jobs(&base, 5_000, now);

        let job = &jobs[&mid_upload];
        assert!(matches!(job.status, JobStatus::AwaitingUpload));
        assert!(!base.join(&mid_upload).join("input.zip").exists());
        let job_json =
            std::fs::read_to_string(base.join(&mid_upload).join("job.json")).expect("job.json");
        assert!(job_json.contains("awaiting_upload"), "{job_json}");

        let job = &jobs[&mid_validation];
        assert!(matches!(job.status, JobStatus::Error));
        assert_eq!(job.error.as_deref(), Some("interrupted by restart"));
        assert!(job.input_path.is_none());
        assert!(!base.join(&mid_validation).join("input.zip").exists());

        assert!(!jobs.contains_key(&stale_pending));
        assert!(!base.join(&stale_pending).exists());
        assert!(matches!(jobs[&done].status, JobStatus::Success));
        assert!(jobs.values().all(|job| !job.active));
        std::fs::remove_dir_all(&base).ok();
    }

    /// A client that disconnects mid-upload drops the handler future; the job
    /// must go back to accepting uploads rather than sit in `Processing`.
    #[tokio::test]
    async fn a_dropped_upload_hands_the_job_back() {
        let state = test_state("dropped_upload");
        let job_id = add_job(&state, JobStatus::AwaitingUpload);
        let input = state.base_dir.join(&job_id).join("input.zip");
        let body = Body::from_stream(
            futures_util::stream::once(async {
                Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"partial"))
            })
            .chain(futures_util::stream::pending()),
        );
        let request = Request::builder()
            .method("PUT")
            .body(body)
            .expect("request");

        let outcome = tokio::time::timeout(
            Duration::from_millis(200),
            upload_job(State(state.clone()), AxumPath(job_id.clone()), request),
        )
        .await;
        assert!(outcome.is_err(), "the upload must still be streaming");

        let job = get_job(&state, &job_id).expect("job");
        assert!(matches!(job.status, JobStatus::AwaitingUpload));
        assert!(!job.active);
        assert!(!input.exists(), "the partial upload must be removed");
        assert!(matches!(
            try_begin_processing(&state, &job_id, LeaseStage::Upload),
            BeginOutcome::Started(_)
        ));
        std::fs::remove_dir_all(&state.base_dir).ok();
    }

    #[test]
    fn an_abandoned_worker_marks_its_job_failed() {
        let state = test_state("abandoned_worker");
        let job_id = add_job(&state, JobStatus::AwaitingUpload);
        let BeginOutcome::Started(lease) =
            try_begin_processing(&state, &job_id, LeaseStage::Processing)
        else {
            panic!("job must be claimable");
        };
        drop(lease);
        let job = get_job(&state, &job_id).expect("job");
        assert!(matches!(job.status, JobStatus::Error));
        assert!(!job.active);
        std::fs::remove_dir_all(&state.base_dir).ok();
    }

    // --- Cleanup ------------------------------------------------------------

    #[tokio::test]
    async fn cleanup_removes_expired_jobs_and_orphans_off_the_lock() {
        let state = test_state("cleanup");
        let expired = add_job(&state, JobStatus::Success);
        let fresh = add_job(&state, JobStatus::Error);
        edit_job(&state, &expired, |job| job.updated_at_millis = 0);
        let orphan = state.base_dir.join("orphan");
        std::fs::create_dir_all(&orphan).expect("orphan");

        // A 1 ms TTL makes the orphan (seconds-old mtime at worst) expire too.
        tokio::time::sleep(Duration::from_millis(5)).await;
        edit_job(&state, &fresh, |job| {
            job.updated_at_millis = current_millis()
        });
        cleanup_jobs(&state, 60_000).await;
        assert!(get_job(&state, &expired).is_none());
        assert!(!state.base_dir.join(&expired).exists());
        assert!(get_job(&state, &fresh).is_some());
        assert!(state.base_dir.join(&fresh).exists());
        assert!(orphan.exists(), "a fresh orphan is kept");

        cleanup_jobs(&state, 1).await;
        assert!(!orphan.exists());
        std::fs::remove_dir_all(&state.base_dir).ok();
    }
}
