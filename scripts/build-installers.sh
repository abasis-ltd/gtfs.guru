#!/bin/bash
# Build GTFS Validator installers for the current platform
# 
# Usage:
#   ./scripts/build-installers.sh          # Build for current platform
#   ./scripts/build-installers.sh debug    # Build debug version
#   ./scripts/build-installers.sh release  # Build release version (default)
#
# Output:
#   macOS:   target/release/bundle/dmg/*.dmg
#   Windows: target/release/bundle/msi/*.msi, target/release/bundle/nsis/*.exe
#   Linux:   target/release/bundle/deb/*.deb, target/release/bundle/appimage/*.AppImage

set -e

BUILD_TYPE="${1:-release}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT/crates/gtfs_validator_gui"

echo "🔧 Building GTFS Validator installers..."
echo "   Platform: $(uname -s)"
echo "   Build type: $BUILD_TYPE"
echo ""

# Check if Tauri CLI is installed
if ! command -v cargo-tauri &> /dev/null; then
    echo "📦 Installing Tauri CLI..."
    cargo install tauri-cli --version "^2"
fi

# Build based on type
if [ "$BUILD_TYPE" = "debug" ]; then
    cargo tauri build --debug
else
    cargo tauri build
fi

echo ""
echo "✅ Build complete! Installers are in:"
echo "   $PROJECT_ROOT/target/release/bundle/"
echo ""

# List generated bundles
if [ -d "$PROJECT_ROOT/target/release/bundle" ]; then
    echo "📦 Generated installers:"
    find "$PROJECT_ROOT/target/release/bundle" -name "*.dmg" -o -name "*.app" -o -name "*.deb" -o -name "*.AppImage" -o -name "*.msi" -o -name "*.exe" 2>/dev/null | head -20
fi
