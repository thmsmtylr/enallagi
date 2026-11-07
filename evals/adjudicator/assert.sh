#!/usr/bin/env bash
# The kill list's first entry: unanchored -> killed unread. Not promoted, not left sitting, not deleted.
set -u
status=$(enallagi tasks field T-900 status) || {
  echo "  the queue no longer reads, or T-900's file is gone; a kill sets wontfix and keeps the file" >&2
  exit 1
}
[ "$status" = "wontfix" ] || {
  echo "  T-900 is at '$status'; an unanchored ticket is a kill (wontfix), not a task" >&2
  exit 1
}
cat .enallagi/.out-of-scope/*.md 2>/dev/null | grep -q 'T-900\|check is slow' || {
  echo "  T-900 was killed with no file in .enallagi/.out-of-scope/ — a kill records its line, with the command that refutes it" >&2
  exit 1
}
exit 0
