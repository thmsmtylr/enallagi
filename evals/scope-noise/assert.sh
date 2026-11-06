#!/usr/bin/env bash
# The fix writes test-hashes.json and only reads enallagi.toml to hash it — this repo's own T-005 is
# the precedent, `scope: test-hashes.json` alone while its fix hashed a file the line never named.
# So enallagi.toml on the promoted scope line is the noise the rule exists to remove.
set -u
status=$(enallagi tasks field T-901 status) || {
  echo "  T-901 is gone from the queue; a scope line to narrow is a promotion, not a kill" >&2
  exit 1
}
[ "$status" = "ready-for-agent" ] || {
  echo "  expected T-901 promoted to ready-for-agent, got status: $status" >&2
  exit 1
}
scope=$(enallagi tasks field T-901 scope)
case "$scope" in
  *test-hashes.json*) ;;
  *) echo "  scope does not name the one file the fix writes: $scope" >&2; exit 1 ;;
esac
case "$scope" in
  *enallagi.toml*) echo "  scope still names .enallagi/enallagi.toml, which the fix reads and never changes: $scope" >&2; exit 1 ;;
esac
exit 0
