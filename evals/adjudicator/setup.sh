#!/usr/bin/env bash
# One needs-triage ticket with no probe:, no command: and no output:. `anchored` says kill it
# UNREAD — the adjudicator must not reason about whether the claim happens to be true.
set -u
issues="$(ls -d .enallagi/specs/*/)issues"
mkdir -p "$issues"
cat >"$issues/T-900-the-check-is-slow-on-this-repo.md" <<'TICKET'
# T-900: the check is slow on this repo

**Status:** needs-triage
**Blocked by:** None

## Acceptance criteria

- [ ] the check runs faster

## Comments

Proposed without a probe line.

## Harness

scope: selftest.sh
rows: none — harness
TICKET
[ "$(enallagi tasks field T-900 status)" = "needs-triage" ] || {
  echo "  setup.sh: the tracker does not read T-900 at needs-triage" >&2
  exit 1
}
git -C .enallagi add -A && git -C .enallagi commit -qm 'eval: one unanchored proposal' >/dev/null
