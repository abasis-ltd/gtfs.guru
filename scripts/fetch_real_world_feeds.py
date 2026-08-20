#!/usr/bin/env python3
"""Fetch the real-world GTFS feeds used for parity checks.

These feeds are large (~143 MB together) and are deliberately *not* committed:
`test-gtfs-feeds/real-world/manifest.json` keeps the link, the license and the
SHA-256 of the snapshot the parity numbers were taken from, and this script
turns those links back into files under a gitignored cache directory.

Publishers refresh their feeds continuously, so a download usually will *not*
reproduce the recorded SHA-256. That is expected: the hash identifies the
snapshot, it is not a download precondition. The script reports drift, records
what it actually got in `fetched.json` next to the feeds, and only fails on a
mismatch when `--require-frozen` is passed.

Usage
    scripts/fetch_real_world_feeds.py                    # fetch every feed
    scripts/fetch_real_world_feeds.py --feed boston_mbta # fetch one
    scripts/fetch_real_world_feeds.py --check            # verify, no network
    scripts/fetch_real_world_feeds.py --list             # show the catalog
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import shutil
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_DEST = ROOT / "test-gtfs-feeds" / "real-world"
MANIFEST_NAME = "manifest.json"
RECORD_NAME = "fetched.json"

USER_AGENT = "gtfs-guru-fetch-real-world-feeds"
TIMEOUT_SECONDS = 300
CHUNK_BYTES = 1 << 20


def load_manifest(path: pathlib.Path) -> tuple[list[dict], str]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"manifest not found: {path}")
    except json.JSONDecodeError as error:
        raise SystemExit(f"manifest is not valid JSON: {path}: {error}")

    feeds = data.get("feeds")
    if not isinstance(feeds, list) or not feeds:
        raise SystemExit(f"manifest has no feeds: {path}")
    for feed in feeds:
        for key in ("name", "sourceUrl", "sha256", "sizeBytes"):
            if key not in feed:
                raise SystemExit(
                    f"manifest entry {feed.get('name', '?')!r} is missing {key!r}"
                )
    return feeds, str(data.get("retrievedOn", "the snapshot"))


def select(feeds: list[dict], names: list[str]) -> list[dict]:
    if not names:
        return feeds
    known = {feed["name"]: feed for feed in feeds}
    unknown = [name for name in names if name not in known]
    if unknown:
        raise SystemExit(
            f"unknown feed(s): {', '.join(unknown)}. "
            f"Known: {', '.join(sorted(known))}"
        )
    return [known[name] for name in names]


def digest(path: pathlib.Path) -> tuple[str, int]:
    """SHA-256 and size of `path`, hashed in chunks so 76 MB stays off the heap."""
    sha = hashlib.sha256()
    size = 0
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(CHUNK_BYTES)
            if not chunk:
                break
            sha.update(chunk)
            size += len(chunk)
    return sha.hexdigest(), size


def download(url: str, target: pathlib.Path) -> None:
    """Stream `url` into `target`, leaving no partial file behind on failure."""
    partial = target.with_suffix(target.suffix + ".part")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
            with partial.open("wb") as handle:
                shutil.copyfileobj(response, handle, CHUNK_BYTES)
    except (urllib.error.URLError, OSError) as error:
        partial.unlink(missing_ok=True)
        raise SystemExit(f"{url}: {error}")
    partial.replace(target)


def human(size: int) -> str:
    megabytes = size / (1 << 20)
    return f"{megabytes:.1f} MB" if megabytes >= 1 else f"{size} B"


def report(feed: dict, actual_sha: str, actual_size: int, retrieved_on: str) -> bool:
    """Print one feed's state. Returns True when it matches the frozen snapshot."""
    if actual_sha == feed["sha256"]:
        print(f"  {feed['name']}: frozen snapshot, {human(actual_size)}")
        return True
    print(
        f"  {feed['name']}: upstream moved since {retrieved_on} "
        f"({human(actual_size)}, sha256 {actual_sha[:12]}..., "
        f"expected {feed['sha256'][:12]}...)"
    )
    return False


def write_record(dest: pathlib.Path, entries: dict[str, dict]) -> None:
    """Record what is actually on disk so a parity run can be described later."""
    record = {
        "note": (
            "Written by scripts/fetch_real_world_feeds.py. Describes the bytes "
            "currently in this directory, which may be newer than the snapshot "
            "recorded in manifest.json."
        ),
        "fetchedAt": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "feeds": entries,
    }
    (dest / RECORD_NAME).write_text(
        json.dumps(record, indent=2) + "\n", encoding="utf-8"
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Fetch the real-world GTFS feeds listed in the manifest.",
    )
    parser.add_argument(
        "--dest",
        type=pathlib.Path,
        default=DEFAULT_DEST,
        help=f"where to put the feeds (default: {DEFAULT_DEST.relative_to(ROOT)})",
    )
    parser.add_argument(
        "--manifest",
        type=pathlib.Path,
        default=None,
        help=f"manifest to read (default: <dest>/{MANIFEST_NAME})",
    )
    parser.add_argument(
        "--feed",
        action="append",
        default=[],
        metavar="NAME",
        help="fetch only this feed; repeatable",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify what is already on disk and download nothing",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="print the catalog and exit",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="re-download feeds that are already present",
    )
    parser.add_argument(
        "--require-frozen",
        action="store_true",
        help="fail when a feed no longer matches the SHA-256 in the manifest",
    )
    args = parser.parse_args()

    dest: pathlib.Path = args.dest
    manifest_path = args.manifest or dest / MANIFEST_NAME
    manifest_feeds, retrieved_on = load_manifest(manifest_path)
    feeds = select(manifest_feeds, args.feed)

    if args.list:
        for feed in feeds:
            print(f"{feed['name']}\t{human(feed['sizeBytes'])}\t{feed['sourceUrl']}")
            print(f"\tprovider: {feed.get('provider', '?')}")
            print(f"\tlicense:  {feed.get('licenseUrl', '?')}")
        return 0

    if not args.check:
        dest.mkdir(parents=True, exist_ok=True)

    entries: dict[str, dict] = {}
    missing: list[str] = []
    drifted: list[str] = []

    print(f"Real-world feeds in {dest}:")
    for feed in feeds:
        target = dest / f"{feed['name']}.zip"

        if target.exists() and not args.force:
            actual_sha, actual_size = digest(target)
        elif args.check:
            print(f"  {feed['name']}: not fetched")
            missing.append(feed["name"])
            continue
        else:
            print(f"  {feed['name']}: downloading {feed['sourceUrl']}")
            download(feed["sourceUrl"], target)
            actual_sha, actual_size = digest(target)

        if not report(feed, actual_sha, actual_size, retrieved_on):
            drifted.append(feed["name"])
        entries[feed["name"]] = {
            "sourceUrl": feed["sourceUrl"],
            "sha256": actual_sha,
            "sizeBytes": actual_size,
            "matchesManifest": actual_sha == feed["sha256"],
        }

    if entries and not args.check:
        write_record(dest, entries)

    if missing:
        print(
            f"\n{len(missing)} feed(s) not fetched: {', '.join(missing)}\n"
            f"Run: scripts/fetch_real_world_feeds.py",
            file=sys.stderr,
        )
        return 1

    if drifted and args.require_frozen:
        print(
            f"\n{len(drifted)} feed(s) no longer match the frozen snapshot: "
            f"{', '.join(drifted)}",
            file=sys.stderr,
        )
        return 1

    if drifted:
        print(
            f"\n{len(drifted)} feed(s) are newer than the recorded snapshot. "
            "Parity numbers taken against them will differ from the manifest.",
        )

    return 0


if __name__ == "__main__":
    sys.exit(main())
