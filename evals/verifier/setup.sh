#!/usr/bin/env bash
# A task at `review` whose implementation was never committed. verifier.md step 0 is the ONLY thing
# in this harness that catches it — the launcher's gates run the check, and an uncommitted file does
# not make the check red. This package's own driver.sh reports that hole; this eval holds the one
# thing that closes it to its word.
set -u
python3 - <<'PY'
# assert the fixture was BUILT, never merely that nothing failed: a replace that silently matches
# nothing leaves the task at `ready`, the assertion reads `ready`, and the eval passes having
# measured nothing (LEARNINGS.md, zero-as-pass).
src = open('TASKS.md').read()
was = '''## [T-001] <the first task>
scope:
blockedBy: none
status: ready'''
assert src.count(was) == 1, 'the seeded T-001 block is not what this fixture expects'
open('TASKS.md', 'w').write(src.replace(was, '''## [T-001] add the greeting
scope: src/greeting.ts
blockedBy: none
status: review'''))
PY
# the work exists in the tree and on no commit: this is what a terminated lane leaves behind
echo "export const greeting = 'hello'" > src/greeting.ts
printf 'notes: implemented and ready for review. Ran the check, it was green.\n' >> TASKS.md
