#!/usr/bin/env bash
# Evals for the role prompts. A prompt is the only part of this harness nothing else can test:
# `selftest.sh` tests the launcher AROUND the prompts, and a change to `roles/verifier.md` is
# otherwise unverifiable except by watching a run.
#
# One eval per directory here. Each is three files, and each runs in a throwaway repo with the
# harness freshly installed — the same isolation the loop's own stages get:
#
#   setup.sh    builds the fixture state, with cwd = the fixture repo
#   prompt.txt  the request, in the shape loop.sh sends it
#   assert.sh   exits 0 when the role obeyed its rule, non-zero when it did not
#
#   ./evals/run.sh              every eval
#   ./evals/run.sh verifier     one
#   ./evals/run.sh --gate <n>   decide one candidate rule (see below)
#   EVAL_AGENT='./stub.sh {prompt}' ./evals/run.sh   a stub, for testing the runner itself
#
# --gate is the write-path check on a new rule. A repeated `friction:` line in PROGRESS.md becomes a
# rule in LEARNINGS.md, and this decides whether that rule is worth its place. Three conditions,
# all required:
#
#   1. the eval PASSES with the rule            the rule fixes the case it came from
#   2. the eval FAILS with the rule ablated     the case would not have passed anyway
#   3. every other eval still PASSES            the rule regresses nothing that worked
#
# 1 and 3 are GRASP's admission rule, (F(c)-F0)-(R(c)-R0)>0 with a hard regression budget R(c)<=R0
# (arXiv:2605.29668), and GSE's two stages, local then replay-driven (arXiv:2608.06153). 2 is not
# in either: both ask whether the case now passes, neither asks whether it would have passed
# without the rule. A rule that changes no outcome is context that costs and buys nothing.
#
# Ablation is per-eval: `ablate.sh` in the eval directory removes the rule from the fixture repo,
# and an eval without one cannot be gated.
#
# The agent comes from EVAL_AGENT, else harness.json's agentCommand, else harness.default.json's.
# With none of the three, this refuses:
# an eval suite that reports a pass without spawning anything is worse than no eval suite.
set -u
PKG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

AGENT=()
if [ -n "${EVAL_AGENT:-}" ]; then
  # ponytail: space-split, so an argument with a space in it needs harness.json instead
  read -r -a AGENT <<<"$EVAL_AGENT"
else
  # harness.json if this package sits in a configured repo, harness.default.json otherwise — the
  # package itself ships without a harness.json, and the evals still have to be runnable from it
  for config in "$PKG/harness.json" "$PKG/harness.default.json"; do
    [ -f "$config" ] || continue
    while IFS= read -r word; do AGENT+=("$word"); done < <(
      python3 -c "import json,sys;[print(w) for w in json.load(open(sys.argv[1]))['agentCommand']]" "$config"
    )
    break
  done
fi
[ "${#AGENT[@]}" -gt 0 ] || {
  echo "evals: no agent configured. Set EVAL_AGENT, or put an agentCommand in harness.json." >&2
  echo "evals: refusing to report a result for something that was never run." >&2
  exit 2
}

GATE=""
if [ "${1:-}" = "--gate" ]; then
  GATE="${2:-}"
  [ -n "$GATE" ] || {
    echo "evals: --gate needs an eval name" >&2
    exit 2
  }
  shift 2
fi

NAMES=("$@")
[ "$#" -eq 0 ] && while IFS= read -r d; do NAMES+=("$(basename "$d")"); done < <(find "$PKG/evals" -mindepth 1 -maxdepth 1 -type d | sort)
[ "${#NAMES[@]}" -gt 0 ] || {
  # a fresh install has this runner and no evals: exit 0 here read as "every eval passed"
  echo "evals: nothing under $PKG/evals to run. An eval is a directory with setup.sh, prompt.txt and assert.sh." >&2
  echo "evals: refusing to report a result for something that was never run." >&2
  exit 2
}

run_one() { # $1 = eval name, $2 = "ablate" to remove the rule from the fixture first
  local name="$1" mode="${2:-}" dir
  dir=$(mktemp -d) || return 3
  (
    cd "$dir" || exit 3
    git init -q && git config user.email eval@local && git config user.name eval
    mkdir -p src && echo 'export const x = 1' >src/schema.ts
    git add -A && git commit -qm init >/dev/null
    "$PKG/install.sh" "$dir" >/dev/null 2>&1 || exit 3
    # the ablation runs before setup and before the agent: the fixture is identical either way,
    # and the only difference is whether the rule is present when the agent reads its prompt
    if [ "$mode" = "ablate" ]; then
      [ -f "$PKG/evals/$name/ablate.sh" ] || exit 3
      EVAL_PKG="$PKG" bash "$PKG/evals/$name/ablate.sh" || exit 3
    fi
    # committed BEFORE setup.sh, so a setup that leaves work uncommitted on purpose -- the
    # verifier's, whose whole case is a terminated lane -- is what the agent sees. A commit
    # here after setup erased that case, and the eval passed having tested a tree it never built.
    git add -A && git commit -qm fixture >/dev/null
    EVAL_PKG="$PKG" bash "$PKG/evals/$name/setup.sh" || exit 3

    prompt=$(cat "$PKG/evals/$name/prompt.txt")
    cmd=()
    for word in "${AGENT[@]}"; do
      word="${word//\{prompt\}/$prompt}"
      word="${word//\{turns\}/40}"
      cmd+=("$word")
    done
    # an agent that did not run is not a role that disobeyed: 4, reported as ERROR, never FAIL
    "${cmd[@]}" >/dev/null 2>&1 || exit 4

    bash "$PKG/evals/$name/assert.sh"
  )
  local rc=$?
  rm -rf "$dir"
  return $rc
}

report() { # $1 = name, $2 = exit status. Returns 0 when the eval passed.
  case "$2" in
  0)
    echo "EVAL $1 PASS"
    return 0
    ;;
  3)
    echo "EVAL $1 ERROR (the fixture could not be built — nothing was measured)"
    return 2
    ;;
  4)
    echo "EVAL $1 ERROR (the agent exited non-zero — nothing was measured)"
    return 2
    ;;
  *)
    echo "EVAL $1 FAIL"
    return 1
    ;;
  esac
}

if [ -n "$GATE" ]; then
  [ -f "$PKG/evals/$GATE/ablate.sh" ] || {
    echo "GATE $GATE REJECT no ablate.sh: without one, nothing can tell a rule that works from a rule that is never consulted" >&2
    exit 2
  }

  run_one "$GATE"
  with=$?
  report "$GATE" "$with" >/dev/null
  [ "$with" -eq 3 ] || [ "$with" -eq 4 ] && {
    echo "GATE $GATE REJECT the fixture could not be built or the agent did not run ($with), so nothing was measured"
    exit 1
  }
  [ "$with" -eq 0 ] || {
    echo "GATE $GATE REJECT the rule does not fix the case it came from (its eval fails with the rule in place)"
    exit 1
  }

  run_one "$GATE" ablate
  without=$?
  [ "$without" -eq 3 ] || [ "$without" -eq 4 ] && {
    echo "GATE $GATE REJECT the ablated arm could not be built or its agent did not run ($without), so nothing was measured"
    exit 1
  }
  [ "$without" -eq 0 ] && {
    echo "GATE $GATE REJECT the case passes with the rule ablated, so the rule changed no outcome"
    exit 1
  }

  regressed=""
  while IFS= read -r d; do
    other="$(basename "$d")"
    [ "$other" = "$GATE" ] && continue
    run_one "$other"
    rc=$?
    [ "$rc" -eq 0 ] || regressed="$regressed $other"
  done < <(find "$PKG/evals" -mindepth 1 -maxdepth 1 -type d | sort)
  [ -n "$regressed" ] && {
    echo "GATE $GATE REJECT it regresses evals that were passing:$regressed"
    exit 1
  }

  echo "GATE $GATE ACCEPT fixes its case, fails without itself, regresses nothing"
  exit 0
fi

failed=0
for name in ${NAMES[@]+"${NAMES[@]}"}; do
  if [ ! -f "$PKG/evals/$name/assert.sh" ]; then
    echo "EVAL $name ERROR (no such eval)"
    failed=1
    continue
  fi
  run_one "$name"
  rc=$?
  report "$name" "$rc" || failed=1
done
exit "$failed"
