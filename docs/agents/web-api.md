# Web API (gtfs-guru-web)

## Scope

- Axum-based HTTP service that runs the validator and serves report artifacts.
- Listens on `0.0.0.0:3000` inside the process. Production Compose binds that
  port to `127.0.0.1` on the host and puts Caddy in front.

## Configuration

- `GTFS_VALIDATOR_WEB_BASE_DIR` sets the job workspace directory (default: `target/web_jobs`).
- `GTFS_VALIDATOR_WEB_PUBLIC_BASE_URL` sets the base URL used for upload/report links.
- `GTFS_VALIDATOR_WEB_MAX_UPLOAD_BYTES` caps streamed upload and URL-download size (default: 512 MiB).
- `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_JOBS` caps concurrent validations (default: 4).
- `GTFS_VALIDATOR_WEB_MAX_QUEUED_JOBS` sizes two separate caps (default: 64 each):
  jobs queued or running, and jobs awaiting an upload. A job awaiting its upload
  holds no admission permit, so the two are counted independently.
- `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_UPLOADS` caps concurrent upload streams (default: 4).
- `GTFS_VALIDATOR_WEB_MAX_CREATE_JOB_REQUESTS_PER_MINUTE` rate-limits `POST /create-job` (default: 60).
- `GTFS_VALIDATOR_WEB_PROCESSING_TIMEOUT_SECONDS` reclaims jobs stuck in `Processing` (default: 1800).
- `GTFS_VALIDATOR_WEB_PUBSUB_TOKEN` is required for `POST /run-validator`. Send it as
  `x-pubsub-token` or `Authorization: Bearer ...`. Unset or empty → 401.
- `GTFS_VALIDATOR_MAX_MEMBER_BYTES` / `GTFS_VALIDATOR_MAX_TOTAL_BYTES` cap zip
  inflation in the core loader (library defaults 4 GiB / 8 GiB). The 2 GiB
  Compose file lowers these.
- `GTFS_VALIDATOR_WEB_MAX_PROXY_BYTES` caps CORS-proxy responses (default: 70 MiB).
- `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_PROXY_REQUESTS` caps concurrent proxy fetches (default: 4).
- `GTFS_VALIDATOR_WEB_MAX_PROXY_REQUESTS_PER_MINUTE` applies a global proxy rate limit (default: 60).

## Core Endpoints

- `GET /healthz` returns `ok` for health checks.
- `GET /version` returns the running version.
- `GET /cors-proxy?url=<percent-encoded-url>` fetches a public HTTP(S) URL for the same-origin
  browser UI. Private/reserved addresses and cross-site browser requests are rejected.
- `POST /create-job` creates a job. Optional JSON body supports `countryCode` and `url`.
  Returns 429 when the create-job rate limit or pending-upload cap is hit.
- `PUT /upload/:job_id` streams a GTFS zip to disk. A `Content-Length` over the
  cap is refused with 413 before the job is claimed; the job is then claimed
  before the body is read, so a missing id returns 404 without buffering the
  upload. A body that exceeds the cap while streaming also returns 413.
- `POST /run-validator` is the optional Pub/Sub restart hook and requires
  `GTFS_VALIDATOR_WEB_PUBSUB_TOKEN`.
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
