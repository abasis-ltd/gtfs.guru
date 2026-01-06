#!/usr/bin/env python3
"""
Benchmark comparison script for Rust vs Java GTFS validators.

Usage:
    python3 scripts/benchmark_compare.py benchmark-feeds/nl.zip

This will run both validators and output a comparison table.
"""
import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path


def run_with_timing(cmd: list[str], label: str) -> tuple[float, str]:
    """Run command and return (wall_time, output)."""
    print(f"Running {label}...", file=sys.stderr)
    start = time.perf_counter()
    result = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.perf_counter() - start
    return elapsed, result.stdout + result.stderr


def run_rust_validator(input_path: Path, output_dir: Path, timing_json: bool = True) -> dict:
    """Run Rust validator and parse timing."""
    cmd = [
        "./target/release/gtfs-guru",
        "-i", str(input_path),
        "-o", str(output_dir),
    ]
    if timing_json:
        cmd.append("--timing-json")
    
    wall_time, output = run_with_timing(cmd, "Rust validator")
    
    # Parse timing JSON from stdout
    timing_data = {}
    try:
        # Find JSON in output
        lines = output.split("\n")
        json_start = None
        for i, line in enumerate(lines):
            if line.strip().startswith("{"):
                json_start = i
                break
        if json_start is not None:
            json_str = "\n".join(lines[json_start:])
            timing_data = json.loads(json_str)
    except json.JSONDecodeError:
        pass
    
    return {
        "wall_time": wall_time,
        "timing": timing_data,
        "raw_output": output,
    }


def run_java_validator(jar_path: Path, input_path: Path, output_dir: Path) -> dict:
    """Run Java validator and extract timing from logs."""
    cmd = [
        "java", "-jar", str(jar_path),
        "-i", str(input_path),
        "-o", str(output_dir),
    ]
    
    wall_time, output = run_with_timing(cmd, "Java validator")
    
    # Parse validation time from output
    validation_time = None
    for line in output.split("\n"):
        if "Validation took" in line:
            # Extract seconds: "Validation took 110.867 seconds"
            parts = line.split()
            for i, p in enumerate(parts):
                if p == "took" and i + 1 < len(parts):
                    try:
                        validation_time = float(parts[i + 1])
                    except ValueError:
                        pass
    
    return {
        "wall_time": wall_time,
        "validation_time": validation_time,
        "raw_output": output,
    }


def format_time(seconds: float) -> str:
    """Format time in human-readable way."""
    if seconds < 1:
        return f"{seconds*1000:.0f}ms"
    elif seconds < 60:
        return f"{seconds:.2f}s"
    else:
        return f"{int(seconds//60)}:{int(seconds%60):02d}"


def main():
    parser = argparse.ArgumentParser(description="Compare Rust vs Java GTFS validator performance")
    parser.add_argument("input", type=Path, help="GTFS input file (zip)")
    parser.add_argument("--jar", type=Path, default=Path("benchmark-feeds/gtfs-validator.jar"))
    parser.add_argument("--rust-binary", type=Path, default=Path("./target/release/gtfs-guru"))
    parser.add_argument("--output-dir", type=Path, default=Path("/tmp/benchmark"))
    parser.add_argument("--json", action="store_true", help="Output JSON instead of table")
    args = parser.parse_args()
    
    # Ensure output dirs exist
    rust_output = args.output_dir / "rust"
    java_output = args.output_dir / "java"
    rust_output.mkdir(parents=True, exist_ok=True)
    java_output.mkdir(parents=True, exist_ok=True)
    
    # Run validators
    rust_result = run_rust_validator(args.input, rust_output)
    java_result = run_java_validator(args.jar, args.input, java_output)
    
    if args.json:
        result = {
            "input": str(args.input),
            "rust": {
                "wall_time_s": rust_result["wall_time"],
                "timing": rust_result["timing"],
            },
            "java": {
                "wall_time_s": java_result["wall_time"],
                "validation_time_s": java_result["validation_time"],
            },
        }
        print(json.dumps(result, indent=2))
    else:
        # Format as table
        print("\n" + "=" * 60)
        print(f"Benchmark: {args.input.name}")
        print("=" * 60)
        print(f"{'Metric':<25} {'Rust':<15} {'Java':<15} {'Ratio':<10}")
        print("-" * 60)
        
        rust_wall = rust_result["wall_time"]
        java_wall = java_result["wall_time"]
        ratio = rust_wall / java_wall if java_wall > 0 else float("inf")
        print(f"{'Wall time':<25} {format_time(rust_wall):<15} {format_time(java_wall):<15} {ratio:.2f}x")
        
        # Extract timing details
        rust_loading = rust_result["timing"].get("loading", {}).get("total_s", 0)
        rust_validation = rust_result["timing"].get("validation", {}).get("total_s", 0)
        java_validation = java_result["validation_time"] or 0
        
        if rust_loading:
            print(f"{'Loading':<25} {format_time(rust_loading):<15} {'N/A':<15}")
        
        if rust_validation and java_validation:
            val_ratio = rust_validation / java_validation if java_validation > 0 else 0
            print(f"{'Validation':<25} {format_time(rust_validation):<15} {format_time(java_validation):<15} {val_ratio:.2f}x")
        
        print("=" * 60)
        
        # Top slow validators
        if rust_result["timing"].get("validation", {}).get("items"):
            print("\nTop 5 slowest validators (Rust):")
            items = sorted(
                rust_result["timing"]["validation"]["items"],
                key=lambda x: x["duration_s"],
                reverse=True
            )[:5]
            for item in items:
                print(f"  {item['name']:<40} {format_time(item['duration_s'])}")


if __name__ == "__main__":
    main()
