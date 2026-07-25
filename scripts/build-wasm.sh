#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
WASM_CRATE="$PROJECT_ROOT/crates/gtfs_validator_wasm"
WASM_THREADS_TOOLCHAIN="${WASM_THREADS_TOOLCHAIN:-nightly-2026-03-01}"
WASM_OPT_LEVEL="${WASM_OPT_LEVEL:--Oz}"
WASM_MT_OPT_LEVEL="${WASM_MT_OPT_LEVEL:-$WASM_OPT_LEVEL}"
WASM_MT_SIMD="${WASM_MT_SIMD:-1}"

echo "Building GTFS Validator WASM..."

# Check for wasm-pack
if ! command -v wasm-pack &> /dev/null; then
    echo "wasm-pack not found. Installing..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# Build for web target (ES modules, used in browsers)
echo "Building for web target..."
wasm-pack build "$WASM_CRATE" --target web --release --out-dir pkg

# Build a separate browser package with shared memory and Rayon workers. Keep
# this opt-in so Node.js and npm consumers retain the portable single-threaded
# build by default.
echo "Building multi-threaded web target..."
rustup toolchain install "$WASM_THREADS_TOOLCHAIN" --profile minimal \
    --target wasm32-unknown-unknown --component rust-src
MT_TARGET_FEATURES="+atomics,+bulk-memory,+mutable-globals"
MT_WASM_OPT_FEATURES="--enable-threads"
if [ "$WASM_MT_SIMD" = "1" ]; then
    MT_TARGET_FEATURES="$MT_TARGET_FEATURES,+simd128"
    MT_WASM_OPT_FEATURES="$MT_WASM_OPT_FEATURES --enable-simd"
fi
# Keep the maximum one wasm page below 4 GiB. An exact 2^32 maximum wraps in
# some linker/toolchain combinations and can silently produce an unusable
# shared-memory declaration.
RUSTFLAGS="-C target-feature=$MT_TARGET_FEATURES -C link-arg=--shared-memory -C link-arg=--max-memory=4294901760 -C link-arg=--import-memory -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base" \
    rustup run "$WASM_THREADS_TOOLCHAIN" wasm-pack build "$WASM_CRATE" \
    --target web --release --out-dir pkg-mt \
    -- --features threads -Z build-std=panic_abort,std

# wasm-bindgen-rayon 1.3 still calls the deprecated two-argument initializer.
# Normalize its generated no-bundler helper to the current options object API.
MT_WORKER_HELPER="$(find "$WASM_CRATE/pkg-mt" -name 'workerHelpers.no-bundler.js' -print -quit)"
if [ -n "$MT_WORKER_HELPER" ]; then
    perl -pi -e 's/pkg\.default\(data\.module, data\.memory\)/pkg.default({ module_or_path: data.module, memory: data.memory })/' "$MT_WORKER_HELPER"
fi

# Build for Node.js target
echo "Building for Node.js target..."
wasm-pack build "$WASM_CRATE" --target nodejs --release --out-dir pkg-node

# Binaryen 108 (shipped by Ubuntu 24.04) can emit a multi-threaded module whose
# function table cannot grow during wasm-bindgen-rayon initialization. Refuse
# to run an optimizer version older than the one used by CI and releases.
BINARYEN_MIN_VERSION=131
BINARYEN_VERSION=""
if command -v wasm-opt &> /dev/null; then
    BINARYEN_VERSION="$(wasm-opt --version | grep -Eo '[0-9]+' | tail -n 1)"
fi

# Optimize with a known-good wasm-opt if available.
if [ -n "$BINARYEN_VERSION" ] && [ "$BINARYEN_VERSION" -ge "$BINARYEN_MIN_VERSION" ]; then
    echo "Optimizing WASM binary with wasm-opt..."
    WASM_OPT_FLAGS="$WASM_OPT_LEVEL --enable-bulk-memory --enable-nontrapping-float-to-int"
    WASM_MT_OPT_FLAGS="$WASM_MT_OPT_LEVEL --enable-bulk-memory --enable-nontrapping-float-to-int $MT_WASM_OPT_FEATURES"

    WEB_WASM="$(ls "$WASM_CRATE/pkg/"*_bg.wasm 2>/dev/null | head -n 1)"
    MT_WASM="$(ls "$WASM_CRATE/pkg-mt/"*_bg.wasm 2>/dev/null | head -n 1)"
    NODE_WASM="$(ls "$WASM_CRATE/pkg-node/"*_bg.wasm 2>/dev/null | head -n 1)"

    if [ -z "$WEB_WASM" ] || [ -z "$MT_WASM" ] || [ -z "$NODE_WASM" ]; then
        echo "Expected *_bg.wasm in pkg/, pkg-mt/, and pkg-node/ but did not find them."
        exit 1
    fi

    wasm-opt $WASM_OPT_FLAGS -o "${WEB_WASM}.opt" "$WEB_WASM"
    mv "${WEB_WASM}.opt" "$WEB_WASM"
    wasm-opt $WASM_MT_OPT_FLAGS -o "${MT_WASM}.opt" "$MT_WASM"
    mv "${MT_WASM}.opt" "$MT_WASM"
    wasm-opt $WASM_OPT_FLAGS -o "${NODE_WASM}.opt" "$NODE_WASM"
    mv "${NODE_WASM}.opt" "$NODE_WASM"

    # Report sizes
    WEB_SIZE=$(du -h "$WEB_WASM" | cut -f1)
    MT_SIZE=$(du -h "$MT_WASM" | cut -f1)
    NODE_SIZE=$(du -h "$NODE_WASM" | cut -f1)
    echo "Optimized sizes: web=$WEB_SIZE, web-mt=$MT_SIZE, node=$NODE_SIZE"
else
    if [ -n "$BINARYEN_VERSION" ]; then
        echo "wasm-opt $BINARYEN_VERSION is older than the required $BINARYEN_MIN_VERSION; skipping optimization."
    else
        echo "wasm-opt not found. Skipping optimization."
    fi
    echo "Install Binaryen $BINARYEN_MIN_VERSION or newer to enable optimization."
fi

# Copy additional files to pkg
echo "Copying additional files..."
cp "$WASM_CRATE/js/"*.js "$WASM_CRATE/pkg/" 2>/dev/null || true
cp "$WASM_CRATE/types/"*.d.ts "$WASM_CRATE/pkg/" 2>/dev/null || true
cp "$WASM_CRATE/js/worker-mt.js" "$WASM_CRATE/pkg-mt/worker-mt.js"

# Apply package.json template if exists
if [ -f "$WASM_CRATE/package.json.template" ]; then
    echo "Applying package.json template..."
    # Merge template with generated package.json
    VERSION=$(grep -o '"version": "[^"]*"' "$WASM_CRATE/pkg/package.json" | head -1 | cut -d'"' -f4)
    sed -E "s/\"version\": \"[^\"]+\"/\"version\": \"$VERSION\"/" "$WASM_CRATE/package.json.template" > "$WASM_CRATE/pkg/package.json.new"
    mv "$WASM_CRATE/pkg/package.json.new" "$WASM_CRATE/pkg/package.json"
fi

# Keep both checked-in website copies aligned with the generated browser
# packages. The Node.js package remains a library artifact only.
echo "Syncing browser packages to website copies..."
for WEBSITE_ROOT in "$PROJECT_ROOT/website" "$PROJECT_ROOT/crates/gtfs_validator_web/website"; do
    mkdir -p "$WEBSITE_ROOT/pkg" "$WEBSITE_ROOT/pkg-mt"
    cp -R "$WASM_CRATE/pkg/." "$WEBSITE_ROOT/pkg/"
    # Deliberately exclude wasm-pack's generated .gitignore: these static-site
    # artifacts must be visible to Git.
    cp -R "$WASM_CRATE/pkg-mt/"* "$WEBSITE_ROOT/pkg-mt/"
done

echo "Generating notice documentation..."
cargo run --quiet -p gtfs-guru-web --bin generate-notice-pages

echo ""
echo "Build complete!"
echo "Web package: $WASM_CRATE/pkg/"
echo "Multi-threaded web package: $WASM_CRATE/pkg-mt/"
echo "Node.js package: $WASM_CRATE/pkg-node/"
