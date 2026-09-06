#!/usr/bin/env bash
# UserPromptSubmit: print the skill contract from harness.json, on every prompt.
#
# Per prompt, not per session. `SessionStart` fires once and decays as the context grows;
# `UserPromptSubmit` fires every turn and its stdout reaches the model (TASKS.md [T-041]
# criterion 2, measured 2026-09-03 against `claude -p`).
#
# A reporter, never a gate: it exits 0 whatever it finds, so a prompt can never deadlock on it.
# Refusal is what `.harness/hooks/immutable.sh` and the launcher's gates are for.
#
# The list is read from harness.json at run time and never hardcoded here: two files naming the
# same skills is one file that goes stale.
set -u

ROOT="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"

CONFIG="$ROOT/harness.json" python3 - <<'PY'
import json, os

try:
    skills = json.load(open(os.environ['CONFIG'])).get('skills') or []
except Exception:
    skills = []
if skills:
    print('The skills this harness relies on, and the gate that enforces each:')
    for s in skills:
        print('- %s — %s (gate: %s)'
              % (s.get('name', '?'), s.get('why', '?'), s.get('gate', 'none')))
PY

exit 0
