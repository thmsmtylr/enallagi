#!/usr/bin/env bash
# Remove the rule this eval exists to hold, and nothing else: the verifier.md paragraph that files a
# defect outside the criteria as a needs-triage ticket.
set -u
python3 - <<'PY'
import io
path = '.enallagi/roles/verifier.md'
text = io.open(path, encoding='utf-8').read()
paras = [p for p in text.split('\n\n') if p.startswith('A defect outside the criteria')]
assert len(paras) == 1, 'ablate.sh: the rule it removes is not in the prompt, so nothing was measured'
io.open(path, 'w', encoding='utf-8').write(text.replace('\n\n' + paras[0], ''))
PY
