#!/usr/bin/env bash
# Six findings in two classes across three rounds, and one friction of no class. `enallagi audit`
# hands them to the auditor role, which assert.sh runs and grades.
set -u

# the agent under test is the one `enallagi eval` names, so `enallagi audit` spawns it as the auditor
if [ -n "${EVAL_AGENT:-}" ]; then
  python3 - <<'PY'
import json, os, shlex
words = shlex.split(os.environ['EVAL_AGENT'])
path = '.enallagi/enallagi.toml'
text = open(path).read()
assert '[agent]' not in text, 'the seeded config already holds an [agent] table'
# a custom preset declares no hosts, so the sandbox proxy would refuse the agent's API
hosts = ['api.anthropic.com', 'platform.claude.com']
open(path, 'a').write('\n[agent]\npreset = "custom"\ncommand = %s\n\n[sandbox]\nhosts = %s\n' % (json.dumps(words), json.dumps(hosts)))
PY
fi

cat >>.enallagi/PROGRESS.md <<'ENTRY'

## 2026-09-01 — T-001 — landed
friction: `enallagi worktree` reported merged true on a failed fetch, so the lane looked landed
next: T-002

## 2026-09-02 — T-002 — landed
friction: the new test passed with the filter deleted, so it guarded nothing
next: T-003

## 2026-09-03 — T-003 — landed
friction: the lane waited on a slow disk
next: none
ENTRY

python3 - <<'PY'
path = '.enallagi/DECISIONS.md'
text = open(path).read()
heading = '\n## Rejected findings\n'
assert text.count(heading) == 1, 'the seeded DECISIONS.md has no one ## Rejected findings heading'
text = text.replace(heading, heading + '\n- [2026-09-03] a mutant survived: the citation floor scanned comments while the defect sat in a fixture — refuted by `cargo test -q floor`\n', 1)
text += '''
## [T-010] spawn loses the stage result
scope: src/agent.rs
status: done
notes: https://github.com/o/r/pull/3#discussion_r11
  > spawn returns Err after a full StageResult, discarding the stage's accounting

## [T-011] the reader
scope: src/read.rs
status: done
notes: VERIFIER 2026-09-03
  REJECTED: the probe returns ok on a failed read
'''
open(path, 'w').write(text)
PY

issues="$(ls -d .enallagi/specs/*/)issues"
mkdir -p "$issues"
cat >"$issues/T-012-the-citation-test.md" <<'TICKET'
# T-012: the citation test

**Status:** needs-triage
**Blocked by:** None

## Comments

https://github.com/o/r/pull/4#discussion_r12
  > this test selects 0 tests, so it passes with the logic deleted

## Harness

scope: src/floor.rs
TICKET

if ! { grep -q 'merged true on a failed fetch' .enallagi/PROGRESS.md && [ "$(enallagi tasks field T-012 status)" = needs-triage ] && grep -q 'a mutant survived' .enallagi/DECISIONS.md; }; then
  echo "  setup.sh: the six findings did not all land in the fixture" >&2
  exit 1
fi
