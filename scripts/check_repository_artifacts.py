#!/usr/bin/env python3
"""Reject forbidden build outputs and require canonical embedded WASM assets."""

from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys
from collections.abc import Iterable


BLOCKED_PREFIXES = (
    "benchmark-java/",
    "benchmark-rust/",
    "cargo-flamegraph.trace/",
    "crates/gtfs_validator_wasm/pkg/",
    "crates/gtfs_validator_wasm/pkg-mt/",
    "crates/gtfs_validator_wasm/pkg-node/",
    "crates/gtfs_validator_web/website/pkg/",
    "crates/gtfs_validator_web/website/pkg-mt/",
    "dist/",
    "web-app/bin/",
)

CANONICAL_WEBSITE_PREFIXES = ("website/pkg/", "website/pkg-mt/")
REQUIRED_WEBSITE_ASSETS = (
    "website/pkg/gtfs_guru_wasm.js",
    "website/pkg/gtfs_guru_wasm_bg.wasm",
    "website/pkg/package.json",
    "website/pkg/worker.js",
    "website/pkg-mt/gtfs_guru_wasm.js",
    "website/pkg-mt/gtfs_guru_wasm_bg.wasm",
    "website/pkg-mt/package.json",
    "website/pkg-mt/worker-mt.js",
)


def is_blocked(path: str, purge_canonical_website: bool = False) -> bool:
    prefixes = BLOCKED_PREFIXES
    if purge_canonical_website:
        prefixes += CANONICAL_WEBSITE_PREFIXES
    return path.startswith(prefixes) or (
        path.startswith("output") and "/" in path
    )


def find_blocked_paths(
    paths: Iterable[str], purge_canonical_website: bool = False
) -> list[str]:
    return sorted(
        path for path in paths if is_blocked(path, purge_canonical_website)
    )


def tracked_paths(repository: pathlib.Path) -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=repository,
        check=True,
        capture_output=True,
    )
    return [
        path.decode("utf-8", errors="surrogateescape")
        for path in result.stdout.split(b"\0")
        if path
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--paths-from-stdin",
        action="store_true",
        help="check newline-delimited paths instead of the current Git index",
    )
    parser.add_argument(
        "--purge-canonical-website",
        action="store_true",
        help="also reject website/pkg and website/pkg-mt during history cleanup",
    )
    parser.add_argument(
        "--repository",
        type=pathlib.Path,
        help="repository to inspect (defaults to the checkout containing this script)",
    )
    args = parser.parse_args()

    if args.paths_from_stdin and args.repository is not None:
        parser.error("--repository cannot be combined with --paths-from-stdin")

    if args.paths_from_stdin:
        paths = (line.rstrip("\n") for line in sys.stdin)
        subject = "repository history"
    else:
        repository = (
            args.repository.resolve()
            if args.repository is not None
            else pathlib.Path(__file__).resolve().parent.parent
        )
        paths = tracked_paths(repository)
        subject = "Git index"

    materialized_paths = list(paths)
    violations = find_blocked_paths(
        materialized_paths, purge_canonical_website=args.purge_canonical_website
    )
    missing = []
    if not args.paths_from_stdin:
        tracked = set(materialized_paths)
        missing = [path for path in REQUIRED_WEBSITE_ASSETS if path not in tracked]

    if not violations and not missing:
        print(f"No forbidden build artifacts found in {subject}.")
        if not args.paths_from_stdin:
            print("Canonical embedded WASM assets are present.")
        return 0

    if violations:
        print(f"Forbidden build artifacts found in {subject}:", file=sys.stderr)
        for path in violations:
            print(f"  {path}", file=sys.stderr)
    if missing:
        print("Required embedded WASM assets are not tracked:", file=sys.stderr)
        for path in missing:
            print(f"  {path}", file=sys.stderr)
    print("Refresh WASM assets with scripts/build-wasm.sh.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
