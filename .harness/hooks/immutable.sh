#!/usr/bin/env bash
# PreToolUse: refuse an edit to test-hashes.json or any file it covers. Bash `sed -i` bypasses the hook,
# but precheck compares against `git show HEAD:test-hashes.json`, so a working-tree tamper of both still fails.
set -u
ROOT="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[ -n "$ROOT" ] || exit 0
[ -f "$ROOT/test-hashes.json" ] || exit 0
INPUT=$(cat)
# ponytail: refuses a whole package.json, not just its scripts block — a dependency edit is "a decision to bring back" (CLAUDE.md, Stack)
HIT=$(ROOT="$ROOT" python3 -c '
import glob, json, os, sys
root = os.environ["ROOT"]
try: target = json.load(sys.stdin).get("tool_input", {}).get("file_path", "")
except Exception: sys.exit(0)
if not target: sys.exit(0)
target = os.path.realpath(os.path.join(root, target))
hashes = os.path.join(root, "test-hashes.json")
if os.path.realpath(hashes) == target:
    print("test-hashes.json (the reference itself)"); sys.exit(0)
keys = json.load(open(hashes))
scripts_key = "package.json#scripts"
if scripts_key in keys:
    # the same source of truth check.ts scriptFiles() reads, so an M2 package is refused without anyone remembering
    workspaces = json.load(open(os.path.join(root, "package.json"))).get("workspaces", [])
    covered = [os.path.join(root, "package.json")]
    for pattern in workspaces:
        covered += sorted(glob.glob(os.path.join(root, pattern, "package.json")))
    for found in covered:
        if os.path.realpath(found) == target:
            print(os.path.relpath(found, root) + " (its scripts, via " + scripts_key + ")"); sys.exit(0)
for key in keys:
    if "#" in key: continue
    if os.path.realpath(os.path.join(root, key)) == target:
        print(key); break
' <<< "$INPUT") || exit 0
[ -z "$HIT" ] && exit 0
echo "immutable: $HIT is covered by test-hashes.json (SPEC.md §0.2, tests-immutable and harness-immutable). This edit is refused. If the change is genuinely needed, stop the task and write it into the task notes." >&2
exit 2
