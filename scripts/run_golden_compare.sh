#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "Usage: $0 FEED_PATH EXPECTED_DIR ACTUAL_DIR [validator args...]" >&2
  echo "Env:" >&2
  echo "  GTFS_VALIDATOR_BIN: use a prebuilt binary instead of cargo." >&2
  echo "  COMPARE_EXTRA_JSON: extra JSON files to compare (space-separated)." >&2
  echo "  COMPARE_FLAGS: pass extra args to compare_reports.py." >&2
  echo "  COMPARE_HTML_NAME: override the HTML filename." >&2
  echo "  UPDATE_EXPECTED_ON_FAIL=1: copy actual outputs into expected on compare failure." >&2
  echo "See docs/golden.md for more details." >&2
  exit 1
fi

feed_path="$1"
expected_dir="$2"
actual_dir="$3"
shift 3

if [ -n "${GTFS_VALIDATOR_BIN:-}" ]; then
  "$GTFS_VALIDATOR_BIN" --input "$feed_path" --output_base "$actual_dir" "$@"
else
  cargo run -p gtfs_validator_cli -- --input "$feed_path" --output_base "$actual_dir" "$@"
fi

compare_args=()
if [ -n "${COMPARE_FLAGS:-}" ]; then
  read -r -a compare_args <<<"${COMPARE_FLAGS}"
fi
if [ -n "${COMPARE_EXTRA_JSON:-}" ]; then
  read -r -a extra_json <<<"${COMPARE_EXTRA_JSON}"
  for name in "${extra_json[@]}"; do
    compare_args+=("--extra-json" "${name}")
  done
fi
if [ -n "${COMPARE_HTML_NAME:-}" ]; then
  compare_args+=("--html-name" "${COMPARE_HTML_NAME}")
fi

if ! python3 "$(dirname "$0")/compare_reports.py" "$expected_dir" "$actual_dir" "${compare_args[@]}"; then
  if [ -n "${UPDATE_EXPECTED_ON_FAIL:-}" ]; then
    mkdir -p "$expected_dir"
    html_name_override=""
    if [ -n "${COMPARE_FLAGS:-}" ]; then
      read -r -a flag_parts <<<"${COMPARE_FLAGS}"
      for ((i=0; i<${#flag_parts[@]}; i++)); do
        if [[ "${flag_parts[$i]}" == "--html-name" && $((i+1)) -lt ${#flag_parts[@]} ]]; then
          html_name_override="${flag_parts[$((i+1))]}"
        elif [[ "${flag_parts[$i]}" == --html-name=* ]]; then
          html_name_override="${flag_parts[$i]#--html-name=}"
        fi
      done
    fi
    if [ -n "${COMPARE_HTML_NAME:-}" ]; then
      html_name_override="${COMPARE_HTML_NAME}"
    fi

    files=("report.json" "system_errors.json")
    if [ -n "${html_name_override}" ]; then
      files+=("${html_name_override}")
    else
      files+=("report.html")
    fi
    if [ -n "${COMPARE_EXTRA_JSON:-}" ]; then
      read -r -a extra_json <<<"${COMPARE_EXTRA_JSON}"
      files+=("${extra_json[@]}")
    fi
    for name in "${files[@]}"; do
      if [[ "$name" = /* || "$name" == *"../"* ]]; then
        continue
      fi
      source="${actual_dir}/${name}"
      target="${expected_dir}/${name}"
      if [ -f "$source" ]; then
        mkdir -p "$(dirname "$target")"
        cp "$source" "$target"
      fi
    done
    echo "Updated expected outputs."
  fi
  exit 1
fi
