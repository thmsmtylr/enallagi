#!/usr/bin/env bash
# A task at `review` whose criteria all pass and whose test leaves one defect outside them: it pushes
# into the module-level REGISTRY and never takes it out. The right verdict is done plus one proposed block.
# Anything else a verifier could fairly raise is closed here, so the one block has one candidate.
set -u
command -v bun >/dev/null || {
  echo "  setup.sh: bun is not on PATH, so the check this fixture names cannot run" >&2
  exit 1
}

# the project, before any task: a bun test check, a §0.4 that names only what it runs, no template block
cat >package.json <<'JSON'
{ "name": "fixture", "private": true, "scripts": { "check": "bun test" } }
JSON
python3 - <<'PY'
# assert the fixture was BUILT: a replace that matches nothing leaves the template in place and measures nothing
import re
spec = open('.enallagi/SPEC.md').read()
stages = re.search(r'`bun run check` runs these stages.*?\n```\n.*?\n```\n\n`trace` is .*?\n\n', spec, re.S)
assert stages, 'the seeded .enallagi/SPEC.md §0.4 is not what this fixture expects'
open('.enallagi/SPEC.md', 'w').write(spec.replace(stages.group(0),
    '`bun run check` runs `bun test` and nothing else. This project has no typecheck, lint,\n'
    'precheck or trace stage, and none is owed.\n\n'))
src = open('.enallagi/TASKS.md').read()
assert '## [T-001]' not in src, 'the seeded queue already holds a T-001 block'
PY
git add package.json .enallagi/SPEC.md .enallagi/TASKS.md && git commit -qm 'eval: a bun project' >/dev/null

# the queue, committed on its own so the verifier's $BASE is this commit and the diff is only the work
cat >>.enallagi/TASKS.md <<'BLOCK'
## [T-001] add register, which adds an entry to REGISTRY
scope: src/registry.js, src/registry.test.js
blockedBy: none
status: ready
rows: none — harness
criteria:
  - `register(name)` adds name to `REGISTRY`, and a test in src/registry.test.js fails when it does not
  - `bun run check` exits 0
notes:
BLOCK
git add .enallagi/TASKS.md && git commit -qm 'queue: T-001' >/dev/null

cat >src/registry.js <<'JS'
export const REGISTRY = ["builtin"];

export function register(name) {
  REGISTRY.push(name);
}
JS
cat >src/registry.test.js <<'JS'
import { expect, test } from "bun:test";
import { REGISTRY, register } from "./registry.js";

test("register adds the entry to REGISTRY", () => {
  expect(REGISTRY).not.toContain("extra");
  register("extra");
  expect(REGISTRY).toContain("extra");
});

test("REGISTRY starts with builtin", () => {
  expect(REGISTRY[0]).toBe("builtin");
});
JS
python3 - <<'PY'
src = open('.enallagi/TASKS.md').read()
block = src[src.index('## [T-001] add register,'):]
assert block.count('status: ready\n') == 1 and block.endswith('notes:\n'), 'the queued T-001 block is not what this fixture expects'
done = block.replace('status: ready\n', 'status: review\n').replace('notes:\n',
    'notes: implementer: added REGISTRY and register with two tests. `bun run check` → `2 pass 0 fail 3 expect() calls`, exit 0.\n')
open('.enallagi/TASKS.md', 'w').write(src.replace(block, done))
PY
cat >>.enallagi/PROGRESS.md <<'ENTRY'

## 2026-09-15 — T-001 — landed
rows: none — harness
check: `bun run check` → 2 pass, 0 fail, exit 0
what happened: Added REGISTRY and register with two tests. T-001 moved ready → review.
friction: none
next: T-001 awaits the verifier.
ENTRY
out=$(bun run check 2>&1)
printf '%s\n' "$out" | grep -q ' 2 pass' || {
  echo "  setup.sh: the fixture's check did not run its two tests green: $out" >&2
  exit 1
}
grep -q '^status: review' .enallagi/TASKS.md || {
  echo "  setup.sh: T-001 is not at review, so there is nothing to verify" >&2
  exit 1
}
git add .enallagi/TASKS.md .enallagi/PROGRESS.md src/registry.js src/registry.test.js && git commit -qm 'feat: T-001 register' >/dev/null
