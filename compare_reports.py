import json
import sys

def load_json(path):
    with open(path, 'r') as f:
        return json.load(f)

def aggregate_wasm_notices(notices_list):
    """
    Aggregates a flat list of notices by code.
    Returns a dict: {code: count}
    """
    counts = {}
    for notice in notices_list:
        code = notice.get('code')
        if code:
            counts[code] = counts.get(code, 0) + 1
    return counts

def parse_cli_notices(cli_report):
    """
    Parses CLI report notices.
    Returns a dict: {code: totalNotices}
    """
    counts = {}
    notices = cli_report.get('notices', [])
    for group in notices:
        code = group.get('code')
        count = group.get('totalNotices', 0)
        if code:
            counts[code] = count
    return counts

def main():
    try:
        cli_data = load_json('cli_report.json/report.json')
        wasm_data = load_json('wasm_report.json')
    except Exception as e:
        print(f"Error loading files: {e}")
        sys.exit(1)

    wasm_counts = aggregate_wasm_notices(wasm_data)
    cli_counts = parse_cli_notices(cli_data)

    print("--- Comparison Results ---")
    
    all_codes = set(wasm_counts.keys()) | set(cli_counts.keys())
    match = True

    for code in sorted(all_codes):
        c_count = cli_counts.get(code, 0)
        w_count = wasm_counts.get(code, 0)
        
        if c_count == w_count:
            print(f"MATCH: {code} (Count: {c_count})")
        else:
            print(f"MISMATCH: {code} - CLI: {c_count}, WASM: {w_count}")
            match = False
            
    if match:
        print("\nSUCCESS: Results are identical.")
    else:
        print("\nFAILURE: Results differ.")
        sys.exit(1)

if __name__ == "__main__":
    main()
