import os
import subprocess
import json
import shutil
from pathlib import Path
import time

# Paths
current_dir = Path.cwd()
tests_data_dir = current_dir / "mobility-data-test-feeds"
output_base_dir = current_dir / "output_benchmark_comparison"
validator_jar = current_dir / "benchmark-feeds/gtfs-validator.jar"
rust_bin = "target/release/gtfs-guru"
java_bin = "java"

# Ensure output directory exists and is clean
if output_base_dir.exists():
    shutil.rmtree(output_base_dir)
output_base_dir.mkdir(parents=True, exist_ok=True)

# Find all test cases (zip files and directories that look like feeds)
test_cases = []

def is_gtfs_dir(path):
    # Heuristic: contains txt files or is a known test dir
    if not path.is_dir(): return False
    return any(f.suffix == ".txt" for f in path.iterdir())

# Recursive search for test cases
print(f"Searching for test cases in {tests_data_dir}...")
for root, dirs, files in os.walk(tests_data_dir):
    root_path = Path(root)
    # Check for zip files
    for file in files:
        if file.endswith(".zip"):
            test_cases.append(root_path / file)
    
    # Check for directories that might be unzipped feeds
    # We avoid recursing into them if we treat them as a feed
    # But os.walk recurses anyway. We need to be careful not to double count.
    # For this pass, let's treat any directory containing .txt files as a potential feed
    # IF it's not a parent of another feed we strictly define.
    # Actually, mobility-data-test-feeds structure is nested.
    # Let's rely on leaf directories or zips.
    
    # Simplified approach: valid test cases usually have specific names or structure
    # Let's inspect known subdirs
    pass

# Refined search based on directory structure seen in list_dir
# mobility-data-test-feeds has: OpenTripPlanner-data, gtfs-realtime-validator-data, sample-gtfs-feed, transitfeed-data, transitland-data
# We will walk and add any zip or dir that seems relevant.
for entry in tests_data_dir.rglob("*"):
    if entry.is_file() and entry.suffix == ".zip":
        test_cases.append(entry)
    elif entry.is_dir() and is_gtfs_dir(entry):
        # Avoid adding parent directories if they contain zips that we already added? 
        # Or if they are just containers.
        # Let's just add it. The validator will fail if it's invalid.
        test_cases.append(entry)

# Remove duplicates (e.g. if we added a dir that is inside another dir, maybe ok)
test_cases = sorted(list(set(test_cases)))
print(f"Found {len(test_cases)} potential test cases.")

results = []

for i, test_path in enumerate(test_cases):
    test_name = test_path.relative_to(tests_data_dir).as_posix().replace("/", "_")
    if test_path.suffix == ".zip":
        test_name += "_zip"
    
    print(f"[{i+1}/{len(test_cases)}] Benchmarking: {test_name}")
    
    case_output_dir = output_base_dir / test_name
    case_output_dir.mkdir(parents=True, exist_ok=True)
    
    # --- Java Run ---
    java_start = time.time()
    java_cmd = [
        java_bin, "-jar", str(validator_jar),
        "--input", str(test_path),
        "--output_base", str(case_output_dir / "java")
    ]
    java_success = False
    java_notices_count = 0
    java_codes = []
    
    try:
        proc = subprocess.run(java_cmd, capture_output=True, text=True, timeout=60)
        java_duration = time.time() - java_start
        if proc.returncode == 0:
            java_success = True
            report_path = case_output_dir / "java" / "report.json"
            if report_path.exists():
                with open(report_path) as f:
                    data = json.load(f)
                    notices = data.get("notices", [])
                    java_notices_count = len(notices)
                    java_codes = sorted(list(set(n.get("code") for n in notices)))
    except Exception as e:
        print(f"  Java failed: {e}")
        java_duration = 0

    # --- Rust Run ---
    rust_start = time.time()
    rust_cmd = [
        rust_bin,
        "--input", str(test_path),
        "--output-base", str(case_output_dir / "rust") # Note: rust might use underscore or dash, checking help implies output_base or similar. 
        # Checking CLI: usually --output-base. Let's assume standard.
        # Wait, previous script used --output_base. Let's double check CLI args if it fails.
    ]
    # Adjust for rust cli args if needed. Assuming standard `gtfs-guru --input <X> --output <Y>` or similar.
    # Previous script `compare_google_tests.py` used `run --release ... -- --input ...`
    # We are running binary directly.
    rust_cmd = [rust_bin, "--input", str(test_path), "--output", str(case_output_dir / "rust")]

    rust_success = False
    rust_notices_count = 0
    rust_codes = []
    
    try:
        proc = subprocess.run(rust_cmd, capture_output=True, text=True, timeout=60)
        rust_duration = time.time() - rust_start
        if proc.returncode == 0:
            rust_success = True
            report_path = case_output_dir / "rust" / "report.json" # Rust might output validation_report.json
            if not report_path.exists():
                report_path = case_output_dir / "rust" / "validation_report.json"
            
            if report_path.exists():
                with open(report_path) as f:
                    data = json.load(f)
                    notices = data.get("notices", [])
                    rust_notices_count = len(notices)
                    rust_codes = sorted(list(set(n.get("code") for n in notices)))
    except Exception as e:
        print(f"  Rust failed: {e}")
        rust_duration = 0
        
    print(f"  Java: {java_notices_count} notices ({java_duration:.2f}s) | Rust: {rust_notices_count} notices ({rust_duration:.2f}s)")
    if java_codes != rust_codes:
         print(f"  MISMATCH CODES: Java={java_codes} Rust={rust_codes}")

    results.append({
        "name": test_name,
        "java": {
            "success": java_success,
            "count": java_notices_count,
            "duration": java_duration,
            "codes": java_codes
        },
        "rust": {
            "success": rust_success,
            "count": rust_notices_count,
            "duration": rust_duration,
            "codes": rust_codes
        },
        "match": java_codes == rust_codes and java_notices_count == rust_notices_count
    })

# Save Summary
with open(output_base_dir / "summary_comparison.json", "w") as f:
    json.dump(results, f, indent=2)

print(f"Benchmark complete. Results saved to {output_base_dir}/summary_comparison.json")
