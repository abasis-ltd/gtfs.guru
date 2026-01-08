import argparse
import json
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


def write_summary(output_base_dir, results):
    summary_path = output_base_dir / "summary_all_tests.json"
    with summary_path.open("w") as f:
        json.dump(results, f, indent=2)

    report_path = output_base_dir / "report.md"
    with report_path.open("w") as f:
        f.write("# GTFS Validator Parity Report\n\n")
        f.write(f"Tested {len(results)} feeds.\n\n")
        f.write("| Feed | Java Total | Rust Total | Match | Duration (J/R) |\n")
        f.write("| :--- | :---: | :---: | :---: | :---: |\n")
        for r in results:
            m = "✅" if r["match"] else "❌"
            f.write(
                f"| {r['name']} | {r['java']['total']} | {r['rust']['total']} | {m} "
                f"| {r['java']['duration']:.2f}s / {r['rust']['duration']:.2f}s |\n"
            )


def rebuild_summary(output_base_dir):
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
        if java_notices is None:
            java_notices = java_meta.get("notices") or {}

        rust_notices = get_notices_per_file(rust_report)
        if rust_notices is None and rust_alt_report.exists():
            rust_notices = get_notices_per_file(rust_alt_report)
        if rust_notices is None:
            rust_notices = rust_meta.get("notices") or {}

        java_total = total_notices(java_notices)
        rust_total = total_notices(rust_notices)

        results.append(
            {
                "name": name,
                "path": old.get("path", ""),
                "java": {
                    "success": java_meta.get("success", False),
                    "total": java_total,
                    "duration": java_meta.get("duration", 0),
                    "notices": java_notices,
                },
                "rust": {
                    "success": rust_meta.get("success", False),
                    "total": rust_total,
                    "duration": rust_meta.get("duration", 0),
                    "notices": rust_notices,
                },
                "match": java_notices == rust_notices,
            }
        )
    return results


def run_full_comparison(current_dir):
    all_test_dirs = [
        current_dir / "mobility-data-test-feeds",
        current_dir / "test-gtfs-feeds",
        current_dir / "benchmark-feeds",
    ]
    output_base_dir = current_dir / "output_all_tests_comparison"
    validator_jar = current_dir / "benchmark-feeds/gtfs-validator.jar"
    rust_bin = current_dir / "target/release/gtfs-guru"
    java_bin = "java"

    if output_base_dir.exists():
        shutil.rmtree(output_base_dir)
    output_base_dir.mkdir(parents=True, exist_ok=True)

    test_cases = []
    for test_dir in all_test_dirs:
        if not test_dir.exists():
            print(f"Warning: {test_dir} does not exist. Skipping.")
            continue
        for entry in test_dir.rglob("*"):
            # Avoid processing files inside output directories or .git etc.
            if any(part.startswith(".") for part in entry.parts) or "output" in entry.as_posix():
                continue
            if entry.is_file() and entry.suffix == ".zip":
                test_cases.append(entry)
            elif entry.is_dir() and is_gtfs_dir(entry):
                test_cases.append(entry)

    test_cases = sorted(list(set(test_cases)))
    print(f"Found {len(test_cases)} potential test cases.")

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
        java_cmd = [
            java_bin,
            "-jar",
            str(validator_jar),
            "--input",
            str(test_path),
            "--output_base",
            str(java_out),
        ]
        java_success = False
        try:
            proc = subprocess.run(java_cmd, capture_output=True, text=True, timeout=120)
            java_duration = time.time() - java_start
            if proc.returncode == 0:
                java_success = True
        except Exception:
            java_duration = 0

        # --- Rust Run ---
        rust_out = case_output_dir / "rust"
        rust_out.mkdir(parents=True, exist_ok=True)
        rust_start = time.time()
        rust_cmd = [str(rust_bin), "--input", str(test_path), "--output", str(rust_out)]
        rust_success = False
        try:
            proc = subprocess.run(rust_cmd, capture_output=True, text=True, timeout=120)
            rust_duration = time.time() - rust_start
            if proc.returncode == 0:
                rust_success = True
        except Exception:
            rust_duration = 0

        # Analyze results
        java_notices = get_notices_per_file(java_out / "report.json") or {}
        rust_notices = get_notices_per_file(rust_out / "report.json")
        if rust_notices is None:
            rust_notices = get_notices_per_file(rust_out / "validation_report.json") or {}

        java_total = total_notices(java_notices)
        rust_total = total_notices(rust_notices)

        print(
            f"  Java: {java_total} notices ({java_duration:.2f}s) | "
            f"Rust: {rust_total} notices ({rust_duration:.2f}s)"
        )

        results.append(
            {
                "name": test_name,
                "path": str(test_path),
                "java": {
                    "success": java_success,
                    "total": java_total,
                    "duration": java_duration,
                    "notices": java_notices,
                },
                "rust": {
                    "success": rust_success,
                    "total": rust_total,
                    "duration": rust_duration,
                    "notices": rust_notices,
                },
                "match": java_notices == rust_notices,
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
    return parser.parse_args()


def main():
    args = parse_args()
    current_dir = Path.cwd()
    output_base_dir = current_dir / "output_all_tests_comparison"

    if args.summary_only:
        if not output_base_dir.exists():
            print(f"Output directory missing: {output_base_dir}")
            return
        results = rebuild_summary(output_base_dir)
    else:
        results = run_full_comparison(current_dir)

    write_summary(output_base_dir, results)
    print(f"Benchmark complete. Results saved to {output_base_dir}")


if __name__ == "__main__":
    main()
