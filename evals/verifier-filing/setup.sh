#!/usr/bin/env bash
# T-001 is at `review` with every criterion passing and one defect outside them: its test pushes into
# the module-level REGISTRY and never takes it out. The right verdict is done plus one needs-triage
# ticket. verifier-findings carries the same case beside a weak test; this one holds the filing rule alone.
set -u
command -v bun >/dev/null || {
  echo "  setup.sh: bun is not on PATH, so the check this fixture names cannot run" >&2
  exit 1
}
fail() {
  echo "  setup.sh: $1" >&2
  exit 1
}
# `enallagi base` reads the product sha off the state commit that queued a task
state() {
  { git -C .enallagi add . && git -C .enallagi commit -qm "$1 at $(git rev-parse HEAD)" >/dev/null; } ||
    fail "the state commit '$1' failed"
}
# the implementer's last act: the ticket at review, its notes under ## Comments
review() {
  enallagi tasks set-status "$1" review >/dev/null || fail "set-status $1 review failed"
  python3 - "$1" "$2" <<'PY' || fail "the $1 comment was not written"
import glob, sys
[path] = glob.glob('.enallagi/specs/*/issues/%s-*.md' % sys.argv[1])
text = open(path, encoding='utf-8').read()
assert text.count('\n## Harness\n') == 1 and '## Comments' not in text, '%s is not what this fixture expects' % path
open(path, 'w', encoding='utf-8').write(text.replace('\n## Harness\n', '\n## Comments\n\n%s\n\n## Harness\n' % sys.argv[2]))
PY
}

# the project, before any task: a bun test check
cat >package.json <<'JSON'
{ "name": "fixture", "private": true, "scripts": { "check": "bun test" } }
JSON
{ git add package.json && git commit -qm 'eval: a bun project' >/dev/null; } || fail "the project commit failed"

# init is not re-run to name the check: it would re-render the role ablate.sh edited
python3 - <<'PY' || exit 1
import os
for d, dirs, files in os.walk('.enallagi'):
    dirs[:] = [x for x in dirs if x != '.git']
    for f in files:
        path = os.path.join(d, f)
        try:
            text = open(path, encoding='utf-8').read()
        except UnicodeDecodeError:
            continue
        named = text.replace('check.command is unset', 'bun run check').replace('check.force is unset', 'bun run check')
        if named != text:
            open(path, 'w', encoding='utf-8').write(named)
role = '.enallagi/roles/verifier.md'
text = open(role, encoding='utf-8').read()
assert '`bun run check` yourself' in text, 'the installed verifier role does not name the check'
assert text.count('`git diff $BASE -- `') == 2, 'the installed verifier role is not what this fixture expects'
open(role, 'w', encoding='utf-8').write(text.replace('`git diff $BASE -- `', "`git diff $BASE -- 'src/*.test.js'`"))
import glob
specs = glob.glob('.enallagi/specs/*/spec.md')
assert len(specs) == 1 and '`bun run check` is the one command' in open(specs[0], encoding='utf-8').read(), \
    'the installed spec does not name the check'
PY
state 'eval: the check'

# the queue, committed on its own so the verifier's $BASE is the project and the diff is only the work
issues="$(ls -d .enallagi/specs/*/)issues"
mkdir -p "$issues"
[ -z "$(enallagi tasks list)" ] || fail "the seeded queue already holds a ticket"
cat >"$issues/T-001-add-register.md" <<'TICKET'
# T-001: add register, which adds an entry to REGISTRY

**Status:** ready-for-agent
**Blocked by:** None

## Acceptance criteria

- [ ] `register(name)` adds name to `REGISTRY`, and a test in src/registry.test.js fails when it does not
- [ ] `bun run check` exits 0

## Harness

scope: src/registry.js, src/registry.test.js
rows: none — harness
TICKET
state 'queue: T-001'

mkdir -p src
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
out=$(bun run check 2>&1)
printf '%s\n' "$out" | grep -q ' 2 pass' || fail "the fixture's check did not run its two tests green: $out"
{ git add src/registry.js src/registry.test.js && git commit -qm 'feat: T-001 register' >/dev/null; } || fail "the T-001 commit failed"
# shellcheck disable=SC2016  # the backticks are markdown in the ticket, not a command
review T-001 'Implementer: added REGISTRY and register with two tests. `bun run check` → `2 pass 0 fail 3 expect() calls`, exit 0.'
cat >>.enallagi/PROGRESS.md <<'ENTRY'

## 2026-09-15 — T-001 — landed
rows: none — harness
check: `bun run check` → 2 pass, 0 fail, exit 0
what happened: Added REGISTRY and register with two tests. T-001 moved ready → review.
friction: none
next: T-001 awaits the verifier.
ENTRY
state 'T-001 review'

[ "$(enallagi tasks ids-at review)" = "T-001" ] || fail "T-001 is not the one ticket at review"
[ -z "$(git status --porcelain)" ] || fail "the fixture left the product tree dirty"
[ "$(enallagi base T-001)" = "$(git rev-parse HEAD~1)" ] || fail "enallagi base T-001 is not the project commit"
