#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
WASM_CRATE="$PROJECT_ROOT/crates/gtfs_validator_wasm"

echo "Building GTFS Validator WASM..."

# Check for wasm-pack
if ! command -v wasm-pack &> /dev/null; then
    echo "wasm-pack not found. Installing..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# Build for web target (ES modules, used in browsers)
echo "Building for web target..."
wasm-pack build "$WASM_CRATE" --target web --release --out-dir pkg

# Build for Node.js target
echo "Building for Node.js target..."
wasm-pack build "$WASM_CRATE" --target nodejs --release --out-dir pkg-node

# Build multithreaded web variant (wasm threads via wasm-bindgen-rayon).
#
# The rayon pool parallelizes CSV parsing and the validator run (~5x on
# 1M-row feeds vs the single-threaded worker). Requires a cross-origin-isolated
# page (COOP/COEP headers) — the site falls back to pkg/worker.js otherwise.
#
# Toolchain notes (hard-won):
#  * Needs nightly + rust-src for -Z build-std (std rebuilt with atomics).
#  * wasm-bindgen's thread transform needs shared IMPORTED memory and the TLS
#    symbols exported. rustc auto-configures all of this when you pass ONLY
#    the target features — but then the shared-memory maximum is left at the
#    1GB default, which large feeds exceed (grow fails -> a rayon worker traps
#    -> the join hangs). Overriding --max-memory disables rustc's auto set, so
#    we must then spell out the FULL flag set: --shared-memory --import-memory
#    --max-memory and the four TLS exports.
#  * --max-memory must be < 4GB exactly: 4294967296 wraps to 0 in the linker.
#    We use 4GB - 64KB (4294901760).
#  * parking_lot (under dashmap) needs its "nightly" feature on wasm+atomics,
#    or its fallback thread parker panics under lock contention and hangs the
#    pool. Wired through the wasm crate's `threads` feature.
#  * wasm-bindgen-rayon's workerHelpers.js does `import('../../..')` (a directory
#    import that only resolves under a bundler). We serve static files with no
#    bundler, so we rewrite it to the concrete module file after the build.
#  * nightly-2026-03-01 (rustc 1.96) + wasm-bindgen 0.2.126 is known good.
BUILT_MT=0
MT_TOOLCHAIN="${MT_TOOLCHAIN:-nightly-2026-03-01}"
echo "Building multithreaded (threads) web variant..."
if rustup toolchain list 2>/dev/null | grep -q "^${MT_TOOLCHAIN}"; then
    RUSTUP_TOOLCHAIN="$MT_TOOLCHAIN" \
    RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--shared-memory -C link-arg=--import-memory -C link-arg=--max-memory=4294901760 -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base" \
    wasm-pack build "$WASM_CRATE" --target web --release --out-dir pkg-mt \
        -- --features threads -Z build-std=std,panic_abort
    # Rewrite the no-bundler-incompatible directory import in the rayon glue.
    for WH in "$WASM_CRATE"/pkg-mt/snippets/wasm-bindgen-rayon-*/src/workerHelpers.js; do
        [ -f "$WH" ] && sed -i.bak "s#await import('../../..')#await import('../../../gtfs_guru_wasm.js')#" "$WH" && rm -f "$WH.bak"
    done
    BUILT_MT=1
else
    echo "  toolchain '$MT_TOOLCHAIN' not found — skipping multithreaded build."
    echo "  Enable with: rustup toolchain install $MT_TOOLCHAIN --component rust-src"
fi

# Optimize with wasm-opt if available
if command -v wasm-opt &> /dev/null; then
    echo "Optimizing WASM binary with wasm-opt..."
    WASM_OPT_FLAGS="-Oz --enable-bulk-memory --enable-nontrapping-float-to-int"

    WEB_WASM="$(ls "$WASM_CRATE/pkg/"*_bg.wasm 2>/dev/null | head -n 1)"
    NODE_WASM="$(ls "$WASM_CRATE/pkg-node/"*_bg.wasm 2>/dev/null | head -n 1)"

    if [ -z "$WEB_WASM" ] || [ -z "$NODE_WASM" ]; then
        echo "Expected *_bg.wasm in pkg/ and pkg-node/ but did not find them."
        exit 1
    fi

    wasm-opt $WASM_OPT_FLAGS -o "${WEB_WASM}.opt" "$WEB_WASM"
    mv "${WEB_WASM}.opt" "$WEB_WASM"
    wasm-opt $WASM_OPT_FLAGS -o "${NODE_WASM}.opt" "$NODE_WASM"
    mv "${NODE_WASM}.opt" "$NODE_WASM"

    # Report sizes
    WEB_SIZE=$(du -h "$WEB_WASM" | cut -f1)
    NODE_SIZE=$(du -h "$NODE_WASM" | cut -f1)
    echo "Optimized sizes: web=$WEB_SIZE, node=$NODE_SIZE"

    # The multithreaded binary must keep its atomics + shared memory, so it
    # needs --enable-threads (and a less aggressive -O2 to be safe).
    if [ "$BUILT_MT" = "1" ]; then
        MT_WASM="$(ls "$WASM_CRATE/pkg-mt/"*_bg.wasm 2>/dev/null | head -n 1)"
        if [ -n "$MT_WASM" ]; then
            wasm-opt -O2 --enable-threads --enable-bulk-memory --enable-nontrapping-float-to-int \
                -o "${MT_WASM}.opt" "$MT_WASM"
            mv "${MT_WASM}.opt" "$MT_WASM"
            MT_SIZE=$(du -h "$MT_WASM" | cut -f1)
            echo "Optimized sizes: web-mt=$MT_SIZE"
        fi
    fi
else
    echo "wasm-opt not found. Skipping optimization."
    echo "Install binaryen to enable: brew install binaryen (macOS) or apt install binaryen (Linux)"
fi

# Copy additional files to pkg (single-threaded worker + main-thread wrapper).
echo "Copying additional files..."
cp "$WASM_CRATE/js/worker.js" "$WASM_CRATE/js/index.js" "$WASM_CRATE/pkg/" 2>/dev/null || true
cp "$WASM_CRATE/types/"*.d.ts "$WASM_CRATE/pkg/" 2>/dev/null || true

# The multithreaded worker only makes sense next to the pkg-mt module.
if [ "$BUILT_MT" = "1" ]; then
    cp "$WASM_CRATE/js/worker-mt.js" "$WASM_CRATE/pkg-mt/" 2>/dev/null || true
    # wasm-pack drops a `*` .gitignore into the out-dir, which would make any
    # website copy of pkg-mt ignore itself. Remove it.
    rm -f "$WASM_CRATE/pkg-mt/.gitignore"
fi

# Apply package.json template if exists
if [ -f "$WASM_CRATE/package.json.template" ]; then
    echo "Applying package.json template..."
    # Merge template with generated package.json
    VERSION=$(grep -o '"version": "[^"]*"' "$WASM_CRATE/pkg/package.json" | head -1 | cut -d'"' -f4)
    sed -E "s/\"version\": \"[^\"]+\"/\"version\": \"$VERSION\"/" "$WASM_CRATE/package.json.template" > "$WASM_CRATE/pkg/package.json.new"
    mv "$WASM_CRATE/pkg/package.json.new" "$WASM_CRATE/pkg/package.json"
fi

echo ""
echo "Build complete!"
echo "Web package: $WASM_CRATE/pkg/"
echo "Node.js package: $WASM_CRATE/pkg-node/"
if [ "$BUILT_MT" = "1" ]; then
    echo "Web package (multithreaded): $WASM_CRATE/pkg-mt/"
fi
