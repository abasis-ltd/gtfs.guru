#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "Usage: $0 MANIFEST_TSV ACTUAL_ROOT [validator args...]" >&2
  echo "Manifest format: feed_path<TAB>expected_dir<TAB>case_name(optional)<TAB>extra_json(optional)<TAB>html_name(optional)<TAB>validator_args(optional)<TAB>compare_flags(optional)" >&2
  echo "Env:" >&2
  echo "  GTFS_VALIDATOR_BIN: use a prebuilt binary instead of cargo." >&2
  echo "  COMPARE_FLAGS: pass extra args to compare_reports.py." >&2
  echo "  COMPARE_EXTRA_JSON: extra JSON files to compare (space-separated)." >&2
  echo "  COMPARE_HTML_NAME: override the HTML filename for compare." >&2
  echo "  CASE_FILTER: glob pattern to select case_name entries." >&2
  echo "  MANIFEST_VALIDATE_FLAGS: pass args to validate_golden_manifest.py." >&2
  echo "  SKIP_MANIFEST_VALIDATE=1: skip manifest validation." >&2
  echo "  UPDATE_EXPECTED_ON_FAIL=1: copy actual outputs into expected on compare failure." >&2
  echo "See docs/golden.md for more details." >&2
  exit 1
fi

manifest="$1"
actual_root="$2"
shift 2

if [ ! -f "$manifest" ]; then
  echo "Manifest not found: $manifest" >&2
  exit 1
fi

mkdir -p "$actual_root"

validate_args=()
if [ -n "${MANIFEST_VALIDATE_FLAGS:-}" ]; then
  read -r -a validate_args <<<"${MANIFEST_VALIDATE_FLAGS}"
fi
if [ -z "${SKIP_MANIFEST_VALIDATE:-}" ]; then
  if ! python3 "$(dirname "$0")/validate_golden_manifest.py" "$manifest" "${validate_args[@]}"; then
    exit 1
  fi
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

fail_count=0
line_no=0

while IFS=$'\t' read -r feed_path expected_dir case_name extra_json html_name validator_args compare_flags; do
  line_no=$((line_no + 1))
  if [ -z "${feed_path}" ] || [[ "${feed_path}" == \#* ]]; then
    continue
  fi
  if [ -z "${expected_dir}" ]; then
    echo "Line ${line_no}: missing expected_dir" >&2
    fail_count=$((fail_count + 1))
    continue
  fi

  if [ -z "${case_name}" ]; then
    case_name="$(basename "$expected_dir")"
  fi
  if [ -n "${CASE_FILTER:-}" ]; then
    if [[ ! "$case_name" == ${CASE_FILTER} ]]; then
      continue
    fi
  fi

  actual_dir="${actual_root}/${case_name}"
  mkdir -p "$actual_dir"

  if [ ! -e "$feed_path" ]; then
    echo "Line ${line_no}: feed not found: ${feed_path}" >&2
    fail_count=$((fail_count + 1))
    continue
  fi

  case_validator_args=()
  if [ -n "${validator_args}" ]; then
    IFS=' ' read -r -a case_validator_args <<<"${validator_args}"
  fi

  echo "Running ${case_name}"
  if [ -n "${GTFS_VALIDATOR_BIN:-}" ]; then
    if ! "$GTFS_VALIDATOR_BIN" --input "$feed_path" --output_base "$actual_dir" "$@" "${case_validator_args[@]}"; then
      echo "Line ${line_no}: validator failed for ${case_name}" >&2
      fail_count=$((fail_count + 1))
      continue
    fi
  else
    if ! cargo run -p gtfs_validator_cli -- --input "$feed_path" --output_base "$actual_dir" "$@" "${case_validator_args[@]}"; then
      echo "Line ${line_no}: validator failed for ${case_name}" >&2
      fail_count=$((fail_count + 1))
      continue
    fi
  fi

  case_compare_args=("${compare_args[@]}")
  if [ -n "${extra_json}" ]; then
    IFS=' ' read -r -a case_extra_json <<<"${extra_json}"
    for name in "${case_extra_json[@]}"; do
      case_compare_args+=("--extra-json" "${name}")
    done
  fi
  if [ -n "${compare_flags}" ]; then
    IFS=' ' read -r -a case_flags <<<"${compare_flags}"
    case_compare_args+=("${case_flags[@]}")
  fi

  if [ -n "${html_name}" ]; then
    case_compare_args+=("--html-name" "${html_name}")
  fi

  if ! python3 "$(dirname "$0")/compare_reports.py" "$expected_dir" "$actual_dir" "${case_compare_args[@]}"; then
    echo "Line ${line_no}: compare failed for ${case_name}" >&2
    if [ -n "${UPDATE_EXPECTED_ON_FAIL:-}" ]; then
      mkdir -p "$expected_dir"
      html_name_override=""
      for ((i=0; i<${#case_compare_args[@]}; i++)); do
        if [[ "${case_compare_args[$i]}" == "--html-name" && $((i+1)) -lt ${#case_compare_args[@]} ]]; then
          html_name_override="${case_compare_args[$((i+1))]}"
        elif [[ "${case_compare_args[$i]}" == --html-name=* ]]; then
          html_name_override="${case_compare_args[$i]#--html-name=}"
        fi
      done

      files=("report.json" "system_errors.json")
      if [ -n "${html_name_override}" ]; then
        files+=("${html_name_override}")
      elif [ -n "${html_name}" ]; then
        files+=("${html_name}")
      else
        files+=("report.html")
      fi
      if [ -n "${COMPARE_EXTRA_JSON:-}" ]; then
        read -r -a global_extra <<<"${COMPARE_EXTRA_JSON}"
        files+=("${global_extra[@]}")
      fi
      if [ -n "${extra_json}" ]; then
        read -r -a case_extra <<<"${extra_json}"
        files+=("${case_extra[@]}")
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
      echo "Updated expected outputs for ${case_name}."
    fi
    fail_count=$((fail_count + 1))
  fi
done < "$manifest"

if [ "$fail_count" -gt 0 ]; then
  echo "Golden suite finished with ${fail_count} failure(s)." >&2
  exit 1
fi

echo "Golden suite passed."
