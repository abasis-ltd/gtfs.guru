#!/usr/bin/env python3
"""Fail when a release tag and package metadata disagree."""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SEMVER = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def fail(message: str) -> None:
    print(f"release version check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--tag",
        help="Release tag/version to validate (for example v1.0.0).",
    )
    args = parser.parse_args()

    crate_versions: dict[str, str] = {}
    for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        package = load_toml(manifest).get("package", {})
        name = package.get("name")
        version = package.get("version")
        if name and version:
            crate_versions[name] = version

    if not crate_versions:
        fail("no crate versions found")
    unique_versions = set(crate_versions.values())
    if len(unique_versions) != 1:
        details = ", ".join(f"{name}={version}" for name, version in crate_versions.items())
        fail(f"Cargo packages are out of sync: {details}")
    cargo_version = unique_versions.pop()

    expected = args.tag.removeprefix("refs/tags/").removeprefix("v") if args.tag else cargo_version
    if not SEMVER.fullmatch(expected):
        fail(f"'{expected}' is not a valid SemVer version")
    if cargo_version != expected:
        fail(f"tag version {expected} does not match Cargo version {cargo_version}")

    tauri_path = ROOT / "crates/gtfs_validator_gui/tauri.conf.json"
    tauri_version = json.loads(tauri_path.read_text(encoding="utf-8"))["version"]
    if tauri_version != expected:
        fail(f"Tauri version {tauri_version} does not match {expected}")

    pyproject = load_toml(ROOT / "crates/gtfs_validator_python/pyproject.toml")["project"]
    if "version" in pyproject or "version" not in pyproject.get("dynamic", []):
        fail("Python version must be dynamic so maturin reads it from Cargo.toml")

    npm_template = json.loads(
        (ROOT / "crates/gtfs_validator_wasm/package.json.template").read_text(encoding="utf-8")
    )
    if npm_template["name"] != "@abasisltd/gtfs-guru-wasm":
        fail(f"unexpected npm package name {npm_template['name']}")
    if npm_template["version"] != expected:
        fail(f"npm template version {npm_template['version']} does not match {expected}")

    lock = load_toml(ROOT / "Cargo.lock")
    locked_internal = {
        package["name"]: package["version"]
        for package in lock.get("package", [])
        if package.get("name") in crate_versions
    }
    if locked_internal != crate_versions:
        fail("Cargo.lock does not match the workspace package versions; run cargo check")

    print(f"release metadata is consistent for {expected} ({len(crate_versions)} crates)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
