#!/usr/bin/env python3
import argparse
import os
import subprocess
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run a single golden case with optional expected update.",
    )
    parser.add_argument("feed_path", type=Path)
    parser.add_argument("expected_dir", type=Path)
    parser.add_argument("actual_dir", type=Path)
    parser.add_argument("validator_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    if not args.feed_path.exists():
        print(f"Feed not found: {args.feed_path}")
        return 1

    cmd = [
        "python3",
        str(Path(__file__).parent / "run_golden_compare.py"),
        str(args.feed_path),
        str(args.expected_dir),
        str(args.actual_dir),
    ] + args.validator_args

    env = os.environ.copy()
    if env.get("UPDATE_EXPECTED"):
        env["UPDATE_EXPECTED_ON_FAIL"] = "1"

    return subprocess.call(cmd, env=env)


if __name__ == "__main__":
    raise SystemExit(main())
