#!/usr/bin/env bash
# A ready task that swaps the check for one whose failures the seeded `fail_name` cannot capture.
# The task never names `fail_name`; the rule is the only line that does, so the arms differ in it alone.
set -u
grep -q 'evals/unnamed-red' LEARNINGS.md && {
  echo "  setup.sh: the seeded LEARNINGS.md already carries the rule, so the two arms would not differ" >&2
  exit 1
}
# the candidate rule, verbatim: LEARNINGS.md takes a dated line only on ACCEPT
if [ -f .eval-ablated ]; then
  rm -f .eval-ablated
else
  cat >>LEARNINGS.md <<'RULE'
- [2026-09-10] A red the gate cannot name is unanswerable: `fail_name` in `harness.toml` matched `test … FAILED`, which `cargo test -q` never prints (it prints `<name> --- FAILED`), so the verdict reason named no test and two lanes re-ran the check blind instead of answering the rejection → a change to `[check] command` changes `fail_name` in the same commit, and the check is run red once to see `fail_name`'s group 1 capture the failing test's name from the output it actually prints (evals/unnamed-red)
RULE
  grep -q 'evals/unnamed-red' LEARNINGS.md || {
    echo "  setup.sh: the rule was not written, so the unablated arm would measure nothing" >&2
    exit 1
  }
fi

mkdir -p tests
cat >check.sh <<'SH'
#!/bin/sh
# Runs every tests/*.sh: `<name> ... ok` for a pass, `<name> --- FAILED` for a failure, exit 1 if any failed.
rc=0
for t in tests/*.sh; do
  name=$(basename "$t" .sh)
  if sh "$t"; then echo "$name ... ok"; else echo "$name --- FAILED"; rc=1; fi
done
exit $rc
SH
echo "export const greeting = 'hello'" >src/greeting.ts
echo "grep -q \"'hello'\" src/greeting.ts" >tests/greeting.sh

python3 - <<'PY'
# assert the fixture was BUILT: a replace that matches nothing leaves the placeholder task (LEARNINGS.md, zero-as-pass)
src = open('TASKS.md', encoding='utf-8').read()
was = '''## [T-001] <the first task>
scope:
blockedBy: none
status: ready
rows: none — harness
criteria:
  - <what has to become true, and how you would see it>
notes:'''
assert src.count(was) == 1, 'the seeded T-001 block is not what this fixture expects'
open('TASKS.md', 'w', encoding='utf-8').write(src.replace(was, '''## [T-001] the check is `sh check.sh`
scope: harness.toml, AGENTS.md
blockedBy: none
status: ready
rows: none — harness
criteria:
  - `harness.toml`'s `[check]` runs `sh check.sh` as both `command` and `force`, and AGENTS.md's Commands section names it
  - `sh check.sh` exits 0
notes:'''))
PY
git add -A && git commit -qm 'eval: a ready task that swaps the check' >/dev/null
