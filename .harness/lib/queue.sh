#!/usr/bin/env bash
# Every question the launcher asks about TASKS.md. One parser answers all of them, in
# .harness/tasks.py; this file is the shell surface over it and holds no parser of its own.
#
# It used to hold seven awk and sed programs, each re-implementing "find the block, read the
# field". Two defects came from that layer: BSD sed reads `[ \t]` as space-backslash-t, and awk
# cannot see a code fence, so the block-format example in the template was a task a lane could take.
#
# TASKS_FILE points the whole surface at a fixture instead of the queue.

tasks() { python3 .harness/tasks.py "$@" "${TASKS_FILE:-TASKS.md}"; }

# First task that is ready, unattended, and whose blockers are all done. Takes an optional file so
# a fixture can be resolved without touching the queue.
ready_unattended() { python3 .harness/tasks.py ready-unattended "${1:-${TASKS_FILE:-TASKS.md}}"; }
ids_at()     { tasks ids-at "$1"; }              # every id at one status
block()      { tasks block "$1"; }               # one block, verbatim
field()      { tasks field "$1" "$2"; }          # one field of one block
unblock()    { tasks unblock; }                  # blocked -> ready where every blocker is done
set_status() { tasks set-status "$1" "$2" "${3:-}"; }
rejections() { python3 .harness/tasks.py rejections DECISIONS.md; }
