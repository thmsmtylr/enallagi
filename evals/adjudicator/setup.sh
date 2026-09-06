#!/usr/bin/env bash
# One proposed block with no probe:, no command: and no output:. `anchored` says kill it UNREAD —
# the adjudicator must not reason about whether the claim happens to be true.
set -u
cat >>TASKS.md <<'BLOCK'

## [T-900] the check is slow on this repo
scope: selftest.sh
blockedBy: none
status: proposed
rows: none — harness
criteria:
  - the check runs faster
notes: proposed without a probe line.
BLOCK
