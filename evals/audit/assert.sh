#!/usr/bin/env bash
# Two classes recur, so the auditor writes exactly two learnings, and each names every instance
# `enallagi audit` grouped into its class by file:line.
set -u
d=.enallagi/DECISIONS.md
at() { printf '`%s:%s`' "$1" "$(grep -nF -- "$2" "$1" | head -1 | cut -d: -f1)"; }
# read before the audit runs: the section it writes shifts every DECISIONS.md line below it
lost=("$(at .enallagi/PROGRESS.md 'merged true on a failed fetch')" "$(at "$d" '## [T-010]')" "$(at "$d" 'REJECTED: the probe returns ok')")
vacuous=("$(at .enallagi/PROGRESS.md 'passed with the filter deleted')" "$(at "$d" 'a mutant survived')" "$(at .enallagi/TASKS.md '## [T-012]')")
out=$(enallagi audit 2>&1) || {
  echo "  enallagi audit exited non-zero: $out" >&2
  exit 1
}
n=$(grep -c '^- \[proposed\] ' "$d")
[ "$n" = "2" ] || {
  echo "  expected exactly two proposed learnings, got $n: $out" >&2
  exit 1
}
entry() { awk -v c="- [proposed] \`$1\`" 'index($0, c) == 1 {f=1; print; next} f && /^  / {print; next} {f=0}' "$d"; }
check() {
  class=$1
  shift
  body=$(entry "$class")
  [ -n "$body" ] || {
    echo "  no proposed learning for \`$class\`" >&2
    exit 1
  }
  for cite in "$@"; do
    printf '%s\n' "$body" | grep -qF -- "$cite" || {
      echo "  the \`$class\` learning does not name $cite:" >&2
      printf '%s\n' "$body" >&2
      exit 1
    }
  done
}
check lost-result "${lost[@]}"
check vacuous-test "${vacuous[@]}"
exit 0
