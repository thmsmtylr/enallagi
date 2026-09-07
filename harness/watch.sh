#!/usr/bin/env bash
# Live status board for a running loop.sh. Read-only, safe to run in a second
# terminal alongside the loop. Ctrl-C to stop.
set -u
LOG=__HARNESS_DIR__/run.log # one tab-separated row per spawned stage: time, iteration, role, task, seconds, rc, cost
PID=__HARNESS_DIR__/loop.pid
while :; do
  printf '\033[H\033[2J'
  echo "loop: $([ -s "$PID" ] && kill -0 "$(cat "$PID")" 2>/dev/null && echo "running (pid $(cat "$PID"))" || echo stopped)   $(date +%H:%M:%S)"
  echo "last stage: $(tail -1 "$LOG" 2>/dev/null | awk -F'\t' '{print "iteration " $2 ", " $3 " " $4 ", " $5 "s, rc " $6}')"
  echo
  python3 __HARNESS_DIR__/tasks.py list
  echo
  git log --oneline -3
  echo
  echo "--- log tail ---"
  tail -6 "$LOG"
  sleep 5
done
