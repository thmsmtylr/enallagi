#!/usr/bin/env bash
# Live status board for a running loop.sh. Read-only, safe to run in a second
# terminal alongside the loop. Ctrl-C to stop.
set -u
LOG=__HARNESS_DIR__/logs/run.log
while :; do
  printf '\033[H\033[2J'
  echo "loop: $(pgrep -f '__HARNESS_DIR__/loop.sh' >/dev/null && echo running || echo stopped)   $(date +%H:%M:%S)"
  echo "phase: $(grep '^=== Iteration' "$LOG" 2>/dev/null | tail -1)   agent up $(ps -o etime= -p "$(pgrep -f '__AGENT_BINARY__' | head -1)" 2>/dev/null | tr -d ' ' || echo -)"
  echo
  python3 __HARNESS_DIR__/tasks.py list
  echo
  git log --oneline -3
  echo
  echo "--- log tail ---"
  tail -6 "$LOG"
  sleep 5
done
