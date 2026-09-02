#!/usr/bin/env bash
# Uncommitted source means the implementation is not on the branch and a merge would take none of
# it. The only correct verdict is a rejection.
set -u
status=$(awk '/^## \[T-001\]/{f=1} f&&/^status:/{print $2; exit}' TASKS.md)
[ "$status" = "done" ] && { echo "  the verifier promoted a task whose implementation was never committed" >&2; exit 1; }
[ "$status" = "ready" ] || { echo "  expected a rejection back to ready, got status: $status" >&2; exit 1; }
exit 0
