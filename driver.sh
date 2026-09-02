#!/usr/bin/env bash
# This package's own driver — the worked example of what `driverCommand` points at.
#
# The eleven other probes read text. This one reaches the artifact: it installs the package into a
# throwaway repository and drives one request through the installed loop, four times, each with a
# lane that is sloppy in exactly one way. Then it reads the PERSISTENT EFFECT — the task's status,
# the working tree, the digest — and prints a `FINDING ` line for every sloppiness the harness let
# through. Nothing it prints comes from what the loop SAID; a loop can describe the correct verdict
# without reaching it.
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
    mkdir -p src && echo 'export const x = 1' > src/schema.ts
    git add -A && git commit -qm init >/dev/null

    "$PKG/install.sh" "$d" >/dev/null 2>&1 || exit 3

    cat > src/lane.sh <<'LANE'
#!/usr/bin/env bash
# Stands in for the coding agent. Does the work, and is sloppy in exactly one way.
case "$1" in
  *"roles/implementer.md"*)
    echo "the work" > src/allowed.ts
    [ "$MODE" = "out-of-scope" ] && echo "not mine" > src/sneaky.ts
    sed -i.bak 's/^status: ready/status: review/' TASKS.md && rm -f TASKS.md.bak
    [ "$MODE" != "no-progress" ] && printf '\n## driver — T-001 — landed\nfriction: none\n' >> PROGRESS.md
    # the whole point of this mode: the work never lands on the branch.
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
    git add -A && git commit -qm setup >/dev/null

    MODE="$mode" .harness/loop.sh 1 2>&1
    # the persistent effect, read from the tree and not from anything the loop said
    echo "EFFECT status=$(awk '/^## \[T-001\]/{f=1} f&&/^status:/{print $2; exit}' TASKS.md)"
    echo "EFFECT dirty=$(git status --porcelain | grep -c . || true)"
  )
  rc=$?
  rm -rf "$d"
  return $rc
}

status_of() { printf '%s\n' "$1" | sed -n 's/^EFFECT status=//p' | tail -1; }
unreached=0

for mode in uncommitted no-progress out-of-scope red-check; do
  out=$(drive "$mode") || { echo "driver: could not reach the artifact in mode $mode" >&2
                            printf '%s\n' "$out" | tail -5 >&2; unreached=1; continue; }
  status=$(status_of "$out")
  dirty=$(printf '%s\n' "$out" | sed -n 's/^EFFECT dirty=//p' | tail -1)
  case "$mode" in
    uncommitted)
      [ "$status" = "done" ] && [ "${dirty:-0}" -gt 0 ] && echo "FINDING a lane left its implementation uncommitted and the task still reached done with $dirty dirty path(s). Nothing the launcher runs reads the working tree; only verifier.md step 0 does, and a prompt is not a gate." ;;
    no-progress)
      printf '%s\n' "$out" | grep -q 'wrote no PROGRESS.md entry' \
        || echo "FINDING a lane wrote no PROGRESS.md entry and the digest did not say so. The next iteration inherits nothing and cannot tell that it is the second attempt." ;;
    out-of-scope)
      [ "$status" = "done" ] && echo "FINDING a lane edited a file outside its scope: globs and the task still reached done. The one-scope rail is not enforced on this tree." ;;
    red-check)
      [ "$status" = "done" ] && echo "FINDING a lane reached done with the floor red. The green rail is not enforced on this tree." ;;
  esac
done

[ "$unreached" -eq 0 ] || exit 3
exit 0
