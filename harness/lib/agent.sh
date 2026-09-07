#!/usr/bin/env bash
# Spawning one agent process per stage: the command per role, the spinner, the rate-limit wait.
# Sourced by loop.sh. Tokens are substituted by install.sh.

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
  scout) AGENT_CMD=("${AGENT_CMD_SCOUT[@]}") ;;
  adjudicator) AGENT_CMD=("${AGENT_CMD_ADJUDICATOR[@]}") ;;
  implementer) AGENT_CMD=("${AGENT_CMD_IMPLEMENTER[@]}") ;;
  verifier) AGENT_CMD=("${AGENT_CMD_VERIFIER[@]}") ;;
  *) AGENT_CMD=("${AGENT_CMD_DEFAULT[@]}") ;;
  esac
  AGENT_ROLE="$1"
}

# shellcheck disable=SC1003 # not an escaped quote: inside single quotes a backslash is literal,
# and these are the four spinner frames | / - \ that spin() cycles through.
FRAMES='|/-\'
spin() {
  local label="$1"
  shift
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
      [ $((e - last)) -ge 60 ] && {
        printf '  ... %s %dm%02ds\n' "$label" $((e / 60)) $((e % 60))
        last=$e
      }
      sleep 5
    fi
  done
  wait "$pid"
  local rc=$?
  e=$((SECONDS - start))
  [ -n "$tty" ] && {
    printf '\r\033[2K  %s  %dm%02ds\n' "$label" $((e / 60)) $((e % 60)) >&3
    exec 3>&-
  }
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
    [ -f STOP ] && {
      echo "STOP during limit wait."
      return 1
    }
    printf '\r\033[2K  waiting out session limit: %dm left' $((left / 60))
    sleep $((left < 60 ? left : 60))
    left=$((left - 60))
  done
  printf '\r\033[2K'
  return 0
}

# One record per spawned stage. Wall clock is portable; cost is not, so it is read from whatever
# the agent printed using a pattern from harness.json, and left empty when nothing matched.
RUN_LOG="__HARNESS_DIR__/run.log"
SPENT_SECONDS=0
SPENT_USD=0
ROLE_SECONDS=""

log_stage() { # $1 = role, $2 = task, $3 = seconds, $4 = exit code, $5 = cost or empty
  # shellcheck disable=SC2154 # $i is the launcher's iteration counter (harness/loop.sh:33 sets it,
  # :106 increments it); this file is a module loop.sh sources, so shellcheck cannot see it.
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$i" "$1" "${2:--}" "$3" "$4" "${5:-}" >>"$RUN_LOG"
  SPENT_SECONDS=$((SPENT_SECONDS + $3))
  ROLE_SECONDS="${ROLE_SECONDS}$1 $3"$'\n'
  [ -n "${5:-}" ] && SPENT_USD=$(python3 -c "print(round($SPENT_USD + $5, 4))" 2>/dev/null || echo "$SPENT_USD")
  # A dollar budget over a cost nothing reports is no budget: the shipped `claude -p` prints text
  # unless --output-format json is on the command, so costSed matched nothing and BUDGET_USD
  # never halted anything. Recorded here, read by over_budget at the next stage boundary.
  [ -n "${BUDGET_USD:-}" ] && [ -z "${5:-}" ] && COST_MISSING=1
  return 0
}
COST_MISSING=""

# Checked before every stage, never during one: a half-finished stage is worse than a slow run.
# Unset or zero means no limit.
over_budget() {
  [ -n "$COST_MISSING" ] && {
    halt "BUDGET_USD is set and the last stage reported no cost, so the budget cannot be enforced. agentCommand must print a cost costSed can read (claude: add --output-format json), or unset BUDGET_USD."
    return 0
  }
  [ "${BUDGET_SECONDS:-0}" -gt 0 ] && [ "$SPENT_SECONDS" -ge "${BUDGET_SECONDS:-0}" ] && {
    halt "the run has spent ${SPENT_SECONDS}s of its ${BUDGET_SECONDS}s budget."
    return 0
  }
  [ -n "${BUDGET_USD:-}" ] && [ "$(python3 -c "print(1 if $SPENT_USD >= ${BUDGET_USD:-0} else 0)" 2>/dev/null || echo 0)" = "1" ] && {
    halt "the run has spent \$$SPENT_USD of its \$$BUDGET_USD budget."
    return 0
  }
  return 1
}

run_agent() {
  local label="$1" prompt="$2" turns="$3" out rc hit wait_s started cost tries=0
  # DRY_RUN reads the run without buying it: every stage announces itself here and nothing spawns.
  if [ -n "${DRY_RUN:-}" ]; then
    echo "  DRY_RUN would spawn: $label as role ${AGENT_ROLE:-default} via ${AGENT_CMD[0]} (turns: $turns)"
    printf '%s\n' "$prompt" | sed 's/^/    | /'
    return 0
  fi
  started=$SECONDS
  while :; do
    out=$(mktemp)
    local cmd=() word
    for word in "${AGENT_CMD[@]}"; do
      word="${word//\{prompt\}/$prompt}"
      word="${word//\{turns\}/$turns}"
      cmd+=("$word")
    done
    spin "$label" "${cmd[@]}" >"$out" 2>&1
    rc=$?
    cat "$out"
    hit=$(grep -m1 -i "__RATE_LIMIT_PATTERN__" "$out" || true)
    cost=$(sed -n '__COST_SED__' "$out" | tail -1)
    rm -f "$out"
    # A limit is a notice with a reset time in it. The words alone are not one: a lane working on
    # this file quotes them, and the launcher slept a day on its own comment. Two retries, then
    # halt -- an agent that prints the notice forever would otherwise hold the run forever.
    if [ -z "$hit" ] || ! wait_s=$(seconds_until_reset "$hit") || [ "$tries" -ge 2 ]; then
      [ -n "$hit" ] && [ "$tries" -ge 2 ] && echo "  session limit persisted after $tries retries -- giving up on $label."
      log_stage "${AGENT_ROLE:-default}" "${TASK:-}" $((SECONDS - started)) "$rc" "$cost"
      return $rc
    fi
    tries=$((tries + 1))
    echo "  session limit. sleeping $((wait_s / 60))m, then retrying $label ($tries of 2)."
    sleep_until "$wait_s" || return 1
  done
}
