#!/usr/bin/env bash
set -u
BASE="${1:-}"
ROOT="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}"
# `cd ""` returns 0, so an empty root has to be caught before the cd, not by it
if [ -z "$ROOT" ] || ! cd "$ROOT"; then
  echo "check-covered: no project root" >&2
  exit 2
fi
[ -f package.json ] || { echo "check-covered: no package.json in $PWD" >&2; exit 2; }

CHANGED=$( { git status --porcelain -uall | sed 's/^...//'
             [ -n "$BASE" ] && git diff --name-only "$BASE...HEAD" 2>/dev/null
           } | grep -E '\.(ts|tsx|js|jsx|mjs|cjs)$' | sort -u )

GAPS=$(bunx turbo typecheck lint test --dry=json "${@:2}" 2>/dev/null | CHANGED="$CHANGED" python3 -c '
import json, os, sys
changed = [f for f in os.environ["CHANGED"].split("\n") if f]
try: tasks = json.load(sys.stdin).get("tasks", [])
except Exception:
    print("turbo produced no plan; the gate can see nothing"); sys.exit(0)
real = [t for t in tasks if t.get("command") != "<NONEXISTENT>"]
if not real:
    print("turbo executed zero tasks"); sys.exit(0)
if not changed: sys.exit(0)
dirs = {}
for t in real:
    dirs.setdefault(t.get("directory") or ".", set()).add(t.get("task"))
need = {"typecheck", "lint", "test"}
for f in changed:
    owners = [d for d in dirs if d != "." and (f == d or f.startswith(d + "/"))]
    if not owners:
        print("UNCOVERED  " + f + "  (no turbo task covers this path)")
        continue
    d = max(owners, key=len)
    missing = sorted(need - dirs[d])
    if missing: print("PARTIAL    " + f + "  (" + d + " has no " + ", ".join(missing) + ")")
') || { echo "check-covered: cannot read turbo's plan" >&2; exit 2; }

[ -n "$GAPS" ] && { echo "changed but unchecked:" >&2; printf '%s\n' "$GAPS" | sed 's/^/  /' >&2; exit 2; }

# turbo sets TURBO_HASH inside a task, so re-running the check here recurses; a gate reports what it
# executed, so a check this process did not run is exit 2 and never a pass
[ -n "${TURBO_HASH:-}" ] && { echo "check-covered: nested under turbo, check not re-run" >&2; exit 2; }

OUT=$(bun run check 2>&1); RED=$?
[ "$RED" -eq 0 ] && exit 0

FAILED=$(printf '%s\n' "$OUT" | sed -n 's/.*(fail) //p' | sed 's/ \[[0-9.]*m*s\]$//' | sort -u)
BASELINE=$([ -f .check-baseline ] && sed 's/#.*//; s/[[:space:]]*$//' .check-baseline | grep -v '^$')
FORGIVEN=$(printf '%s\n' "$FAILED" | grep -xF -f <(printf '%s\n' "$BASELINE"))
UNFORGIVEN=$(printf '%s\n' "$FAILED" | grep -vxF -f <(printf '%s\n' "$BASELINE"))
# a task that failed without printing a bun test name has nothing the baseline can forgive
NOT_A_TEST=$(printf '%s\n' "$OUT" | sed -n 's/^[[:space:]]*Failed:[[:space:]]*//p' | tr ' ,' '\n' | grep -v '#test$' | grep -v '^$')

[ -n "$FORGIVEN" ] && { echo "check-covered: forgiven by .check-baseline:" >&2; printf '%s\n' "$FORGIVEN" | sed 's/^/  /' >&2; }
[ -n "$FAILED" ] && [ -z "$UNFORGIVEN" ] && [ -z "$NOT_A_TEST" ] && exit 0

echo "check RED:" >&2
[ -n "$UNFORGIVEN" ] && { echo "not on the baseline:" >&2; printf '%s\n' "$UNFORGIVEN" | sed 's/^/  /' >&2; }
[ -n "$NOT_A_TEST" ] && { echo "failed outside bun test, nothing to forgive:" >&2; printf '%s\n' "$NOT_A_TEST" | sed 's/^/  /' >&2; }
printf '%s\n' "$OUT" | tail -30 >&2
exit 2
