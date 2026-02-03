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


def validate_manifest(manifest: Path) -> bool:
    if os.environ.get("SKIP_MANIFEST_VALIDATE"):
        return True

    validate_args = parse_split(os.environ.get("MANIFEST_VALIDATE_FLAGS", ""))
    cmdline = [
        "python3",
        str(Path(__file__).parent / "validate_golden_manifest.py"),
        str(manifest),
    ] + validate_args
    return subprocess.call(cmdline) == 0


def validator_cmd(gtfs_bin: str | None):
    if gtfs_bin:
        return [gtfs_bin]
    return ["cargo", "run", "-p", "gtfs-guru", "--"]


def update_expected(
    expected_dir: Path,
    actual_dir: Path,
    extra_json,
    html_name: str | None,
) -> None:
    expected_dir.mkdir(parents=True, exist_ok=True)
    names = ["report.json", "system_errors.json", "report.html"]
    for name in extra_json:
        if name not in names:
            names.append(name)
    if html_name and html_name not in names:
        names.append(html_name)
    for name in names:
        path = Path(name)
        if path.is_absolute() or ".." in path.parts:
            continue
        source = actual_dir / name
        if source.exists():
            target = expected_dir / name
            target.write_bytes(source.read_bytes())


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Run golden suite from a TSV manifest with quoting support. "
            "Set CASE_FILTER to match case_name patterns."
        ),
    )
    parser.add_argument("manifest", type=Path)
    parser.add_argument("actual_root", type=Path)
    parser.add_argument("validator_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    if not args.manifest.exists():
        print(f"Manifest not found: {args.manifest}")
        return 1

    if not validate_manifest(args.manifest):
        return 1

    args.actual_root.mkdir(parents=True, exist_ok=True)

    compare_args = compare_args_from_env()
    gtfs_bin = os.environ.get("GTFS_VALIDATOR_BIN")
    case_filter = os.environ.get("CASE_FILTER")
    cmd_base = validator_cmd(gtfs_bin)

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
                case_compare_flags,
            ) = parts[:7]

            if not feed_path:
                print(f"Line {line_no}: missing feed_path")
                failures += 1
                continue
            if not expected_dir:
                print(f"Line {line_no}: missing expected_dir")
                failures += 1
                continue

            expected_path = Path(expected_dir)
            if not case_name:
                case_name = expected_path.name

            if case_filter and not fnmatch.fnmatchcase(case_name, case_filter):
                continue

            actual_dir = args.actual_root / case_name
            actual_dir.mkdir(parents=True, exist_ok=True)

            if not Path(feed_path).exists():
                print(f"Line {line_no}: feed not found: {feed_path}")
                failures += 1
                continue

            case_validator_args = parse_split(validator_args)

            print(f"Running {case_name}")
            validator_cmdline = (
                cmd_base
                + ["--input", feed_path, "--output_base", str(actual_dir)]
                + args.validator_args
                + case_validator_args
            )
            if subprocess.call(validator_cmdline) != 0:
                print(f"Line {line_no}: validator failed for {case_name}")
                failures += 1
                continue

            case_compare_args = list(compare_args)
            for name in parse_split(extra_json):
                case_compare_args.extend(["--extra-json", name])
            if html_name:
                case_compare_args.extend(["--html-name", html_name])
            case_compare_args.extend(parse_split(case_compare_flags))
            html_name_override = extract_html_name(case_compare_args)

            compare_cmdline = [
                "python3",
                str(Path(__file__).parent / "compare_reports.py"),
                expected_dir,
                str(actual_dir),
            ] + case_compare_args
            if subprocess.call(compare_cmdline) != 0:
                print(f"Line {line_no}: compare failed for {case_name}")
                if os.environ.get("UPDATE_EXPECTED_ON_FAIL"):
                    update_expected(
                        Path(expected_dir),
                        actual_dir,
                        parse_split(extra_json),
                        html_name_override or html_name,
                    )
                    print(f"Expected outputs updated for {case_name}.")
                failures += 1

    if failures:
        print(f"Golden suite finished with {failures} failure(s).")
        return 1

    print("Golden suite passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
