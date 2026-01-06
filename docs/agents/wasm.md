# WASM (gtfs-guru-wasm)

## Scope

- WebAssembly build of the core validator for browser and Node usage.
- Core dependency disables rayon for WASM (`default-features = false`).

## Build Script

- Use `scripts/build-wasm.sh` to build both targets:
  - Web package: `crates/gtfs_validator_wasm/pkg/`
  - Node package: `crates/gtfs_validator_wasm/pkg-node/`
- The script installs `wasm-pack` if missing and runs `wasm-opt` when available.
- Extra JS and type definitions are copied from `crates/gtfs_validator_wasm/js/` and `types/`.

## Manual Build

```bash
# Web target
wasm-pack build crates/gtfs_validator_wasm --target web --release --out-dir pkg

# Node target
wasm-pack build crates/gtfs_validator_wasm --target nodejs --release --out-dir pkg-node
```

## Size Optimization

- `scripts/build-wasm.sh` runs `wasm-opt -Oz` when `binaryen` is installed.
- For size-sensitive builds, prefer this script and keep the optimized `.wasm` outputs.
