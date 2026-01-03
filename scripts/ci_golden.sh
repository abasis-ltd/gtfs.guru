#!/usr/bin/env bash
set -euo pipefail

manifest="${GOLDEN_MANIFEST:-scripts/golden_manifest.tsv}"
actual_dir="${GOLDEN_ACTUAL_DIR:-golden_actual}"
compare_flags="${COMPARE_FLAGS:---strip-runtime-fields --skip-html --ignore-notice-order --ignore-input --sort-summary-arrays --ignore-validator-version}"

if [ ! -f "$manifest" ]; then
  echo "Golden manifest not found: ${manifest} (set GOLDEN_MANIFEST to override)"
  echo "Skipping golden suite."
  exit 0
fi

COMPARE_FLAGS="${compare_flags}" scripts/golden.py suite "$manifest" "$actual_dir"
