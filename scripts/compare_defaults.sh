#!/usr/bin/env bash
set -euo pipefail

COMPARE_FLAGS="${COMPARE_FLAGS:---strip-runtime-fields --skip-html --ignore-notice-order --ignore-input --sort-summary-arrays --ignore-validator-version}"
compare_args=()
if [ -n "${COMPARE_FLAGS}" ]; then
  read -r -a compare_args <<<"${COMPARE_FLAGS}"
fi

if [ "$#" -eq 0 ]; then
  echo "${COMPARE_FLAGS}"
  exit 0
fi

exec python3 scripts/compare_reports.py "$@" "${compare_args[@]}"
