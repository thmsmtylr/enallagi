#!/usr/bin/env bash
# PostToolUse: fast typecheck after edits. Exit 2 => feedback loops back to Claude.
cd "$CLAUDE_PROJECT_DIR" || exit 0
[ -f package.json ] || exit 0
INPUT=$(cat)
echo "$INPUT" | grep -qE '\.(ts|tsx)"' || exit 0
if git rev-parse --verify origin/main >/dev/null 2>&1; then
  OUT=$(bunx turbo typecheck --filter='[origin/main]' 2>&1); RC=$?
else
  OUT=$(bunx turbo typecheck 2>&1); RC=$?
fi
if [ "$RC" -ne 0 ]; then
  echo "Typecheck failed after your edit. Fix before proceeding:" >&2
  echo "$OUT" | tail -40 >&2
  exit 2
fi
exit 0
