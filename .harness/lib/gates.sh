#!/usr/bin/env bash
# The gates the launcher runs behind a verifier's verdict. Each forces an unsupported `done` back
# to `ready` and records why. Sourced by loop.sh.

# LEARNINGS.md 2026-08-25: nothing between "the verifier wrote done" and "TASKS.md says done" ever
# executed a command, so a false VERIFIED was indistinguishable from a real one. The verifier is an
# agent reporting on itself; this runs the gate and forces a `done` the tree cannot support back to
# `ready`. `blocked-is-allowed` names this function as its enforcement.
gate_verdict() { # $1 = task id
  local task="$1" out left
  [ "$(field "$task" status)" = "done" ] || return 0
  # Found by driver.sh on 2026-09-02, and by nothing else: a lane that never commits leaves the work
  # in the tree, the check is green either way, and the task reached `done` with the implementation
  # on no branch — a merge would take none of it. `verifier.md` step 0 says reject, and a prompt is
  # not a gate. ponytail: STOP is the harness's own marker and never a lane's work; anything else
  # untracked at this point is.
  left=$(git status --porcelain 2>/dev/null | grep -v ' STOP$' | grep -c . || true)
  if [ "${left:-0}" -gt 0 ]; then
    echo "  !! [$task] GATE FAILED -- done, and $left path(s) are uncommitted. The implementation is not on the branch."
    git status --short | sed 's/^/     /'
    set_status "$task" "ready" "the verifier returned done with $left uncommitted path(s): the work is not on the branch"
    git add TASKS.md 2>/dev/null
    git diff --cached --quiet 2>/dev/null || git commit -q -m "chore($task): harness gate rejected a done verdict with work off the branch"
    WARNINGS="${WARNINGS}  $task was forced back to ready: done with $left uncommitted path(s)."$'\n'
    return 1
  fi
  if out=$(.harness/hooks/check-gate.sh 2>&1); then
    echo "  gate: $task done, and the gate agrees."
    return 0
  fi
  echo "  !! [$task] GATE FAILED -- the verifier said done and the gate is red. Forced back to ready."
  printf '%s\n' "$out" | tail -30 | sed 's/^/     /'
  set_status "$task" "ready" "the verifier returned done and the gate was red at $(git rev-parse --short HEAD 2>/dev/null). $(printf '%s' "$out" | grep -A2 'not on the baseline' | tail -1 | sed 's/^ *//')"
  git add TASKS.md 2>/dev/null
  git diff --cached --quiet 2>/dev/null || git commit -q -m "chore($task): harness gate rejected a false VERIFIED"
  WARNINGS="${WARNINGS}  $task was forced back to ready by the gate: the verifier said done, the gate was red."$'\n'
  return 1
}

# The matcher, split out so the selftest can assert it with no git history. `case` globs are
# permissive -- `*` crosses `/` here, unlike pathname expansion -- so `src/*` covers `src/a/b.ts`.
# ponytail: permissive matching lets a nested file through a shallow glob; fnmatch in python if
# a lane ever exploits that.
in_scope() { # $1 = path, then the glob patterns
  local f="$1" pat; shift
  for pat in "$@"; do
    [ -n "$pat" ] || continue
    case "$f" in $pat) return 0 ;; esac
  done
  return 1
}

# Every task writes these by protocol, so they are in scope for all of them.
BOOKKEEPING="TASKS.md PROGRESS.md PROGRESS.archive.md DECISIONS.md LEARNINGS.md"

# `one-scope` named "the harness's scope check" and no such check existed: the M2 mechanism was a
# PLANNER that keeps lanes disjoint, not a check that a lane stayed inside its own (the `one-scope` rail).
# This is the check -- the iteration's own commits, diffed against the task's `scope:` globs.
#
# It also routes the three loops: a diff touching the harness -- the launcher, the
# hooks, the baseline, the hashes, harness.json -- under a task that did not declare
# `rows: none — harness` is a product task editing the measure of its own product lever, and the
# article's rule is that one round may not do both. ponytail: the harness set is this literal list,
# so a project whose check script lives elsewhere adds it here.
gate_scope() { # $1 = task id, $2 = the sha the iteration started at
  local task="$1" base="$2" rows out_of="" harness_hit="" f reason
  local -a pats
  [ "$(field "$task" status)" = "done" ] || return 0
  [ -n "$base" ] || return 0
  IFS=' ' read -r -a pats <<< "$(field "$task" scope | tr -d '` ' | tr ',' ' ')"
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    case " $BOOKKEEPING " in *" $f "*) continue ;; esac
    case "$f" in
      .harness/*|*/hooks/*|.check-baseline|test-hashes.json|harness.json) harness_hit="$harness_hit $f" ;;
    esac
    # `${pats[@]+...}`, never `${pats+...}`: bash 3.2 is macOS's /bin/bash and expands an empty
    # array as an unbound variable under `set -u` even when the array itself is set.
    in_scope "$f" ${pats[@]+"${pats[@]}"} || out_of="$out_of $f"
  done <<< "$(git diff --name-only "$base" HEAD 2>/dev/null)"

  rows=$(field "$task" rows)
  case "$rows" in *none*harness*) harness_hit="" ;; esac
  [ -z "$out_of" ] && [ -z "$harness_hit" ] && { echo "  scope: $task stayed inside its scope."; return 0; }

  reason=""
  [ -n "$out_of" ] && reason="touched ${out_of# }, which the scope line does not name"
  [ -n "$harness_hit" ] && reason="${reason:+$reason; }touched the harness (${harness_hit# }) with rows: ${rows:-unset}, not \`none — harness\`"
  echo "  !! [$task] SCOPE FAILED -- $reason. Forced back to ready."
  set_status "$task" "ready" "the verifier returned done and the scope gate rejected it: $reason"
  git add TASKS.md 2>/dev/null
  git diff --cached --quiet 2>/dev/null || git commit -q -m "chore($task): harness scope gate rejected a done verdict"
  WARNINGS="${WARNINGS}  $task was forced back to ready by the scope gate: $reason."$'\n'
  return 1
}

