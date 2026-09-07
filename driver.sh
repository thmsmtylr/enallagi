#!/usr/bin/env bash
# This package's own driver — the worked example of what `driverCommand` points at.
#
# The twelve other probes read text. This one reaches the artifact: it installs the package into a
# throwaway repository and drives one request through the installed loop, four times, each with a
# lane that is sloppy in exactly one way. Then it reads the PERSISTENT EFFECT — the task's status,
# the working tree, the digest, the log — and prints a `FINDING ` line for every sloppiness the
# harness let through. Nothing it prints comes from what the loop SAID; a loop can describe the
# correct verdict without reaching it.
#
# `unlabelled` is not one of those four and is reached only as `--unlabelled`: it counts rather than
# accuses, reading the `git log` of the repository it is run FROM for commits naming no task. It is
# deliberately not driven. `drive()` works inside a `mktemp -d` that is `rm -rf`'d before the probe
# prints, so a sha read out of there resolves nowhere, and the one unlabelled commit in that sandbox
# is made by this file's own lane fixture — a finding manufactured by the instrument. It has no
# round boundary either (`git log --format='%h %s' | grep -vc 'T-[0-9][0-9][0-9]'` → 57 of 171
# commits in this repository at 5842a69, 2026-09-06), so the caller chooses the history.
#
# Contract (the same one harness.json documents for any driver):
#   exit 0  whenever it REACHED the artifact, whatever it found
#   exit >0 only when it could not reach it — that is `PROBE driver ERROR`, and a scout proposes
#           nothing from a probe that did not run
#
# Usage:  ./driver.sh            from anywhere
#         driverCommand: "$HARNESS_ROOT/driver.sh"   in harness.json, with HARNESS_DRIVER=1
set -u
PKG="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# One throwaway repo, one installed harness, one iteration, one sloppy lane. Everything this
# function prints is read back by the caller; nothing is judged in here.
drive() { # $1 = the lane's sloppiness
  local mode="$1" d rc
  d=$(mktemp -d) || return 3
  (
    cd "$d" || exit 3
    git init -q && git config user.email driver@local && git config user.name driver
    mkdir -p src && echo 'export const x = 1' >src/schema.ts
    git add -A && git commit -qm 'chore: T-001 init' >/dev/null

    "$PKG/install.sh" "$d" >/dev/null 2>&1 || exit 3

    cat >src/lane.sh <<'LANE'
#!/usr/bin/env bash
# Stands in for the coding agent. Does the work, and is sloppy in exactly one way.
case "$1" in
  *"roles/implementer.md"*)
    echo "the work" > src/allowed.ts
    [ "$MODE" = "out-of-scope" ] && echo "not mine" > src/sneaky.ts
    sed -i.bak 's/^status: ready/status: review/' TASKS.md && rm -f TASKS.md.bak
    [ "$MODE" != "no-progress" ] && printf '\n## driver — T-001 — landed\nfriction: none\n' >> PROGRESS.md
    # this mode exists for exactly this: the work never lands on the branch.
    # `if`, never `[ ] && ...` as the last statement of a branch: a false test is the script's
    # exit status, the launcher reads a non-zero lane as a halt, and the verify stage never runs.
    if [ "$MODE" != "uncommitted" ]; then git add -A && git commit -qm "feat: T-001" >/dev/null; fi
    ;;
  *"roles/verifier.md"*)
    sed -i.bak 's/^status: review/status: done/' TASKS.md && rm -f TASKS.md.bak ;;
esac
LANE
    chmod +x src/lane.sh

    MODE="$mode" python3 - <<'CFG'
import json, os
c = json.load(open('harness.json'))
c['agentCommand'] = ['./src/lane.sh', '{prompt}', '{turns}']
# a red floor is its own mode; every other mode gets a green one so the gate under test is the
# one the mode is about
c['check'] = c['checkForce'] = 'false' if os.environ['MODE'] == 'red-check' else 'true'
json.dump(c, open('harness.json', 'w'), indent=2)
CFG
    "$PKG/install.sh" "$d" >/dev/null 2>&1 || exit 3

    python3 - <<'TASK'
s = open('TASKS.md').read().replace('''## [T-001] <the first task>
scope:
blockedBy: none''', '''## [T-001] <the first task>
scope: src/allowed.ts
blockedBy: none''')
open('TASKS.md', 'w').write(s)
TASK
    git add -A && git commit -qm 'chore: T-001 setup' >/dev/null

    MODE="$mode" .harness/loop.sh 1 2>&1
    # the persistent effect, read from the tree and not from anything the loop said
    echo "EFFECT status=$(awk '/^## \[T-001\]/{f=1} f&&/^status:/{print $2; exit}' TASKS.md)"
    echo "EFFECT dirty=$(git status --porcelain | grep -c . || true)"
    echo "EFFECT stages=$(grep -c . .harness/run.log 2>/dev/null || true)"
  )
  rc=$?
  rm -rf "$d"
  return $rc
}

# The operator's half, on the repo at $PWD. It reports and blocks nothing: a release, a halt
# resolution or a contract edit is the operator's by the rails and is expected to appear here.
# The count is the signal. Reached only as `--unlabelled`, never from `drive()` — see the header.
unlabelled() {
  git log --format='%h %s' |
    grep -v 'T-[0-9][0-9][0-9]' |
    sed 's/^/FINDING a commit on this round names no task: /'
}
if [ "${1:-}" = "--unlabelled" ]; then
  unlabelled
  exit 0
fi

status_of() { printf '%s\n' "$1" | sed -n 's/^EFFECT status=//p' | tail -1; }
unreached=0

for mode in uncommitted no-progress out-of-scope red-check; do
  out=$(drive "$mode") || {
    echo "driver: could not reach the artifact in mode $mode" >&2
    printf '%s\n' "$out" | tail -5 >&2
    unreached=1
    continue
  }
  status=$(status_of "$out")
  dirty=$(printf '%s\n' "$out" | sed -n 's/^EFFECT dirty=//p' | tail -1)
  stages=$(printf '%s\n' "$out" | sed -n 's/^EFFECT stages=//p' | tail -1)
  # `status` is `ready` both when the gate held and when the loop never ran; the run log tells
  # them apart. A mode in which no stage was spawned reached nothing, and is not a pass.
  [ "${stages:-0}" -gt 0 ] || {
    echo "driver: mode $mode spawned no stage (run.log is empty), so no gate was reached" >&2
    printf '%s\n' "$out" | tail -5 >&2
    unreached=1
    continue
  }
  case "$mode" in
  uncommitted)
    [ "$status" = "done" ] && [ "${dirty:-0}" -gt 0 ] && echo "FINDING a lane left its implementation uncommitted and the task still reached done with $dirty dirty path(s). Nothing the launcher runs reads the working tree; only verifier.md step 0 does, and a prompt is not a gate."
    ;;
  no-progress)
    printf '%s\n' "$out" | grep -q 'wrote no PROGRESS.md entry' ||
      echo "FINDING a lane wrote no PROGRESS.md entry and the digest did not say so. The next iteration inherits nothing and cannot tell that it is the second attempt."
    ;;
  out-of-scope)
    [ "$status" = "done" ] && echo "FINDING a lane edited a file outside its scope: globs and the task still reached done. The one-scope rail is not enforced on this tree."
    ;;
  red-check)
    [ "$status" = "done" ] && echo "FINDING a lane reached done with the floor red. The green rail is not enforced on this tree."
    ;;
  esac
done

[ "$unreached" -eq 0 ] || exit 3
exit 0
