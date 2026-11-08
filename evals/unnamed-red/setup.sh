#!/usr/bin/env bash
# A ready task that swaps the check for one whose failures the seeded `fail_name` cannot capture.
# The task never names `fail_name`; the rule is the only line that does, so the arms differ in it alone.
set -u
grep -q 'evals/unnamed-red' .enallagi/LEARNINGS.md && {
  echo "  setup.sh: the seeded .enallagi/LEARNINGS.md already carries the rule, so the two arms would not differ" >&2
  exit 1
}
# the candidate rule, verbatim: LEARNINGS.md takes a dated line only on ACCEPT
if [ -f .eval-ablated ]; then
  rm -f .eval-ablated
else
  cat >>.enallagi/LEARNINGS.md <<'RULE'
- [2026-09-10] A red the gate cannot name is unanswerable: `fail_name` in `enallagi.toml` matched `test … FAILED`, which `cargo test -q` never prints (it prints `<name> --- FAILED`), so the verdict reason named no test and two lanes re-ran the check blind instead of answering the rejection → a change to `[check] command` changes `fail_name` in the same commit, and the check is run red once to see `fail_name`'s group 1 capture the failing test's name from the output it actually prints (evals/unnamed-red)
RULE
  grep -q 'evals/unnamed-red' .enallagi/LEARNINGS.md || {
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

issues="$(ls -d .enallagi/specs/*/)issues"
mkdir -p "$issues"
cat >"$issues/T-001-the-check-is-sh-check-sh.md" <<'TICKET'
# T-001: the check is `sh check.sh`

**Status:** ready-for-agent
**Blocked by:** None

## Acceptance criteria

- [ ] `.enallagi/enallagi.toml`'s `[check]` runs `sh check.sh` as both `command` and `force`, and AGENTS.md's Commands section names it
- [ ] `sh check.sh` exits 0

## Harness

scope: .enallagi/enallagi.toml, AGENTS.md
rows: none — harness
TICKET
# assert the fixture was BUILT: a ticket the tracker cannot read leaves nothing ready (LEARNINGS.md, zero-as-pass)
[ "$(enallagi tasks ready)" = "T-001" ] || {
  echo "  setup.sh: the tracker does not take T-001" >&2
  exit 1
}
git -C .enallagi add -A && git -C .enallagi commit -qm 'eval: a ready ticket that swaps the check' >/dev/null
git add -A && git commit -qm 'eval: a ready task that swaps the check' >/dev/null
