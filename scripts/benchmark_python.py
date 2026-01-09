import gtfs_guru as gtfs_validator
import time
import sys

def main():
    if len(sys.argv) < 2:
        print("Usage: python benchmark_python.py <path_to_gtfs.zip>")
        sys.exit(1)

    gtfs_path = sys.argv[1]
    
    print(f"Benchmarking Python validator on {gtfs_path}...")
    
    start_time = time.time()
    result = gtfs_validator.validate(gtfs_path)
    total_time = time.time() - start_time
    
    print(f"Validation complete.")
    print(f"Internal validation time: {result.validation_time_seconds:.4f}s")
    print(f"Total time (including library overhead): {total_time:.4f}s")
    print(f"Errors: {result.error_count}")
    print(f"Warnings: {result.warning_count}")

if __name__ == "__main__":
    main()
