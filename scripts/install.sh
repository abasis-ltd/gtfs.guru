#!/usr/bin/env bash
set -euo pipefail

REPO="abasis-ltd/gtfs.guru"
BIN_NAME="gtfs-guru"

INSTALL_DIR="${INSTALL_DIR:-${BIN_DIR:-$HOME/.local/bin}}"
LINUX_FLAVOR="${GTFS_GURU_LINUX_FLAVOR:-gnu}"
VERSION="${GTFS_GURU_VERSION:-}"
CHECKSUMS_FILE="gtfs-guru-SHA256SUMS.txt"
VERIFY_CHECKSUMS=1

if ! command -v tar >/dev/null 2>&1; then
  echo "tar is required but not found."
  exit 1
fi

if command -v curl >/dev/null 2>&1; then
  FETCH="curl -fL"
elif command -v wget >/dev/null 2>&1; then
  FETCH="wget -qO-"
else
  echo "curl or wget is required but not found."
  exit 1
fi

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin) os="macos" ;;
  Linux) os="linux" ;;
  *) echo "Unsupported OS: $os"; exit 1 ;;
esac

case "$arch" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="arm64" ;;
  *) echo "Unsupported architecture: $arch"; exit 1 ;;
esac

if [ "$os" = "linux" ] && [ "$arch" = "x86_64" ] && [ "$LINUX_FLAVOR" = "gnu" ]; then
  if command -v ldd >/dev/null 2>&1; then
    if ldd --version 2>&1 | grep -qi musl; then
      LINUX_FLAVOR="musl"
    fi
  fi
fi

asset=""
if [ "$os" = "macos" ]; then
  if [ "$arch" = "arm64" ]; then
    asset="gtfs-guru-macos-arm64.tar.gz"
  else
    asset="gtfs-guru-macos-x86_64.tar.gz"
  fi
elif [ "$os" = "linux" ]; then
  if [ "$arch" = "arm64" ]; then
    asset="gtfs-guru-linux-aarch64.tar.gz"
  else
    if [ "$LINUX_FLAVOR" = "musl" ]; then
      asset="gtfs-guru-linux-x86_64-musl.tar.gz"
    else
      asset="gtfs-guru-linux-x86_64.tar.gz"
    fi
  fi
fi

if [ -z "$asset" ]; then
  echo "Could not determine a compatible release asset."
  exit 1
fi

if [ -n "$VERSION" ]; then
  BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
else
  BASE_URL="https://github.com/$REPO/releases/latest/download"
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

echo "Downloading $BASE_URL/$asset"
if [[ "$FETCH" == curl* ]]; then
  $FETCH "$BASE_URL/$asset" -o "$tmpdir/$asset"
else
  $FETCH "$BASE_URL/$asset" > "$tmpdir/$asset"
fi

echo "Downloading $BASE_URL/$CHECKSUMS_FILE"
if [[ "$FETCH" == curl* ]]; then
  if ! $FETCH "$BASE_URL/$CHECKSUMS_FILE" -o "$tmpdir/$CHECKSUMS_FILE"; then
    echo "Warning: $CHECKSUMS_FILE not found. Skipping verification."
    VERIFY_CHECKSUMS=0
  fi
else
  if ! $FETCH "$BASE_URL/$CHECKSUMS_FILE" > "$tmpdir/$CHECKSUMS_FILE"; then
    echo "Warning: $CHECKSUMS_FILE not found. Skipping verification."
    VERIFY_CHECKSUMS=0
  fi
fi

if [ "$VERIFY_CHECKSUMS" -eq 1 ]; then
  if command -v sha256sum >/dev/null 2>&1; then
    SHA256_CMD="sha256sum"
  elif command -v shasum >/dev/null 2>&1; then
    SHA256_CMD="shasum -a 256"
  else
    echo "sha256sum or shasum is required to verify downloads."
    exit 1
  fi

  expected_hash="$(grep -E "^[A-Fa-f0-9]{64}[[:space:]]+${asset}$" "$tmpdir/$CHECKSUMS_FILE" | awk '{print $1}' | head -1)"
  if [ -z "$expected_hash" ]; then
    echo "Checksum for $asset not found in $CHECKSUMS_FILE."
    exit 1
  fi

  actual_hash="$($SHA256_CMD "$tmpdir/$asset" | awk '{print $1}')"
  if [ "$expected_hash" != "$actual_hash" ]; then
    echo "Checksum verification failed for $asset."
    exit 1
  fi
fi

tar -xzf "$tmpdir/$asset" -C "$tmpdir"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmpdir/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"

echo "Installed $BIN_NAME to $INSTALL_DIR/$BIN_NAME"
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
  echo "Add $INSTALL_DIR to your PATH to run $BIN_NAME."
fi
