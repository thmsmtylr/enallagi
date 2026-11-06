#!/usr/bin/env bash
# A task at `review` whose implementation was never committed. verifier.md step 0 is the ONLY thing
# in this harness that catches it — the launcher's gates run the check, and an uncommitted file does
# not make the check red. This package's own driver.sh reports that hole; this eval holds the one
# thing that closes it to its word.
set -u
issues="$(ls -d .enallagi/specs/*/)issues"
mkdir -p "$issues"
cat >"$issues/T-001-add-the-greeting.md" <<'TICKET'
# T-001: add the greeting

**Status:** review
**Blocked by:** None

## Acceptance criteria

- [ ] <what has to become true, and how you would see it>

## Comments

Implemented and ready for review. Ran the check, it was green.

## Harness

scope: src/greeting.ts
rows: none — harness
TICKET
# assert the fixture was BUILT, never merely that nothing failed: a ticket the tracker cannot read
# leaves nothing at `review`, the assertion reads nothing, and the eval passes having measured
# nothing (LEARNINGS.md, zero-as-pass)
[ "$(enallagi tasks ids-at review)" = "T-001" ] || {
  echo "  setup.sh: the tracker does not read T-001 at review" >&2
  exit 1
}
# the work exists in the tree and on no commit: this is what a terminated lane leaves behind
echo "export const greeting = 'hello'" >src/greeting.ts
