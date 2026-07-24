#!/usr/bin/env bash
# Generate platform icons from a transparent square master image.

set -euo pipefail

SOURCE_ICON="${1:-}"
if [[ -z "$SOURCE_ICON" ]]; then
    echo "Usage: $0 <source-icon.png>"
    exit 1
fi

if [[ ! -f "$SOURCE_ICON" ]]; then
    echo "Source icon not found: $SOURCE_ICON"
    exit 1
fi

if ! command -v magick >/dev/null 2>&1; then
    echo "ImageMagick is required to preserve transparency and build a valid .ico file."
    exit 1
fi

ICONS_DIR="crates/gtfs_validator_gui/icons"
mkdir -p \
    "$ICONS_DIR/ios" \
    "$ICONS_DIR/android/mipmap-hdpi" \
    "$ICONS_DIR/android/mipmap-mdpi" \
    "$ICONS_DIR/android/mipmap-xhdpi" \
    "$ICONS_DIR/android/mipmap-xxhdpi" \
    "$ICONS_DIR/android/mipmap-xxxhdpi"

resize_icon() {
    local size="$1"
    local output="$2"

    magick "$SOURCE_ICON" \
        -background none \
        -alpha on \
        -resize "${size}x${size}" \
        -gravity center \
        -extent "${size}x${size}" \
        "PNG32:$output"
}

generate_icons() {
    local specification size output

    for specification in "$@"; do
        size="${specification%%:*}"
        output="${specification#*:}"
        resize_icon "$size" "$output"
    done
}

# Tauri and Windows Store assets.
generate_icons \
    "32:$ICONS_DIR/32x32.png" \
    "64:$ICONS_DIR/64x64.png" \
    "128:$ICONS_DIR/128x128.png" \
    "256:$ICONS_DIR/128x128@2x.png" \
    "512:$ICONS_DIR/icon.png" \
    "30:$ICONS_DIR/Square30x30Logo.png" \
    "44:$ICONS_DIR/Square44x44Logo.png" \
    "71:$ICONS_DIR/Square71x71Logo.png" \
    "89:$ICONS_DIR/Square89x89Logo.png" \
    "107:$ICONS_DIR/Square107x107Logo.png" \
    "142:$ICONS_DIR/Square142x142Logo.png" \
    "150:$ICONS_DIR/Square150x150Logo.png" \
    "284:$ICONS_DIR/Square284x284Logo.png" \
    "310:$ICONS_DIR/Square310x310Logo.png" \
    "50:$ICONS_DIR/StoreLogo.png"

# iOS assets.
generate_icons \
    "20:$ICONS_DIR/ios/AppIcon-20x20@1x.png" \
    "40:$ICONS_DIR/ios/AppIcon-20x20@2x.png" \
    "40:$ICONS_DIR/ios/AppIcon-20x20@2x-1.png" \
    "60:$ICONS_DIR/ios/AppIcon-20x20@3x.png" \
    "29:$ICONS_DIR/ios/AppIcon-29x29@1x.png" \
    "58:$ICONS_DIR/ios/AppIcon-29x29@2x.png" \
    "58:$ICONS_DIR/ios/AppIcon-29x29@2x-1.png" \
    "87:$ICONS_DIR/ios/AppIcon-29x29@3x.png" \
    "40:$ICONS_DIR/ios/AppIcon-40x40@1x.png" \
    "80:$ICONS_DIR/ios/AppIcon-40x40@2x.png" \
    "80:$ICONS_DIR/ios/AppIcon-40x40@2x-1.png" \
    "120:$ICONS_DIR/ios/AppIcon-40x40@3x.png" \
    "120:$ICONS_DIR/ios/AppIcon-60x60@2x.png" \
    "180:$ICONS_DIR/ios/AppIcon-60x60@3x.png" \
    "76:$ICONS_DIR/ios/AppIcon-76x76@1x.png" \
    "152:$ICONS_DIR/ios/AppIcon-76x76@2x.png" \
    "167:$ICONS_DIR/ios/AppIcon-83.5x83.5@2x.png" \
    "1024:$ICONS_DIR/ios/AppIcon-512@2x.png"

# Android assets.
generate_icons \
    "72:$ICONS_DIR/android/mipmap-hdpi/ic_launcher.png" \
    "162:$ICONS_DIR/android/mipmap-hdpi/ic_launcher_foreground.png" \
    "72:$ICONS_DIR/android/mipmap-hdpi/ic_launcher_round.png" \
    "48:$ICONS_DIR/android/mipmap-mdpi/ic_launcher.png" \
    "108:$ICONS_DIR/android/mipmap-mdpi/ic_launcher_foreground.png" \
    "48:$ICONS_DIR/android/mipmap-mdpi/ic_launcher_round.png" \
    "96:$ICONS_DIR/android/mipmap-xhdpi/ic_launcher.png" \
    "216:$ICONS_DIR/android/mipmap-xhdpi/ic_launcher_foreground.png" \
    "96:$ICONS_DIR/android/mipmap-xhdpi/ic_launcher_round.png" \
    "144:$ICONS_DIR/android/mipmap-xxhdpi/ic_launcher.png" \
    "324:$ICONS_DIR/android/mipmap-xxhdpi/ic_launcher_foreground.png" \
    "144:$ICONS_DIR/android/mipmap-xxhdpi/ic_launcher_round.png" \
    "192:$ICONS_DIR/android/mipmap-xxxhdpi/ic_launcher.png" \
    "432:$ICONS_DIR/android/mipmap-xxxhdpi/ic_launcher_foreground.png" \
    "192:$ICONS_DIR/android/mipmap-xxxhdpi/ic_launcher_round.png"

# macOS .icns. ImageMagick explicitly writes RGBA PNGs so iconutil keeps alpha.
ICONSET_DIR="$(mktemp -d /tmp/gtfs-guru.iconset.XXXXXX)"
trap 'rm -rf "$ICONSET_DIR"' EXIT
ICONSET="$ICONSET_DIR/GTFS-Guru.iconset"
mkdir -p "$ICONSET"

generate_icons \
    "16:$ICONSET/icon_16x16.png" \
    "32:$ICONSET/icon_16x16@2x.png" \
    "32:$ICONSET/icon_32x32.png" \
    "64:$ICONSET/icon_32x32@2x.png" \
    "128:$ICONSET/icon_128x128.png" \
    "256:$ICONSET/icon_128x128@2x.png" \
    "256:$ICONSET/icon_256x256.png" \
    "512:$ICONSET/icon_256x256@2x.png" \
    "512:$ICONSET/icon_512x512.png" \
    "1024:$ICONSET/icon_512x512@2x.png"

iconutil -c icns "$ICONSET" -o "$ICONS_DIR/icon.icns"

# A real multi-resolution Windows icon.
magick "$SOURCE_ICON" \
    -background none \
    -alpha on \
    -define icon:auto-resize=256,128,64,48,32,16 \
    "$ICONS_DIR/icon.ico"

resize_icon 32 "docs/assets/favicon.png"

echo "All icons generated successfully."
