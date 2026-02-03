import argparse
import json
import os
import shlex
import shutil
import subprocess
import time
from collections import defaultdict
from pathlib import Path


def is_gtfs_dir(path):
    if not path.is_dir():
        return False
    # If it contains any .txt files, we assume it's a GTFS dir.
    return any(f.suffix == ".txt" for f in path.iterdir())


def sample_filename(sample):
    for key in ("filename", "childFilename", "parentFilename"):
        value = sample.get(key)
        if value:
            return value
    return None


def get_notices_per_file(report_path):
    if not report_path.exists():
        return None
    try:
        with report_path.open() as f:
            data = json.load(f)
    except Exception:
        return None
    notices = data.get("notices", [])
    file_notices = defaultdict(lambda: defaultdict(int))
    for notice in notices:
        code = notice.get("code")
        if not code:
            continue
        count = notice.get("totalNotices")
        if count is None:
            count = notice.get("total_notices")
        samples = notice.get("sampleNotices") or notice.get("notices") or []
        if count is None:
            count = len(samples)
        if not count:
            continue

        if samples:
            filenames = [sample_filename(sample) or "unknown" for sample in samples]
            if count == len(filenames):
                for filename in filenames:
                    file_notices[filename][code] += 1
            else:
                distinct = {name for name in filenames}
                if len(distinct) == 1:
                    filename = next(iter(distinct))
                    file_notices[filename][code] += int(count)
                else:
                    file_notices["unknown"][code] += int(count)
        else:
            filename = notice.get("filename") or notice.get("file") or "unknown"
            file_notices[filename][code] += int(count)
    return file_notices


def total_notices(file_notices):
    return sum(sum(codes.values()) for codes in file_notices.values())


def aggregate_notices_by_code(file_notices):
    aggregated = defaultdict(int)
    for codes in file_notices.values():
        for code, count in codes.items():
            aggregated[code] += count
    return dict(aggregated)


def merge_file_notices(base, extra):
    if base is None:
        base = {}
    if not extra:
        return base
    for filename, codes in extra.items():
        bucket = base.setdefault(filename, {})
        for code, count in codes.items():
            bucket[code] = bucket.get(code, 0) + count
    return base


def read_system_errors(path):
    if not path.exists():
        return []
    try:
        with path.open() as f:
            data = json.load(f)
    except Exception:
        return []
    return data.get("notices", []) or []


def detect_java_failure(java_out_dir, java_success=None, java_returncode=None):
    notices = read_system_errors(java_out_dir / "system_errors.json")
    for notice in notices:
        samples = notice.get("sampleNotices") or []
        for sample in samples:
            exception = str(sample.get("exception") or "")
            message = str(sample.get("message") or "")
            if "OutOfMemoryError" in exception or "OutOfMemoryError" in message:
                return "oom"
            if "Java heap space" in message:
                return "oom"
    if notices and all(notice.get("code") == "i_o_error" for notice in notices):
        return None
    if java_returncode is not None:
        if java_returncode != 0:
            return "error"
    elif java_success is False:
        return "error"
    return None


def write_summary(output_base_dir, results, match_by):
    summary_path = output_base_dir / "summary_all_tests.json"
    with summary_path.open("w") as f:
        json.dump(results, f, indent=2)

    report_path = output_base_dir / "report.md"
    with report_path.open("w") as f:
        f.write("# GTFS Validator Parity Report\n\n")
        f.write(f"Match mode: {match_by}\n\n")
        f.write(f"Tested {len(results)} feeds.\n\n")
        f.write("| Feed | Java Total | Rust Total | Match | Duration (J/R) |\n")
        f.write("| :--- | :---: | :---: | :---: | :---: |\n")
        for r in results:
            java_failed = r["java"].get("failed", False)
            if java_failed:
                reason = r["java"].get("failure_reason")
                java_total = "Java failed (OOM)" if reason == "oom" else "Java failed"
                match_value = "N/A"
            else:
                java_total = r["java"]["total"]
                match_value = "✅" if r["match"] else "❌"
            f.write(
                f"| {r['name']} | {java_total} | {r['rust']['total']} | {match_value} "
                f"| {r['java']['duration']:.2f}s / {r['rust']['duration']:.2f}s |\n"
            )


def rebuild_summary(output_base_dir, match_by):
    summary_path = output_base_dir / "summary_all_tests.json"
    existing_summary = []
    if summary_path.exists():
        with summary_path.open() as f:
            existing_summary = json.load(f)

    existing_by_name = {entry["name"]: entry for entry in existing_summary}
    if existing_summary:
        case_names = [entry["name"] for entry in existing_summary]
    else:
        case_names = sorted(
            entry.name for entry in output_base_dir.iterdir() if entry.is_dir()
        )

    results = []
    for name in case_names:
        case_dir = output_base_dir / name
        if not case_dir.exists():
            continue
        old = existing_by_name.get(name, {})
        java_meta = old.get("java") or {}
        rust_meta = old.get("rust") or {}

        java_report = case_dir / "java" / "report.json"
        rust_report = case_dir / "rust" / "report.json"
        rust_alt_report = case_dir / "rust" / "validation_report.json"

        java_notices = get_notices_per_file(java_report)
        java_system_errors = get_notices_per_file(case_dir / "java" / "system_errors.json")
        if java_notices is None:
            java_notices = java_meta.get("notices") or {}
        java_notices = merge_file_notices(java_notices, java_system_errors)

        java_failure_reason = detect_java_failure(
            case_dir / "java",
            java_meta.get("success"),
            java_meta.get("returncode"),
        )
        java_outputs_present = (
            java_report.exists()
            or (case_dir / "java" / "system_errors.json").exists()
        )
        if not java_failure_reason and not java_outputs_present:
            java_failure_reason = java_meta.get("failure_reason")

        rust_notices = get_notices_per_file(rust_report)
        if rust_notices is None and rust_alt_report.exists():
            rust_notices = get_notices_per_file(rust_alt_report)
        rust_system_errors = get_notices_per_file(case_dir / "rust" / "system_errors.json")
        if rust_notices is None:
            rust_notices = rust_meta.get("notices") or {}
        rust_notices = merge_file_notices(rust_notices, rust_system_errors)

        java_total = total_notices(java_notices)
        rust_total = total_notices(rust_notices)

        match_by_file = java_notices == rust_notices
        match_by_code = (
            aggregate_notices_by_code(java_notices)
            == aggregate_notices_by_code(rust_notices)
        )
        if java_failure_reason:
            match_value = False
        else:
            match_value = match_by_code if match_by == "code" else match_by_file

        results.append(
            {
                "name": name,
                "path": old.get("path", ""),
                "java": {
                    "success": java_meta.get("success", False),
                    "total": java_total,
                    "duration": java_meta.get("duration", 0),
                    "notices": java_notices,
                    "failed": bool(java_failure_reason),
                    "failure_reason": java_failure_reason,
                },
                "rust": {
                    "success": rust_meta.get("success", False),
                    "total": rust_total,
                    "duration": rust_meta.get("duration", 0),
                    "notices": rust_notices,
                },
                "match": match_value,
                "match_by_code": match_by_code,
                "match_by_file": match_by_file,
            }
        )
    return results


def run_full_comparison(
    current_dir,
    match_by,
    runner_cmd=None,
    max_size_mb=None,
    java_xmx=None,
    java_args=None,
):
    all_test_dirs = [
        current_dir / "mobility-data-test-feeds",
        current_dir / "test-gtfs-feeds",
        current_dir / "benchmark-feeds",
    ]
    output_base_dir = current_dir / "output_all_tests_comparison"
    validator_jar = current_dir / "benchmark-feeds/gtfs-validator.jar"
    rust_bin = current_dir / "target/release/gtfs-guru"
    java_bin = "java"
    java_xmx = java_xmx or os.environ.get("JAVA_XMX", "8G")
    java_args = java_args or os.environ.get("JAVA_ARGS")

    if output_base_dir.exists():
        shutil.rmtree(output_base_dir)
    output_base_dir.mkdir(parents=True, exist_ok=True)

    max_size_bytes = None
    if max_size_mb is not None:
        max_size_bytes = int(max_size_mb * 1024 * 1024)

    test_cases = []
    skipped_by_size = []
    for test_dir in all_test_dirs:
        if not test_dir.exists():
            print(f"Warning: {test_dir} does not exist. Skipping.")
            continue
        for entry in test_dir.rglob("*"):
            # Avoid processing files inside output directories or .git etc.
            if any(part.startswith(".") for part in entry.parts) or "output" in entry.as_posix():
                continue
            if entry.is_file() and entry.suffix == ".zip":
                if max_size_bytes is not None and entry.stat().st_size > max_size_bytes:
                    skipped_by_size.append(entry)
                    continue
                test_cases.append(entry)
            elif entry.is_dir() and is_gtfs_dir(entry):
                test_cases.append(entry)

    test_cases = sorted(list(set(test_cases)))
    print(f"Found {len(test_cases)} potential test cases.")
    if skipped_by_size:
        print(f"Skipped {len(skipped_by_size)} files larger than {max_size_mb} MB.")
        for skipped in skipped_by_size:
            print(f"  - {skipped}")

    results = []
    for i, test_path in enumerate(test_cases):
        parent_dir = None
        for td in all_test_dirs:
            try:
                test_path.relative_to(td)
                parent_dir = td
                break
            except ValueError:
                continue

        test_name = test_path.relative_to(parent_dir).as_posix().replace("/", "_")
        if test_path.suffix == ".zip":
            test_name += "_zip"

        print(f"[{i+1}/{len(test_cases)}] Benchmarking: {test_name}")

        case_output_dir = output_base_dir / test_name
        case_output_dir.mkdir(parents=True, exist_ok=True)

        # --- Java Run ---
        java_out = case_output_dir / "java"
        java_out.mkdir(parents=True, exist_ok=True)
        java_start = time.time()
        if java_args:
            java_cmd = [java_bin] + shlex.split(java_args) + [
                "-jar",
                str(validator_jar),
                "--input",
                str(test_path),
                "--output_base",
                str(java_out),
            ]
        else:
            java_cmd = [
                java_bin,
                f"-Xmx{java_xmx}",
                "-jar",
                str(validator_jar),
                "--input",
                str(test_path),
                "--output_base",
                str(java_out),
            ]
        java_success = False
        java_returncode = None
        try:
            proc = subprocess.run(java_cmd, capture_output=True, text=True, timeout=120)
            java_duration = time.time() - java_start
            java_returncode = proc.returncode
            java_success = java_returncode == 0
        except Exception:
            java_duration = 0

        # --- Rust (or Custom Runner) Run ---
        rust_out = case_output_dir / "rust"
        rust_out.mkdir(parents=True, exist_ok=True)
        rust_start = time.time()
        
        if runner_cmd:
            # Split command string into list for subprocess
            cmd_parts = shlex.split(runner_cmd)
            rust_cmd = cmd_parts + ["--input", str(test_path), "--output", str(rust_out)]
        else:
            rust_cmd = [str(rust_bin), "--input", str(test_path), "--output", str(rust_out)]
            
        rust_success = False
        try:
            # Increase timeout for custom runners (e.g. Node or Python startup)
            proc = subprocess.run(rust_cmd, capture_output=True, text=True, timeout=180)
            rust_duration = time.time() - rust_start
            if proc.returncode == 0:
                rust_success = True
            else:
                print(f"  Runner failed with code {proc.returncode}")
                # print(f"  Stderr: {proc.stderr}") 
        except Exception as e:
            print(f"  Runner execution error: {e}")
            rust_duration = 0

        # Analyze results
        java_notices = get_notices_per_file(java_out / "report.json") or {}
        java_system_errors = get_notices_per_file(java_out / "system_errors.json")
        java_notices = merge_file_notices(java_notices, java_system_errors)
        java_failure_reason = detect_java_failure(
            java_out, java_success, java_returncode
        )
        rust_notices = get_notices_per_file(rust_out / "report.json")
        if rust_notices is None:
            rust_notices = get_notices_per_file(rust_out / "validation_report.json") or {}
        rust_system_errors = get_notices_per_file(rust_out / "system_errors.json")
        rust_notices = merge_file_notices(rust_notices, rust_system_errors)

        java_total = total_notices(java_notices)
        rust_total = total_notices(rust_notices)

        print(
            f"  Java: {java_total} notices ({java_duration:.2f}s) | "
            f"Rust: {rust_total} notices ({rust_duration:.2f}s)"
        )

        match_by_file = java_notices == rust_notices
        match_by_code = (
            aggregate_notices_by_code(java_notices)
            == aggregate_notices_by_code(rust_notices)
        )
        if java_failure_reason:
            match_value = False
        else:
            match_value = match_by_code if match_by == "code" else match_by_file

        results.append(
            {
                "name": test_name,
                "path": str(test_path),
                "java": {
                    "success": java_success,
                    "returncode": java_returncode,
                    "total": java_total,
                    "duration": java_duration,
                    "notices": java_notices,
                    "failed": bool(java_failure_reason),
                    "failure_reason": java_failure_reason,
                },
                "rust": {
                    "success": rust_success,
                    "total": rust_total,
                    "duration": rust_duration,
                    "notices": rust_notices,
                },
                "match": match_value,
                "match_by_code": match_by_code,
                "match_by_file": match_by_file,
            }
        )
    return results


def parse_args():
    parser = argparse.ArgumentParser(
        description="Compare Rust and Java GTFS validator outputs."
    )
    parser.add_argument(
        "--summary-only",
        action="store_true",
        help="Rebuild summary using existing outputs without re-running validators.",
    )
    parser.add_argument(
        "--match-by",
        choices=["code", "file"],
        default="code",
        help="Compute match using notice codes only or per-file breakdown.",
    )
    parser.add_argument(
        "--runner",
        type=str,
        help="Custom command to run validation (e.g., 'python3 scripts/run_python_single.py'). Replaces the Rust binary.",
    )
    parser.add_argument(
        "--max-size-mb",
        type=float,
        help="Skip input zip files larger than this size in MB.",
    )
    parser.add_argument(
        "--java-xmx",
        type=str,
        help="Set Java max heap size (e.g. 8G). Defaults to JAVA_XMX env or 8G.",
    )
    parser.add_argument(
        "--java-args",
        type=str,
        help="Extra Java args to prepend before -jar (overrides JAVA_ARGS env).",
    )
    return parser.parse_args()


def main():
    args = parse_args()
    current_dir = Path.cwd()
    output_base_dir = current_dir / "output_all_tests_comparison"

    if args.summary_only:
        if not output_base_dir.exists():
            print(f"Output directory missing: {output_base_dir}")
            return
        results = rebuild_summary(output_base_dir, args.match_by)
    else:
        results = run_full_comparison(
            current_dir,
            match_by=args.match_by,
            runner_cmd=args.runner,
            max_size_mb=args.max_size_mb,
            java_xmx=args.java_xmx,
            java_args=args.java_args,
        )

    write_summary(output_base_dir, results, args.match_by)
    print(f"Benchmark complete. Results saved to {output_base_dir}")


if __name__ == "__main__":
    main()
