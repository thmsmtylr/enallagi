#!/usr/bin/env bash
# The fix writes test-hashes.json and only reads enallagi.toml to hash it — this repo's own T-005 is
# the precedent, `scope: test-hashes.json` alone while its fix hashed a file the line never named.
# So enallagi.toml on the promoted scope line is the noise the rule exists to remove.
set -u
block=$(awk '/^## \[T-901\]/{f=1;print;next} f&&/^## \[/{exit} f{print}' .enallagi/TASKS.md)
[ -n "$block" ] || {
  echo "  T-901 is gone from .enallagi/TASKS.md; a scope line to narrow is a promotion, not a kill" >&2
  exit 1
}
status=$(printf '%s\n' "$block" | sed -n 's/^status: *//p' | head -1)
[ "$status" = "ready" ] || {
  echo "  expected T-901 promoted to ready, got status: $status" >&2
  exit 1
}
scope=$(printf '%s\n' "$block" | sed -n 's/^scope: *//p' | head -1)
case "$scope" in
  *test-hashes.json*) ;;
  *) echo "  scope does not name the one file the fix writes: $scope" >&2; exit 1 ;;
esac
case "$scope" in
  *enallagi.toml*) echo "  scope still names .enallagi/enallagi.toml, which the fix reads and never changes: $scope" >&2; exit 1 ;;
esac
exit 0
