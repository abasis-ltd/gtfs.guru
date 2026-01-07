#!/usr/bin/env python3
"""
Benchmark comparison script for Rust vs Java GTFS validators.

Usage:
    python3 scripts/benchmark_compare.py benchmark-feeds/nl.zip --iterations 3 --warmup

This will run both validators multiple times and output a comparison table with statistics.
"""
import argparse
import json
import os
import subprocess
import sys
import time
import statistics
from pathlib import Path


def run_with_timing(cmd: list[str], label: str) -> tuple[float, str]:
    """Run command and return (wall_time, output)."""
    start = time.perf_counter()
    result = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.perf_counter() - start
    return elapsed, result.stdout + result.stderr


def run_rust_validator(rust_binary: Path, input_path: Path, output_dir: Path, timing_json: bool = True) -> dict:
    """Run Rust validator and parse timing."""
    cmd = [
        str(rust_binary),
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
            # Filter out lines after JSON if any
            if "}" in json_str:
                json_str = json_str[:json_str.rfind("}")+1]
            timing_data = json.loads(json_str)
    except (json.JSONDecodeError, ValueError):
        pass
    
    return {
        "wall_time": wall_time,
        "timing": timing_data,
        "raw_output": output,
    }


def run_java_validator(jar_path: Path, input_path: Path, output_dir: Path) -> dict:
    """Run Java validator and extract timing from logs."""
    cmd = [
        "java", "-Xmx8G", "-jar", str(jar_path),
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
    if seconds < 0.001:
        return f"{seconds*1e6:.0f}µs"
    elif seconds < 1:
        return f"{seconds*1000:.1f}ms"
    elif seconds < 60:
        return f"{seconds:.2f}s"
    else:
        return f"{int(seconds//60)}:{int(seconds%60):05.2f}"


def get_stats(data: list[float]) -> dict:
    if not data:
        return {"min": 0, "max": 0, "mean": 0, "median": 0}
    return {
        "min": min(data),
        "max": max(data),
        "mean": statistics.mean(data),
        "median": statistics.median(data),
    }


def main():
    parser = argparse.ArgumentParser(description="Compare Rust vs Java GTFS validator performance")
    parser.add_argument("input", type=Path, help="GTFS input file (zip)")
    parser.add_argument("--jar", type=Path, default=Path("benchmark-feeds/gtfs-validator.jar"))
    parser.add_argument("--rust-binary", type=Path, default=Path("./target/release/gtfs-guru"))
    parser.add_argument("--output-dir", type=Path, default=Path("tmp/benchmark"))
    parser.add_argument("--iterations", type=int, default=3, help="Number of measured iterations")
    parser.add_argument("--warmup", action="store_true", help="Perform a warm-up run")
    parser.add_argument("--json", action="store_true", help="Output JSON instead of table")
    args = parser.parse_args()
    
    if not args.input.exists():
        print(f"Error: Input file {args.input} not found")
        sys.exit(1)
    
    # Ensure output dirs exist
    rust_output = args.output_dir / "rust"
    java_output = args.output_dir / "java"
    rust_output.mkdir(parents=True, exist_ok=True)
    java_output.mkdir(parents=True, exist_ok=True)
    
    if args.warmup:
        print(f"Performing warm-up for {args.input.name}...", file=sys.stderr)
        run_rust_validator(args.rust_binary, args.input, rust_output, timing_json=False)
        run_java_validator(args.jar, args.input, java_output)

    rust_walls = []
    rust_loads = []
    rust_vals = []
    java_walls = []
    java_vals = []

    print(f"Running {args.iterations} iterations for {args.input.name}...", file=sys.stderr)
    for i in range(args.iterations):
        print(f"  Iteration {i+1}/{args.iterations}...", file=sys.stderr)
        
        # Rust
        r = run_rust_validator(args.rust_binary, args.input, rust_output)
        rust_walls.append(r["wall_time"])
        rust_loads.append(r["timing"].get("loading", {}).get("total_s", 0))
        rust_vals.append(r["timing"].get("validation", {}).get("total_s", 0))
        
        # Java
        j = run_java_validator(args.jar, args.input, java_output)
        java_walls.append(j["wall_time"])
        java_vals.append(j["validation_time"] or 0)

    rust_stats = {
        "wall": get_stats(rust_walls),
        "load": get_stats(rust_loads),
        "val": get_stats(rust_vals),
    }
    java_stats = {
        "wall": get_stats(java_walls),
        "val": get_stats(java_vals),
    }

    if args.json:
        result = {
            "input": str(args.input),
            "iterations": args.iterations,
            "rust": rust_stats,
            "java": java_stats,
        }
        print(json.dumps(result, indent=2))
    else:
        # Format as table
        print("\n" + "=" * 80)
        print(f"Benchmark: {args.input.name} ({args.iterations} iterations)")
        print("=" * 80)
        print(f"{'Metric':<20} | {'Rust (Median)':<15} | {'Java (Median)':<15} | {'Ratio':<10}")
        print("-" * 80)
        
        rw_med = rust_stats["wall"]["median"]
        jw_med = java_stats["wall"]["median"]
        w_ratio = jw_med / rw_med if rw_med > 0 else 0
        print(f"{'Wall Time':<20} | {format_time(rw_med):<15} | {format_time(jw_med):<15} | {w_ratio:.2f}x (Rust faster)")
        
        rv_med = rust_stats["val"]["median"]
        jv_med = java_stats["val"]["median"]
        v_ratio = jv_med / rv_med if rv_med > 0 else 0
        print(f"{'Validation Only':<20} | {format_time(rv_med):<15} | {format_time(jv_med):<15} | {v_ratio:.2f}x (Rust faster)")
        
        rl_med = rust_stats["load"]["median"]
        print(f"{'Loading (Rust)':<20} | {format_time(rl_med):<15} | {'N/A':<15} |")
        
        print("-" * 80)
        print(f"Rust Wall Range: {format_time(rust_stats['wall']['min'])} - {format_time(rust_stats['wall']['max'])}")
        print(f"Java Wall Range: {format_time(java_stats['wall']['min'])} - {format_time(java_stats['wall']['max'])}")
        print("=" * 80)


if __name__ == "__main__":
    main()
