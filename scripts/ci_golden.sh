#!/usr/bin/env bash
set -euo pipefail

manifest="${GOLDEN_MANIFEST:-scripts/golden_manifest.ci.tsv}"
actual_dir="${GOLDEN_ACTUAL_DIR:-golden_actual}"
compare_flags="${COMPARE_FLAGS:---strip-runtime-fields --skip-html --ignore-notice-order --ignore-input --sort-summary-arrays --ignore-validator-version --float-precision 12}"

if [ ! -f "$manifest" ]; then
  echo "Golden manifest not found: ${manifest} (set GOLDEN_MANIFEST to override)" >&2
  exit 1
fi

COMPARE_FLAGS="${compare_flags}" scripts/golden.py suite "$manifest" "$actual_dir"
