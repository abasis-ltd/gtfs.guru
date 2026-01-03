#!/usr/bin/env python3
import argparse
import csv
import shlex
from pathlib import Path


def split_items(value: str):
    return [item for item in shlex.split(value) if item]


def resolve_expected_file(expected_dir: Path, name: str) -> Path:
    path = Path(name)
    if path.is_absolute():
        return path
    return expected_dir / name


def ignored_names() -> set[str]:
    return {"target", ".DS_Store", "Thumbs.db"}


def has_expected_outputs(expected: Path, ignored: set[str]) -> bool:
    for entry in expected.iterdir():
        if entry.name in ignored:
            continue
        return True
    return False


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate golden manifest entries before running the suite.",
    )
    parser.add_argument("manifest", type=Path)
    parser.add_argument(
        "--skip-existence",
        action="store_true",
        help="Skip file existence checks.",
    )
    parser.add_argument(
        "--allow-empty-expected",
        action="store_true",
        help="Allow expected_dir to be empty.",
    )
    parser.add_argument(
        "--warn-empty-expected",
        action="store_true",
        help="Warn instead of failing if expected_dir is empty.",
    )
    parser.add_argument(
        "--ignore-name",
        action="append",
        default=[],
        help="Additional names to ignore when checking expected_dir content.",
    )
    parser.add_argument(
        "--no-column-warn",
        action="store_true",
        help="Disable warnings for lines with fewer than 7 columns.",
    )
    args = parser.parse_args()

    if not args.manifest.exists():
        print(f"Manifest not found: {args.manifest}")
        return 1

    errors = 0
    with args.manifest.open("r", encoding="utf-8", newline="") as handle:
        for index, raw_line in enumerate(handle, start=1):
            line = raw_line.rstrip("\n")
            if not line or line.lstrip().startswith("#"):
                continue
            parts = next(csv.reader([raw_line], delimiter="\t"))
            if len(parts) < 2:
                print(
                    f"Line {index}: expected at least 2 columns (feed_path, expected_dir)."
                )
                errors += 1
                continue
            if len(parts) < 7 and not args.no_column_warn:
                print(
                    f"Line {index}: warning: fewer than 7 columns; use tabs for empty fields."
                )
            feed_path = parts[0] if len(parts) > 0 else ""
            expected_dir = parts[1] if len(parts) > 1 else ""
            extra_json = parts[3] if len(parts) > 3 else ""
            html_name = parts[4] if len(parts) > 4 else ""

            if not feed_path:
                print(f"Line {index}: missing feed_path")
                errors += 1
                continue
            if not expected_dir:
                print(f"Line {index}: missing expected_dir")
                errors += 1
                continue

            feed = Path(feed_path)
            expected = Path(expected_dir)
            if not args.skip_existence:
                if not feed.exists():
                    print(f"Line {index}: feed not found: {feed}")
                    errors += 1
                if not expected.exists():
                    print(f"Line {index}: expected_dir not found: {expected}")
                    errors += 1
                else:
                    ignored = ignored_names().union(args.ignore_name)
                    if not args.allow_empty_expected:
                        if not has_expected_outputs(expected, ignored):
                            message = (
                                f"Line {index}: expected_dir is empty: {expected}"
                            )
                            if args.warn_empty_expected:
                                print(f"Warning: {message}")
                            else:
                                print(message)
                                errors += 1

                if extra_json:
                    for name in split_items(extra_json):
                        check_path = resolve_expected_file(expected, name)
                        if not check_path.exists():
                            print(
                                f"Line {index}: extra JSON missing: {check_path}"
                            )
                            errors += 1

                if html_name:
                    check_path = resolve_expected_file(expected, html_name)
                    if not check_path.exists():
                        print(f"Line {index}: HTML missing: {check_path}")
                        errors += 1

    if errors:
        print(f"Manifest validation failed with {errors} error(s).")
        return 1

    print("Manifest validation passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
