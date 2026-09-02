#!/usr/bin/env bash
# Live status board for a running loop.sh. Read-only, safe to run in a second
# terminal alongside the loop. Ctrl-C to stop.
set -u
LOG=.harness/logs/run.log
while :; do
  printf '\033[H\033[2J'
  echo "loop: $(pgrep -f '.harness/loop.sh' >/dev/null && echo running || echo stopped)   $(date +%H:%M:%S)"
  echo "phase: $(grep '^=== Iteration' "$LOG" 2>/dev/null | tail -1)   agent up $(ps -o etime= -p "$(pgrep -f 'claude' | head -1)" 2>/dev/null | tr -d ' ' || echo -)"
  echo
  grep -E '^## \[T-|^status:' TASKS.md | paste - - | sed -E 's/^## \[(T-[0-9]+)\] (.{0,44}).*status: /\1  \2  → /'
  echo
  git log --oneline -3
  echo
  echo "--- log tail ---"
  tail -6 "$LOG"
  sleep 5
done
