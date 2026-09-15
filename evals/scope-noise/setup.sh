#!/usr/bin/env bash
# A proposed block whose scope names one file the fix writes and one it only reads. The rule decides
# which of the two survives promotion; nothing else in the fixture distinguishes them.
set -u
grep -q 'evals/scope-noise' .enallagi/LEARNINGS.md && {
  echo "  setup.sh: the seeded .enallagi/LEARNINGS.md already carries the rule, so the two arms would not differ" >&2
  exit 1
}
# the candidate rule, verbatim: it is not in this repo's LEARNINGS.md, which takes a dated line only
# on ACCEPT, so the eval carries the text it is measuring rather than reading a line that is not there
if [ -f .eval-ablated ]; then
  rm -f .eval-ablated
else
  cat >>.enallagi/LEARNINGS.md <<'RULE'
- [2026-09-10] A scope line that names a file the task never touches is noise the scope gate cannot tell from a forgotten edit
  → `scope:` names only the files the fix writes; a file the task merely reads is not scope. Four
  entries paid for it before this line (`grep -c 'sat on the scope line' PROGRESS.md` → 4), and
  T-005 got it right: `scope: test-hashes.json` alone, while its fix read and hashed a file the
  scope line never named. (evals/scope-noise)
RULE
  grep -q 'evals/scope-noise' .enallagi/LEARNINGS.md || {
    echo "  setup.sh: the rule was not written, so the unablated arm would measure nothing" >&2
    exit 1
  }
fi

# the block quotes a run, never a remembered line number: the adjudicator kills a figure it cannot re-derive
finding=$(harness probe | grep '^FINDING hash-uncovered ')
[ -n "$finding" ] || {
  echo "  setup.sh: harness probe emitted no hash-uncovered FINDING, so the proposal would be killed as unreproducible" >&2
  exit 1
}
{
  printf '\n## [T-901] %s\n' "${finding#FINDING hash-uncovered }"
  printf 'scope: .enallagi/test-hashes.json, .enallagi/harness.toml\n'
  printf 'blockedBy: none\n'
  printf 'status: proposed\n'
  printf 'probe: hash-uncovered\n'
  printf 'rows: none — harness\n'
  # shellcheck disable=SC2016  # the backticks are markdown in the task block, not a command
  printf 'command: `harness probe`\n'
  printf 'output: |\n  %s\n' "$finding"
  printf 'notes: proposed from the output above.\n'
} >>.enallagi/TASKS.md
git add -A && git commit -qm 'eval: one proposal with a read-only file on its scope line' >/dev/null
