#!/usr/bin/env python3
"""Offline tests for the tracked build-artifact policy."""

from check_repository_artifacts import find_blocked_paths, is_blocked


def test_generated_outputs_are_blocked() -> None:
    blocked = [
        "web-app/bin/gtfs-guru",
        "dist/gtfs-guru.tar.gz",
        "crates/gtfs_validator_wasm/pkg-node/gtfs_guru_wasm_bg.wasm",
        "crates/gtfs_validator_web/website/pkg/gtfs_guru_wasm_bg.wasm",
        "benchmark-java/report.json",
        "output-guru-ny/report.html",
        "cargo-flamegraph.trace/Trace1.run",
    ]
    assert all(is_blocked(path) for path in blocked)
    assert find_blocked_paths(reversed(blocked)) == sorted(blocked)


def test_canonical_website_assets_are_only_blocked_during_history_cleanup() -> None:
    canonical = [
        "website/pkg/gtfs_guru_wasm_bg.wasm",
        "website/pkg-mt/gtfs_guru_wasm_bg.wasm",
    ]
    assert not find_blocked_paths(canonical)
    assert find_blocked_paths(
        canonical, purge_canonical_website=True
    ) == sorted(canonical)


def test_source_and_intentional_binary_fixtures_are_allowed() -> None:
    allowed = [
        "crates/gtfs_validator_wasm/src/lib.rs",
        "crates/gtfs_validator_gui/icons/icon.ico",
        "test-gtfs-feeds/base-valid.zip",
        "website/og-image.png",
        "benchmark-feeds/gtfs-validator.jar",
    ]
    assert not any(is_blocked(path) for path in allowed)


if __name__ == "__main__":
    test_generated_outputs_are_blocked()
    test_canonical_website_assets_are_only_blocked_during_history_cleanup()
    test_source_and_intentional_binary_fixtures_are_allowed()
    print("Repository artifact policy tests passed.")
