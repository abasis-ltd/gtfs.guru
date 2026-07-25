#!/usr/bin/env bash
# Download the gtfs-guru CLI for the current runner, verify its checksum, and
# put it on PATH for the rest of the job.
set -euo pipefail

REPO="abasis-ltd/gtfs.guru"
CHECKSUMS_FILE="gtfs-guru-SHA256SUMS.txt"
VERSION="${INPUT_VERSION:-latest}"

case "$(uname -s)" in
  Linux) os="linux" ;;
  Darwin) os="macos" ;;
  MINGW* | MSYS* | CYGWIN* | Windows_NT) os="windows" ;;
  *) echo "Unsupported runner OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="arm64" ;;
  *) echo "Unsupported runner architecture: $(uname -m)" >&2; exit 1 ;;
esac

bin_name="gtfs-guru"
case "$os" in
  linux)
    bin_name="gtfs-guru"
    if [ "$arch" = "arm64" ]; then
      asset="gtfs-guru-linux-aarch64.tar.gz"
    else
      # The musl build has no glibc floor, so it also runs on container images
      # older than the one the release was built on.
      asset="gtfs-guru-linux-x86_64-musl.tar.gz"
    fi
    ;;
  macos)
    if [ "$arch" = "arm64" ]; then
      asset="gtfs-guru-macos-arm64.tar.gz"
    else
      asset="gtfs-guru-macos-x86_64.tar.gz"
    fi
    ;;
  windows)
    asset="gtfs-guru-windows-x64.zip"
    bin_name="gtfs-guru.exe"
    ;;
esac

if [ "$VERSION" = "latest" ] || [ -z "$VERSION" ]; then
  base_url="https://github.com/$REPO/releases/latest/download"
else
  base_url="https://github.com/$REPO/releases/download/$VERSION"
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

echo "Downloading $base_url/$asset"
curl -fsSL "$base_url/$asset" -o "$tmpdir/$asset"

# A release without a checksum file is a release we refuse to trust: the whole
# point of pinning a downloaded binary in CI is knowing what ran.
echo "Downloading $base_url/$CHECKSUMS_FILE"
curl -fsSL "$base_url/$CHECKSUMS_FILE" -o "$tmpdir/$CHECKSUMS_FILE"

if command -v sha256sum >/dev/null 2>&1; then
  sha256_cmd="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  sha256_cmd="shasum -a 256"
else
  echo "Neither sha256sum nor shasum is available; cannot verify the download." >&2
  exit 1
fi

expected="$(grep -E "^[A-Fa-f0-9]{64}[[:space:]]+\*?${asset}$" "$tmpdir/$CHECKSUMS_FILE" | awk '{print $1}' | head -1)"
if [ -z "$expected" ]; then
  echo "No checksum for $asset in $CHECKSUMS_FILE." >&2
  exit 1
fi
actual="$($sha256_cmd "$tmpdir/$asset" | awk '{print $1}')"
if [ "$expected" != "$actual" ]; then
  echo "Checksum mismatch for $asset (expected $expected, got $actual)." >&2
  exit 1
fi

if [ "${asset##*.}" = "zip" ]; then
  if command -v unzip >/dev/null 2>&1; then
    unzip -q "$tmpdir/$asset" -d "$tmpdir"
  elif command -v 7z >/dev/null 2>&1; then
    7z x -y -o"$tmpdir" "$tmpdir/$asset" >/dev/null
  else
    powershell -NoProfile -Command \
      "Expand-Archive -LiteralPath '$(cygpath -w "$tmpdir/$asset" 2>/dev/null || echo "$tmpdir/$asset")' -DestinationPath '$(cygpath -w "$tmpdir" 2>/dev/null || echo "$tmpdir")' -Force"
  fi
else
  tar -xzf "$tmpdir/$asset" -C "$tmpdir"
fi

install_dir="${RUNNER_TEMP_DIR:-$tmpdir}/gtfs-guru-bin"
mkdir -p "$install_dir"
mv "$tmpdir/$bin_name" "$install_dir/$bin_name"
chmod +x "$install_dir/$bin_name"

echo "$install_dir" >> "$GITHUB_PATH"
export PATH="$install_dir:$PATH"

# --version only exists from v1.0.0 onwards; fall back to what was requested
# rather than failing the install on an older pin.
resolved="$("$install_dir/$bin_name" --version 2>/dev/null | awk '{print $NF}')" || true
[ -n "$resolved" ] || resolved="$VERSION"

echo "version=$resolved" >> "$GITHUB_OUTPUT"
echo "Installed gtfs-guru $resolved to $install_dir"
