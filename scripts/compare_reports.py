#!/usr/bin/env python3
import argparse
import difflib
import json
import sys
from pathlib import Path


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def normalize_text(text: str) -> str:
    return text.replace("\r\n", "\n")


def diff_text(label: str, left: str, right: str, left_path: Path, right_path: Path) -> bool:
    if left == right:
        return True
    diff = difflib.unified_diff(
        left.splitlines(),
        right.splitlines(),
        fromfile=str(left_path),
        tofile=str(right_path),
        lineterm="",
    )
    print(f"{label} differs:")
    print("\n".join(diff))
    return False


def stable_json(value) -> str:
    return json.dumps(value, sort_keys=True, ensure_ascii=False)


def normalize_report(
    data,
    ignore_summary: bool,
    ignore_summary_fields,
    ignore_notice_order: bool,
    sort_summary_arrays: bool,
):
    if not isinstance(data, dict):
        return data

    if ignore_summary:
        data = dict(data)
        data.pop("summary", None)
    else:
        if ignore_summary_fields and isinstance(data.get("summary"), dict):
            summary = dict(data["summary"])
            for field in ignore_summary_fields:
                summary.pop(field, None)
            data = dict(data)
            data["summary"] = summary
        if sort_summary_arrays and isinstance(data.get("summary"), dict):
            summary = dict(data["summary"])
            for key in ("files", "gtfsFeatures"):
                value = summary.get(key)
                if isinstance(value, list):
                    summary[key] = sorted(value)
            data = dict(data)
            data["summary"] = summary

    if ignore_notice_order and isinstance(data.get("notices"), list):
        notices = []
        for notice in data["notices"]:
            if not isinstance(notice, dict):
                notices.append(notice)
                continue
            notice_copy = dict(notice)
            sample = notice_copy.get("sampleNotices")
            if isinstance(sample, list):
                notice_copy["sampleNotices"] = sorted(
                    sample, key=stable_json
                )
            notices.append(notice_copy)
        data = dict(data)
        data["notices"] = sorted(notices, key=stable_json)
    return data


def compare_json(
    label: str,
    left_path: Path,
    right_path: Path,
    ignore_summary: bool,
    ignore_summary_fields,
    ignore_notice_order: bool,
    sort_summary_arrays: bool,
) -> bool:
    if not left_path.exists() or not right_path.exists():
        missing = []
        if not left_path.exists():
            missing.append(str(left_path))
        if not right_path.exists():
            missing.append(str(right_path))
        print(f"{label} missing: {', '.join(missing)}")
        return False
    left = normalize_report(
        load_json(left_path),
        ignore_summary=ignore_summary,
        ignore_summary_fields=ignore_summary_fields,
        ignore_notice_order=ignore_notice_order,
        sort_summary_arrays=sort_summary_arrays,
    )
    right = normalize_report(
        load_json(right_path),
        ignore_summary=ignore_summary,
        ignore_summary_fields=ignore_summary_fields,
        ignore_notice_order=ignore_notice_order,
        sort_summary_arrays=sort_summary_arrays,
    )
    if left == right:
        return True
    left_dump = json.dumps(left, sort_keys=True, indent=2, ensure_ascii=False)
    right_dump = json.dumps(right, sort_keys=True, indent=2, ensure_ascii=False)
    return diff_text(label, left_dump, right_dump, left_path, right_path)


def compare_html(label: str, left_path: Path, right_path: Path) -> bool:
    if not left_path.exists() or not right_path.exists():
        missing = []
        if not left_path.exists():
            missing.append(str(left_path))
        if not right_path.exists():
            missing.append(str(right_path))
        print(f"{label} missing: {', '.join(missing)}")
        return False
    left = normalize_text(left_path.read_text(encoding="utf-8"))
    right = normalize_text(right_path.read_text(encoding="utf-8"))
    return diff_text(label, left, right, left_path, right_path)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compare GTFS validator output directories.",
    )
    parser.add_argument("expected_dir", type=Path)
    parser.add_argument("actual_dir", type=Path)
    parser.add_argument(
        "--extra-json",
        action="append",
        default=[],
        help="Additional JSON files to compare (repeatable).",
    )
    parser.add_argument("--ignore-summary", action="store_true")
    parser.add_argument(
        "--ignore-summary-field",
        action="append",
        default=[],
        help="Summary fields to ignore (repeatable, e.g. validatedAt).",
    )
    parser.add_argument(
        "--strip-runtime-fields",
        action="store_true",
        help="Ignore validatedAt, validationTimeSeconds, memoryUsageRecords, outputDirectory.",
    )
    parser.add_argument(
        "--ignore-input",
        action="store_true",
        help="Ignore gtfsInput in the report summary.",
    )
    parser.add_argument(
        "--ignore-validator-version",
        action="store_true",
        help="Ignore validatorVersion in the report summary.",
    )
    parser.add_argument(
        "--ignore-notice-order",
        action="store_true",
        help="Sort notices and sampleNotices before comparing.",
    )
    parser.add_argument(
        "--sort-summary-arrays",
        action="store_true",
        help="Sort summary arrays like files and gtfsFeatures before comparing.",
    )
    parser.add_argument("--skip-html", action="store_true")
    parser.add_argument("--html-name", default="report.html")
    args = parser.parse_args()

    expected_dir = args.expected_dir
    actual_dir = args.actual_dir

    checks = [
        ("report.json", "report.json"),
        ("system_errors.json", "system_errors.json"),
    ]
    checks.extend((name, name) for name in args.extra_json)

    ignore_summary_fields = list(args.ignore_summary_field)
    if args.strip_runtime_fields:
        ignore_summary_fields.extend(
            [
                "validatedAt",
                "validationTimeSeconds",
                "memoryUsageRecords",
                "outputDirectory",
            ]
        )
    if args.ignore_input:
        ignore_summary_fields.append("gtfsInput")
    if args.ignore_validator_version:
        ignore_summary_fields.append("validatorVersion")

    ok = True
    for label, name in checks:
        left_path = expected_dir / name
        right_path = actual_dir / name
        if not compare_json(
            label,
            left_path,
            right_path,
            ignore_summary=args.ignore_summary,
            ignore_summary_fields=ignore_summary_fields,
            ignore_notice_order=args.ignore_notice_order,
            sort_summary_arrays=args.sort_summary_arrays,
        ):
            ok = False

    if not args.skip_html:
        html_name = args.html_name
        left_path = expected_dir / html_name
        right_path = actual_dir / html_name
        if not compare_html(html_name, left_path, right_path):
            ok = False

    if ok:
        print("Outputs match.")
        return 0

    print("Outputs differ.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
