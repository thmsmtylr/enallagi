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
#   EVAL_AGENT='./stub.sh {prompt}' ./evals/run.sh   a stub, for testing the runner itself
#
# The agent comes from harness.json's agentCommand, or from EVAL_AGENT. With neither, this refuses:
# an eval suite that reports a pass without spawning anything is worse than no eval suite.
set -u
PKG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

AGENT=()
if [ -n "${EVAL_AGENT:-}" ]; then
  # ponytail: space-split, so an argument with a space in it needs harness.json instead
  read -r -a AGENT <<< "$EVAL_AGENT"
elif [ -f "$PKG/harness.json" ]; then
  while IFS= read -r word; do AGENT+=("$word"); done < <(
    python3 -c "import json,sys;[print(w) for w in json.load(open(sys.argv[1]))['agentCommand']]" "$PKG/harness.json")
fi
[ "${#AGENT[@]}" -gt 0 ] || {
  echo "evals: no agent configured. Set EVAL_AGENT, or put an agentCommand in harness.json." >&2
  echo "evals: refusing to report a result for something that was never run." >&2
  exit 2; }

NAMES=("$@")
[ "$#" -eq 0 ] && while IFS= read -r d; do NAMES+=("$(basename "$d")"); done < <(find "$PKG/evals" -mindepth 1 -maxdepth 1 -type d | sort)

failed=0
for name in ${NAMES[@]+"${NAMES[@]}"}; do
  [ -f "$PKG/evals/$name/assert.sh" ] || { echo "EVAL $name ERROR (no such eval)"; failed=1; continue; }
  dir=$(mktemp -d) || exit 3
  (
    cd "$dir" || exit 3
    git init -q && git config user.email eval@local && git config user.name eval
    mkdir -p src && echo 'export const x = 1' > src/schema.ts
    git add -A && git commit -qm init >/dev/null
    "$PKG/install.sh" "$dir" >/dev/null 2>&1 || exit 3
    EVAL_PKG="$PKG" bash "$PKG/evals/$name/setup.sh" || exit 3
    git add -A && git commit -qm fixture >/dev/null

    prompt=$(cat "$PKG/evals/$name/prompt.txt")
    cmd=()
    for word in "${AGENT[@]}"; do
      word="${word//\{prompt\}/$prompt}"; word="${word//\{turns\}/40}"
      cmd+=("$word")
    done
    "${cmd[@]}" >/dev/null 2>&1

    bash "$PKG/evals/$name/assert.sh"
  )
  case "$?" in
    0) echo "EVAL $name PASS" ;;
    3) echo "EVAL $name ERROR (the fixture could not be built — nothing was measured)"; failed=1 ;;
    *) echo "EVAL $name FAIL"; failed=1 ;;
  esac
  rm -rf "$dir"
done
exit "$failed"
