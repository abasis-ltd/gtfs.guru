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

if __name__ == "__main__":
    try:
        import gtfs_guru
        import json
        
        gtfs_path = "test-gtfs-feeds/base-valid.zip"
        if not os.path.exists(gtfs_path):
            print(f"Error: {gtfs_path} not found")
            sys.exit(1)

        print(f"Validating {gtfs_path}...")
        
        result = gtfs_guru.validate(gtfs_path, date="2024-01-01")
        json_output = result.to_json()
        
        with open("output_python.json", "w") as f:
            f.write(json_output)
        print("Wrote output to output_python.json")

    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)
