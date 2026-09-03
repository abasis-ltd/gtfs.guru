#!/usr/bin/env python3
"""Fail when the deployed site is not serving the browser validator in this tree.

The website is deployed by hand (`scripts/deploy-website.sh`), so the live
files can silently fall behind the repository. When that happened, the site
kept serving an old `script.js` next to a newer `pkg/`, the worker's module
import 404'd, and every feed - down to nine bytes - was reported to users as
"too large to validate in the browser". Nothing in CI noticed, because the
repository itself was fine.

This compares the bytes actually served with the bytes in this working tree for
the assets the in-browser validator needs, and reports anything missing or
stale. Run it after a deploy, or any time the site misbehaves:

    python3 scripts/check_deployed_site.py                 # https://gtfs.guru
    python3 scripts/check_deployed_site.py --base-url http://localhost:8901
"""

from __future__ import annotations

import argparse
import hashlib
import pathlib
import sys
import urllib.error
import urllib.request

# Everything the browser needs to load and run a validation. A stale or missing
# file in this set breaks validation for every visitor and every feed size, so
# each one is a deploy failure rather than a warning.
CRITICAL_ASSETS = (
    "script.js",
    "style.css",
    "pkg/worker.js",
    "pkg/gtfs_guru_wasm.js",
    "pkg/gtfs_guru_wasm_bg.wasm",
    "pkg-mt/worker-mt.js",
    "pkg-mt/gtfs_guru_wasm.js",
    "pkg-mt/gtfs_guru_wasm_bg.wasm",
    "pkg-mt/snippets/wasm-bindgen-rayon-38edf6e439f6d70d/src/workerHelpers.no-bundler.js",
    "demo/gtfs-guru-demo.zip",
)

# The multithreaded tier only engages on a cross-origin-isolated page. Without
# these the site still validates, just single-threaded, so they are reported
# separately from a hard failure.
ISOLATION_HEADERS = {
    "cross-origin-opener-policy": "same-origin",
    "cross-origin-embedder-policy": "require-corp",
}


def digest(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def fetch(url: str, timeout: float) -> tuple[int, bytes, dict[str, str]]:
    request = urllib.request.Request(url, headers={"User-Agent": "gtfs-guru-deploy-check"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            headers = {k.lower(): v for k, v in response.headers.items()}
            return response.status, response.read(), headers
    except urllib.error.HTTPError as error:
        return error.code, b"", {}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="https://gtfs.guru")
    parser.add_argument(
        "--website-root",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent.parent / "website",
    )
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()

    base = args.base_url.rstrip("/")
    failures: list[str] = []
    warnings: list[str] = []

    status, _, headers = fetch(f"{base}/", args.timeout)
    if status != 200:
        print(f"::error::{base}/ returned HTTP {status}")
        return 1

    for header, expected in ISOLATION_HEADERS.items():
        actual = headers.get(header)
        if actual != expected:
            warnings.append(
                f"{header}: {actual or 'absent'} (expected {expected}) — "
                "the multithreaded validator tier will stay disabled"
            )

    for asset in CRITICAL_ASSETS:
        local_path = args.website_root / asset
        if not local_path.exists():
            failures.append(f"{asset}: missing from {args.website_root} — run scripts/build-wasm.sh")
            continue

        status, payload, _ = fetch(f"{base}/{asset}", args.timeout)
        if status != 200:
            failures.append(f"{asset}: HTTP {status} — not deployed")
            continue

        served, local = digest(payload), digest(local_path.read_bytes())
        if served != local:
            failures.append(
                f"{asset}: deployed {len(payload)} bytes ({served[:12]}) "
                f"but this tree has {local_path.stat().st_size} bytes ({local[:12]}) — stale deploy"
            )

    for warning in warnings:
        print(f"::warning::{warning}")
    for failure in failures:
        print(f"::error::{failure}")

    if failures:
        print(
            f"\n{len(failures)} problem(s) with {base}. "
            "Redeploy with: ./scripts/deploy-website.sh <server>",
            file=sys.stderr,
        )
        return 1

    print(f"{base} is serving the {len(CRITICAL_ASSETS)} critical assets from this tree.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
