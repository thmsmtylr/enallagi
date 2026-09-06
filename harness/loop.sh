#!/usr/bin/env bash
# The sequential agent loop: one lane, one task, one fresh session per task.
# This is the harness entrypoint. Multi-lane parallelism is deliberately
# not shipped — see README.md "What is not here".
#
# Tokens like __CHECK__ are substituted by install.sh from harness.json.
#
# Each iteration is one task in a fresh context: N short sessions that each
# read the repo state cold and leave it green, rather than one long session
# that degrades. Per-bug accuracy falls 58.9% -> 36.5% when an agent inherits
# its own prior state (ChainSWE, via arXiv:2607.27283).
#
# Usage:  ./loop.sh [max_iterations]
#         ./loop.sh --selftest        assert ready_unattended against a fixture queue
#         DRY_RUN=1 ./loop.sh 1       print the stage plan and the probes, spawn nothing
# Stop:   touch STOP  (checked before every stage, not only between iterations)
#
# NOTE on permissions: for unattended runs you need either
#   --dangerously-skip-permissions   (only inside a container/VM you trust)
# or a permissions allowlist tuned to your repo. The invocation itself is
# harness.json's agentCommand: claude -p, codex exec, gemini -p, opencode run,
# copilot -p, goose run and aider --message are all known-good forms.
# Start attended (no flag, watch it work), go unattended only once you
# trust the gates.

set -u
# print mode terminates a still-running background subagent at 600s; that is how iteration 1
# of the 2026-08-27 run implemented all of T-024 and was killed before it could commit
export CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=0
# Three, because the article's batch is three rounds and a batch boundary is a decision point,
# not a budget. Raise it once the halts below have earned your trust.
MAX_ITER="${1:-3}"
i=0

# The launcher is the stage sequence, the halts and the digest. Everything else is a module:
# queue.sh answers questions about TASKS.md, agent.sh spawns one process per stage and records what
# it cost, gates.sh holds what forces a done verdict back to ready.
for module in queue agent gates; do
  . "__HARNESS_DIR__/lib/$module.sh"
done

# What the launcher still owns is asserted here. The queue resolver moved to tasks.py and is
# asserted there against its own fixtures; this runs that suite rather than duplicating it.
SELFTEST_FAIL=0
assert() {
  [ "$2" = "$3" ] && {
    echo "ok    $1"
    return
  }
  echo "FAIL  $1"
  echo "      want [$2] got [$3]"
  SELFTEST_FAIL=1
}

selftest() {
  python3 __HARNESS_DIR__/tasks.py --selftest || SELFTEST_FAIL=1

  # in_scope decides every scope rejection, so it is asserted rather than trusted.
  in_scope "src/a.ts" "src/*.ts" "docs/*"
  assert "a file the scope names is in scope" "0" "$?"
  in_scope "docs/x.md" "src/*.ts"
  assert "a file no glob names is out of scope" "1" "$?"
  in_scope "src/deep/a.ts" "src/**"
  assert "a glob covers what is nested under it" "0" "$?"
  in_scope "src/a.ts"
  assert "an empty scope line puts every file out of scope" "1" "$?"
  return "$SELFTEST_FAIL"
}

# `no-clarification-left`: a marker anywhere in the contract stops the run before it starts.
# CLAUDE.md names this script as the enforcement; until the harness was packaged, nothing here
# implemented it and the rail was a wish.

# After the definitions: the selftest asserts set_status, and needs no contract to do it.
[ "${1:-}" = "--selftest" ] && {
  selftest
  exit $?
}

# A backticked mention is prose about the marker -- the rail's own definition is one -- and a bare
# one is a real marker. Never use a harness control token as an English word (LEARNINGS.md).
# shellcheck disable=SC2016 # the single quotes are deliberate: the backticks and the brackets are
# literal characters in __SPEC__'s prose, and expanding either would make the filter match nothing.
CLARIFY=$(grep -n '\[NEEDS CLARIFICATION\]' __SPEC__ 2>/dev/null | grep -v '`\[NEEDS CLARIFICATION\]`' | head -3)
[ -n "$CLARIFY" ] && {
  echo "HALT: __SPEC__ carries [NEEDS CLARIFICATION]. The loop does not start."
  printf '%s\n' "$CLARIFY" | sed 's/^/  /'
  exit 1
}

# Three of four iterations on 2026-08-27 ended in a question to nobody: each lane ran `ps`,
# found the loop.sh that spawned it, and applied `one checkout is one writer` to its own parent.
LANE="You are this loop's own lane, spawned by __HARNESS_DIR__/loop.sh. There is no human in this session
and no answer will come, so never end a turn on a question -- decide and act. A running loop.sh
or agent process in ps is your PARENT process, not a competing writer: LEARNINGS.md's one-checkout-one-writer
rule is about a second operator, and it does not apply to the process that started you."

# Accumulated as the run goes, because by the end the tree no longer says what moved.
LANDED=""
ROWS=""
PROMOTED=""
KILLED=""
HALTS=""
WARNINGS=""
HALTED=""
DRY_ROUNDS=0
halt() {
  HALTED=1
  HALTS="${HALTS}  $1"$'\n'
  echo "HALT: $1"
}
listing() {
  echo "$1"
  [ -n "$2" ] && printf '%s' "$2" || echo "  none"
}

# `touch STOP` has to land between stages too: a round is now up to four agents long.
stop_now() {
  [ -f STOP ] || return 1
  echo "STOP file found, exiting."
  return 0
}

# Three blocks already sit at needs-spec (T-044, T-057, T-064 at 14261a7), so halting on the
# status itself would end every run before its first task. The halt is on a NEW one appearing.
NEEDS_SPEC_AT_START=" $(ids_at needs-spec | tr '\n' ' ')"
needs_spec_halt() {
  local t
  for t in $(ids_at needs-spec); do
    case "$NEEDS_SPEC_AT_START" in *" $t "*) continue ;; esac
    halt "$t went to needs-spec. The contract does not answer it and no lane may decide it."
    return 0
  done
  return 1
}

while [ "$i" -lt "$MAX_ITER" ]; do
  i=$((i + 1))
  stop_now && break
  over_budget && break
  needs_spec_halt && break

  if [ -n "${DRY_RUN:-}" ]; then
    echo "  DRY_RUN: skipping unblock and archive-done, which rewrite TASKS.md"
  else
    unblock
    # A done task's verdict is history, not queue. Moving it out keeps the file every
    # fresh process must re-read small; `reap()` is the precedent for cleanup that does
    # not wait to be remembered (LEARNINGS 2026-08-24).
    __HARNESS_DIR__/archive-done.sh || true
  fi

  # Anything left to do, that a lane is allowed to touch? A bare grep for the
  # status literal also matches `attended: true` tasks, which need a human
  # credential or resource — parallel-worktrees.sh skips those on auto-select
  # and this did not, so an attended scaffold task got launched unattended.
  TASK=$(ready_unattended)

  if [ -n "${DRY_RUN:-}" ]; then
    echo "=== Iteration $i: DRY_RUN plan ==="
    [ -n "$TASK" ] &&
      echo "  stages: implement $TASK -> verify $TASK -> commit the verdict -> assert PROGRESS.md grew" ||
      echo "  stages: scout -> adjudicate -> commit the round -> re-check ready_unattended ($DRY_ROUNDS dry rounds so far, 2 ends the run)"
    echo "  probes, which are the scout's whole input:"
    # HARNESS_DRIVER here and at the scout below, never exported at the top of this file: the
    # driver installs throwaway repos and costs ~45s, and a global export would also reach every
    # lane's __CHECK__ run. The scout is the only stage whose input is probe output, and the dry
    # run has to print what that scout will actually see.
    HARNESS_DRIVER=1 __HARNESS_DIR__/hooks/probes.sh || true
  fi

  # An empty queue used to `break` here, which called an unfinished milestone finished. It is
  # not exhaustion: the scout turns probe output into proposals, the adjudicator promotes or
  # kills them, and only two consecutive rounds that leave nothing takeable end the run.
  if [ -z "$TASK" ]; then
    # This halt used to fire BEFORE the scout, which made the comment above it a lie: every
    # `ready` block being `attended: true` is the ordinary state of this queue, so the loop
    # could never reach the stage that turns probe output into unattended work. It fires now
    # only once a scout+adjudicate round has already left nothing takeable (2026-08-31).
    if [ "$DRY_ROUNDS" -ge 1 ]; then
      for t in $(ids_at ready); do
        [ "$(field "$t" attended)" = "true" ] &&
          {
            halt "$t is attended: true and it is all that is left. A human has to run it."
            break
          }
      done
      [ -n "$HALTED" ] && break
    fi

    stop_now && break
    over_budget && break
    echo "=== Iteration $i: scout (queue empty, $DRY_ROUNDS dry rounds so far) ==="
    READY_BEFORE=$(ids_at ready)
    REJ_BEFORE=$(rejections)
    agent_for scout
    HARNESS_DRIVER=1 run_agent "scout" "$LANE Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __HARNESS_DIR__/roles/scout.md: read that file first and follow it exactly. Run __HARNESS_DIR__/hooks/probes.sh and append to TASKS.md one 'status: proposed' block per FINDING line, each carrying probe:, command:, output: and rows:. Zero FINDING lines is zero blocks, which is a valid outcome and not something to escalate. Never promote, never fix, never edit any file a finding names. Then stop." 30 ||
      {
        echo "scout exited $? -- halting."
        break
      }

    stop_now && break
    over_budget && break
    echo "=== Iteration $i: adjudicate ==="
    ADJ_OUT=$(mktemp)
    agent_for adjudicator
    run_agent "adjudicate" "$LANE Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __HARNESS_DIR__/roles/adjudicator.md: read that file first and follow it exactly. Act on every block with 'status: proposed' in TASKS.md, in file order. Promote it to 'status: ready' with a scope and criteria an agent that has read only CLAUDE.md, __SPEC__, LEARNINGS.md and the block can run, or kill it and append one line to '## Rejected findings' in DECISIONS.md. A finding whose fix needs a change to __SPEC__ or CLAUDE.md is neither: leave it at proposed and print a line beginning HALT that names the block's id. Do not commit; this loop commits your round. Then stop." 40 2>&1 | tee "$ADJ_OUT"
    [ "${PIPESTATUS[0]}" -eq 0 ] || {
      echo "adjudicate exited non-zero -- halting."
      rm -f "$ADJ_OUT"
      break
    }

    if [ -z "${DRY_RUN:-}" ]; then
      git add TASKS.md DECISIONS.md 2>/dev/null
      git diff --cached --quiet 2>/dev/null || git commit -q -m "queue: scout and adjudicator round (iteration $i)"
      ADJ_HALT=$(grep -Ei '^[[:space:]]*halt' "$ADJ_OUT" | grep -Eo 'T-[0-9]+' | head -1)
      [ -n "$ADJ_HALT" ] && halt "the adjudicator halted on $ADJ_HALT: a fix that needs __SPEC__ or CLAUDE.md is a human's call."
    fi
    rm -f "$ADJ_OUT"

    NEW_KILLS=$(diff <(printf '%s\n' "$REJ_BEFORE") <(rejections) | sed -n 's/^> //p')
    [ -n "$NEW_KILLS" ] && KILLED="${KILLED}${NEW_KILLS}"$'\n'
    NEW_READY=$(diff <(printf '%s\n' "$READY_BEFORE") <(ids_at ready) | sed -n 's/^> //p' | tr '\n' ' ')
    [ -n "$NEW_READY" ] && PROMOTED="$PROMOTED $NEW_READY"

    for t in $(ids_at proposed); do
      block "$t" | grep -qE '__SPEC__|__CONTEXT_FILE__' &&
        {
          halt "$t is still proposed and its fix names __SPEC__ or CLAUDE.md. Neither is a lane's to edit."
          break
        }
    done
    [ -n "$HALTED" ] && break
    needs_spec_halt && break

    # A dry round is one that leaves nothing a lane can legally take -- `ready_unattended` empty
    # after the pair. An empty queue on its own is not exhaustion; two of these in a row is.
    if [ -n "$(ready_unattended)" ]; then
      DRY_ROUNDS=0
      echo "  the round left takeable work. Dry counter reset to 0."
    else
      DRY_ROUNDS=$((DRY_ROUNDS + 1))
      echo "  dry round $DRY_ROUNDS: nothing a lane can take."
      [ "$DRY_ROUNDS" -ge 2 ] && {
        echo "Two consecutive dry rounds. Nothing left to do."
        break
      }
    fi
    continue
  fi

  PROG_BEFORE=$(cat PROGRESS.md 2>/dev/null | wc -c)
  # the sha the iteration starts at: everything gate_scope judges is committed after this point
  ITER_BASE=$(git rev-parse HEAD 2>/dev/null || true)

  stop_now && break
  over_budget && break
  echo "=== Iteration $i: implement $TASK ==="
  agent_for implementer
  run_agent "$TASK implement" "$LANE Read __CONTEXT_FILE__, __SPEC__, LEARNINGS.md, TASKS.md, git log --oneline -20, and the TAIL of PROGRESS.md (tail -200 PROGRESS.md -- it is append-only and newest-last, so reading it from the top gives you the oldest entries and none of the handoff). The tail and the log are what the one-row rail has you re-read at the start of an iteration. Your role is defined in __HARNESS_DIR__/roles/implementer.md: read that file first and follow it exactly. Complete exactly ONE task: the first with status 'ready' whose blockers are done and which is NOT marked 'attended: true'. If that task's scope files already carry uncommitted work, a prior lane was terminated mid-flight: finish it, never restart it and never discard it. Follow the task protocol strictly. Before you stop you MUST git add the paths named on the task's scope: line (never git add -A, LEARNINGS.md 2026-08-26), commit them, paste the exact commands and their output into the task's notes:, and set status: review. You MUST also append this iteration's PROGRESS.md entry in the format written at the top of that file -- what happened, which rows moved, and any BLOCKED with its written reason -- and include it in that commit. An implementation left uncommitted is a lost iteration." 120 ||
    {
      echo "implement exited $? -- halting rather than reporting a finished iteration."
      break
    }

  stop_now && break
  over_budget && break
  echo "=== Iteration $i: verify $TASK ==="
  agent_for verifier
  run_agent "$TASK verify" "$LANE Read __CONTEXT_FILE__, __SPEC__ and TASKS.md. Your role is defined in __HARNESS_DIR__/roles/verifier.md: read that file first and follow it exactly. Verify every task with status 'review'. Promote to done or reject to ready with concrete reasons, and commit the verdict. If nothing is at review, say so in one line and stop; that is a valid outcome, not something to escalate. Then stop." 100 ||
    {
      echo "verify exited $? -- halting."
      break
    }

  # The verifier writes its verdict into the working tree and stops. Nothing here
  # committed it, so a rejection could sit uncommitted until a human noticed.
  if [ -z "${DRY_RUN:-}" ]; then
    git add TASKS.md 2>/dev/null
    git diff --cached --quiet 2>/dev/null || git commit -q -m "verify: $TASK verdict"

    gate_verdict "$TASK" || true
    gate_scope "$TASK" "$ITER_BASE" || true

    case "$(field "$TASK" status)" in
    done)
      LANDED="$LANDED $TASK"
      R=$(field "$TASK" rows)
      [ -n "$R" ] && ROWS="${ROWS}  $TASK: $R"$'\n'
      ;;
    *) WARNINGS="${WARNINGS}  $TASK ended the iteration at $(field "$TASK" status), not done."$'\n' ;;
    esac
    # The entry is all the next iteration inherits, so a silent iteration is itself the finding.
    [ "$(cat PROGRESS.md 2>/dev/null | wc -c)" -gt "$PROG_BEFORE" ] ||
      WARNINGS="${WARNINGS}  iteration $i wrote no PROGRESS.md entry for $TASK."$'\n'

    needs_spec_halt && break
  fi

done

echo
echo "=== digest: $i iteration(s) ==="
echo "wall clock: ${SPENT_SECONDS}s across $(wc -l <"$RUN_LOG" 2>/dev/null | tr -d ' ') stage(s)${SPENT_USD:+, cost \$$SPENT_USD}"
[ -n "$ROLE_SECONDS" ] && printf '%s' "$ROLE_SECONDS" | awk 'NF{t[$1]+=$2} END{for(r in t) printf "  %s: %ds\n", r, t[r]}' | sort
echo "tasks landed:${LANDED:- none}"
listing "rows turned green:" "$ROWS"
echo "findings promoted:${PROMOTED:- none}"
listing "findings killed:" "$KILLED"
listing "halts:" "$HALTS"
listing "warnings:" "$WARNINGS"
echo "Loop finished after $i iteration(s)."
