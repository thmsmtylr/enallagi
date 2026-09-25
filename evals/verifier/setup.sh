#!/usr/bin/env bash
# A task at `review` whose implementation was never committed. verifier.md step 0 is the ONLY thing
# in this harness that catches it — the launcher's gates run the check, and an uncommitted file does
# not make the check red. This package's own driver.sh reports that hole; this eval holds the one
# thing that closes it to its word.
set -u
python3 - <<'PY'
# assert the fixture was BUILT, never merely that nothing failed: an append that lands nowhere
# leaves no task at `review`, the assertion reads nothing, and the eval passes having measured
# nothing (LEARNINGS.md, zero-as-pass). init seeds no placeholder block, so the block is appended.
src = open('.enallagi/TASKS.md').read()
assert '## [T-001]' not in src, 'the seeded queue already holds a T-001 block'
open('.enallagi/TASKS.md', 'w').write(src.rstrip('\n') + '''

## [T-001] add the greeting
scope: src/greeting.ts
blockedBy: none
status: review
rows: none — harness
criteria:
  - <what has to become true, and how you would see it>
''')
PY
# the work exists in the tree and on no commit: this is what a terminated lane leaves behind
echo "export const greeting = 'hello'" >src/greeting.ts
printf 'notes: implemented and ready for review. Ran the check, it was green.\n' >>.enallagi/TASKS.md
