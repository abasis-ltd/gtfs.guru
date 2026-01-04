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

# Optimize with wasm-opt if available
if command -v wasm-opt &> /dev/null; then
    echo "Optimizing WASM binary with wasm-opt..."
    WASM_OPT_FLAGS="-Oz --enable-bulk-memory --enable-nontrapping-float-to-int"
    wasm-opt $WASM_OPT_FLAGS -o "$WASM_CRATE/pkg/gtfs_validator_wasm_bg.wasm.opt" "$WASM_CRATE/pkg/gtfs_validator_wasm_bg.wasm"
    mv "$WASM_CRATE/pkg/gtfs_validator_wasm_bg.wasm.opt" "$WASM_CRATE/pkg/gtfs_validator_wasm_bg.wasm"
    wasm-opt $WASM_OPT_FLAGS -o "$WASM_CRATE/pkg-node/gtfs_validator_wasm_bg.wasm.opt" "$WASM_CRATE/pkg-node/gtfs_validator_wasm_bg.wasm"
    mv "$WASM_CRATE/pkg-node/gtfs_validator_wasm_bg.wasm.opt" "$WASM_CRATE/pkg-node/gtfs_validator_wasm_bg.wasm"

    # Report sizes
    WEB_SIZE=$(du -h "$WASM_CRATE/pkg/gtfs_validator_wasm_bg.wasm" | cut -f1)
    NODE_SIZE=$(du -h "$WASM_CRATE/pkg-node/gtfs_validator_wasm_bg.wasm" | cut -f1)
    echo "Optimized sizes: web=$WEB_SIZE, node=$NODE_SIZE"
else
    echo "wasm-opt not found. Skipping optimization."
    echo "Install binaryen to enable: brew install binaryen (macOS) or apt install binaryen (Linux)"
fi

# Copy additional files to pkg
echo "Copying additional files..."
cp "$WASM_CRATE/js/"*.js "$WASM_CRATE/pkg/" 2>/dev/null || true
cp "$WASM_CRATE/types/"*.d.ts "$WASM_CRATE/pkg/" 2>/dev/null || true

# Apply package.json template if exists
if [ -f "$WASM_CRATE/package.json.template" ]; then
    echo "Applying package.json template..."
    # Merge template with generated package.json
    VERSION=$(grep -o '"version": "[^"]*"' "$WASM_CRATE/pkg/package.json" | head -1 | cut -d'"' -f4)
    sed "s/\"version\": \"0.1.0\"/\"version\": \"$VERSION\"/" "$WASM_CRATE/package.json.template" > "$WASM_CRATE/pkg/package.json.new"
    mv "$WASM_CRATE/pkg/package.json.new" "$WASM_CRATE/pkg/package.json"
fi

echo ""
echo "Build complete!"
echo "Web package: $WASM_CRATE/pkg/"
echo "Node.js package: $WASM_CRATE/pkg-node/"
