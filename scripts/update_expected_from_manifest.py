#!/usr/bin/env python3
import argparse
import csv
import os
import shlex
import subprocess
from pathlib import Path
import fnmatch


def parse_split(value: str):
    if not value:
        return []
    return [item for item in shlex.split(value) if item]


def ignored_names() -> set[str]:
    return {"target", ".DS_Store", "Thumbs.db"}


def has_expected_outputs(expected: Path, ignored: set[str]) -> bool:
    for entry in expected.iterdir():
        if entry.name in ignored:
            continue
        return True
    return False


def resolve_expected_file(expected_dir: Path, name: str) -> Path:
    path = Path(name)
    if path.is_absolute():
        return path
    return expected_dir / name


def validator_cmd(gtfs_bin: str | None):
    if gtfs_bin:
        return [gtfs_bin]
    return ["cargo", "run", "-p", "gtfs_validator_cli", "--"]


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Generate expected outputs for a manifest. "
            "Set CASE_FILTER to match case_name patterns."
        ),
    )
    parser.add_argument("manifest", type=Path)
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--overwrite",
        action="store_true",
        help="Overwrite existing expected outputs.",
    )
    group.add_argument(
        "--skip-existing",
        action="store_true",
        help="Skip cases that already have expected outputs.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print actions without running the validator.",
    )
    parser.add_argument(
        "--allow-missing-extra-json",
        action="store_true",
        help="Allow missing extra JSON outputs.",
    )
    parser.add_argument(
        "--warn-missing-extra-json",
        action="store_true",
        help="Warn instead of failing if extra JSON outputs are missing.",
    )
    parser.add_argument("validator_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    if not args.manifest.exists():
        print(f"Manifest not found: {args.manifest}")
        return 1

    gtfs_bin = os.environ.get("GTFS_VALIDATOR_BIN")
    case_filter = os.environ.get("CASE_FILTER")
    cmd_base = validator_cmd(gtfs_bin)
    ignored = ignored_names()

    failures = 0
    with args.manifest.open("r", encoding="utf-8", newline="") as handle:
        for line_no, raw_line in enumerate(handle, start=1):
            line = raw_line.rstrip("\n")
            if not line or line.lstrip().startswith("#"):
                continue
            parts = next(csv.reader([raw_line], delimiter="\t"))
            parts.extend([""] * (7 - len(parts)))
            (
                feed_path,
                expected_dir,
                case_name,
                extra_json,
                html_name,
                validator_args,
                _compare_flags,
            ) = parts[:7]

            if not feed_path:
                print(f"Line {line_no}: missing feed_path")
                failures += 1
                continue
            if not expected_dir:
                print(f"Line {line_no}: missing expected_dir")
                failures += 1
                continue

            feed = Path(feed_path)
            expected = Path(expected_dir)
            if not case_name:
                case_name = expected.name

            if case_filter and not fnmatch.fnmatchcase(case_name, case_filter):
                continue

            if not feed.exists():
                print(f"Line {line_no}: feed not found: {feed}")
                failures += 1
                continue

            if expected.exists() and has_expected_outputs(expected, ignored):
                if args.skip_existing:
                    print(f"Skipping {case_name} (expected exists)")
                    continue
                if not args.overwrite:
                    print(
                        f"Line {line_no}: expected already exists for {case_name}. "
                        "Use --overwrite or --skip-existing."
                    )
                    failures += 1
                    continue

            expected.mkdir(parents=True, exist_ok=True)

            case_args = parse_split(validator_args)
            extra_json_items = parse_split(extra_json)
            cmdline = (
                cmd_base
                + ["--input", str(feed), "--output_base", str(expected)]
                + args.validator_args
                + case_args
            )
            if html_name:
                cmdline += ["--html_report_name", html_name]
            if "notice_schema.json" in extra_json_items:
                cmdline += ["--export_notices_schema"]

            print(f"Generating expected for {case_name}")
            if args.dry_run:
                print(" ", " ".join(cmdline))
                continue

            if subprocess.call(cmdline) != 0:
                print(f"Line {line_no}: validator failed for {case_name}")
                failures += 1
                continue

            if html_name:
                html_path = resolve_expected_file(expected, html_name)
                if not html_path.exists():
                    print(f"Line {line_no}: HTML missing: {html_path}")
                    failures += 1

            if extra_json_items:
                for name in extra_json_items:
                    check_path = resolve_expected_file(expected, name)
                    if not check_path.exists():
                        message = (
                            f"Line {line_no}: extra JSON missing: {check_path}"
                        )
                        if args.allow_missing_extra_json:
                            continue
                        if args.warn_missing_extra_json:
                            print(f"Warning: {message}")
                        else:
                            print(message)
                            failures += 1

    if failures:
        print(f"Expected generation finished with {failures} failure(s).")
        return 1

    print("Expected generation finished successfully.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
