#!/usr/bin/env bash
# Build a high-contrast app icon from the transparent GTFS Guru brand mark.

set -euo pipefail

MARK="${1:-}"
OUTPUT="${2:-}"

if [[ -z "$MARK" || -z "$OUTPUT" ]]; then
    echo "Usage: $0 <transparent-mark.png> <output.png>"
    exit 1
fi

if [[ ! -f "$MARK" ]]; then
    echo "Brand mark not found: $MARK"
    exit 1
fi

if ! command -v magick >/dev/null 2>&1; then
    echo "ImageMagick is required."
    exit 1
fi

WORK_DIR="$(mktemp -d /tmp/gtfs-guru-app-icon.XXXXXX)"
trap 'rm -rf "$WORK_DIR"' EXIT

MASK="$WORK_DIR/squircle-mask.png"
BACKGROUND="$WORK_DIR/background.png"
MARK_WHITE="$WORK_DIR/mark-white.png"
FRAME="$WORK_DIR/frame.png"

# A macOS-style continuous rounded rectangle with transparent outer corners.
magick -size 1024x1024 xc:none \
    -fill white \
    -draw "path 'M 512,64 C 790,64 960,64 960,320 L 960,704 C 960,960 790,960 512,960 C 234,960 64,960 64,704 L 64,320 C 64,64 234,64 512,64 Z'" \
    "PNG32:$MASK"

# Saturated enough for a dark Dock, restrained enough to keep the white mark clear.
magick -size 1024x1024 xc:none \
    -sparse-color barycentric \
    "0,0 #2457D6 1024,0 #177FD0 0,1024 #0E7490 1024,1024 #0D9488" \
    "$MASK" -alpha off -compose CopyOpacity -composite \
    "PNG32:$BACKGROUND"

# Preserve the recognizable mark, but give its fine lines enough weight at 32–48 px.
magick "$MARK" \
    -trim +repage \
    -resize 650x700 \
    -channel A -morphology Dilate Disk:3 +channel \
    -fill white -colorize 100 \
    "PNG32:$MARK_WHITE"

# A quiet inset highlight keeps the tile edge visible on both light and dark surfaces.
magick -size 1024x1024 xc:none \
    -fill none -stroke 'rgba(255,255,255,0.20)' -strokewidth 4 \
    -draw "path 'M 512,72 C 784,72 952,72 952,324 L 952,700 C 952,952 784,952 512,952 C 240,952 72,952 72,700 L 72,324 C 72,72 240,72 512,72 Z'" \
    "PNG32:$FRAME"

magick "$BACKGROUND" "$FRAME" -compose Over -composite \
    "$MARK_WHITE" -gravity center -geometry +0+2 -compose Over -composite \
    "PNG32:$OUTPUT"

echo "Created $OUTPUT"
