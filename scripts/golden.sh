#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "Usage: $0 MANIFEST_TSV ACTUAL_ROOT" >&2
  echo "Set UPDATE_EXPECTED=1 to regenerate expected outputs." >&2
  echo "Set UPDATE_EXPECTED_FLAGS to pass flags to update_expected_from_manifest.py." >&2
  exit 1
fi

manifest="$1"
actual_root="$2"

if [ -n "${UPDATE_EXPECTED:-}" ]; then
  update_flags="${UPDATE_EXPECTED_FLAGS:---overwrite}"
  scripts/update_expected_from_manifest.py ${update_flags} "$manifest"
fi

scripts/run_golden_suite.sh "$manifest" "$actual_root"
