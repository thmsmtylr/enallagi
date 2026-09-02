#!/usr/bin/env bash
# Stop hook. The gate runs on the state the workflow actually ENDS in, or it is decoration:
# an earlier version short-circuited on a clean `git diff`, and every lane commits before it
# stops, so at Stop the tree was always clean and the only machine gate never fired
# (LEARNINGS.md 2026-08-25).
#
# check-covered.sh is the build-system-aware gate and takes precedence where an adapter has
# installed one; check-gate.sh is the portable floor.
set -u
INPUT=$(cat)
echo "$INPUT" | grep -q '"stop_hook_active":true' && exit 0
ROOT="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || echo .)}"
HOOKS="$ROOT/__HARNESS_DIR__/hooks"
[ -x "$HOOKS/check-covered.sh" ] && exec "$HOOKS/check-covered.sh"
exec "$HOOKS/check-gate.sh"
