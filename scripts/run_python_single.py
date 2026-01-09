import argparse
import sys
import time
from pathlib import Path

try:
    import gtfs_guru as gtfs_validator
except ImportError:
    print("Error: gtfs_guru python package not found. Please install it first.")
    sys.exit(1)

def main():
    parser = argparse.ArgumentParser(description="Run GTFS validation via Python bindings.")
    parser.add_argument("--input", required=True, help="Path to input GTFS zip file.")
    parser.add_argument("--output", required=True, help="Path to output directory.")
    args = parser.parse_args()

    input_path = Path(args.input)
    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"Validating {input_path}...")
    
    start_time = time.time()
    # The pyo3 bindings return a ValidationResult object
    try:
        result = gtfs_validator.validate(str(input_path))
        validation_time = result.validation_time_seconds
    except Exception as e:
        print(f"Validation failed: {e}")
        sys.exit(1)
        
    total_time = time.time() - start_time

    # Save the JSON report using the save_json method
    output_file = output_dir / "report.json"
    result.save_json(str(output_file))

    print(f"Report written to {output_file}")
    print(f"Time: {validation_time:.2f}s (internal), {total_time:.2f}s (total)")

if __name__ == "__main__":
    main()
