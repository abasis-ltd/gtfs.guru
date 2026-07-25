# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| Latest release | Yes |
| Older releases | No; please upgrade |

## Reporting a vulnerability

Please report security issues privately, not in a public issue:

- Open a [GitHub security advisory](https://github.com/abasis-ltd/gtfs.guru/security/advisories/new) (preferred), or
- Email **gtfs@abasis.ai**.

Include the affected component, the impact, and reproduction steps or a feed
that demonstrates the issue. We aim to acknowledge reports within three working
days.

## Scope

In scope:

- Parsing untrusted feeds, including crashes, path traversal, and unbounded
  memory or CPU use.
- Unauthenticated `gtfs-guru-web` request handling and job isolation.
- Server-side fetching of user-controlled URLs (`--url` and `/cors-proxy`).
  These paths must not reach loopback, private, link-local, or otherwise
  reserved addresses.
- The desktop application's update channel.

Out of scope:

- Findings that require an already compromised host or malicious local user.
- Incorrect validation results for otherwise valid feeds; please report those
  as normal bugs.
- Resource exhaustion caused only by feeds larger than an operator's configured
  limits.

## Notes for operators

- Do not add an unrestricted forwarding proxy in front of the service. Keep
  browser remote-feed requests on the guarded `/cors-proxy?url=...` endpoint.
- Configure limits for your hardware:
  `GTFS_VALIDATOR_WEB_MAX_UPLOAD_BYTES`,
  `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_JOBS`,
  `GTFS_VALIDATOR_WEB_MAX_QUEUED_JOBS`,
  `GTFS_VALIDATOR_WEB_MAX_PROXY_BYTES`,
  `GTFS_VALIDATOR_WEB_MAX_CONCURRENT_PROXY_REQUESTS`, and
  `GTFS_VALIDATOR_WEB_MAX_PROXY_REQUESTS_PER_MINUTE`.
- Set `POSTGRES_PASSWORD` and `UMAMI_SECRET`; Compose refuses to start without
  them.
- Uploaded feeds are stored under `GTFS_VALIDATOR_WEB_BASE_DIR`.
  `GTFS_VALIDATOR_WEB_JOB_TTL_SECONDS` controls retention.
