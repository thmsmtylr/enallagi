#!/usr/bin/env bash
# The package's own floor. Installs into a throwaway repo and asserts the harness works there.
# Two of the bugs this caught were template edits that broke the probes' row parser, which is
# exactly the kind of thing nobody re-checks by hand.
#
#   ./selftest.sh          run it
#   KEEP=1 ./selftest.sh   leave the scratch repo behind to poke at
set -u
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
T="${TMPDIR:-/tmp}/harness-selftest-$$"
FAIL=0

SKIPPED=0
ok() { echo "ok    $1"; }
bad() { echo "FAIL  $1"; [ $# -gt 1 ] && echo "      $2"; FAIL=1; }
is() { [ "$2" = "$3" ] && ok "$1" || bad "$1" "want [$2] got [$3]"; }
# A gate asserts what it EXECUTED. An assertion that did not run prints `skip`, never `ok`, and the
# summary says how many — otherwise an exit-criteria row reads green on a run that drove nothing
# (LEARNINGS.md, zero-as-pass; TASKS.md T-001, rejected 2026-09-02 for exactly this).
skip() { echo "skip  $1"; [ $# -gt 1 ] && echo "      $2"; SKIPPED=$((SKIPPED + 1)); return 0; }

mkdir -p "$T/src" && cd "$T" || exit 2
git init -q && echo 'export const x = 1' > src/schema.ts
git add -A && git -c user.email=t@t -c user.name=t commit -qm init

# --- install ----------------------------------------------------------------
SKILLS=".claude/skills"
OUT=$("$SRC/install.sh" "$T" 2>&1); RC=$?
is "install exits 0 on a fresh repo" "0" "$RC"
[ "$RC" -eq 0 ] || { printf '%s\n' "$OUT" | sed 's/^/      /'; exit 1; }
echo "$OUT" | grep -q 'every token substituted' && ok "no __TOKEN__ survives substitution" \
  || bad "no __TOKEN__ survives substitution"

python3 - <<'PY'
import json
c = json.load(open('harness.json'))
c.update({'check': './src/fakecheck.sh', 'checkForce': './src/fakecheck.sh',
          'agentCommand': ['./src/fakeagent.sh', '{prompt}', '{turns}']})
json.dump(c, open('harness.json', 'w'), indent=2)
PY
printf '#!/usr/bin/env bash\necho "fake agent ran"\n' > src/fakeagent.sh && chmod +x src/fakeagent.sh
printf '#!/usr/bin/env bash\nexit 0\n' > src/fakecheck.sh && chmod +x src/fakecheck.sh
"$SRC/install.sh" "$T" >/dev/null 2>&1
is "re-install is idempotent" "0" "$?"
git add -A && git -c user.email=t@t -c user.name=t commit -qm harness

# --- the launcher -----------------------------------------------------------
.harness/loop.sh --selftest > /tmp/hs-loop.$$ 2>&1
is "loop.sh --selftest passes" "0" "$?"
is "loop.sh --selftest asserts something" "0" "$(grep -c FAIL /tmp/hs-loop.$$)"
rm -f /tmp/hs-loop.$$

DRY_RUN=1 .harness/loop.sh 1 2>&1 | grep -q 'DRY_RUN would spawn' \
  && ok "a dry iteration plans implement and verify" || bad "a dry iteration plans implement and verify"
DRY_RUN=1 .harness/loop.sh 1 2>&1 | grep -q 'via ./src/fakeagent.sh' \
  && ok "the launcher spawns the configured agent, not a hardcoded one" \
  || bad "the launcher spawns the configured agent, not a hardcoded one"
DRY_RUN=1 .harness/loop.sh 1 2>&1 | grep -q '.harness/roles/implementer.md' \
  && ok "the implement stage points the agent at its role file" \
  || bad "the implement stage points the agent at its role file"

printf '\n[NEEDS CLARIFICATION] which store?\n' >> SPEC.md
.harness/loop.sh 1 2>&1 | grep -q 'HALT: SPEC.md carries' \
  && ok "a bare clarification marker halts the loop" || bad "a bare clarification marker halts the loop"
git checkout -- SPEC.md
.harness/loop.sh --selftest >/dev/null 2>&1
is "a backticked marker in the template does not halt it" "0" "$?"

# --- the probes -------------------------------------------------------------
# portability: the core install must name no vendor and no vendor directory
# prose may name a vendor among others; an executable path or process guard may not
VENDOR=$(grep -rnE '\.claude/(hooks|agents)|pgrep -f .claude|spin [^|]*\bclaude\b' .harness 2>/dev/null || true)
is "no installed script hardcodes a vendor path or process" "" "$VENDOR"
[ -s AGENTS.md ] && ok "AGENTS.md is the core context file" || bad "AGENTS.md is the core context file"
grep -q 'AGENTS.md' CLAUDE.md && ok "CLAUDE.md is a pointer at it" || bad "CLAUDE.md is a pointer at it"
grep -q 'AGENTS.md' GEMINI.md && ok "GEMINI.md is a pointer at it" || bad "GEMINI.md is a pointer at it"
grep -q 'AGENTS.md' .github/copilot-instructions.md \
  && ok "copilot-instructions.md is a pointer at it" || bad "copilot-instructions.md is a pointer at it"
is "the context file stays short" "under" \
  "$([ "$(wc -l < AGENTS.md)" -lt 80 ] && echo under || wc -l < AGENTS.md)"
grep -q '^name: running-the-loop' "$SKILLS/running-the-loop/SKILL.md" \
  && ok "the project skill is valid agentskills.io frontmatter" \
  || bad "the project skill is valid agentskills.io frontmatter"

P=$(.harness/hooks/probes.sh 2>&1); is "probes.sh exits 0 (every probe ran)" "0" "$?"
is "no probe errored" "0" "$(printf '%s\n' "$P" | grep -c 'PROBE .* ERROR')"
is "the row parser reads the seeded criteria table" "1" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE spec-untested //p')"
is "a fresh install has two unenforced rails, both wanting test-hashes.json" "2" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE rail-unenforced //p')"
is "the seeded criterion is untested and no task in flight names it" "1" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE queue-uncovered //p')"
is "harness-immutable names loop.sh and no test-hashes.json covers it" "1" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE hash-uncovered //p')"
is "the seeded PROGRESS.md repeats no friction" "0" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE friction-repeat //p')"
printf '%s\n' "$P" | grep -q '^PROBE driver OFF' \
  && ok "with no driverCommand the driver says so rather than scoring zero" \
  || bad "with no driverCommand the driver says so rather than scoring zero"
is "the seeded queue is clean" "0" "$(printf '%s\n' "$P" | sed -n 's/^PROBE queue-hygiene //p')"
is "every seeded learning names a file, command or hook" "0" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE learning-unenforced //p')"
is "the tree has no litter" "0" "$(printf '%s\n' "$P" | sed -n 's/^PROBE litter //p')"

# --- the PROGRESS.md rollover -----------------------------------------------
for n in 1 2 3 4 5 6; do printf '\n## fixture entry %s\nfriction: none\nnext: nothing\n' "$n" >> PROGRESS.md; done
git add -A && git commit -qm progress >/dev/null 2>&1
PROGRESS_MAX=12 PROGRESS_KEEP=4 .harness/archive-done.sh >/dev/null 2>&1
is "the oldest entry moves out of PROGRESS.md" "1" "$(grep -c 'fixture entry 1' PROGRESS.archive.md 2>/dev/null || echo 0)"
is "and is gone from the file the loop reads" "0" "$(grep -c 'fixture entry 1' PROGRESS.md)"
is "the newest entry stays" "1" "$(grep -c 'fixture entry 6' PROGRESS.md)"
is "the entry format the next iteration needs stays" "1" "$(grep -c '^## Entry format' PROGRESS.md)"
# the header keeps the entry-format fence, so the check is on what follows the archive note
is "the split lands on an entry heading, never inside one" "## fixture entry 6" \
  "$(awk '/^<!-- Entries before this point/{f=1; next} f && NF {print; exit}' PROGRESS.md)"
git add -A && git commit -qm rolled >/dev/null 2>&1

# --- the driver, the only probe that does not read text ---------------------
printf '#!/usr/bin/env bash\necho "FINDING the artifact answered but wrote nothing to the store"\n' > src/fakedriver.sh
chmod +x src/fakedriver.sh
python3 -c "
import json; c = json.load(open('harness.json'))
c['driverCommand'] = '\$HARNESS_ROOT/src/fakedriver.sh'
json.dump(c, open('harness.json', 'w'), indent=2)"
"$SRC/install.sh" "$T" >/dev/null 2>&1
D=$(HARNESS_DRIVER=1 .harness/hooks/probes.sh 2>&1); DRC=$?
is "a configured driver runs and exits 0" "0" "$DRC"
is "the driver's shortfall is one FINDING" "1" "$(printf '%s\n' "$D" | sed -n 's/^PROBE driver //p')"
printf '%s\n' "$D" | grep -q '^FINDING driver .*wrote nothing to the store' \
  && ok "the driver's FINDING line reaches the scout verbatim" \
  || bad "the driver's FINDING line reaches the scout verbatim"
# `env -u`, so this assertion still means what it says under `HARNESS_DRIVER=1 ./selftest.sh`
D=$(env -u HARNESS_DRIVER .harness/hooks/probes.sh 2>&1)
printf '%s\n' "$D" | grep -q '^PROBE driver OFF' \
  && ok "configured but HARNESS_DRIVER unset is still off" || bad "configured but HARNESS_DRIVER unset is still off"
# a driver that cannot reach the artifact records no score and the scout proposes nothing from it
printf '#!/usr/bin/env bash\necho "connection refused" >&2\nexit 7\n' > src/fakedriver.sh
D=$(HARNESS_DRIVER=1 .harness/hooks/probes.sh 2>&1); DRC=$?
is "a driver that cannot reach the artifact fails the whole probe run" "1" "$DRC"
printf '%s\n' "$D" | grep -q '^PROBE driver ERROR' \
  && ok "an unreachable artifact is ERROR, never a count of zero" || bad "an unreachable artifact is ERROR, never a count of zero"
rm -f src/fakedriver.sh

# the example driver ships with every install: driverCommand needs somewhere to point, and a
# skeleton that finds nothing is the honest starting state
is "an example driver installs into the harness directory" "0" \
  "$([ -x .harness/driver.example.sh ] && .harness/driver.example.sh >/dev/null 2>&1; echo $?)"

# --- this package's own driver, against the installed artifact ---------------
# HARNESS_DRIVER gates it out of the ordinary run: it installs four throwaway repos and drives four
# loop iterations, ~45s. `./selftest.sh` stays fast; `HARNESS_DRIVER=1 ./selftest.sh` asserts the
# worked example still reaches the artifact and still reports what it found.
if [ -n "${HARNESS_DRIVER:-}" ]; then
  DOUT=$("$SRC/driver.sh" 2>&1); DRC=$?
  # What is asserted is the INSTRUMENT: it reached the artifact (rc 0) and everything it said was a
  # finding — never a verdict, a pass or a fail. Deliberately NOT "it found at least one thing":
  # that would assert the harness stays broken, and the first hole it found (a lane that never
  # commits still reaching done) was closed by `gate_verdict` the same day, at which point the
  # probe correctly went quiet.
  DN=$(printf '%s\n' "$DOUT" | grep -v '^FINDING ' | grep -c . || true)
  is "the package driver reports shortfalls as FINDING lines" "0 0" "$DRC $DN"
else
  skip "the package driver reports shortfalls as FINDING lines" \
       "HARNESS_DRIVER is unset, so nothing drove the artifact. Not a pass."
fi

# --- the gate, on delta -----------------------------------------------------
gate() { .harness/hooks/check-gate.sh >/dev/null 2>&1; echo $?; }
printf '#!/usr/bin/env bash\necho "(fail) a thing that broke"\nexit 1\n' > src/fakecheck.sh
is "red and not on the baseline is a rejection" "2" "$(gate)"
echo "a thing that broke" >> .check-baseline
is "red and recorded on the baseline is forgiven" "0" "$(gate)"
printf '#!/usr/bin/env bash\necho "(fail) a thing that broke"\necho "(fail) something new"\nexit 1\n' > src/fakecheck.sh
is "one forgiven and one new failure is still a rejection" "2" "$(gate)"
printf '#!/usr/bin/env bash\necho boom >&2\nexit 1\n' > src/fakecheck.sh
is "red it cannot name forgives nothing, it fails closed" "2" "$(gate)"
printf '#!/usr/bin/env bash\nexit 0\n' > src/fakecheck.sh
is "green is green" "0" "$(gate)"

# --- the scope gate, end to end --------------------------------------------
# in_scope is asserted in loop.sh --selftest; this is the gate around it, which rewrites TASKS.md.
git config user.email t@t && git config user.name t
LANE_LOG="${TMPDIR:-/tmp}/harness-lane-$$.log"
cat > src/fakelane.sh <<'LANE'
#!/usr/bin/env bash
case "$1" in
  *"roles/implementer.md"*)
    date +%s%N > src/allowed.ts
    [ -n "${SNEAK:-}" ] && date +%s%N > "$SNEAK"
    sed -i.bak 's/^status: ready/status: review/' TASKS.md && rm -f TASKS.md.bak
    printf '\n## fixture — T-101 — landed\nfriction: none\n' >> PROGRESS.md
    # `if`, never `[ x ] && y` as a branch's last statement: a false test is the script's exit
    # status and the launcher reads a non-zero lane as a halt (PROGRESS.md, T-001)
    if [ -z "${NO_COMMIT:-}" ]; then git add -A && git commit -qm "feat: T-101"; fi ;;
  *"roles/verifier.md"*)
    sed -i.bak 's/^status: review/status: done/' TASKS.md && rm -f TASKS.md.bak ;;
esac
LANE
chmod +x src/fakelane.sh
python3 -c "
import json; c = json.load(open('harness.json'))
c['agentCommand'] = ['./src/fakelane.sh', '{prompt}', '{turns}']
json.dump(c, open('harness.json', 'w'), indent=2)"
"$SRC/install.sh" "$T" >/dev/null 2>&1
lane() { # $1 = what the lane touches, $2 = the task's rows, $3 = extra scope globs
  python3 - "$2" "${3:-}" <<'FIXTURE'
import re, sys
# the lane's sed is blanket, so T-101 has to be the only block at `ready` or another block's
# status moves and the assertion reads a flip the gate never made
s = re.sub(r'(?ms)^## \[T-101\].*?(?=\n## |\Z)', '', open('TASKS.md').read()).rstrip()
s = s.replace('status: ready', 'status: blocked')
open('TASKS.md', 'w').write(s + """

## [T-101] the fixture lane
scope: src/allowed.ts, src/fakelane.sh%s
blockedBy: none
status: ready
rows: %s
criteria:
  - it exists
notes:
""" % ((', ' + sys.argv[2] if sys.argv[2] else ''), sys.argv[1]))
FIXTURE
  git add -A >/dev/null && git commit -qm fixture >/dev/null
  # outside the repo: the fixture lane runs `git add -A`, so a log inside it becomes part of the
  # diff the scope gate is judging
  SNEAK="$1" NO_COMMIT="${NO_COMMIT:-}" .harness/loop.sh 1 > "$LANE_LOG" 2>&1
  git add -A >/dev/null 2>&1 && git commit -qm "whatever the lane left" >/dev/null 2>&1
  awk '/^## \[T-101\]/{f=1} f&&/^status:/{print $2; exit}' TASKS.md
}
is "a lane inside its scope keeps its done verdict" "done" "$(lane '' 'none — harness')"
is "a lane that never committed is forced back to ready" "ready" "$(NO_COMMIT=1 lane '' 'none — harness')"
is "a lane that leaves its scope is forced back to ready" "ready" "$(lane src/sneaky.ts 'none — harness')"
# the message is the line a human acts on, so it is asserted, not only the status it produced
is "and the rejection names the file it is rejecting" "1" \
  "$(grep -c 'SCOPE FAILED -- touched src/sneaky.ts' "$LANE_LOG")"
rm -f src/sneaky.ts
is "a harness edit a harness task declared is allowed" "done" \
  "$(lane .check-baseline 'none — harness' .check-baseline)"
is "the same edit under a product task is forced back to ready" "ready" \
  "$(lane .check-baseline '`src/thing.test.ts::a name copied from your suite`' .check-baseline)"
rm -f src/fakelane.sh "$LANE_LOG"

# --- worktree isolation -----------------------------------------------------
# Fresh sessions isolate context; only a worktree isolates the checkout. The lane records what the
# PARENT looked like while it was working, from inside the worktree, and that recording is what the
# first assertion reads — "untouched afterwards" would prove nothing.
cat > src/wtlane.sh <<'WTLANE'
#!/usr/bin/env bash
P=$(dirname "$(git rev-parse --git-common-dir)")
case "$1" in
  *"roles/implementer.md"*)
    printf '%s %s\n' "$(git -C "$P" rev-parse HEAD)" "$(git -C "$P" status --porcelain | grep -c . || true)" > wt-observed.txt
    date +%s%N > src/allowed.ts
    [ -n "${SECOND_WRITER:-}" ] && git -C "$P" commit -q --allow-empty -m "a second writer moved the parent"
    sed -i.bak 's/^status: ready/status: review/' TASKS.md && rm -f TASKS.md.bak
    printf '\n## fixture — T-201 — landed\nfriction: none\n' >> PROGRESS.md
    git add -A && git commit -qm "feat: T-201" ;;
  *"roles/verifier.md"*)
    sed -i.bak 's/^status: review/status: done/' TASKS.md && rm -f TASKS.md.bak ;;
esac
WTLANE
chmod +x src/wtlane.sh
python3 -c "
import json; c = json.load(open('harness.json'))
c['agentCommand'] = ['./src/wtlane.sh', '{prompt}', '{turns}']
json.dump(c, open('harness.json', 'w'), indent=2)"
"$SRC/install.sh" "$T" >/dev/null 2>&1
wt_task() {
  python3 - <<'FIXTURE'
import re
s = re.sub(r'(?ms)^## \[T-201\].*?(?=\n## |\Z)', '', open('TASKS.md').read()).rstrip()
s = s.replace('status: ready', 'status: blocked')
open('TASKS.md', 'w').write(s + """

## [T-201] the worktree fixture lane
scope: src/allowed.ts, wt-observed.txt
blockedBy: none
status: ready
rows: none — harness
criteria:
  - it exists
notes:
""")
FIXTURE
  git add -A >/dev/null && git commit -qm wt-fixture >/dev/null
}

wt_task
PRE=$(git rev-parse HEAD)
.harness/worktree.sh 1 >/dev/null 2>&1
is "a worktree lane leaves the parent checkout untouched" "$PRE 0" "$(cat wt-observed.txt 2>/dev/null)"
is "and the parent branch fast-forwarded to the lane" "1" "$(git log --oneline "$PRE"..HEAD | grep -c 'feat: T-201')"
is "the worktree is removed once it has merged" "0" "$(git worktree list | grep -c lane/)"

# the second-writer case: the parent moves under the lane, so the merge is a decision, not a step
wt_task
PRE=$(git rev-parse HEAD)
SECOND_WRITER=1 .harness/worktree.sh 1 >/dev/null 2>&1
is "a lane that cannot fast forward is left for a human" "1" "$?"
is "and its work is still there, unmerged" "1" "$(git worktree list | grep -c lane/)"
is "and the parent took none of it" "0" "$(git log --oneline "$PRE"..HEAD | grep -c 'feat: T-201')"
LANE_DIR=$(git worktree list | awk '/lane\//{print $1}')
git worktree remove --force "$LANE_DIR" >/dev/null 2>&1
git branch -D "$(git branch --list 'lane/*' | tr -d ' *')" >/dev/null 2>&1
rm -f src/wtlane.sh

# --- the eval runner, against the role prompts -------------------------------
# The evals themselves need a real agent, which `./selftest.sh` must not spawn. What is asserted
# here is the RUNNER: that a role obeying its rule is a PASS, that one breaking it is a FAIL, and
# that with nothing configured to spawn it refuses instead of reporting either.
cat > src/evalobeys.sh <<'OBEYS'
#!/usr/bin/env bash
# the verifier's rule: uncommitted source means the work is not on the branch. Reject to ready.
sed -i.bak 's/^status: review/status: ready/' TASKS.md && rm -f TASKS.md.bak
OBEYS
cat > src/evalbreaks.sh <<'BREAKS'
#!/usr/bin/env bash
# the false VERIFIED: promote it anyway
sed -i.bak 's/^status: review/status: done/' TASKS.md && rm -f TASKS.md.bak
BREAKS
chmod +x src/evalobeys.sh src/evalbreaks.sh
is "the eval runner passes a role that obeys its rule" "EVAL verifier PASS" \
  "$(EVAL_AGENT="$T/src/evalobeys.sh {prompt}" "$SRC/evals/run.sh" verifier 2>/dev/null)"
is "the eval runner fails a role that breaks its rule" "EVAL verifier FAIL" \
  "$(EVAL_AGENT="$T/src/evalbreaks.sh {prompt}" "$SRC/evals/run.sh" verifier 2>/dev/null | tail -1)"
# with no agent anywhere, a refusal — never a pass for something that was never run
cp -R "$SRC" "$T/pkgcopy" 2>/dev/null; rm -f "$T/pkgcopy/harness.json" "$T/pkgcopy/harness.default.json"
is "with no agent configured the evals refuse rather than report" "2" \
  "$("$T/pkgcopy/evals/run.sh" verifier >/dev/null 2>&1; echo $?)"
rm -rf "$T/pkgcopy" src/evalobeys.sh src/evalbreaks.sh

# --- teardown ---------------------------------------------------------------
cd /
[ -n "${KEEP:-}" ] && echo "kept: $T" || rm -rf "$T"
echo
if [ "$FAIL" -ne 0 ]; then
  echo "harness selftest: FAILURES above."
elif [ "$SKIPPED" -gt 0 ]; then
  echo "harness selftest: all assertions that RAN passed, and $SKIPPED did not run. Set HARNESS_DRIVER=1 for the whole floor."
else
  echo "harness selftest: all assertions passed."
fi
exit "$FAIL"
