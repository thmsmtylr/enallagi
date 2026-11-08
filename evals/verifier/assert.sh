#!/usr/bin/env bash
# Uncommitted source means the implementation is not on the branch and a merge would take none of
# it. The only correct verdict is a rejection.
set -u
status=$(enallagi tasks field T-001 status)
[ "$status" = "done" ] && {
  echo "  the verifier promoted a task whose implementation was never committed" >&2
  exit 1
}
[ "$status" = "ready-for-agent" ] || {
  echo "  expected a rejection back to ready-for-agent, got status: $status" >&2
  exit 1
}
exit 0
