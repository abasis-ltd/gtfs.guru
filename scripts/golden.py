#!/usr/bin/env python3
import argparse
import os
import shlex
import subprocess
from pathlib import Path


def call_script(name: str, args, env=None) -> int:
    script = Path(__file__).parent / name
    cmd = ["python3", str(script)] + args
    return subprocess.call(cmd, env=env)


def parse_split(value: str):
    if not value:
        return []
    return [item for item in shlex.split(value) if item]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Golden workflow helper (suite/single/validate/update). See docs/golden.md.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    suite = sub.add_parser("suite", help="Run golden suite (Python runner).")
    suite.add_argument("manifest", type=Path)
    suite.add_argument("actual_root", type=Path)
    suite.add_argument("validator_args", nargs=argparse.REMAINDER)

    single = sub.add_parser("single", help="Run single golden compare.")
    single.add_argument("feed_path", type=Path)
    single.add_argument("expected_dir", type=Path)
    single.add_argument("actual_dir", type=Path)
    single.add_argument("validator_args", nargs=argparse.REMAINDER)

    validate = sub.add_parser("validate", help="Validate manifest.")
    validate.add_argument("manifest", type=Path)
    validate.add_argument("flags", nargs=argparse.REMAINDER)

    update = sub.add_parser("update", help="Update expected outputs.")
    update.add_argument("manifest", type=Path)
    update.add_argument("validator_args", nargs=argparse.REMAINDER)

    args = parser.parse_args()

    if args.command == "suite":
        env = os.environ.copy()
        if env.get("UPDATE_EXPECTED"):
            update_flags = parse_split(env.get("UPDATE_EXPECTED_FLAGS", ""))
            if not update_flags:
                update_flags = ["--overwrite"]
            update_args = (
                update_flags + [str(args.manifest)] + args.validator_args
            )
            result = call_script(
                "update_expected_from_manifest.py", update_args
            )
            if result != 0:
                return result
        return call_script(
            "run_golden_suite.py",
            [str(args.manifest), str(args.actual_root)] + args.validator_args,
        )
    if args.command == "single":
        env = os.environ.copy()
        if env.get("UPDATE_EXPECTED"):
            env["UPDATE_EXPECTED_ON_FAIL"] = "1"
        return call_script(
            "run_golden_compare.py",
            [
                str(args.feed_path),
                str(args.expected_dir),
                str(args.actual_dir),
            ]
            + args.validator_args,
            env=env,
        )
    if args.command == "validate":
        return call_script(
            "validate_golden_manifest.py",
            [str(args.manifest)] + args.flags,
        )
    if args.command == "update":
        return call_script(
            "update_expected_from_manifest.py",
            [str(args.manifest)] + args.validator_args,
        )

    return 1


if __name__ == "__main__":
    raise SystemExit(main())
