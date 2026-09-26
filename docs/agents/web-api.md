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
- `GTFS_VALIDATOR_WEB_PENDING_UPLOAD_TTL_SECONDS` drops a job that has waited this
  long since creation without an upload (default: 900), freeing its pending slot.
  Applied by the cleanup sweep (every 60 s) and on startup.
- `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_UPLOADS` caps concurrent upload streams (default: 4).
- `GTFS_VALIDATOR_WEB_MAX_CREATE_JOB_REQUESTS_PER_MINUTE` rate-limits `POST /create-job`
  globally (default: 60); `..._PER_MINUTE_PER_CLIENT` limits one client (default: 10).
  The per-client check runs first, so requests it refuses do not use up the global budget.
- `GTFS_VALIDATOR_WEB_TRUSTED_PROXIES` lists peers (addresses or CIDRs, comma-separated)
  whose `X-Forwarded-For` identifies the client for the per-client limits (default:
  `127.0.0.0/8,::1,172.16.0.0/12,192.168.0.0/16`). The Docker ranges are in the
  default because the production container is reached through docker-proxy, which
  shows up as the bridge gateway rather than loopback. From any other peer the
  header is ignored and the TCP peer address is used. The header is read right to
  left, skipping trusted hops. IPv6 clients are keyed by their /64. An empty value
  trusts nobody. Narrow it if the port is reachable directly from a LAN in those ranges.
- `GTFS_VALIDATOR_WEB_PROCESSING_TIMEOUT_SECONDS` (default: 1800) marks a validation
  that has run this long as failed (`validation timed out after N seconds`). It is
  measured from when validation starts, so time spent queueing for a run permit or
  uploading does not count. A timed-out job is not deleted while its worker is
  still running, since the worker cannot be cancelled and may still be writing. It
  cannot be re-uploaded either (409). Cleanup reclaims it by the job TTL once the
  worker exits, and the worker's late result does not overwrite the timeout.
- `GTFS_VALIDATOR_WEB_JOB_TTL_SECONDS` removes finished jobs after this long (default:
  86400; 0 keeps them forever).
- `GTFS_VALIDATOR_WEB_UPLOAD_IDLE_TIMEOUT_SECONDS` abandons an upload that stalls
  between body chunks (default: 60). Without it a half-finished `PUT /upload/:id`
  holds its upload and admission permits until the client disconnects.
- `GTFS_VALIDATOR_WEB_UPLOAD_TIMEOUT_SECONDS` caps the whole upload (default: 1800).
- `GTFS_VALIDATOR_WEB_PUBSUB_TOKEN` is required for `POST /run-validator`. Send it as
  `x-pubsub-token` or `Authorization: Bearer ...`. Unset or empty → 401.
- `GTFS_VALIDATOR_MAX_MEMBER_BYTES` / `GTFS_VALIDATOR_MAX_TOTAL_BYTES` cap zip
  inflation in the core loader (library defaults 4 GiB / 8 GiB). The 2 GiB
  Compose file lowers these.
- `GTFS_VALIDATOR_WEB_MAX_PROXY_BYTES` caps CORS-proxy responses (default: 70 MiB).
- `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_PROXY_REQUESTS` caps concurrent proxy fetches (default: 4).
- `GTFS_VALIDATOR_WEB_MAX_PROXY_REQUESTS_PER_MINUTE` applies a global proxy rate limit (default: 60);
  `..._PER_MINUTE_PER_CLIENT` limits one client (default: 10).
- `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_PROXY_REQUESTS_PER_CLIENT` caps one client's
  in-flight proxy fetches (default: 2), so one client cannot hold every proxy permit.
- `GTFS_VALIDATOR_WEB_PROXY_TIMEOUT_SECONDS` caps one proxy fetch end to end (default:
  120). It is enforced between body reads. Each read is also bounded to 30 s, so a
  fetch ends by about 150 s at worst and then returns 504. Server-side job downloads
  (`create-job` with `url`) have a fixed 600 s overall and 60 s per-read limit.

## Core Endpoints

- `GET /healthz` returns `ok` for health checks.
- `GET /version` returns the running version.
- `GET /cors-proxy?url=<percent-encoded-url>` fetches a public HTTP(S) URL for the same-origin
  browser UI. Private/reserved addresses and cross-site browser requests are rejected.
- `POST /create-job` creates a job. Optional JSON body supports `countryCode` and `url`.
  An empty body is accepted. A non-empty body must be sent as `application/json`
  (otherwise 415), and malformed JSON returns 400. A browser request with a
  cross-site `Sec-Fetch-Site` gets 403, because a page on another site could
  otherwise create jobs from its visitors' browsers without a CORS preflight.
  Returns 429 when the create-job rate limit or pending-upload cap is hit.
- `PUT /upload/:job_id` streams a GTFS zip to disk. A `Content-Length` over the
  cap is refused with 413 before the job is claimed; the job is then claimed
  before the body is read, so a missing id returns 404 without buffering the
  upload. A body that exceeds the cap while streaming also returns 413. If the
  client disconnects mid-upload the job goes back to `awaiting_upload` and the
  partial file is removed, so the same id can be uploaded again.
- `POST /run-validator` is the optional Pub/Sub restart hook and requires
  `GTFS_VALIDATOR_WEB_PUBSUB_TOKEN`.
- `GET /jobs/:job_id/status` returns status and report URLs.
- `GET /jobs/:job_id/report.json`, `/report.html`, `/system_errors.json` return artifacts.

## Job Flow

1. `POST /create-job` to get a job id.
2. Upload a feed to `/upload/:job_id` (or provide a URL at job creation).
3. Poll `/jobs/:job_id/status` until `success`.
4. Fetch report artifacts from the job URLs.

## Job Lifecycle and Disk

- `input.zip` is deleted as soon as validation finishes, whether it succeeded or
  failed. Only the reports under `output/` are kept for the job TTL. A retry
  (re-upload, or the Pub/Sub hook) needs a fresh upload, and the hook reports
  `missing input` for a job whose archive is gone.
- On startup, a job left `processing` by a restart is resolved. If its upload never
  completed it goes back to `awaiting_upload`. Otherwise it becomes `error`
  (`interrupted by restart`). Either way its `input.zip` is removed. Pending jobs
  past their TTL are dropped.
- There is no cap on the total bytes in `GTFS_VALIDATOR_WEB_BASE_DIR`. Disk use is
  bounded by the caps above: pending jobs hold no archive, at most
  `MAX_QUEUED_JOBS` archives of up to `MAX_UPLOAD_BYTES` are in flight, and
  finished jobs keep only their reports until the TTL.
- The cleanup sweep decides under the jobs lock from in-memory state only, then
  writes metadata and removes directories on the blocking pool.

## Local Run

```bash
cargo run --release -p gtfs-guru-web
```
