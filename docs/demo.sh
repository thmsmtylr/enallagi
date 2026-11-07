#!/usr/bin/env bash
# One spec, decompose to dark merge, in a repository this script creates and deletes.
#
#   docs/demo.sh    no arguments, no credential, no network
#
# The coding agent is replaced by `src/lane.sh`, a fixture that does what the role prompt it is
# handed asks for. GitHub is replaced by the twin the binary serves (`enallagi twin gh`), with Kilo
# and Greptile scripted to review the pull request. Everything else is the real package: the real
# `enallagi init`, the real launcher, the real gates, the sandbox, the holdouts and the merge step.
#
# ENALLAGI_BIN names the binary. It defaults to this checkout's release build, which is built here
# when it is absent.
set -u
PKG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENALLAGI_BIN="${ENALLAGI_BIN:-$PKG/target/release/enallagi}"
if [ ! -x "$ENALLAGI_BIN" ]; then
  (cd "$PKG" && cargo build --release -q) || {
    echo "demo: no $ENALLAGI_BIN and cargo build --release failed" >&2
    exit 3
  }
fi
export ENALLAGI_BIN

D=$(mktemp -d "${TMPDIR:-/tmp}/enallagi-demo.XXXXXX") || exit 3
D=$(cd "$D" && pwd -P)
# the machine is left as it was found, on every exit path
trap 'rm -rf "$D"' EXIT INT TERM

step() { printf '\n== %s\n' "$1"; }

cat <<'INTRO'
enallagi demo: one spec goes from decompose to a dark merge, reviewed by Kilo and Greptile twins.
Nothing is installed on your machine, no credential is read and no network is used.
INTRO

R="$D/repo"
mkdir -p "$R" && cd "$R" || exit 3
git init -q -b main
git config user.email demo@local
git config user.name demo
git config commit.gpgsign false # a signing key on this machine is not the demo's business
mkdir -p src && echo 'export const x = 1' >src/schema.ts

# The stand-in for the coding agent, handed the role prompt a real agent would be. It does each
# role's protocol and nothing else; a returned ticket's implementer answers the bots' findings.
cat >src/lane.sh <<'LANE'
#!/usr/bin/env bash
ticket() { ls .enallagi/specs/*/issues/T-001-*.md; }
case "$1" in
  *"roles/decomposer.md"*)
    mkdir -p "$(ls -d .enallagi/specs/*/)issues"
    cat >"$(ls -d .enallagi/specs/*/)issues/01-offset.md" <<'DRAFT'
# 01: Keep the offset

**What to build:** `src/offset.ts` exports the offset.

**Blocked by:** None

**Status:** ready-for-agent

- [ ] src/offset.ts exports the offset
DRAFT
    ;;
  *"roles/adjudicator.md"*)
    # triage: the drafted ticket gets its scope and goes to the writers
    if grep -q '^\*\*Status:\*\* needs-triage' "$(ticket)" 2>/dev/null; then
      printf 'scope: src/offset.ts, src/offset.test.ts\nrows: none — harness\n' >>"$(ticket)"
      "$ENALLAGI_BIN" tasks set-status T-001 ready-for-agent 'the demo lane triaged it' >/dev/null
    fi
    ;;
  *"roles/implementer.md"*)
    if grep -q '^finding: F-1' "$(ticket)"; then
      echo 'test("offset_kept", () => {})' >src/offset.test.ts
      printf 'answer: F-1 accept offset_kept\nanswer: F-2 accept offset_kept\n' >>"$(ticket)"
    else
      echo 'export const offset = 0' >src/offset.ts
    fi
    "$ENALLAGI_BIN" tasks set-status T-001 review 'the demo lane implemented it' >/dev/null
    git add -A && git commit -qm 'feat: T-001 keep the offset' >/dev/null
    ;;
  *"roles/verifier.md"*)
    "$ENALLAGI_BIN" tasks set-status T-001 done 'the demo lane verified it' >/dev/null
    printf 'verdict: verified by verifier\nspec: none\nstandards: none\n' >>"$(ticket)"
    ;;
esac
echo '{"total_cost_usd": 0.01}'
LANE
chmod +x src/lane.sh

# The skills the role prompts name are declared with `path:` sources this script writes, so the real
# resolve, vendor and lock path runs with nothing to fetch.
SKILLS="to-tickets triage tdd diagnosing-bugs codebase-design writing-for-agents domain-modeling code-review ponytail ponytail-review caveman-commit receiving-code-review verification-before-completion"
for id in $SKILLS; do
  mkdir -p "vendor/$id"
  printf '# %s\n' "$id" >"vendor/$id/SKILL.md"
done

mkdir -p .enallagi
cat >.enallagi/enallagi.toml <<'TOML'
[agent]
preset = "custom"
command = ["./src/lane.sh", "{prompt}", "{turns}"]

[agent.usage]
cost = "total_cost_usd"

[check]
command = "true"

[pr]
merge = "auto"

[respond]
quiet = "0s"
TOML
for id in $SKILLS; do
  printf '\n[[skill]]\nid = "%s"\nsource = "path:vendor/%s"\npath = ""\ngate = "none"\nwhy = "the demo fixture"\n' \
    "$id" "$id" >>.enallagi/enallagi.toml
done
# .enallagi stays untracked, so init makes it its own repository: the traceless install
git add src vendor && git commit -qm 'chore: init' >/dev/null

step "install into a throwaway repo  ($D)"
INSTALL=$("$ENALLAGI_BIN" init 2>&1) || {
  printf '%s\n' "$INSTALL" >&2
  echo "demo: install failed" >&2
  exit 3
}
printf 'installed %s files\n' "$(printf '%s\n' "$INSTALL" | grep -c '^  wrote: ')"
"$ENALLAGI_BIN" skills sync >/dev/null || exit 3
git add -A && git commit -qm 'chore: vendor the declared skills' >/dev/null

SPEC="$(ls -d .enallagi/specs/*/)"
SLUG=$(basename "$SPEC")
cat >"$SPEC/spec.md" <<'SPEC'
# Keep the offset

**Status:** ready-for-agent

The parser drops the timezone offset. Keep it.
SPEC
# the operator's holdout: the writer's sandbox cannot read it, and no repository tracks it
mkdir -p ".enallagi/holdouts/$SLUG"
printf '#!/bin/sh\ngrep -q offset src/offset.ts\n' >".enallagi/holdouts/$SLUG/offset.sh"
chmod +x ".enallagi/holdouts/$SLUG/offset.sh"
git -C .enallagi add -A && git -C .enallagi commit -qm 'an accepted spec' >/dev/null

step "a bare origin, and the GitHub twin on PATH with Kilo and Greptile scripted"
git clone -q --bare "$R" "$D/origin.git"
git remote add origin "$D/origin.git" && git fetch -q origin
CHECK='{"name": "test", "status": "COMPLETED", "conclusion": "SUCCESS"}'
cat >"$D/twin.json" <<JSON
{
  "repo": "demo/repo",
  "origin": "$D/origin.git",
  "default_branch": "main",
  "script": [
    {"head": "*", "round": 0, "checks": [$CHECK], "bots": [
      {"bot": "kilo", "path": "src/offset.ts", "line": 1, "text": "The offset is not covered by a test."},
      {"bot": "greptile", "path": "", "line": 0, "text": "The offset export has no test."}
    ]},
    {"head": "*", "round": 1, "checks": [$CHECK]}
  ]
}
JSON
mkdir -p "$D/bin"
printf '#!/bin/sh\nexec /usr/bin/env ENALLAGI_TWIN=%s %s twin gh "$@"\n' "$D/twin.json" "$ENALLAGI_BIN" >"$D/bin/gh"
chmod +x "$D/bin/gh"
export PATH="$D/bin:$PATH"
echo "origin $D/origin.git; gh is the twin"

step "the run: merge = \"auto\", BUDGET_USD=1"
# a stage, the holdout gate, the respond round's answered findings, and the merge step's line
BUDGET_USD=1 "$ENALLAGI_BIN" run --iterations 6 --no-tui 2>&1 | sed -nE \
  -e 's/.* stage\.start .* stage=([a-z]+) .*/  stage \1/p' \
  -e 's/.* gate=holdout .* reason="(.*)"/  holdout gate: \1/p' \
  -e 's/.* gate=findings .* reason="([0-9]+ finding.*)"/  respond round: \1/p' \
  -e 's/^  (T-[0-9]+: (merged|left open).*)/  \1/p' \
  -e 's/^(HALT: .*)/  \1/p'

step "what persisted, read back off the twin and the origin"
printf 'T-001  status: %s\n' "$("$ENALLAGI_BIN" tasks list | awk '/^T-001/{print $NF}')"
git -C "$D/origin.git" log --oneline --format='%s' -1 main | sed 's/^/origin main  /'
"$D/bin/gh" pr view 1 --json state | sed 's/.*"state":"\([A-Z]*\)".*/pull 1  \1/'
