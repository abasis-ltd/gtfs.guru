#!/usr/bin/env python3
import argparse
import os
import shlex
import subprocess
from pathlib import Path


def parse_split(value: str):
    if not value:
        return []
    return [item for item in shlex.split(value) if item]


def compare_args_from_env():
    args = []
    args.extend(parse_split(os.environ.get("COMPARE_FLAGS", "")))
    extra_json = parse_split(os.environ.get("COMPARE_EXTRA_JSON", ""))
    for name in extra_json:
        args.extend(["--extra-json", name])
    html_name = os.environ.get("COMPARE_HTML_NAME")
    if html_name:
        args.extend(["--html-name", html_name])
    return args


def extract_html_name(args) -> str | None:
    for idx, value in enumerate(args):
        if value == "--html-name" and idx + 1 < len(args):
            return args[idx + 1]
        if value.startswith("--html-name="):
            return value.split("=", 1)[1]
    return None


def validator_cmd(gtfs_bin: str | None):
    if gtfs_bin:
        return [gtfs_bin]
    return ["cargo", "run", "-p", "gtfs_validator_cli", "--"]


def update_expected(expected_dir: Path, actual_dir: Path) -> None:
    if not expected_dir.exists():
        expected_dir.mkdir(parents=True, exist_ok=True)
    names = ["report.json", "system_errors.json", "report.html"]
    names.extend(parse_split(os.environ.get("COMPARE_EXTRA_JSON", "")))
    html_name = os.environ.get("COMPARE_HTML_NAME")
    if not html_name:
        html_name = extract_html_name(parse_split(os.environ.get("COMPARE_FLAGS", "")))
    if html_name:
        names.append(html_name)
    seen = set()
    ordered = []
    for name in names:
        if name and name not in seen:
            seen.add(name)
            ordered.append(name)
    for name in ordered:
        path = Path(name)
        if path.is_absolute() or ".." in path.parts:
            continue
        source = actual_dir / name
        if source.exists():
            target = expected_dir / name
            target.write_bytes(source.read_bytes())


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run validator and compare outputs for one case.",
    )
    parser.add_argument("feed_path", type=Path)
    parser.add_argument("expected_dir", type=Path)
    parser.add_argument("actual_dir", type=Path)
    parser.add_argument("validator_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    if not args.feed_path.exists():
        print(f"Feed not found: {args.feed_path}")
        return 1
    if not args.expected_dir.exists():
        print(f"Expected dir not found: {args.expected_dir}")
        return 1

    args.actual_dir.mkdir(parents=True, exist_ok=True)

    gtfs_bin = os.environ.get("GTFS_VALIDATOR_BIN")
    cmd_base = validator_cmd(gtfs_bin)

    validator_cmdline = (
        cmd_base
        + ["--input", str(args.feed_path), "--output_base", str(args.actual_dir)]
        + args.validator_args
    )
    if subprocess.call(validator_cmdline) != 0:
        print("Validator failed.")
        return 1

    compare_cmdline = [
        "python3",
        str(Path(__file__).parent / "compare_reports.py"),
        str(args.expected_dir),
        str(args.actual_dir),
    ] + compare_args_from_env()
    if subprocess.call(compare_cmdline) != 0:
        print("Compare failed.")
        if os.environ.get("UPDATE_EXPECTED_ON_FAIL"):
            update_expected(args.expected_dir, args.actual_dir)
            print("Expected outputs updated from actual.")
        return 1

    print("Outputs match.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
