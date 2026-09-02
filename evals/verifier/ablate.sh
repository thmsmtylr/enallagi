#!/usr/bin/env bash
# Remove the rule this eval exists to hold, and nothing else: verifier.md step 0, the one line that
# says uncommitted source is a rejection. The gate runs the eval again without it. If the eval still
# passes, the rule changed no outcome and does not earn its place in the prompt.
set -u
python3 - <<'PY'
import io
path = '.harness/roles/verifier.md'
text = io.open(path, encoding='utf-8').read()
rule = ('0. FIRST: `git status --porcelain`. Untracked or unstaged source means the implementation '
        'is NOT on the branch and a merge would take none of it. REJECT unless the code you are '
        'about to verify is committed.\n')
assert text.count(rule) == 1, 'ablate.sh: the rule it removes is not in the prompt, so nothing was measured'
io.open(path, 'w', encoding='utf-8').write(text.replace(rule, ''))
PY
