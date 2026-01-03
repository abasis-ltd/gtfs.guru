#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "Usage: $0 FEED_PATH EXPECTED_DIR ACTUAL_DIR" >&2
  echo "Set UPDATE_EXPECTED=1 to regenerate expected outputs." >&2
  exit 1
fi

feed_path="$1"
expected_dir="$2"
actual_dir="$3"

if [ -n "${UPDATE_EXPECTED:-}" ]; then
  UPDATE_EXPECTED_ON_FAIL=1 scripts/run_golden_compare.sh "$feed_path" "$expected_dir" "$actual_dir"
  exit $?
fi

scripts/run_golden_compare.sh "$feed_path" "$expected_dir" "$actual_dir"
