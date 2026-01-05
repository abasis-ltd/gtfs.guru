#!/bin/bash
# Generate all icons from the source logo

set -e

SOURCE_ICON="$1"
if [ -z "$SOURCE_ICON" ]; then
    echo "Usage: $0 <source_icon.png>"
    exit 1
fi

ICONS_DIR="crates/gtfs_validator_gui/icons"

# Create directories
mkdir -p "$ICONS_DIR/ios"
mkdir -p "$ICONS_DIR/android/mipmap-hdpi"
mkdir -p "$ICONS_DIR/android/mipmap-mdpi"
mkdir -p "$ICONS_DIR/android/mipmap-xhdpi"
mkdir -p "$ICONS_DIR/android/mipmap-xxhdpi"
mkdir -p "$ICONS_DIR/android/mipmap-xxxhdpi"

# Main icons (Tauri)
sips -z 32 32 "$SOURCE_ICON" --out "$ICONS_DIR/32x32.png"
sips -z 64 64 "$SOURCE_ICON" --out "$ICONS_DIR/64x64.png"
sips -z 128 128 "$SOURCE_ICON" --out "$ICONS_DIR/128x128.png"
sips -z 256 256 "$SOURCE_ICON" --out "$ICONS_DIR/128x128@2x.png"
sips -z 512 512 "$SOURCE_ICON" --out "$ICONS_DIR/icon.png"

# Windows Store logos
sips -z 30 30 "$SOURCE_ICON" --out "$ICONS_DIR/Square30x30Logo.png"
sips -z 44 44 "$SOURCE_ICON" --out "$ICONS_DIR/Square44x44Logo.png"
sips -z 71 71 "$SOURCE_ICON" --out "$ICONS_DIR/Square71x71Logo.png"
sips -z 89 89 "$SOURCE_ICON" --out "$ICONS_DIR/Square89x89Logo.png"
sips -z 107 107 "$SOURCE_ICON" --out "$ICONS_DIR/Square107x107Logo.png"
sips -z 142 142 "$SOURCE_ICON" --out "$ICONS_DIR/Square142x142Logo.png"
sips -z 150 150 "$SOURCE_ICON" --out "$ICONS_DIR/Square150x150Logo.png"
sips -z 284 284 "$SOURCE_ICON" --out "$ICONS_DIR/Square284x284Logo.png"
sips -z 310 310 "$SOURCE_ICON" --out "$ICONS_DIR/Square310x310Logo.png"
sips -z 50 50 "$SOURCE_ICON" --out "$ICONS_DIR/StoreLogo.png"

# iOS icons
sips -z 20 20 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-20x20@1x.png"
sips -z 40 40 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-20x20@2x.png"
sips -z 40 40 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-20x20@2x-1.png"
sips -z 60 60 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-20x20@3x.png"
sips -z 29 29 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-29x29@1x.png"
sips -z 58 58 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-29x29@2x.png"
sips -z 58 58 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-29x29@2x-1.png"
sips -z 87 87 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-29x29@3x.png"
sips -z 40 40 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-40x40@1x.png"
sips -z 80 80 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-40x40@2x.png"
sips -z 80 80 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-40x40@2x-1.png"
sips -z 120 120 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-40x40@3x.png"
sips -z 120 120 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-60x60@2x.png"
sips -z 180 180 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-60x60@3x.png"
sips -z 76 76 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-76x76@1x.png"
sips -z 152 152 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-76x76@2x.png"
sips -z 167 167 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-83.5x83.5@2x.png"
sips -z 1024 1024 "$SOURCE_ICON" --out "$ICONS_DIR/ios/AppIcon-512@2x.png"

# Android icons
sips -z 72 72 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-hdpi/ic_launcher.png"
sips -z 162 162 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-hdpi/ic_launcher_foreground.png"
sips -z 72 72 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-hdpi/ic_launcher_round.png"

sips -z 48 48 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-mdpi/ic_launcher.png"
sips -z 108 108 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-mdpi/ic_launcher_foreground.png"
sips -z 48 48 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-mdpi/ic_launcher_round.png"

sips -z 96 96 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xhdpi/ic_launcher.png"
sips -z 216 216 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xhdpi/ic_launcher_foreground.png"
sips -z 96 96 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xhdpi/ic_launcher_round.png"

sips -z 144 144 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xxhdpi/ic_launcher.png"
sips -z 324 324 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xxhdpi/ic_launcher_foreground.png"
sips -z 144 144 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xxhdpi/ic_launcher_round.png"

sips -z 192 192 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xxxhdpi/ic_launcher.png"
sips -z 432 432 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xxxhdpi/ic_launcher_foreground.png"
sips -z 192 192 "$SOURCE_ICON" --out "$ICONS_DIR/android/mipmap-xxxhdpi/ic_launcher_round.png"

# Generate .icns for macOS using iconutil
ICONSET_DIR="/tmp/app_icon.iconset"
rm -rf "$ICONSET_DIR"
mkdir -p "$ICONSET_DIR"

sips -z 16 16 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_16x16.png"
sips -z 32 32 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_16x16@2x.png"
sips -z 32 32 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_32x32.png"
sips -z 64 64 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_32x32@2x.png"
sips -z 128 128 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_128x128.png"
sips -z 256 256 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_128x128@2x.png"
sips -z 256 256 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_256x256.png"
sips -z 512 512 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_256x256@2x.png"
sips -z 512 512 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_512x512.png"
sips -z 1024 1024 "$SOURCE_ICON" --out "$ICONSET_DIR/icon_512x512@2x.png"

iconutil -c icns "$ICONSET_DIR" -o "$ICONS_DIR/icon.icns"
rm -rf "$ICONSET_DIR"

# Generate .ico for Windows (using sips to create a multi-resolution PNG, then copy as ico)
# For proper .ico, we need a tool like ImageMagick. As a workaround, copy the 256x256 PNG
sips -z 256 256 "$SOURCE_ICON" --out "$ICONS_DIR/icon.ico"

# favicon for docs
sips -z 32 32 "$SOURCE_ICON" --out "docs/assets/favicon.png"

echo "All icons generated successfully!"
