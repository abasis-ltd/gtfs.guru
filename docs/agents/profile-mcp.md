# Feed Profiles, Explanations, and MCP

## Scope

- `gtfs-guru-profile` derives deterministic facts from a parsed `GtfsFeed` and
  its `NoticeContainer`.
- `gtfs-guru-mcp` exposes those facts through a read-only Model Context
  Protocol server using the official Rust SDK.
- Provider-specific LLM calls do not belong in either crate. The MCP host can
  paraphrase structured facts; calculations remain testable Rust code.

## FeedProfile Contract

- Increment `PROFILE_SCHEMA_VERSION` for breaking serialized-shape changes.
- Counts use unique non-empty entity IDs where applicable.
- The service horizon is seven dates beginning at `analysis_date`.
- Active service combines regular `calendar.txt` rows with additions/removals
  from `calendar_dates.txt`.
- Times remain GTFS service times and may exceed `24:00:00`.
- Notice totals come from exact `NoticeContainer` counters even when stored
  samples are capped.

## MCP Tools

- `validate_gtfs`: profile, exact grouped validation results, and up to three
  concrete examples per code/severity group. Examples carry the available
  file, row, field, message, context values, and suggested fix when one exists.
- `explain_gtfs`: the same validation payload plus a deterministic explanation.
- `get_notice_details`: canonical notice metadata and references.

The default is three returned examples per group. Operators can change it with
`--notice-examples-per-group`; `0` keeps only the exact grouped totals.

The stdio server never writes logs to stdout because that would corrupt MCP
JSON-RPC. Local paths are confined to roots configured with `--allow-dir`.
Network access is disabled unless `--allow-url` is supplied; enabled downloads
reject local/private/reserved addresses, credentials, oversized bodies, and
more than five redirects.

## Transports

The default stdio transport is intended for a local MCP host:

```bash
cargo run --release -p gtfs-guru-mcp -- \
  --allow-dir /path/to/feeds
```

Enable public feed URLs explicitly:

```bash
cargo run --release -p gtfs-guru-mcp -- \
  --allow-dir /path/to/feeds \
  --allow-url \
  --max-download-mib 512
```

Stateless Streamable HTTP is available for remote clients:

```bash
export GTFS_GURU_MCP_BEARER_TOKEN="$(openssl rand -hex 32)"
cargo run --release -p gtfs-guru-mcp -- \
  --transport http \
  --bind 127.0.0.1:3000 \
  --allow-dir /path/to/feeds \
  --allow-url
```

- `/mcp` requires an exact bearer token read from the configured environment
  variable. Tokens shorter than 32 bytes are rejected at startup.
- `/healthz` is intentionally unauthenticated and returns no feed information.
- Request bodies, rolling request rate, concurrent validations, download size,
  download duration, and redirects are bounded.
- The SDK's loopback Host allowlist remains active by default. A reverse proxy
  must pass its external hostname with `--allowed-host`; known browser origins
  can be restricted with `--allowed-origin`.
- Terminate TLS before exposing the endpoint to the internet. Validation is
  synchronous and stateless: this server does not provide a durable job queue
  or multi-tenant usage accounting.

## Verification

```bash
cargo test -p gtfs-guru-profile -p gtfs-guru-mcp
cargo clippy -p gtfs-guru-profile -p gtfs-guru-mcp --all-targets -- -D warnings
```
