# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- Match Java 8.0.1 when transfer and shape matching cannot find coordinates in
  a stop's parent chain; use the S2 point angle for transfer distances.
- Enforce the canonical CSV limit of 512 columns, including quoted fields,
  streamed input, and a final record without a newline. Accept Java floating
  point suffixes and hexadecimal values in both validation and deserialization.
- Remove leftover parser probe timers that compiled for WASM but panicked on
  the first validation (`std::time::Instant` is unsupported there).
- Emit unknown GeoJSON elements in a stable order and release threaded WASM
  validation results immediately, including when result delivery throws.
- Refresh the real-world baseline and remove six obsolete Java parity
  exceptions after canonical row rejection and dependency gating fixes.

## [1.0.0] - 2026-09-15

First stable release. The CLI, core, model, report, profile, MCP, web, WASM,
Python, and desktop crates share the 1.0.0 version and their public APIs are
now covered by semantic versioning.

### Added

- `gtfs-guru diff old.zip new.zip` compares files, feed metadata, agencies,
  routes, stops (including moves over 10 m), trip counts and frequency windows
  by route, and exact validation notice counts. It can emit JSON or Markdown
  and fail CI with `--fail-on-new-errors`.
- `--fail-on <none|error|warning>` for CI quality gates. Reports are written
  before the process exits with status `2`.
- `--badge` and `--badge-svg` write a feed status badge: a shields.io endpoint
  descriptor and a self-contained SVG. `--badge-label` replaces the default
  `GTFS` label. Both paths are used verbatim rather than joined to
  `--output_base`, and both work under `--stdout`.
- A composite GitHub Action in `action/`. It installs a checksum-verified
  release binary, validates a local or remote feed, uploads SARIF to code
  scanning, writes a job summary, publishes the counts as step outputs, and
  fails the job at the chosen severity.
- A one-click example feed on the website: a small two-route network carrying
  deliberate mistakes, rebuilt reproducibly by `scripts/build_demo_feed.py`.
- Shareable report links. The browser report is compressed into the URL
  fragment, so a link can be sent to a colleague or a vendor without the feed
  or its findings ever reaching a server. Long reports keep a sample of each
  issue type and exact per-rule counts.
- `llms.txt`, plus `SoftwareApplication` and `FAQPage` structured data on the
  home page, so AI assistants can answer questions about GTFS Guru from
  something better than a guess.
- `--fix` and `--fix-unsafe` write a repaired copy of the feed. `--fix-output`
  picks the destination, defaulting to `<input>.fixed.<ext>` beside the input.
  The input is never modified and an existing output path is refused. Only the
  CSV records carrying an edit are re-serialized, so line endings, quoting, a
  UTF-8 BOM, and every untouched file survive byte for byte. A fix whose target
  field no longer holds the expected value is reported and skipped rather than
  applied.
- Auto-fix suggestions for the defects with exactly one reading:
  `invalid_color` (`#FF0000` and the `0F0` shorthand), `invalid_date`
  (separator forms of `YYYYMMDD`), `invalid_time` (a missing `:SS`, an all-zero
  fraction), `invalid_url` (a missing scheme), and `invalid_email` (a `mailto:`
  prefix or angle brackets) are safe; a decimal comma in `invalid_float` and a
  redundant fraction in `invalid_integer` need `--fix-unsafe`. Every suggestion
  is re-checked against the validator that rejected the value, so applying one
  cannot introduce a new notice. Ambiguous input gets no suggestion: `01-05-2026`
  could be either day-first or month-first, and `1,500` could be 1.5 or 1500.
- Safe auto-fixes now trim declared GTFS fields and canonically order
  `stop_times.txt` by trip and `stop_sequence` while preserving each raw record.
  `--fix-unsafe` can delete child rows with missing foreign-key parents, with an
  expected-value guard that refuses stale plans. Every repaired feed is
  automatically validated again and reports resolved, remaining, and introduced
  notice totals.
- `gtfs_validator_core::fix` exposing `FixPlan` and `apply_fixes` for embedders.
- `gtfs-guru profile` and `gtfs-guru explain`, backed by the new
  `gtfs-guru-profile` crate. The profile is deterministic: entity counts, route
  types, completeness facts, seven actual service dates with calendar
  exceptions applied, and grouped validation issues. The explanation is derived
  from that same profile, so every statement can be checked without sending the
  feed to an LLM provider.
- `gtfs-guru-mcp`, a read-only MCP server exposing `validate_gtfs`,
  `explain_gtfs`, and `get_notice_details`. It speaks stdio for a local host and
  authenticated stateless Streamable HTTP for a remote one. Local reads are
  confined to the roots passed with `--allow-dir`; downloading a public URL is
  off until `--allow-url` is given. HTTP defaults to 60 authenticated requests
  per rolling minute, four concurrent validations, and 64 KiB request bodies.
  Validation and explanation responses include bounded concrete notice examples
  with available file, row, field, context, and suggested-fix details, so an MCP
  host can show actionable errors instead of only grouped counts.
- The browser validator now renders a local “What ChatGPT receives” preview
  from the real validation report, with exact totals and concrete error
  examples. It does not call an AI provider or upload the feed.
- `gtfs-guru --version`.
- `service_never_active`, which flags calendar.txt rows with no active weekday
  and no added dates in calendar_dates.txt.
- `feed_version` in the Feed Info section of the JSON and HTML reports.
- Multithreaded in-browser validation: a wasm-threads build backed by a rayon
  Web Worker pool, served to cross-origin-isolated pages with an automatic
  fallback to the single-threaded package.
- In-browser validation of large feeds, with the work moved into a Web Worker
  so an out-of-memory failure kills the worker instead of the tab, and a
  main-thread fallback when the worker cannot load.
- An interactive map showing the stops a notice refers to.
- Prebuilt Python wheels for Linux x86_64/aarch64, macOS x86_64/arm64, and
  Windows x64, plus an sdist and post-upload PyPI verification.
- A pre-publish Python wheel smoke test covering the synchronous and
  asynchronous APIs, progress callbacks, notice conversion, and report export.
- A security policy and operator guidance.
- A public compatibility page at <https://gtfs.guru/compatibility/>, generated and
  committed alongside the notice pages. It names the GTFS specification revision
  and the canonical validator release this build answers for, lists what changed
  upstream recently with a support status and a source for each, maps all 191
  notice codes to their canonical rule (or marks them as GTFS Guru additions or
  as deprecated upstream), states every accepted difference, and shows the
  per-file column coverage. Linked from every website page's footer, from the
  home page, from the documentation, and from the release notes.
- `crates/gtfs_validator_core/spec_changes.json`: the curated, versioned record
  of recent upstream changes and this build's support status for each, with
  `gtfs_guru_core::validate_spec_changes` checking every claim against the
  build's own spec surface and the baseline's accepted differences. A claim the
  build does not satisfy fails `cargo test`.
- Two CI gates against stale or self-contradictory metadata: `cargo run -p
  gtfs-guru-web --bin generate-notice-pages -- --check` now runs on every Rust
  CI build, so committed generated pages cannot fall behind the code; and
  `scripts/spec_watch.py check-coordination` fails a pull request that moves the
  baseline without moving the change log the page is built from.
- Spec Watch: a weekly CI run that compares the GTFS specification
  (`google/transit`) and the canonical validator's published rules against what
  this build supports, and opens or updates a single Linear issue when the two
  drift apart. It stays silent when nothing drifted. The accepted upstream state
  lives in `crates/gtfs_validator_core/spec_baseline.json`;
  `docs/spec-watch.md` documents the workflow and the protocol for moving it.
- `gtfs-guru spec-surface` emits the files, fields, enum domains, and notice
  codes this build supports as JSON, derived from the same tables validation
  uses.
- Every JSON report's `summary` now carries `specRevision` and
  `canonicalBaseline`, so a stored report states which specification revision and
  canonical validator release it was produced against.

- `scripts/mdb_parity.py`: the mdb-50 catalogue parity check. Fifty
  MobilityDatabase feeds pinned in `scripts/real_world/mdb50.json`, both
  validators on the same zips, a per-feed diff classified against
  `expected_deltas.json` and `mdb50_expected_deltas.json`. 46 of 50 feeds are
  exact and 50 of 50 exact-on-shared; see `docs/real-world-parity.md`.

### Changed

- Every crate now sets `#![forbid(unsafe_code)]`, except the CLI, which uses
  `#![deny(unsafe_code)]` with a single documented opt-out for the `getrusage`
  call behind its peak-memory reporting. Three hand-written `unsafe impl Send`
  / `Sync` blocks were removed: the types derived both automatically, so the
  impls only suppressed the compiler's check on data shared across rayon
  workers.
- The embedded website no longer serves dotfiles. `include_dir!` bakes in
  whatever is on disk at build time, so an untracked `.DS_Store` or editor swap
  file in `website/` could otherwise be fetched from a locally built server.
- `gtfs-guru-web` is no longer published to crates.io. It is the deployed server
  binary rather than a library, and it now embeds the repo-root `website/`
  directly, which `cargo package` cannot carry. The previously published 0.9.x
  versions stay available; anyone running the server should build it from this
  repository or use the Docker image.
- The website is stored once. `website/` is served by nginx and embedded into
  `gtfs-guru-web` from the same path, replacing the second copy under
  `crates/gtfs_validator_web/` that had to be kept in sync by hand.
- `sitemap.xml` no longer carries a `<lastmod>`. It was stamped with the time
  of the request, so every crawl saw the whole site as freshly modified —
  worse than omitting the field, which is what search engines fall back to.
- The comparison with the Java validator on the home page is a real `<table>`
  instead of a grid of `<div>`s, which is what "X vs Y" answers are extracted
  from.
- Notices are capped per (code, severity) group in memory while the reported
  totals stay exact, so feeds with pervasive issues no longer exhaust the
  browser heap. Reports mark a capped list as truncated.
- In-browser validation is gated on the uncompressed feed size declared in the
  ZIP central directory (150 MB compressed / 700 MB uncompressed) rather than
  the compressed size alone, which predicts peak memory poorly in both
  directions.
- Python bindings now use PyO3 0.29.
- The desktop app shares the workspace's `reqwest` 0.12 dependency instead of
  retaining a second 0.11 TLS stack.
- `[PERF]` diagnostics require `GTFS_PERF_DEBUG`.
- SARIF output identifies the tool and repository as GTFS Guru.
- Documentation reports the current 110 validators and 191 notice codes, and
  uses the measured 4.6–6.7x benchmark results.
- `docs/rules.md` is generated from the notice schema instead of being edited by
  hand, and the generator's `--check` mode fails CI when it drifts. That is what
  moved the documented total from 190 to 191: `leading_or_trailing_whitespaces`
  was already emitted but had never been listed.
- `inconsistent_route_type_for_block_id` and
  `inconsistent_route_type_for_in_seat_transfer` no longer compare extended
  route types. MobilityData's canonical typed route table rejects extended HVT
  values, so the validators that read that table never see them.
- `--threads` is documented as report metadata; actual parallelism is
  controlled with `RAYON_NUM_THREADS`.
- Parallel CSV loading is faster, and the hottest rules were optimized.
- Website polish: mobile navigation, scroll-reveal that degrades without
  JavaScript, clearer size-limit copy, and a cache-buster for site scripts.
- Tagging a release now rebuilds and deploys the website after the GitHub
  Release, crates.io, PyPI, and npm jobs succeed.
- Release binaries ship without debug symbols. Use the `profiling` profile
  (`cargo build --profile profiling`) when a flamegraph needs them.

### Fixed

- Per-validator timings are measured in the browser again. The parallel run
  path hardcoded a zero duration on `wasm32` even though the crate's `Instant`
  is `web_time::Instant`, which is backed by `performance.now()` there, so the
  multithreaded browser build reported every rule as taking no time at all.

- The browser's remote-feed fallback now uses the guarded Rust
  `/cors-proxy?url=...` endpoint instead of an unrestricted nginx forward
  proxy. It rejects private and reserved addresses on every redirect and
  limits request rate, concurrency, response size, and duration.
- IPv6 URL literals are classified directly, so public IPv6 hosts work while
  private and loopback literals remain blocked.
- Zip decompression is capped per member and across the archive.
- Negative `pathways.txt` stair counts are loaded as valid downward stairs
  instead of silently dropping the pathway and producing false reachability
  errors.
- The home page advertised `cargo install gtfs-guru-cli` and a `--json` flag.
  The crate is `gtfs-guru` and the flag is `--stdout`, so both copied commands
  failed for anyone who followed them.
- The home page no longer overflows horizontally on a 375 px viewport; the
  stats row, install commands, and footer links wrap instead. A Chromium check
  asserts the mobile page has no horizontal scroll.
- tzdb backward-compatibility link names (for example `Europe/Nicosia`,
  `US/Eastern`) are accepted, matching Java's `ZoneId`.
- Translation foreign-key lookups are indexed instead of scanned, which fixed a
  quadratic single-threaded hang on feeds with many translations.
- `duplicate_route_name` matches the canonical validator for extended route
  types.
- Service windows account for dates added through calendar_dates.txt.
- Placeholder notice codes from test modules no longer leak into the generated
  notice schema.
- Website container images remove deployment inputs such as `nginx.conf`,
  `Dockerfile`, Compose files, and `.env` from the public document root.
- Compose deployments require explicit PostgreSQL and Umami secrets.
- The Python package version is read from Cargo metadata, so a release can no
  longer publish wheels carrying the previous version.
- Updated `anyhow`, `bytes`, `crossbeam-epoch`, `plist`/`quick-xml`,
  `quinn-proto`, `rustls-webpki`, `tar`, `time`, and `wayland-scanner`.
- `stop_times.txt` references to `pickup_booking_rule_id` and
  `drop_off_booking_rule_id` are now checked when `booking_rules.txt` is
  absent, so every such reference is a `foreign_key_violation`, as the
  canonical validator reports. The check used to be skipped entirely without
  the file, which hid 50,631 dangling references on one real-world feed
  (GTF-33).
- `fare_products.txt` rows are keyed on the specification's primary key,
  `(fare_product_id, rider_category_id, fare_media_id)`, so a product sold to
  several rider categories is no longer a `duplicate_key`. Thorough mode keeps
  the stricter globally unique `fare_product_id` (GTF-34).
- `geo_json_duplicated_element` for repeated MultiPolygon geometries is now
  reported only under `--thorough`, matching the Polygon case. The canonical
  validator uses that code for duplicated JSON keys, so the default profile
  no longer emits it for geometry (GTF-34).
- The four remaining real-world parity differences on `mdb-3234` and
  `mdb-502` are recorded as reviewed in `scripts/real_world/expected_deltas.json`
  with the evidence for keeping each one (GTF-34, GTF-35).
- The demo feed archive is stored rather than deflated, so
  `scripts/build_demo_feed.py --check` agrees with the committed copy on every
  machine. Deflate output is only stable for a given zlib implementation, so a
  contributor whose Python links zlib-ng saw the committed archive as stale and a
  rebuild produced a diff CI rejected. The archive's contents are unchanged.
- `block_trips_with_overlapping_stop_times` now follows the canonical
  validator step for step: a trip's interval is its first and last stop_time
  by `stop_sequence` (both need arrival and departure), a pair whose boundary
  stop repeats the same arrival/departure is a block transfer and not an
  overlap, and two trips only overlap on a date both services are active on,
  with no shortcut for identical `service_id`s. `intersection` is that first
  shared date (`YYYYMMDD`) instead of a time range. Hyderabad (mdb-2457) loses
  793 false errors and SNCB (mdb-686) 177.
- `non_ascii_or_non_printable_char` no longer fires on the `_id` fields the
  canonical validator does not type as IDs: `frequencies.trip_id`,
  `translations.record_id` / `record_sub_id`, `timeframes.service_id`,
  `location_group_stops.location_group_id` / `stop_id`,
  `booking_rules.prior_notice_service_id` and `trips.direction_id`.
- The CSV reader applies univocity's field boundaries before parsing (GTF-35):
  whitespace around a bare field is dropped, a quote after that whitespace
  still opens the field, whitespace inside quotes is kept. `817, "DUS"` is now
  the value `DUS`, not `"DUS"` (Latvia, mdb-992: 28 false
  `mixed_case_recommended_field`), and `leading_or_trailing_whitespaces` is
  reported in default mode exactly when the canonical validator reports it,
  so it no longer needs `--thorough`.
- `mixed_case_recommended_field` tokenises like Java's `[^\p{L}]+`: combining
  vowel signs and tone marks (Thai, Lao, Indic scripts) split a word into
  several caseless tokens, lengths count UTF-16 units, and the notice is held
  back on rows whose fields failed to parse, as Java's entity validators never
  see those rows.
- Shape-to-stop matching uses an operation-for-operation port of
  `S2EdgeUtil.getClosestPoint`, `S2LatLng` and `S2LatLng.getDistance`, so
  near-ties on degenerate shapes (a polyline jittering between two points
  centimetres apart, or repeated points) resolve the way the canonical
  validator resolves them. Thailand (mdb-1831) `stop_has_too_many_matches_for_shape`
  goes from 57 to Java's 63 and `stops_match_shape_out_of_order` from 75 to 71
  against 72; the last pair differs in the final ulp of a trig call.

## [0.9.4] - 2026-02-05

- Release metadata and documentation update for 0.9.4.

[Unreleased]: https://github.com/abasis-ltd/gtfs.guru/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/abasis-ltd/gtfs.guru/compare/v0.9.4...v1.0.0
[0.9.4]: https://github.com/abasis-ltd/gtfs.guru/releases/tag/v0.9.4
