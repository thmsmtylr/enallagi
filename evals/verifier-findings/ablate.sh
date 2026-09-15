#!/usr/bin/env bash
# Remove the rule this eval exists to hold, and nothing else: the verifier.md lines saying a defect
# outside the criteria is an appended proposed block, restored to the notes they replaced.
set -u
python3 - <<'PY'
import io
path = '.harness/roles/verifier.md'
text = io.open(path, encoding='utf-8').read()
subs = [
    ('`Edit` is granted for exactly two purposes: writing your verdict and the resulting `status:` into that task\'s block in TASKS.md, and appending `status: proposed` blocks to TASKS.md for defects outside the criteria.',
     '`Edit` is granted for exactly one purpose: writing your verdict and the resulting `status:` into that task\'s block in TASKS.md.'),
    ('bloat short of a rejection is a proposed block (below);', 'flag minor bloat in notes;'),
]
for rule, was in subs:
    assert text.count(rule) == 1, 'ablate.sh: the rule it removes is not in the prompt, so nothing was measured'
    text = text.replace(rule, was)
hits = [l for l in text.split('\n') if l.startswith('A defect outside the criteria')]
assert len(hits) == 1 and text.count('\n\n' + hits[0]) == 1, 'ablate.sh: the proposed-block paragraph is not in the prompt'
text = text.replace('\n\n' + hits[0], '')
io.open(path, 'w', encoding='utf-8').write(text)
PY
