import os
import sys

# Add release directory to path so we can import the .dylib/.so
sys.path.append(os.path.abspath("target/release"))

# On macOS, pyo3 produces .dylib, but Python expects .so
# We'll create a symlink if it doesn't exist
dylib_path = "target/release/libgtfs_guru.dylib"
so_path = "target/release/gtfs_guru.so"

if os.path.exists(dylib_path) and not os.path.exists(so_path):
    print(f"Creating symlink from {dylib_path} to {so_path}")
    os.symlink(os.path.abspath(dylib_path), so_path)

try:
    import gtfs_guru
    print(f"Successfully imported gtfs_guru version: {gtfs_guru.version()}")
    
    # Test validation on a small file if possible
    # We'll use the one we were testing before: tmp/gtfs_export (6).zip
    gtfs_path = "tmp/gtfs_export (6).zip"
    
    if os.path.exists(gtfs_path):
        print(f"Validating {gtfs_path}...")
        result = gtfs_guru.validate(gtfs_path)
        print(f"Validation Result: {result}")
        print(f"Is valid: {result.is_valid}")
        print(f"Errors: {result.error_count}")
        print(f"Warnings: {result.warning_count}")
        print(f"Infos: {result.info_count}")
        
        # Check first 5 notices
        print("\nFirst 5 notices:")
        for i, notice in enumerate(result.notices[:5]):
            print(f"[{i}] {notice.severity} {notice.code}: {notice.message} (file: {notice.file}, row: {notice.row})")
            
        # Check by_code
        expired = result.by_code("expired_calendar")
        print(f"\nFound {len(expired)} expired_calendar notices via by_code")
        
    else:
        print(f"GTFS file not found at {gtfs_path}, skipping validation test.")
        
except Exception as e:
    print(f"Error during verification: {e}")
    sys.exit(1)
