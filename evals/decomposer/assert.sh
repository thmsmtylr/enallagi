#!/usr/bin/env bash
# The drafts pass the check the tickets gate runs, there are at least two of them, and the spec is
# the one setup.sh committed.
set -u
out=$(enallagi tasks check calc)
code=$?
[ "$code" = 0 ] || {
  echo "  enallagi tasks check calc exited $code:" >&2
  printf '%s\n' "$out" >&2
  exit 1
}
drafts=$(find .enallagi/specs/calc/issues -name '[0-9]*.md' | grep -c .)
[ "$drafts" -ge 2 ] || {
  echo "  expected at least two drafts, got $drafts" >&2
  exit 1
}
git -C .enallagi diff --quiet HEAD -- specs/calc/spec.md || {
  echo "  spec.md changed" >&2
  exit 1
}
exit 0
