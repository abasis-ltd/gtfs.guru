import os
import subprocess
import json
import glob
from pathlib import Path

# Paths
current_dir = Path.cwd()
tests_data_dir = current_dir / "test-gtfs-feeds/google_transitfeed/tests/data"
output_base_dir = current_dir / "output_google_tests"
validator_bin = "cargo"
validator_args = ["run", "--release", "-p", "gtfs-guru-cli", "--"]

# Ensure output directory exists
output_base_dir.mkdir(parents=True, exist_ok=True)

# Find all test cases (directories and zip files)
# We prioritize directories, but if a zip exists with the same name, we might want to pick one.
# Looking at the file list, most are directories.
test_cases = []
for entry in tests_data_dir.iterdir():
    if entry.name.startswith("."):
        continue
    if entry.is_dir():
        test_cases.append(entry)
    elif entry.is_file() and entry.suffix == ".zip":
        # Check if corresponding dir exists, if so skip zip (or vice versa? usually dir is unzipped)
        # The list showed "good_feed" (dir) and "good_feed.zip".
        # Let's prefer directory if available as it is easier to inspect if needed,
        # but the validator accepts both. I'll add them all, but skip zip if dir exists.
        if not (tests_data_dir / entry.stem).exists():
            test_cases.append(entry)

test_cases.sort(key=lambda x: x.name)

metadata = {}
results = []

print(f"Found {len(test_cases)} test cases.")

for test_path in test_cases:
    test_name = test_path.stem
    if test_path.suffix == ".zip":
        test_name += "_zip" # differentiate

    # Skip kml files or likely non-feed directories if any known ignore list existed.
    # But based on names, they look like tests. "bad_eol.zip" etc.

    print(f"Running test: {test_name}...")
    
    case_output_dir = output_base_dir / test_name
    # Clean if exists? GTFS validator might overwrite or fail.
    # We let it overwrite.

    cmd = [validator_bin] + validator_args + [
        "--input", str(test_path),
        "--output_base", str(case_output_dir)
    ]

    try:
        # Run the command
        process = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=120 # 2 minutes max per small test feed
        )

        success = process.returncode == 0
        
        # Parse result (validation_report.json) if successful
        report_file = case_output_dir / "validation_report.json"
        
        notice_count = 0
        notices = []
        if success and report_file.exists():
            try:
                with open(report_file, 'r') as f:
                    report_data = json.load(f)
                    # Use .get() with default empty list to handle missing "notices" key safely
                    notices_list = report_data.get("notices", [])
                    notice_count = len(notices_list)
                    # Capture unique notice codes for summary
                    notices = list(set([n.get("code") for n in notices_list]))
            except Exception as e:
                print(f"Failed to read report for {test_name}: {e}")

        result_entry = {
            "name": test_name,
            "success": success,
            "return_code": process.returncode,
            "notice_count": notice_count,
            "notice_codes": notices,
            "error_log": process.stderr if not success else ""
        }
        results.append(result_entry)
        
        status = "PASS" if success else "FAIL"
        print(f"  Result: {status} (Notices: {notice_count})")

    except subprocess.TimeoutExpired:
        print(f"  TIMED OUT")
        results.append({
            "name": test_name,
            "success": False,
            "return_code": -1,
            "error_log": "Timeout"
        })
    except Exception as e:
        print(f"  EXCEPTION: {e}")

# Save summary
with open(output_base_dir / "summary.json", 'w') as f:
    json.dump(results, f, indent=2)

print("Done. Summary saved to output_google_tests/summary.json")
