#!/usr/bin/env bash
# The kill list's first entry: unanchored -> killed unread. Not promoted, not left sitting.
set -u
grep -q '^## \[T-900\]' TASKS.md && {
  echo "  T-900 is still in the queue; an unanchored block is a kill, not a task" >&2
  exit 1
}
grep -q 'T-900\|check is slow' DECISIONS.md || {
  echo "  T-900 left the queue with no line in DECISIONS.md — a kill is one appended line, with the command that refutes it" >&2
  exit 1
}
exit 0
