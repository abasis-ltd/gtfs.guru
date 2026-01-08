import json
from collections import Counter
from pathlib import Path

output_dir = Path("output_benchmark_comparison")
summary_file = output_dir / "summary_comparison.json"

with open(summary_file, 'r') as f:
    results = json.load(f)

total_tests = len(results)
matches = sum(1 for r in results if r['match'])
mismatches = total_tests - matches

print(f"Total Tests: {total_tests}")
print(f"Matches: {matches}")
print(f"Mismatches: {mismatches}")

rust_only_counts = Counter()
java_only_counts = Counter()

for r in results:
    if not r['match']:
        java_codes = set(r['java']['codes'])
        rust_codes = set(r['rust']['codes'])
        
        for code in java_codes - rust_codes:
            java_only_counts[code] += 1
        for code in rust_codes - java_codes:
            rust_only_counts[code] += 1

print("\n## Rust Only (Java missed):")
for code, count in rust_only_counts.most_common(15):
    print(f"  {code}: {count}")

print("\n## Java Only (Rust missed):")
for code, count in java_only_counts.most_common(15):
    print(f"  {code}: {count}")
