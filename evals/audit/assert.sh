#!/usr/bin/env bash
# Two classes recur, so the auditor writes exactly two learnings, and each names every instance of
# its class by the `file:line` the audit handed it, whatever name the role gives the class.
set -u
d=.enallagi/DECISIONS.md
# shellcheck disable=SC2016  # the backticks are markdown, not a command
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
# the entry that cites the class's first instance, with its continuation lines
entry() {
  awk -v c="$1" '
    function done() { if (f && index(body, c)) print body; f = 0 }
    /^- \[proposed\] / { done(); body = $0; f = 1; next }
    f && /^  / { body = body "\n" $0; next }
    { done() }
    END { done() }' "$d"
}
check() {
  body=$(entry "$1")
  [ -n "$body" ] || {
    echo "  no proposed learning cites $1: $out" >&2
    exit 1
  }
  for cite in "$@"; do
    printf '%s\n' "$body" | grep -qF -- "$cite" || {
      echo "  the learning citing $1 does not name $cite:" >&2
      printf '%s\n' "$body" >&2
      exit 1
    }
  done
}
check "${lost[@]}"
check "${vacuous[@]}"
[ "$(entry "${lost[0]}")" != "$(entry "${vacuous[0]}")" ] || {
  echo "  one learning holds both classes" >&2
  exit 1
}
exit 0
