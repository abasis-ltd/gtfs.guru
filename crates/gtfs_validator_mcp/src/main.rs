#![forbid(unsafe_code)]
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use clap::{Parser, ValueEnum};
use gtfs_guru_mcp::{GtfsGuruMcp, McpConfig};
use rmcp::{
    transport::{
        stdio,
        streamable_http_server::{
            session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
        },
    },
    ServiceExt,
};
use tower_http::limit::RequestBodyLimitLayer;

#[derive(Debug, Parser)]
#[command(name = "gtfs-guru-mcp")]
#[command(about = "Read-only validation-focused MCP server for GTFS Guru")]
#[command(version)]
struct Args {
    /// MCP transport. HTTP listens on --bind and requires a bearer token.
    #[arg(long, value_enum, default_value_t = Transport::Stdio)]
    transport: Transport,

    /// Directory an MCP client may read. Repeat to allow multiple roots.
    /// Defaults to the process working directory.
    #[arg(long = "allow-dir", value_name = "PATH")]
    allowed_roots: Vec<PathBuf>,

    /// Permit agent-requested public HTTP(S) feed downloads.
    #[arg(long)]
    allow_url: bool,

    /// Maximum downloaded feed size in MiB.
    #[arg(long, default_value_t = 512)]
    max_download_mib: usize,

    /// Maximum number of feeds validated concurrently.
    #[arg(long, default_value_t = 4)]
    max_concurrent_validations: usize,

    /// Maximum stored notice samples per code/severity group. Totals stay exact.
    #[arg(long, default_value_t = 100)]
    notice_samples_per_group: usize,

    /// Maximum concrete notice examples returned per code/severity group.
    #[arg(long, default_value_t = 3)]
    notice_examples_per_group: usize,

    /// Address for Streamable HTTP transport.
    #[arg(long, default_value = "127.0.0.1:3000")]
    bind: SocketAddr,

    /// Environment variable containing the HTTP bearer token (minimum 32 bytes).
    #[arg(long, default_value = "GTFS_GURU_MCP_BEARER_TOKEN")]
    bearer_token_env: String,

    /// Accepted Host header. Repeat for reverse-proxy/public hostnames.
    /// Defaults to the MCP SDK's loopback-only allowlist.
    #[arg(long = "allowed-host", value_name = "HOST")]
    allowed_hosts: Vec<String>,

    /// Accepted browser Origin. Repeat to enable Origin validation.
    #[arg(long = "allowed-origin", value_name = "ORIGIN")]
    allowed_origins: Vec<String>,

    /// Maximum authenticated HTTP requests per rolling minute.
    #[arg(long, default_value_t = 60)]
    requests_per_minute: usize,

    /// Maximum MCP HTTP JSON request size in KiB.
    #[arg(long, default_value_t = 64)]
    max_request_kib: usize,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Transport {
    Stdio,
    Http,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let roots = if args.allowed_roots.is_empty() {
        vec![std::env::current_dir().context("resolve current directory")?]
    } else {
        args.allowed_roots.clone()
    };
    let mut config = McpConfig::local(roots)?;
    config.allow_urls = args.allow_url;
    config.max_download_bytes = args
        .max_download_mib
        .checked_mul(1024 * 1024)
        .context("--max-download-mib is too large")?;
    if args.max_concurrent_validations == 0 {
        bail!("--max-concurrent-validations must be greater than zero");
    }
    config.max_concurrent_validations = args.max_concurrent_validations;
    config.notice_samples_per_group = args.notice_samples_per_group;
    config.notice_examples_per_group = args.notice_examples_per_group;

    match args.transport {
        Transport::Stdio => run_stdio(config).await,
        Transport::Http => run_http(config, &args).await,
    }
}

async fn run_stdio(config: McpConfig) -> anyhow::Result<()> {
    let server = GtfsGuruMcp::new(config).serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}

async fn run_http(config: McpConfig, args: &Args) -> anyhow::Result<()> {
    if args.requests_per_minute == 0 {
        bail!("--requests-per-minute must be greater than zero");
    }
    let max_request_bytes = args
        .max_request_kib
        .checked_mul(1024)
        .context("--max-request-kib is too large")?;
    if max_request_bytes == 0 {
        bail!("--max-request-kib must be greater than zero");
    }

    let bearer_token = std::env::var(&args.bearer_token_env)
        .with_context(|| format!("read bearer token from {}", args.bearer_token_env))?;
    if bearer_token.len() < 32 {
        bail!(
            "bearer token in {} must contain at least 32 bytes",
            args.bearer_token_env
        );
    }

    let handler = GtfsGuruMcp::new(config);
    let mut transport_config = StreamableHttpServerConfig::default()
        .with_stateful_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None);
    if !args.allowed_hosts.is_empty() {
        transport_config = transport_config.with_allowed_hosts(args.allowed_hosts.iter().cloned());
    }
    if !args.allowed_origins.is_empty() {
        transport_config =
            transport_config.with_allowed_origins(args.allowed_origins.iter().cloned());
    }
    let cancellation = transport_config.cancellation_token.clone();
    let service: StreamableHttpService<GtfsGuruMcp, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok::<_, std::io::Error>(handler.clone()),
            Arc::new(LocalSessionManager::default()),
            transport_config,
        );

    let security = Arc::new(HttpSecurity {
        bearer_token: bearer_token.into_bytes(),
        rate_limiter: RollingRateLimiter::new(args.requests_per_minute),
    });
    let mcp = Router::new()
        .nest_service("/mcp", service)
        .layer(RequestBodyLimitLayer::new(max_request_bytes))
        .layer(middleware::from_fn_with_state(security, authorize_http));
    let app = Router::new().route("/healthz", get(health)).merge(mcp);
    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("bind Streamable HTTP server to {}", args.bind))?;
    eprintln!(
        "gtfs-guru-mcp Streamable HTTP listening on http://{}/mcp",
        listener.local_addr()?
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await
        .context("serve Streamable HTTP")
}

async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

struct HttpSecurity {
    bearer_token: Vec<u8>,
    rate_limiter: RollingRateLimiter,
}

async fn authorize_http(
    State(security): State<Arc<HttpSecurity>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| constant_time_eq(token.as_bytes(), &security.bearer_token));
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
            "unauthorized",
        )
            .into_response();
    }
    if !security.rate_limiter.allow(Instant::now()) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, HeaderValue::from_static("60"))],
            "rate limit exceeded",
        )
            .into_response();
    }
    next.run(request).await
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut difference = left.len() ^ right.len();
    for index in 0..max_len {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

struct RollingRateLimiter {
    limit: usize,
    requests: Mutex<VecDeque<Instant>>,
}

impl RollingRateLimiter {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            requests: Mutex::new(VecDeque::new()),
        }
    }

    fn allow(&self, now: Instant) -> bool {
        let mut requests = self
            .requests
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while requests
            .front()
            .is_some_and(|request| now.duration_since(*request) >= Duration::from_secs(60))
        {
            requests.pop_front();
        }
        if requests.len() >= self.limit {
            return false;
        }
        requests.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_tokens_require_exact_match() {
        assert!(constant_time_eq(b"0123456789", b"0123456789"));
        assert!(!constant_time_eq(b"0123456789", b"0123456788"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn rolling_rate_limit_releases_expired_requests() {
        let limiter = RollingRateLimiter::new(2);
        let start = Instant::now();
        assert!(limiter.allow(start));
        assert!(limiter.allow(start + Duration::from_secs(1)));
        assert!(!limiter.allow(start + Duration::from_secs(2)));
        assert!(limiter.allow(start + Duration::from_secs(60)));
    }
}
