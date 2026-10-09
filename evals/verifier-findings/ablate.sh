#!/usr/bin/env bash
# Remove the rule this eval exists to hold, and nothing else: the verifier.md lines that have it
# reproduce each claim, restored to the text they replaced. T-002's weak test is then read, never run.
set -u
python3 - <<'PY'
import io, re
path = '.enallagi/roles/verifier.md'
text = io.open(path, encoding='utf-8').read()
subs = [
    ("and a mutation: one a criterion names, or step 5's removal of the logic a test covers. For a mutation, one `Edit` makes the change in the file it names,",
     'and a mutation a criterion names. For a mutation, one `Edit` makes the change the criterion states in the file it names,'),
    ('A check that exits non-zero is a rejection, and the only exception is a failure whose name the baseline below lists. Before deciding, re-run it alone in its shell call. A failure is never put down to the environment, a sandbox or noise to pass the task. ', ''),
    (". Every verdict line carries the command it ran and that command's output. A verdict line with neither is refused by this checklist: rewrite it before you write the status.\n", ':\n'),
    ('7. Try to break it', '6. Try to break it'),
]
for rule, was in subs:
    assert text.count(rule) == 1, 'ablate.sh: the rule it removes is not in the prompt, so nothing was measured'
    text = text.replace(rule, was)
step = re.findall(r'\n5\. Reproduce every claim the diff makes\..*?\n6\. Over-engineering audit', text, re.S)
assert len(step) == 1, 'ablate.sh: the reproduce step is not in the prompt, so nothing was measured'
text = text.replace(step[0], '\n5. Over-engineering audit')
io.open(path, 'w', encoding='utf-8').write(text)
PY
