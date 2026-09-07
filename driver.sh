#!/usr/bin/env bash
# This package's own driver — the worked example of what `driver_command` points at.
#
# The twelve other probes read text. This one reaches the artifact: it installs the harness into a
# throwaway repository and drives one request through the installed binary, four times, each with a
# lane that is sloppy in exactly one way. Then it reads the PERSISTENT EFFECT — the task's status,
# the working tree, the event log — and prints a `FINDING ` line for every sloppiness the harness
# let through. Nothing it prints comes from what the run SAID; a run can describe the correct
# verdict without reaching it.
#
# `unlabelled` is not one of those four and is reached only as `--unlabelled`: it counts rather than
# accuses, reading the `git log` of the repository it is run FROM for commits naming no task. It is
# deliberately not driven. `drive()` works inside a `mktemp -d` that is `rm -rf`'d before the probe
# prints, so a sha read out of there resolves nowhere, and the one unlabelled commit in that sandbox
# is made by this file's own lane fixture — a finding manufactured by the instrument. It has no
# round boundary either, so the caller chooses the history.
#
# Contract (the same one harness.default.toml documents for any driver):
#   exit 0  whenever it REACHED the artifact, whatever it found
#   exit >0 only when it could not reach it — that is `PROBE driver ERROR`, and a scout proposes
#           nothing from a probe that did not run
#
# Usage:  ./driver.sh            from anywhere
#         driver_command = "$HARNESS_ROOT/driver.sh"   in harness.toml, with HARNESS_DRIVER=1
#
# HARNESS_BIN names the binary under test. It defaults to this checkout's release build, which is
# built here when it is absent: the driver measures the artifact, so it must not measure a stale one
# the caller happens to have on PATH.
set -u
PKG="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HARNESS_BIN="${HARNESS_BIN:-$PKG/target/release/harness}"
if [ ! -x "$HARNESS_BIN" ]; then
  (cd "$PKG" && cargo build --release -q) || {
    echo "driver: no $HARNESS_BIN and cargo build --release failed" >&2
    exit 3
  }
fi
[ -x "$HARNESS_BIN" ] || {
  echo "driver: $HARNESS_BIN is not executable" >&2
  exit 3
}
export HARNESS_BIN # the lane fixture runs `harness tasks set-status`

# One throwaway repo, one installed harness, one iteration, one sloppy lane. Everything this
# function prints is read back by the caller; nothing is judged in here.
drive() { # $1 = the lane's sloppiness
  local mode="$1" d rc
  d=$(mktemp -d) || return 3
  (
    cd "$d" || exit 3
    git init -q && git config user.email driver@local && git config user.name driver
    git config commit.gpgsign false
    mkdir -p src && echo 'export const x = 1' >src/schema.ts
    git add -A && git commit -qm 'chore: T-001 init' >/dev/null

    cat >src/lane.sh <<'LANE'
#!/usr/bin/env bash
# Stands in for the coding agent. Does the work, and is sloppy in exactly one way.
case "$1" in
  *"roles/implementer.md"*)
    echo "the work" > src/allowed.ts
    [ "$MODE" = "out-of-scope" ] && echo "not mine" > src/sneaky.ts
    "$HARNESS_BIN" tasks set-status T-001 review 'driver lane implemented' >/dev/null
    [ "$MODE" != "no-progress" ] && printf '\n## driver — T-001 — landed\nfriction: none\n' >> PROGRESS.md
    # this mode exists for exactly this: the work never lands on the branch.
    # `if`, never `[ ] && ...` as the last statement of a branch: a false test is the script's
    # exit status, the launcher reads a non-zero lane as a halt, and the verify stage never runs.
    if [ "$MODE" != "uncommitted" ]; then git add -A && git commit -qm "feat: T-001" >/dev/null; fi
    ;;
  *"roles/verifier.md"*)
    "$HARNESS_BIN" tasks set-status T-001 done 'driver lane verified' >/dev/null ;;
esac
echo '{"total_cost_usd": 0.5}'
LANE
    chmod +x src/lane.sh

    # a red floor is its own mode; every other mode gets a green one so the gate under test is the
    # one the mode is about
    check=true
    [ "$mode" = "red-check" ] && check=false
    cat >harness.toml <<TOML
[agent]
preset = "custom"
command = ["./src/lane.sh", "{prompt}", "{turns}"]

[agent.usage]
cost = "total_cost_usd"

[check]
command = "$check"
TOML

    "$HARNESS_BIN" init >/dev/null 2>&1 || exit 3

    # The seeded T-001 ships with an empty scope: line. Give it one, so the scope gate has
    # something to judge the lane's diff against.
    sed -i.bak 's|^scope:$|scope: src/allowed.ts|' TASKS.md && rm -f TASKS.md.bak
    git add -A && git commit -qm 'chore: T-001 setup' >/dev/null

    MODE="$mode" "$HARNESS_BIN" run 1 --no-tui 2>&1
    # the persistent effect, read from the tree and not from anything the run said
    echo "EFFECT status=$("$HARNESS_BIN" tasks list | awk '/^T-001/{print $NF}')"
    echo "EFFECT dirty=$(git status --porcelain | grep -c . || true)"
    echo "EFFECT stages=$("$HARNESS_BIN" events 2>/dev/null | grep -c 'stage\.end' || true)"
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
  # `status` is `ready` both when the gate held and when the run never happened; the event log
  # tells them apart. A mode in which no stage was spawned reached nothing, and is not a pass.
  [ "${stages:-0}" -gt 0 ] || {
    echo "driver: mode $mode spawned no stage (the event log has none), so no gate was reached" >&2
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
      echo "FINDING a lane wrote no PROGRESS.md entry and the run did not say so. The next iteration inherits nothing and cannot tell that it is the second attempt."
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
