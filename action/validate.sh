#!/usr/bin/env bash
# Run the validator, publish the counts as step outputs, and render a job
# summary. Never exits non-zero on a failing feed — the composite action has a
# separate gate step, so reports and the SARIF upload always happen first.
set -uo pipefail

feed="${INPUT_FEED:-}"
url="${INPUT_URL:-}"
output="${INPUT_OUTPUT:-gtfs-guru-report}"
fail_on="${INPUT_FAIL_ON:-error}"

if [ -n "$feed" ] && [ -n "$url" ]; then
  echo "::error::Set either the feed input or the url input, not both."
  exit 1
fi
if [ -z "$feed" ] && [ -z "$url" ]; then
  echo "::error::Set the feed input (a path in the repository) or the url input (a remote GTFS zip)."
  exit 1
fi
if [ -n "$feed" ] && [ ! -e "$feed" ]; then
  echo "::error::Feed not found at '$feed'. Did the workflow check out the repository first?"
  exit 1
fi

case "$fail_on" in
  none | error | warning) ;;
  *) echo "::error::fail-on must be none, error or warning (got '$fail_on')."; exit 1 ;;
esac

args=(--output_base "$output" --fail-on "$fail_on" --skip_validator_update --pretty)
if [ -n "$feed" ]; then
  args+=(--input "$feed")
else
  args+=(--url "$url")
fi
[ -n "${INPUT_COUNTRY_CODE:-}" ] && args+=(--country_code "$INPUT_COUNTRY_CODE")
[ -n "${INPUT_DATE:-}" ] && args+=(--date "$INPUT_DATE")
[ "${INPUT_THOROUGH:-false}" = "true" ] && args+=(--thorough)
[ "${INPUT_GOOGLE_RULES:-false}" = "true" ] && args+=(--google_rules)

sarif_file=""
if [ "${INPUT_SARIF:-true}" = "true" ]; then
  args+=(--sarif report.sarif.json)
  sarif_file="$output/report.sarif.json"
fi
badge_file="${INPUT_BADGE:-}"
[ -n "$badge_file" ] && args+=(--badge "$badge_file")
[ -n "${INPUT_BADGE_SVG:-}" ] && args+=(--badge-svg "$INPUT_BADGE_SVG")
[ -n "${INPUT_BADGE_LABEL:-}" ] && args+=(--badge-label "$INPUT_BADGE_LABEL")

echo "Running: gtfs-guru ${args[*]}"
gtfs-guru "${args[@]}"
status=$?

# 0 = clean, 2 = the feed did not meet --fail-on. Anything else is the run
# itself failing (bad input, network, unreadable archive), not a verdict.
if [ "$status" -ne 0 ] && [ "$status" -ne 2 ]; then
  echo "::error::gtfs-guru exited with status $status; no verdict was produced."
  exit "$status"
fi

report_json="$output/report.json"
system_errors="$output/system_errors.json"
if [ ! -f "$report_json" ]; then
  echo "::error::Expected a report at $report_json but none was written."
  exit 1
fi

# Reports that exist. A feed that fails to load leaves report.json empty and
# puts the reason in system_errors.json, so both are counted.
report_files=("$report_json")
[ -f "$system_errors" ] && report_files+=("$system_errors")

# Severity totals come from each rule's `totalNotices`, which stays exact even
# when the validator caps how many samples it stores per rule. First line is
# "<errors> <warnings> <infos>"; the rest is a tab-separated rule breakdown.
count_script='
import json, sys
totals = {"ERROR": 0, "WARNING": 0, "INFO": 0}
rules = []
for path in sys.argv[1:]:
    try:
        with open(path, encoding="utf-8") as handle:
            data = json.load(handle)
    except (OSError, ValueError):
        continue
    for group in data.get("notices") or []:
        severity = group.get("severity", "INFO")
        count = int(group.get("totalNotices") or 0)
        totals[severity] = totals.get(severity, 0) + count
        rules.append((count, severity, group.get("code", "unknown")))
rules.sort(key=lambda item: (-item[0], item[2]))
print(totals["ERROR"], totals["WARNING"], totals["INFO"])
for count, severity, code in rules[:15]:
    print("%s\t%s\t%d" % (code, severity, count))
'

python_bin=""
for candidate in python3 python; do
  if command -v "$candidate" >/dev/null 2>&1; then
    python_bin="$candidate"
    break
  fi
done

parsed=""
if [ -n "$python_bin" ]; then
  parsed="$("$python_bin" -c "$count_script" "${report_files[@]}")"
elif command -v jq >/dev/null 2>&1; then
  parsed="$(jq -rs '
    [.[] | (.notices // [])[]] as $n
    | (([$n[] | select(.severity == "ERROR")   | .totalNotices] | add) // 0) as $e
    | (([$n[] | select(.severity == "WARNING") | .totalNotices] | add) // 0) as $w
    | (([$n[] | select(.severity == "INFO")    | .totalNotices] | add) // 0) as $i
    | "\($e) \($w) \($i)",
      ($n | sort_by(-.totalNotices, .code) | .[0:15][]
          | "\(.code)\t\(.severity)\t\(.totalNotices)")
  ' "${report_files[@]}")"
fi

counts_line="$(printf '%s\n' "$parsed" | head -1)"
errors="$(printf '%s' "$counts_line" | awk '{print $1}')"
warnings="$(printf '%s' "$counts_line" | awk '{print $2}')"
infos="$(printf '%s' "$counts_line" | awk '{print $3}')"

case "$errors$warnings$infos" in
  '' | *[!0-9]*)
    echo "::error::Could not read notice counts from $report_json (the runner needs python3 or jq)."
    exit 1
    ;;
esac

valid=false
[ "$errors" -eq 0 ] && valid=true
passed=true
[ "$status" -eq 2 ] && passed=false

{
  echo "errors=$errors"
  echo "warnings=$warnings"
  echo "infos=$infos"
  echo "valid=$valid"
  echo "passed=$passed"
  echo "report-json=$report_json"
  echo "report-html=$output/report.html"
  echo "sarif-file=$sarif_file"
  echo "badge-file=$badge_file"
} >> "$GITHUB_OUTPUT"

echo "Errors: $errors, Warnings: $warnings, Info: $infos"

if [ "${INPUT_SUMMARY:-true}" = "true" ] && [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  if [ "$valid" = "true" ] && [ "$warnings" -eq 0 ]; then
    verdict="✅ Feed is valid"
  elif [ "$valid" = "true" ]; then
    verdict="⚠️ No errors, $warnings warning(s)"
  else
    verdict="❌ $errors error(s)"
  fi

  {
    echo "## GTFS Guru — $verdict"
    echo
    echo "| Severity | Count |"
    echo "| --- | ---: |"
    echo "| Errors | $errors |"
    echo "| Warnings | $warnings |"
    echo "| Info | $infos |"
    echo

    detail="$(printf '%s\n' "$parsed" | tail -n +2)"
    if [ -n "$detail" ]; then
      echo "<details><summary>Top issues by rule</summary>"
      echo
      echo "| Rule | Severity | Count |"
      echo "| --- | --- | ---: |"
      printf '%s\n' "$detail" | while IFS=$'\t' read -r code severity count; do
        [ -n "$code" ] || continue
        echo "| \`$code\` | $severity | $count |"
      done
      echo
      echo "</details>"
      echo
    fi

    echo "_Threshold: \`fail-on: $fail_on\`. Full reports are in \`$output\`._"
  } >> "$GITHUB_STEP_SUMMARY"
fi

exit 0
