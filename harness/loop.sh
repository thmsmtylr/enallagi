#!/usr/bin/env bash
# The sequential agent loop: one lane, one task, one fresh session per task.
# This is the whole harness's entrypoint. Multi-lane parallelism is deliberately
# not shipped — see README.md "What is not here".
#
# Tokens like __CHECK__ are substituted by install.sh from harness.json.
#
# Each iteration = ONE task, FRESH context. This is the whole trick:
# instead of one long session that degrades, you get N short sessions
# that each read the repo state cold and leave it green.
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

# First task that is ready, unattended AND whose blockers are all done. It used to read only
# `status:`, so on 2026-08-27 it handed a lane T-018 while T-018's blocker sat at `ready`; and
# `exit` inside the block ran END, which re-tested the same st/att and printed the id twice.
# One pass, decided in END, sharing unblock()'s isready(). `none` is a spelling of no blocker
# that four blocks in TASKS.md use (`grep -c '^blockedBy: none' TASKS.md` -> 4).
ready_unattended() {
  awk '
    /^## \[T-[0-9]+\]/ { id = $2; gsub(/[][]/, "", id); order[++n] = id; next }
    /^status:/    { s = $0; sub(/^status:[[:space:]]*/, "", s); st[id] = s; next }
    /^attended:/  { a = $0; sub(/^attended:[[:space:]]*/, "", a); att[id] = a; next }
    /^blockedBy:/ { b = $0; sub(/^blockedBy:[[:space:]]*/, "", b); gsub(/[[:space:]]/, "", b); bb[id] = b; next }
    END {
      for (k = 1; k <= n; k++) {
        t = order[k]
        if (st[t] == "ready" && att[t] != "true" && isready(t)) { print t; exit }
      }
    }
    function isready(t,   m, p, j) {
      if (bb[t] == "" || bb[t] == "none") return 1
      m = split(bb[t], p, ",")
      for (j = 1; j <= m; j++) if (st[p[j]] != "done") return 0
      return 1
    }
  ' "${1:-TASKS.md}"
}

# Every id at one status, one block by id, one field of that block, and the kill lines. The
# halts and the digest below are greps over these four and nothing else.
# `[[:space:]]`, never `[ \t]`: BSD sed reads that class as space-backslash-t, so
# `sed -n 's/^attended:[ \t]*//p'` on `attended: true` prints `rue` (run 2026-08-28).
ids_at() { awk -v w="^status:[[:space:]]*$1[[:space:]]*$" '/^## \[T-[0-9]+\]/ { id = $2; gsub(/[][]/, "", id) } $0 ~ w { print id }' TASKS.md; }
block() { awk -v t="$1" '/^## \[T-[0-9]+\]/ { id = $2; gsub(/[][]/, "", id) } id == t' TASKS.md; }
field() { block "$1" | sed -n "s/^$2:[[:space:]]*//p" | head -1; }
rejections() { sed -n '/^## Rejected findings/,/^## \[T-/p' DECISIONS.md 2>/dev/null | grep '^- \[' || true; }

# A task whose blockers are all `done` is runnable, but nothing was flipping it:
# loop.sh only ever looked for `status: ready`, so the queue stalled after every
# task whose successor read `blocked`. Rewrites TASKS.md so the file stays true.
unblock() {
  awk '
    FNR==NR {
      if ($0 ~ /^## \[T-[0-9]+\]/) { id=$2; gsub(/[][]/,"",id) }
      else if ($0 ~ /^status:/)    { s=$0; sub(/^status:[ \t]*/,"",s); st[id]=s }
      else if ($0 ~ /^blockedBy:/) { b=$0; sub(/^blockedBy:[ \t]*/,"",b); gsub(/[ \t]/,"",b); bb[id]=b }
      next
    }
    /^## \[T-[0-9]+\]/ { cur=$2; gsub(/[][]/,"",cur) }
    /^status:[ \t]*blocked/ { if (isready(cur)) { print "status: ready"; next } }
    { print }
    function isready(t,   n,p,i) {
      if (bb[t]=="") return 1
      n=split(bb[t],p,",")
      for (i=1;i<=n;i++) if (st[p[i]]!="done") return 0
      return 1
    }
  ' TASKS.md TASKS.md > TASKS.md.unblock || { rm -f TASKS.md.unblock; return 1; }
  # awk can exit 0 having written nothing; mv would then delete the queue.
  [ "$(wc -l < TASKS.md.unblock)" -eq "$(wc -l < TASKS.md)" ] \
    || { echo "unblock: line count changed, refusing to replace TASKS.md" >&2; rm -f TASKS.md.unblock; return 1; }
  mv TASKS.md.unblock TASKS.md
}

# A headless agent prints nothing until it exits, so the spawning shell sits silent for
# ten minutes at a time and reads as hung. Frames go to /dev/tty, never stdout:
# stdout is piped into tee and a log full of spinner frames is worse than none.
# No controlling tty (cron, a harness-spawned shell) falls back to one heartbeat
# line a minute — the same signal, at a rate a log can hold.
# The headless invocation of whichever coding agent this repo uses. {prompt} and {turns} are
# filled in per stage. Subagents are not portable -- they exist for Claude, Copilot and Cursor and
# not for Codex or Gemini (arXiv:2602.14690, Table 1) -- so role isolation here is one fresh
# PROCESS per stage, which every agent CLI supports.
# One command per role, so the verifier can run on a different model or vendor from the
# implementer. harness.json takes either a word list (every role gets it) or an object keyed by
# role with a 'default'. Independent verification is the reason the field is per-role.
AGENT_CMD_DEFAULT=(__AGENT_COMMAND_DEFAULT__)
AGENT_CMD_SCOUT=(__AGENT_COMMAND_SCOUT__)
AGENT_CMD_ADJUDICATOR=(__AGENT_COMMAND_ADJUDICATOR__)
AGENT_CMD_IMPLEMENTER=(__AGENT_COMMAND_IMPLEMENTER__)
AGENT_CMD_VERIFIER=(__AGENT_COMMAND_VERIFIER__)
AGENT_CMD=("${AGENT_CMD_DEFAULT[@]}")
AGENT_ROLE=default

# Point AGENT_CMD at one role's command. A role harness.json does not name got the default at
# install time, so every branch here is defined.
agent_for() { # $1 = role
  case "$1" in
    scout)       AGENT_CMD=("${AGENT_CMD_SCOUT[@]}") ;;
    adjudicator) AGENT_CMD=("${AGENT_CMD_ADJUDICATOR[@]}") ;;
    implementer) AGENT_CMD=("${AGENT_CMD_IMPLEMENTER[@]}") ;;
    verifier)    AGENT_CMD=("${AGENT_CMD_VERIFIER[@]}") ;;
    *)           AGENT_CMD=("${AGENT_CMD_DEFAULT[@]}") ;;
  esac
  AGENT_ROLE="$1"
}

FRAMES='|/-\'
spin() {
  local label="$1"; shift
  local tty=1
  { exec 3>/dev/tty; } 2>/dev/null || tty=
  "$@" </dev/null &
  local pid=$! start=$SECONDS f=0 e last=0
  while kill -0 "$pid" 2>/dev/null; do
    e=$((SECONDS - start))
    if [ -n "$tty" ]; then
      printf '\r\033[2K  %s %s  %dm%02ds' "${FRAMES:$((f % 4)):1}" "$label" $((e / 60)) $((e % 60)) >&3
      f=$((f + 1))
      sleep 0.2
    else
      [ $((e - last)) -ge 60 ] && { printf '  ... %s %dm%02ds\n' "$label" $((e / 60)) $((e % 60)); last=$e; }
      sleep 5
    fi
  done
  wait "$pid"; local rc=$?
  e=$((SECONDS - start))
  [ -n "$tty" ] && { printf '\r\033[2K  %s  %dm%02ds\n' "$label" $((e / 60)) $((e % 60)) >&3; exec 3>&-; }
  return $rc
}

# "You've hit your session limit . resets 12:40am (Australia/Melbourne)". Without
# this the loop counts an exhausted agent as a finished iteration and burns the
# rest doing nothing, stranding a task at `review` with no verdict.
seconds_until_reset() {
  python3 - "$1" <<'RESET'
import sys, re, datetime
from zoneinfo import ZoneInfo
m = re.search(r'resets\s+(\d{1,2}):(\d{2})\s*([ap])\.?m\.?\s*\(([^)]+)\)', sys.argv[1], re.I)
if not m: sys.exit(1)
h, mi, ap, tz = int(m.group(1)), int(m.group(2)), m.group(3).lower(), m.group(4)
h = h % 12 + (12 if ap == 'p' else 0)
now = datetime.datetime.now(ZoneInfo(tz))
t = now.replace(hour=h, minute=mi, second=0, microsecond=0)
if t <= now: t += datetime.timedelta(days=1)
print(int((t - now).total_seconds()) + 60)
RESET
}

# Sleep in chunks so `touch STOP` still lands during a multi-hour wait.
sleep_until() {
  local left="$1"
  while [ "$left" -gt 0 ]; do
    [ -f STOP ] && { echo "STOP during limit wait."; return 1; }
    printf '\r\033[2K  waiting out session limit: %dm left' $((left / 60))
    sleep $(( left < 60 ? left : 60 )); left=$((left - 60))
  done
  printf '\r\033[2K'; return 0
}

run_agent() {
  local label="$1" prompt="$2" turns="$3" out rc hit wait_s
  # DRY_RUN reads the run without buying it: every stage announces itself here and nothing spawns.
  if [ -n "${DRY_RUN:-}" ]; then
    echo "  DRY_RUN would spawn: $label as role ${AGENT_ROLE:-default} via ${AGENT_CMD[0]} (turns: $turns)"
    printf '%s\n' "$prompt" | sed 's/^/    | /'
    return 0
  fi
  while :; do
    out=$(mktemp)
    local cmd=() word
    for word in "${AGENT_CMD[@]}"; do
      word="${word//\{prompt\}/$prompt}"; word="${word//\{turns\}/$turns}"
      cmd+=("$word")
    done
    spin "$label" "${cmd[@]}" >"$out" 2>&1
    rc=$?
    cat "$out"
    hit=$(grep -m1 -i "__RATE_LIMIT_PATTERN__" "$out" || true)
    rm -f "$out"
    [ -n "$hit" ] || return $rc
    wait_s=$(seconds_until_reset "$hit") || wait_s=1800
    echo "  session limit. sleeping $((wait_s / 60))m, then retrying $label."
    sleep_until "$wait_s" || return 1
  done
}

# The two defects above are asserted against a fixture queue, never against TASKS.md, whose
# answer changes every iteration.
SELFTEST_FAIL=0
SELFTEST_DIR=""
assert() {
  [ "$2" = "$3" ] && { echo "ok    $1"; return; }
  echo "FAIL  $1"; echo "      want [$2] got [$3]"; SELFTEST_FAIL=1
}

selftest() {
  SELFTEST_DIR=$(mktemp -d) || return 1
  trap 'rm -rf "$SELFTEST_DIR"' EXIT
  local out
  cat > "$SELFTEST_DIR/blocked.md" <<'FIXTURE'
## [T-001] a blocker that is done
blockedBy:
status: done

## [T-002] ready, but its blocker is not done
blockedBy: T-009
status: ready

## [T-003] ready, attended, not a lane's to take
blockedBy: T-001
status: ready
attended: true

## [T-004] the one a lane may take
blockedBy: T-001
status: ready

## [T-009] the blocker T-002 is waiting on
blockedBy:
status: ready
FIXTURE
  out=$(ready_unattended "$SELFTEST_DIR/blocked.md")
  assert "a ready task whose blocker is not done is not selected" "" "$(printf '%s\n' "$out" | grep -x T-002)"
  assert "an attended ready task is skipped" "" "$(printf '%s\n' "$out" | grep -x T-003)"
  assert "a ready task whose blockers are all done is selected" "T-004" "$out"
  assert "the answer is printed exactly once" "1" "$(printf '%s\n' "$out" | grep -c '^T-')"

  cat > "$SELFTEST_DIR/released.md" <<'FIXTURE'
## [T-009] the blocker, now done
blockedBy:
status: done

## [T-002] the task it was holding
blockedBy: T-009
status: ready
FIXTURE
  assert "a blocker at done releases its task" "T-002" "$(ready_unattended "$SELFTEST_DIR/released.md")"

  cat > "$SELFTEST_DIR/none.md" <<'FIXTURE'
## [T-006] blockedBy spelled none, which is not a task id
blockedBy: none
status: ready
FIXTURE
  assert "blockedBy none is no blocker" "T-006" "$(ready_unattended "$SELFTEST_DIR/none.md")"

  cat > "$SELFTEST_DIR/nothing.md" <<'FIXTURE'
## [T-005] nothing a lane can take
blockedBy:
status: blocked
FIXTURE
  assert "a queue with nothing ready prints nothing" "" "$(ready_unattended "$SELFTEST_DIR/nothing.md")"

  # set_status is what gate_verdict uses to force a false VERIFIED back to ready, so it is
  # asserted against a fixture rather than trusted.
  cat > "$SELFTEST_DIR/verdict.md" <<'FIXTURE'
## [T-010] the one the gate rejects
blockedBy: none
status: done

## [T-011] a block the gate did not touch
blockedBy: none
status: done
FIXTURE
  TASKS_FILE="$SELFTEST_DIR/verdict.md" set_status T-010 ready "the gate was red"
  assert "set_status rewrites the named block" "status: ready" \
    "$(awk '/^## \[T-010\]/{f=1} f&&/^status:/{print; exit}' "$SELFTEST_DIR/verdict.md")"
  assert "set_status records why, next to the status" "gate: the gate was red" \
    "$(grep -m1 '^gate:' "$SELFTEST_DIR/verdict.md")"
  assert "set_status leaves every other block alone" "status: done" \
    "$(awk '/^## \[T-011\]/{f=1} f&&/^status:/{print; exit}' "$SELFTEST_DIR/verdict.md")"
  assert "set_status rewrites exactly one status line" "1" \
    "$(grep -c '^status: ready' "$SELFTEST_DIR/verdict.md")"

  # in_scope is what gate_scope decides on, so it is asserted rather than trusted.
  in_scope "src/a.ts" "src/*.ts" "docs/*"; assert "a file the scope names is in scope" "0" "$?"
  in_scope "docs/x.md" "src/*.ts"; assert "a file no glob names is out of scope" "1" "$?"
  in_scope "src/deep/a.ts" "src/**"; assert "a glob covers what is nested under it" "0" "$?"
  in_scope "src/a.ts"; assert "an empty scope line puts every file out of scope" "1" "$?"
  return "$SELFTEST_FAIL"
}


# `no-clarification-left`: a marker anywhere in the contract stops the run before it starts.
# CLAUDE.md names this script as the enforcement; until the harness was packaged, nothing here
# implemented it and the rail was a wish.

# Rewrite one block's status and record why, in place. The line-count floor is unblock()'s guard:
# awk can exit 0 having written nothing, and mv would then delete the queue.
set_status() { # $1 = id, $2 = new status, $3 = optional reason. TASKS_FILE for the selftest.
  local f="${TASKS_FILE:-TASKS.md}" before
  before=$(wc -l < "$f")
  awk -v t="$1" -v s="$2" -v r="${3:-}" '
    /^## \[T-[0-9]+\]/ { id = $2; gsub(/[][]/, "", id) }
    id == t && /^status:/ && !done_one { print "status: " s; if (r != "") print "gate: " r; done_one = 1; next }
    { print }
  ' "$f" > "$f.status" || { rm -f "$f.status"; return 1; }
  [ "$(wc -l < "$f.status")" -ge "$before" ] \
    || { echo "set_status: output shrank, refusing to replace $f" >&2; rm -f "$f.status"; return 1; }
  mv "$f.status" "$f"
}

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
  if out=$(__HARNESS_DIR__/hooks/check-gate.sh 2>&1); then
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
      __HARNESS_DIR__/*|*/hooks/*|.check-baseline|test-hashes.json|harness.json) harness_hit="$harness_hit $f" ;;
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

# After the definitions: the selftest asserts set_status, and needs no contract to do it.
[ "${1:-}" = "--selftest" ] && { selftest; exit $?; }

# A backticked mention is prose about the marker -- the rail's own definition is one -- and a bare
# one is a real marker. Never use a harness control token as an English word (LEARNINGS.md).
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
LANDED=""; ROWS=""; PROMOTED=""; KILLED=""; HALTS=""; WARNINGS=""; HALTED=""; DRY_ROUNDS=0
halt() { HALTED=1; HALTS="${HALTS}  $1"$'\n'; echo "HALT: $1"; }
listing() { echo "$1"; [ -n "$2" ] && printf '%s' "$2" || echo "  none"; }

# `touch STOP` has to land between stages too: a round is now up to four agents long.
stop_now() { [ -f STOP ] || return 1; echo "STOP file found, exiting."; return 0; }

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
  i=$((i+1))
  stop_now && break
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
    [ -n "$TASK" ] \
      && echo "  stages: implement $TASK -> verify $TASK -> commit the verdict -> assert PROGRESS.md grew" \
      || echo "  stages: scout -> adjudicate -> commit the round -> re-check ready_unattended ($DRY_ROUNDS dry rounds so far, 2 ends the run)"
    echo "  probes, which are the scout's whole input:"
    __HARNESS_DIR__/hooks/probes.sh || true
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
        [ "$(field "$t" attended)" = "true" ] \
          && { halt "$t is attended: true and it is all that is left. A human has to run it."; break; }
      done
      [ -n "$HALTED" ] && break
    fi

    stop_now && break
    echo "=== Iteration $i: scout (queue empty, $DRY_ROUNDS dry rounds so far) ==="
    READY_BEFORE=$(ids_at ready); REJ_BEFORE=$(rejections)
    agent_for scout
    run_agent "scout" "$LANE Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __HARNESS_DIR__/roles/scout.md: read that file first and follow it exactly. Run __HARNESS_DIR__/hooks/probes.sh and append to TASKS.md one 'status: proposed' block per FINDING line, each carrying probe:, command:, output: and rows:. Zero FINDING lines is zero blocks and that is a success, not something to escalate. Never promote, never fix, never edit any file a finding names. Then stop." 30 \
      || { echo "scout exited $? -- halting."; break; }

    stop_now && break
    echo "=== Iteration $i: adjudicate ==="
    ADJ_OUT=$(mktemp)
    agent_for adjudicator
    run_agent "adjudicate" "$LANE Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __HARNESS_DIR__/roles/adjudicator.md: read that file first and follow it exactly. Act on every block with 'status: proposed' in TASKS.md, in file order. Promote it to 'status: ready' with a scope and criteria an agent that has read only CLAUDE.md, __SPEC__, LEARNINGS.md and the block can run, or kill it and append one line to '## Rejected findings' in DECISIONS.md. A finding whose fix needs a change to __SPEC__ or CLAUDE.md is neither: leave it at proposed and print a line beginning HALT that names the block's id. Do not commit; this loop commits your round. Then stop." 40 2>&1 | tee "$ADJ_OUT"
    [ "${PIPESTATUS[0]}" -eq 0 ] || { echo "adjudicate exited non-zero -- halting."; rm -f "$ADJ_OUT"; break; }

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
      block "$t" | grep -qE '__SPEC__|__CONTEXT_FILE__' \
        && { halt "$t is still proposed and its fix names __SPEC__ or CLAUDE.md. Neither is a lane's to edit."; break; }
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
      [ "$DRY_ROUNDS" -ge 2 ] && { echo "Two consecutive dry rounds. Nothing left to do."; break; }
    fi
    continue
  fi

  PROG_BEFORE=$(cat PROGRESS.md 2>/dev/null | wc -c)
  # the sha the iteration starts at: everything gate_scope judges is committed after this point
  ITER_BASE=$(git rev-parse HEAD 2>/dev/null || true)

  stop_now && break
  echo "=== Iteration $i: implement $TASK ==="
  agent_for implementer
  run_agent "$TASK implement" "$LANE Read __CONTEXT_FILE__, __SPEC__, LEARNINGS.md, TASKS.md, git log --oneline -20, and the TAIL of PROGRESS.md (tail -200 PROGRESS.md -- it is append-only and newest-last, so reading it from the top gives you the oldest entries and none of the handoff). The tail and the log are what the one-row rail has you re-read at the start of an iteration. Your role is defined in __HARNESS_DIR__/roles/implementer.md: read that file first and follow it exactly. Complete exactly ONE task: the first with status 'ready' whose blockers are done and which is NOT marked 'attended: true'. If that task's scope files already carry uncommitted work, a prior lane was terminated mid-flight: finish it, never restart it and never discard it. Follow the task protocol strictly. Before you stop you MUST git add the paths named on the task's scope: line (never git add -A, LEARNINGS.md 2026-08-26), commit them, paste the exact commands and their output into the task's notes:, and set status: review. You MUST also append this iteration's PROGRESS.md entry in the format written at the top of that file -- what happened, which rows moved, and any BLOCKED with its written reason -- and include it in that commit. An implementation left uncommitted is a lost iteration." 60 \
    || { echo "implement exited $? -- halting rather than reporting a finished iteration."; break; }

  stop_now && break
  echo "=== Iteration $i: verify $TASK ==="
  agent_for verifier
  run_agent "$TASK verify" "$LANE Read __CONTEXT_FILE__, __SPEC__ and TASKS.md. Your role is defined in __HARNESS_DIR__/roles/verifier.md: read that file first and follow it exactly. Verify every task with status 'review'. Promote to done or reject to ready with concrete reasons, and commit the verdict. If nothing is at review, say so in one line and stop; that is a valid outcome, not something to escalate. Then stop." 40 \
    || { echo "verify exited $? -- halting."; break; }

  # The verifier writes its verdict into the working tree and stops. Nothing here
  # committed it, so a rejection could sit uncommitted until a human noticed.
  if [ -z "${DRY_RUN:-}" ]; then
    git add TASKS.md 2>/dev/null
    git diff --cached --quiet 2>/dev/null || git commit -q -m "verify: $TASK verdict"

    gate_verdict "$TASK" || true
    gate_scope "$TASK" "$ITER_BASE" || true

    case "$(field "$TASK" status)" in
      done) LANDED="$LANDED $TASK"; R=$(field "$TASK" rows); [ -n "$R" ] && ROWS="${ROWS}  $TASK: $R"$'\n' ;;
      *) WARNINGS="${WARNINGS}  $TASK ended the iteration at $(field "$TASK" status), not done."$'\n' ;;
    esac
    # The entry is all the next iteration inherits, so a silent iteration is itself the finding.
    [ "$(cat PROGRESS.md 2>/dev/null | wc -c)" -gt "$PROG_BEFORE" ] \
      || WARNINGS="${WARNINGS}  iteration $i wrote no PROGRESS.md entry for $TASK."$'\n'


    needs_spec_halt && break
  fi

done

echo
echo "=== digest: $i iteration(s) ==="
echo "tasks landed:${LANDED:- none}"
listing "rows turned green:" "$ROWS"
echo "findings promoted:${PROMOTED:- none}"
listing "findings killed:" "$KILLED"
listing "halts:" "$HALTS"
listing "warnings:" "$WARNINGS"
echo "Loop finished after $i iteration(s)."
