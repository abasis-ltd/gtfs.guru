# Web API (gtfs-guru-web)

## Scope

- Axum-based HTTP service that runs the validator and serves report artifacts.
- Runs on `0.0.0.0:3000` by default.

## Configuration

- `GTFS_VALIDATOR_WEB_BASE_DIR` sets the job workspace directory (default: `target/web_jobs`).
- `GTFS_VALIDATOR_WEB_PUBLIC_BASE_URL` sets the base URL used for upload/report links.
- `GTFS_VALIDATOR_WEB_MAX_PROXY_BYTES` caps CORS-proxy responses (default: 70 MiB).
- `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_PROXY_REQUESTS` caps concurrent proxy fetches (default: 4).
- `GTFS_VALIDATOR_WEB_MAX_PROXY_REQUESTS_PER_MINUTE` applies a global proxy rate limit (default: 60).

## Core Endpoints

- `GET /healthz` returns `ok` for health checks.
- `GET /version` returns the running version.
- `GET /cors-proxy?url=<percent-encoded-url>` fetches a public HTTP(S) URL for the same-origin
  browser UI. Private/reserved addresses and cross-site browser requests are rejected.
- `POST /create-job` creates a job. Optional JSON body supports `countryCode` and `url`.
- `PUT /upload/:job_id` uploads a GTFS zip as raw bytes.
- `GET /jobs/:job_id/status` returns status and report URLs.
- `GET /jobs/:job_id/report.json`, `/report.html`, `/system_errors.json` return artifacts.

## Job Flow

1. `POST /create-job` to get a job id.
2. Upload a feed to `/upload/:job_id` (or provide a URL at job creation).
3. Poll `/jobs/:job_id/status` until `success`.
4. Fetch report artifacts from the job URLs.

## Local Run

```bash
cargo run --release -p gtfs-guru-web
```
