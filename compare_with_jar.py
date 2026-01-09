import json
import sys

def load_json(path):
    with open(path, 'r') as f:
        return json.load(f)

def parse_notices(report):
    counts = {}
    notices = report.get('notices', [])
    for group in notices:
        code = group.get('code')
        count = group.get('totalNotices', 0)
        if code:
            counts[code] = count
    return counts

def main():
    try:
        cli_data = load_json('cli_report.json/report.json')
        jar_data = load_json('jar_report/report.json')
    except Exception as e:
        print(f"Error loading files: {e}")
        sys.exit(1)

    cli_counts = parse_notices(cli_data)
    jar_counts = parse_notices(jar_data)

    print("--- CLI vs Jar Comparison ---")
    all_codes = set(cli_counts.keys()) | set(jar_counts.keys())
    
    match = True

    for code in sorted(all_codes):
        c_count = cli_counts.get(code, 0)
        j_count = jar_counts.get(code, 0)
        
        if c_count == j_count:
            print(f"MATCH: {code} ({c_count})")
        else:
            print(f"MISMATCH: {code} - CLI: {c_count}, Jar: {j_count}")
            match = False

    if match:
        print("\nSUCCESS: All counts match.")
    else:
        print("\nNote: Differences expected between Rust and Java implementations.")

if __name__ == "__main__":
    main()
