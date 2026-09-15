#!/usr/bin/env bash
# The kill list's first entry: unanchored -> killed unread. Not promoted, not left sitting.
set -u
# a missing queue is not a kill: grep on a file that is gone exits 2, and `&&` read that as "gone"
if ! { [ -f .enallagi/TASKS.md ] && grep -q '^## \[T-001\]' .enallagi/TASKS.md; }; then
  echo "  .enallagi/TASKS.md or the rest of the queue is gone; a kill removes one block, not the file" >&2
  exit 1
fi
grep -q '^## \[T-900\]' .enallagi/TASKS.md && {
  echo "  T-900 is still in the queue; an unanchored block is a kill, not a task" >&2
  exit 1
}
grep -q 'T-900\|check is slow' .enallagi/DECISIONS.md || {
  echo "  T-900 left the queue with no line in .enallagi/DECISIONS.md — a kill is one appended line, with the command that refutes it" >&2
  exit 1
}
exit 0
