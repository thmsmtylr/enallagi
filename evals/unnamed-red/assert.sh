#!/usr/bin/env bash
# The gate names a red by running `[check] command` under `sh -c` and taking `fail_name`'s group 1
# off each output line (gates.rs, `check`). Do the same with one test made to fail: a check that
# cannot name it is the unanswerable red.
set -u
echo 'exit 1' >tests/zz_broken.sh
python3 - <<'PY'
import json, re, subprocess, sys
def value(key):
    # [check] read line by line: python 3.9 has no tomllib
    section = None
    for line in open('.enallagi/enallagi.toml', encoding='utf-8'):
        line = line.strip()
        if line.startswith('['):
            section = line
        elif section == '[check]' and re.match(key + r'\s*=', line):
            raw = line.split('=', 1)[1].strip()
            return raw[1:-1] if raw.startswith("'") else json.loads(raw)
    return None
command, pattern = value('command'), value('fail_name')
if not command or not pattern:
    sys.exit('  .enallagi/enallagi.toml [check] has no command or no fail_name')
out = subprocess.run(['sh', '-c', command], capture_output=True, text=True)
if out.returncode == 0:
    sys.exit('  the check stayed green with tests/zz_broken.sh failing, so it runs something else: %s' % command)
names = set()
for line in (out.stdout + out.stderr).splitlines():
    m = re.search(pattern, line)
    if m and m.groups() and m.group(1):
        names.add(m.group(1).strip())
if 'zz_broken' not in names:
    sys.exit('  fail_name %r names %s in the red output of %r, not zz_broken' % (pattern, sorted(names) or 'nothing', command))
PY
rc=$?
rm -f tests/zz_broken.sh
exit $rc
