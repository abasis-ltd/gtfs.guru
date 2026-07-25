# WASM (gtfs-guru-wasm)

## Scope

- WebAssembly build of the core validator for browser and Node usage.
- The default package disables Rayon; the `threads` feature enables Rayon and
  `wasm-bindgen-rayon` in a separate browser build.

## Build Script

- Use `scripts/build-wasm.sh` to build all targets:
  - Web package: `crates/gtfs_validator_wasm/pkg/`
  - Multi-threaded web package: `crates/gtfs_validator_wasm/pkg-mt/`
  - Node package: `crates/gtfs_validator_wasm/pkg-node/`
- The script requires nightly Rust with the `rust-src` component for the
  multi-threaded build and syncs both browser packages into both website copies.
- The script installs `wasm-pack` if missing and runs `wasm-opt` when available.
- Extra JS and type definitions are copied from `crates/gtfs_validator_wasm/js/` and `types/`.

## Manual Build

```bash
# Web target
wasm-pack build crates/gtfs_validator_wasm --target web --release --out-dir pkg

# Multi-threaded web target (requires nightly + rust-src)
RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals,+simd128 -C link-arg=--shared-memory -C link-arg=--max-memory=4294901760 -C link-arg=--import-memory -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base' \
  rustup run nightly-2026-03-01 wasm-pack build crates/gtfs_validator_wasm --target web \
  --release --out-dir pkg-mt -- \
  --features threads -Z build-std=panic_abort,std

# Node target
wasm-pack build crates/gtfs_validator_wasm --target nodejs --release --out-dir pkg-node
```

## Size Optimization

- `scripts/build-wasm.sh` runs `wasm-opt -Oz` when `binaryen` is installed and
  adds `--enable-threads` for the multi-threaded binary.
- For size-sensitive builds, prefer this script and keep the optimized `.wasm` outputs.
- The multi-threaded tier enables `simd128` by default. Set `WASM_MT_SIMD=0`
  for an A/B build without SIMD. `WASM_OPT_LEVEL` and `WASM_MT_OPT_LEVEL`
  independently select the post-link optimization level (for example `-Oz`
  versus `-O3`) without changing the portable single-threaded fallback.

## Hosting requirements

The multi-threaded package needs `SharedArrayBuffer`, so the page must be
cross-origin isolated with these response headers:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

The worker selects `pkg-mt` at runtime only when isolation and shared memory are
available, caps the Rayon pool at eight threads, and otherwise falls back to
`pkg`. iOS always uses the single-threaded build because of its tighter WASM
memory limits. Hosts without the required response headers use the fallback;
Caddy, Nginx, and Cloudflare Pages configurations are included in the repository.

For golden comparisons or benchmarks on an isolated host, load
`pkg/worker.js?threads=off` to force the single-threaded fallback. This override
can only disable threads; it cannot bypass feature detection or isolation.

Panics in a Rayon worker abort the shared WASM instance; Rust's `catch_unwind`
cannot recover from `panic_abort` in this build. JavaScript must recreate the
outer validation worker after a worker-level failure.

Browser validation rejects archives above 150 MB compressed or 700 MB total
uncompressed size. The central-directory check happens before parsing so highly
compressed feeds fail recoverably instead of exhausting the wasm32 heap. CSV
deserialization remains sequential on WASM: browser benchmarks showed severe
contention for larger feeds, while validator execution still uses Rayon. ZIP
members are streamed directly into the sequential CSV reader, so the full
uncompressed `stop_times.txt` or `shapes.txt` is not buffered in WASM memory.

WASM retains at most 1,000 sample notices for each `(code, severity)` pair while
keeping exact totals. JSON samples expose `totalNotices`, and HTML/summary counts
use the exact totals. `ValidationResult.timings_json` reports aggregate feed
loading, parsing time for each GTFS table, index construction, and all
per-validator timings; workers return the parsed breakdown as `timings`.

CI builds through `scripts/build-wasm.sh` and runs
`scripts/test-wasm-browser.mjs` in Chromium. It requires the isolated host to
select the threaded runtime, compares its notices with the forced single-threaded
runtime, and verifies automatic fallback without COOP/COEP.
