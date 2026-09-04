#!/usr/bin/env bash
# The package's own floor. Installs into a throwaway repo and asserts the harness works there.
# Two of the bugs this caught were template edits that broke the probes' row parser, which is
# exactly the kind of thing nobody re-checks by hand.
#
#   ./selftest.sh          run it
#   KEEP=1 ./selftest.sh   leave the scratch repo behind to poke at
# shellcheck disable=SC2015 # the `<test> && ok "name" || bad "name"` assertion idiom below is safe
# here because ok() and skip() cannot fail: each ends in an echo or an assignment, so the `||`
# arm never runs after the `&&` arm did. ponytail: file-level, so a future `A && B || C` whose
# middle term CAN fail is not flagged in this file. The two that could -- the teardown's
# `rm -rf "$T"` and the AGENTS.md line count -- were rewritten as `if` instead of suppressed.
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

# one command per role: a verifier on a different model from the implementer
python3 -c "
import json; c = json.load(open('harness.json'))
c['agentCommand'] = {'default': ['./src/fakeagent.sh', '{prompt}', '{turns}'],
                     'verifier': ['./src/fakeverifier.sh', '{prompt}', '{turns}']}
json.dump(c, open('harness.json', 'w'), indent=2)"
printf '#!/usr/bin/env bash\necho "fake verifier ran"\n' > src/fakeverifier.sh && chmod +x src/fakeverifier.sh
"$SRC/install.sh" "$T" >/dev/null 2>&1
is "a role with its own agent command is spawned with it" "1" \
  "$(DRY_RUN=1 .harness/loop.sh 1 2>&1 | grep -c 'as role verifier via ./src/fakeverifier.sh')"
is "a role with no agent command falls back to the default" "1" \
  "$(DRY_RUN=1 .harness/loop.sh 1 2>&1 | grep -c 'as role implementer via ./src/fakeagent.sh')"
python3 -c "
import json; c = json.load(open('harness.json'))
c['agentCommand'] = ['./src/fakeagent.sh', '{prompt}', '{turns}']
json.dump(c, open('harness.json', 'w'), indent=2)"
rm -f src/fakeverifier.sh
"$SRC/install.sh" "$T" >/dev/null 2>&1
git add -A && git -c user.email=t@t -c user.name=t commit -qm roles

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
  "$(if [ "$(wc -l < AGENTS.md)" -lt 80 ]; then echo under; else wc -l < AGENTS.md; fi)"
grep -q '^name: running-the-loop' "$SKILLS/running-the-loop/SKILL.md" \
  && ok "the project skill is valid agentskills.io frontmatter" \
  || bad "the project skill is valid agentskills.io frontmatter"

# one parser, and the launcher holds none of its own
is "the launcher parses no task blocks itself" "0" \
  "$(grep -cE '## \\\[T-|awk .*TASKS|sed .*TASKS' .harness/loop.sh)"
is "tasks.py answers the queue against fixture files" "0" \
  "$(python3 .harness/tasks.py --selftest >/dev/null 2>&1; echo $?)"
# shellcheck disable=SC2016 # the backticks are a literal markdown fence in the fixture, not a
# command substitution -- this assertion checks that tasks.py ignores a heading inside a fence.
printf '# TASKS\n\n```\n## [T-900] the example in the docs\nblockedBy:\nstatus: ready\n```\n\n## [T-901] the real one\nblockedBy: none\nstatus: ready\n' > /tmp/fence.$$.md
is "a heading inside a code fence is not a task" "T-901" \
  "$(python3 .harness/tasks.py ready-unattended /tmp/fence.$$.md)"
rm -f /tmp/fence.$$.md
is "the launcher is split into sourced modules" "queue agent gates" \
  "$(for m in queue agent gates; do [ -f ".harness/lib/$m.sh" ] && printf '%s ' "$m"; done | sed 's/ $//')"

P=$(.harness/hooks/probes.sh 2>&1); is "probes.sh exits 0 (every probe ran)" "0" "$?"
is "no probe errored" "0" "$(printf '%s\n' "$P" | grep -c 'PROBE .* ERROR')"
is "the row parser reads the seeded criteria table" "1" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE spec-untested //p')"
[ -x evals/run.sh ] && ok "the write-path gate installs where the rules are written" \
  || bad "the write-path gate installs where the rules are written"
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
is "the seeded learnings are all [seed] entries, which predate the gate" "0" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE learning-ungated //p')"

# a rule written from a repeated friction, with nothing that decided it was worth its place
cp LEARNINGS.md "$T/learnings.bak"
# shellcheck disable=SC2016 # literal backticks: this is a LEARNINGS.md line being appended as
# text, and probes.sh greps it as text.
echo '- [2026-09-02] the check cache served a green nobody ran -> always run `./selftest.sh` uncached.' >> LEARNINGS.md
is "a dated learning with no eval is reported" "1" \
  "$(.harness/hooks/probes.sh 2>&1 | sed -n 's/^PROBE learning-ungated //p')"
# and the same rule, once an eval holds it. The eval lives in the repo being checked, which is why
# the fixture creates it here: the package's own evals are not installed into a target repo.
mkdir -p evals/cache-green
sed -i.bak 's|uncached\.|uncached (evals/cache-green).|' LEARNINGS.md && rm -f LEARNINGS.md.bak
is "and the same rule naming an eval that exists is not" "0" \
  "$(.harness/hooks/probes.sh 2>&1 | sed -n 's/^PROBE learning-ungated //p')"
# the library is bounded: every entry is read at the start of every task
for n in 1 2 3 4 5 6 7 8; do
  echo "- [2026-09-0$n] a rule that cost a run -> do the other thing (evals/cache-green)." >> LEARNINGS.md
done
is "a learnings file over its cap is reported" "1" \
  "$(.harness/hooks/probes.sh 2>&1 | grep -c 'against a cap of 12')"
cp "$T/learnings.bak" LEARNINGS.md && rm -f "$T/learnings.bak" && rm -rf evals

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
echo '{"total_cost_usd": 0.5}'
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
  SNEAK="$1" NO_COMMIT="${NO_COMMIT:-}" BUDGET_USD="${BUDGET_USD:-}" .harness/loop.sh 1 > "$LANE_LOG" 2>&1
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
# shellcheck disable=SC2016 # literal backticks: this is the `rows:` field's own text, quoted the
# way TASKS.md writes it.
is "the same edit under a product task is forced back to ready" "ready" \
  "$(lane .check-baseline '`src/thing.test.ts::a name copied from your suite`' .check-baseline)"
# --- the run log and the budget ---------------------------------------------
rm -f .harness/run.log
lane '' 'none — harness' >/dev/null
is "every spawned stage appends one record to the run log" "2" "$(grep -c . .harness/run.log)"
is "and the record carries the role, the seconds and the reported cost" "implementer 0.5" \
  "$(awk -F'\t' 'NR==1{print $3, $7}' .harness/run.log)"

rm -f .harness/run.log
BUDGET_USD=0.4 lane '' 'none — harness' >/dev/null
is "the loop stops before a stage that would exceed the budget" "1" \
  "$(grep -c 'HALT: the run has spent' "$LANE_LOG")"
is "and the stage it would have spawned never ran" "1" "$(grep -c . .harness/run.log)"
rm -f .harness/run.log

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

# A prompt trim is a behaviour change, and only a real agent can show whether a rule survived it.
# Gated like the driver: `./selftest.sh` stays fast and says out loud that it skipped this.
if [ -n "${HARNESS_EVALS:-}" ]; then
  EOUT=$("$SRC/evals/run.sh" 2>&1)
  is "the trimmed role prompts still pass their evals" "3" "$(printf '%s\n' "$EOUT" | grep -c ' PASS$')"
else
  skip "the trimmed role prompts still pass their evals" \
       "HARNESS_EVALS is unset, so no agent was spawned. Not a pass."
fi

# --- the write-path gate on a candidate rule ---------------------------------
# A stub that reads whatever rule is in front of it, so ablating the rule changes what it does.
# That is the only way to test the gate without spawning a real agent per outcome.
cat > src/rulestub.sh <<'STUB'
#!/usr/bin/env bash
# $1 = the prompt. Behaves per role, and on the verifier it obeys the rule only if the rule is there.
set -u
case "$1" in
  *"roles/verifier.md"*)
    case "${STUB_MODE:-follows}" in
      never)  : ;;                                   # never rejects: its eval fails with the rule in place
      always) sed -i.bak 's/^status: review/status: ready/' TASKS.md ;;   # rejects with or without the rule
      *) grep -q 'git status --porcelain' .harness/roles/verifier.md \
           && sed -i.bak 's/^status: review/status: ready/' TASKS.md ;;
    esac
    rm -f TASKS.md.bak ;;
  *"roles/scout.md"*)
    [ "${STUB_MODE:-follows}" = "breakother" ] && exit 0    # writes no proposed block: its eval fails
    printf '\n## [T-950] transcribed from a FINDING line\nscope: x\nblockedBy:\nstatus: proposed\nprobe: litter\ncommand: probes.sh\noutput: |\n  FINDING litter x:0 y\nnotes: proposed.\n' >> TASKS.md ;;
  *"roles/adjudicator.md"*)
    python3 - <<'PY'
import io, re
s = io.open('TASKS.md').read()
io.open('TASKS.md','w').write(re.sub(r'(?ms)^## \[T-900\].*?(?=\n## |\Z)', '', s))
d = io.open('DECISIONS.md').read()
io.open('DECISIONS.md','w').write(d.rstrip() + "\n- [2026-09-02] T-900, the check is slow — refuted by `probes.sh`: unanchored, no probe line.\n")
PY
    ;;
esac
STUB
chmod +x src/rulestub.sh
gate() { STUB_MODE="$1" EVAL_AGENT="$T/src/rulestub.sh {prompt}" "$SRC/evals/run.sh" --gate verifier 2>&1 | tail -1; }
is "a candidate rule that does not fix its case is rejected" \
  "GATE verifier REJECT the rule does not fix the case it came from (its eval fails with the rule in place)" "$(gate never)"
is "a candidate rule whose case passes without it is rejected" \
  "GATE verifier REJECT the case passes with the rule ablated, so the rule changed no outcome" "$(gate always)"
is "a candidate rule that regresses another eval is rejected" \
  "GATE verifier REJECT it regresses evals that were passing: scout" "$(gate breakother)"
is "a candidate rule that fixes its case and regresses nothing is accepted" \
  "GATE verifier ACCEPT fixes its case, fails without itself, regresses nothing" "$(gate follows)"
rm -f src/rulestub.sh

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

# --- the bootstrap record ---------------------------------------------------
# The README claims this package built itself. `docs/bootstrap.sh` reads that claim out of
# `git log` alone, and the assertion that matters is the second one: a check that cannot fail is
# not a check. The fixture is a history the script has never seen, built outside "$T" so the
# scratch repo's `git add -A` never swallows it as a gitlink.
FIX="${TMPDIR:-/tmp}/harness-bootstrap-$$"
mkdir -p "$FIX" && git -C "$FIX" init -q || exit 2
fixcommit() { git -C "$FIX" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "$1"; }
fixcommit 'init'
fixcommit 'chore(dogfood): install the harness'
fixcommit 'feat(x): T-001 a thing'
fixcommit 'verify: T-001 VERIFIED'
fixcommit 'chore: strip the dogfood instance'
fixcommit 'chore(dogfood): install the harness'
fixcommit 'fix(y): T-002 another thing'
fixcommit 'chore: strip the round-2 dogfood instance'
# rounds, and a verifier that never refused: the claim is unsupported and --check must say so
# The cd is guarded: a failed cd is itself status 1, the value this assertion wants, so an absent
# fixture would print `ok` for a run in which bootstrap.sh never executed (LEARNINGS.md,
# zero-as-pass). The inner subshell exits 99 — a status the assertion cannot want — and the outer
# one still reports it, so a broken fixture reads FAIL.
is "a history with no rejection fails the bootstrap check" "1" \
  "$( (cd "$FIX" || exit 99; "$SRC/docs/bootstrap.sh" --check >/dev/null 2>&1); echo $?)"
fixcommit 'chore(dogfood): install the harness'
fixcommit 'verify: T-003 REJECTED — a skipped assertion printed ok'
fixcommit 'chore: strip the round-3 dogfood instance'
# three rounds read out of a repository whose history was written a moment ago, so the record is
# derived rather than transcribed; and --check passes on this package's own history
FIXROUNDS=$(cd "$FIX" && "$SRC/docs/bootstrap.sh" | grep -c '^round ')
FIXRC=$(cd "$FIX" && "$SRC/docs/bootstrap.sh" --check >/dev/null 2>&1; echo $?)
SELFRC=$(cd "$SRC" && ./docs/bootstrap.sh --check >/dev/null 2>&1; echo $?)
is "the bootstrap record is derived from git" "3 0 0" "$FIXROUNDS $FIXRC $SELFRC"
rm -rf "$FIX"

# --- the immutability hashes ------------------------------------------------
# `tests-immutable` and `harness-immutable` (.harness/RAILS.md:57-58) name `test-hashes.json` as
# their enforcement, and nothing else recomputes it here: harness.json `check` is `./selftest.sh`
# and it has no precheck stage, so a hash file only the PreToolUse hook reads enforces nothing.
# Checked against the package source, not the throwaway install. The expected value lists the keys
# instead of counting them, so a key DELETED to make this pass is the thing that fails it
# (LEARNINGS.md, zero-as-pass) -- as does an empty `{}`.
# test-hashes.json is INSTANCE state, not package state: its keys are this dogfood instance's
# .harness/loop.sh and harness.json. The package branch carries no instance, so the file is absent
# there and this assertion cannot run. It reads `skip`, never `ok` -- a hash file that is not there
# enforces nothing, and reporting that as a pass is the zero-as-pass failure this suite exists to
# refuse (LEARNINGS.md).
if [ ! -f "$SRC/test-hashes.json" ]; then
  skip "every file test-hashes.json covers still hashes to its recorded digest" \
       "no test-hashes.json in $SRC: this is the package tree, which carries no installed instance."
else
HASHED=$(cd "$SRC" && python3 - <<'PY'
import hashlib, json
covered = json.load(open('test-hashes.json'))
bad = []
for key, want in sorted(covered.items()):
    got = hashlib.sha256(open(key, 'rb').read()).hexdigest()
    if got != want:
        bad.append('%s: recorded %s, on disk %s' % (key, want[:12], got[:12]))
print('; '.join(bad) if bad else ' '.join(sorted(covered)) + ' match')
PY
)
is "every file test-hashes.json covers still hashes to its recorded digest" \
  ".harness/loop.sh harness.json selftest.sh match" "$HASHED"
fi


# --- the shellcheck floor ---------------------------------------------------
# Checked against the package SOURCE, never the scratch install: `.harness/**` is install.sh's
# output, generated from `harness/**`, so linting a copy of a file already linted at its source
# proves nothing. Full severity -- no `-S` downgrade -- which is what SPEC.md's row says.
# `$SCANNED` carries the file list's own emptiness into the expected value: an empty list makes
# both greps below succeed with nothing, and `xargs` on empty input exits 0 (LEARNINGS.md,
# zero-as-pass). Both assertions `skip` when shellcheck is not installed.
if command -v shellcheck >/dev/null 2>&1; then
  SHLIST=$(cd "$SRC" && git ls-files "*.sh" | grep -v "^\.harness/")
  SCANNED=$([ -n "$SHLIST" ] && echo scanned || echo "no .sh files matched")

  SHOUT=$(cd "$SRC" && printf '%s\n' "$SHLIST" | xargs shellcheck 2>&1); SHRC=$?
  is "every shipped script passes shellcheck at full severity" "scanned 0" "$SCANNED $SHRC"
  [ "$SHRC" -eq 0 ] || printf '%s\n' "$SHOUT" | grep '^In ' | sed 's/^/      /'

  # A suppression is bare unless the same line ends `disable=<codes> # <reason>`. Two shapes count,
  # and shellcheck honours both: an inline directive in a script, where shellcheck accepts any
  # whitespace between the `#` and the word (`#shellcheck disable[=]SC2034` with no space at all is
  # live, checked at 0.11.0), and a file-wide `disable=` line in a `.shellcheckrc`, which silences a
  # code across every script under it and takes a same-line `#` comment too. The rc files are found
  # rather than named so a new one in a subdirectory is scanned as well. `disable[=]` need not be
  # the FIRST key: `# shellcheck source[=]/dev/null disable[=]SC1091` is honoured at 0.11.0, so the
  # pattern spans keys with `[^#]*` rather than requiring adjacency. The pattern is written
  # `disable[=]` so that this file, which the scan covers, does not report itself (TASKS.md T-013:
  # a check that plants the token it looks for).
  RCLIST=$(cd "$SRC" && git ls-files ".shellcheckrc" "*/.shellcheckrc" | grep -v "^\.harness/")
  BARE=$(cd "$SRC" && printf '%s\n' "$SHLIST" "$RCLIST" \
           | xargs grep -nE '#[[:space:]]*shellcheck[[:space:]][^#]*disable[=]|^[[:space:]]*disable[=]' \
           | grep -vE 'disable=[A-Z0-9,]+ +# *[^ ]' || true)
  is "every shellcheck suppression names its reason" "scanned " "$SCANNED $BARE"
else
  skip "every shipped script passes shellcheck at full severity" "shellcheck is not installed"
  skip "every shellcheck suppression names its reason" "shellcheck is not installed"
fi

# --- the floor, on a machine that is not the author's ------------------------
# Reads `.github/workflows/ci.yml`, never merely asserts it exists. The runners are pulled out of
# the `os:` list itself, so one commented out or moved into prose stops counting; an absent file
# yields an empty list, which cannot match the expected value (LEARNINGS.md, zero-as-pass) and is
# reported as its own token rather than as a silent pass. Two runners on purpose: macos-latest is
# bash 3.2 and BSD sed, ubuntu-latest is bash 5 and GNU sed, and both parser defects this package
# has had were that difference. HARNESS_EVALS is asserted ABSENT: the evals spawn a real agent, and
# a CI job holding a model credential is the blast radius this package argues against.
#
# The runner list and the invocation are both read out of the `floor:` job's OWN block, not out of
# the whole file, because the row claims the floor runs on BOTH userlands: the same step moved into
# the single-runner `shfmt:` job leaves the `os:` list untouched and runs on ubuntu only, which is a
# restructure rather than a sabotage and read `ok` until the verifier ablated it. An absent or
# renamed `floor:` job yields an empty block, so the list is empty and the count is 0 -- neither can
# match the expected value (LEARNINGS.md, zero-as-pass), and both are reported as their own token.
#
# THREE patterns carry an exclusion and each is named here, because the previous pass documented an
# anchor on the block and shipped one of its greps without it. `^[^#]*` on the `./selftest.sh` grep
# and on the `HARNESS_EVALS` grep, so a line of ci.yml's own prose cannot stand in for the thing it
# describes (T-013, a check that plants the token it looks for); and `-v '^[^#]*name:'` on the
# `./selftest.sh` grep, because a step `name:` mentioning the floor beside `run: true` is a label,
# not an invocation. `^[^#]*` and NOT `^[[:space:]]*run:`: a `run: |` block invoking the floor on a
# later line is legitimate and a `run:`-anchored pattern would call it absent. HARNESS_EVALS stays
# whole-file on purpose -- scoped to the floor block it would miss the variable set at workflow
# top level, which is strictly worse than reading the whole file.
#
# The `if:` count is asserted 0 at BOTH depths, under the ONE pattern that ships here,
# `^[[:space:]]*(-[[:space:]]+)?if:` -- not `^    if:`, which reads the job key and misses the step
# key, and not the dash-less form, which misses `- if:`. A job-level
# `if: github.event_name != 'pull_request'` is the ordinary CI-minutes edit, and the same shape is
# already in this file on the `scorecard:` job, so the floor can be switched off on a pull request
# with every other token still reading as expected. The floor job legitimately carries no `if:` at
# either depth, so the count is 0 and nothing else. The optional dash is the same idiom the pin
# assertion below uses: `- if: false` as a step's FIRST key is legal YAML, and it disables the floor
# exactly as `if:` on a later line of the same step does.
#
# Three shapes are KNOWN LIMITATIONS, not defects, and each fails loudly rather than silently:
# flow-style `os: [ubuntu-latest, macos-latest]`, which is legal YAML the matrix awk does not read;
# `./selftest.sh` moved into a step `env:` value beside `run: true`; and the string inside an echo
# (`run: echo 'to reproduce locally, run ./selftest.sh'`). The last two are one class -- a mention
# standing in for the thing -- and no line-oriented grep closes it; closing it needs a YAML parser
# plus shell parsing, which is not justified for one row.
CI=".github/workflows/ci.yml"
if [ -f "$SRC/$CI" ]; then
  # literal two spaces, not `[[:space:]]{2}`: ERE interval expressions are not portable across the
  # awk on macOS and the one on ubuntu, and this matrix exists because of exactly that class of gap.
  FLOORJOB=$(awk '/^  floor:/ { f = 1; next } f && /^  [a-z]/ { f = 0 } f' "$SRC/$CI")
  CIOS=$(printf '%s\n' "$FLOORJOB" \
    | awk '/^[[:space:]]*os:/ { f = 1; next } f && /^[[:space:]]*-[[:space:]]/ { print $2; next } f { f = 0 }' \
    | sort | tr '\n' ' ')
  CISELF=$(printf '%s\n' "$FLOORJOB" | grep -E '^[^#]*\./selftest\.sh' | grep -cvE '^[^#]*name:' || true)
  if [ "${CISELF:-0}" -gt 0 ]; then CISELF=invoked; else CISELF=absent; fi
  CIIF=$(printf '%s\n' "$FLOORJOB" | grep -cE '^[[:space:]]*(-[[:space:]]+)?if:' || true)
  CIGOT="${CIOS}selftest:$CISELF if:${CIIF:-?} evals:$(grep -cE '^[^#]*HARNESS_EVALS' "$SRC/$CI" || true)"
else
  CIGOT="no $CI"
fi
is "ci runs the floor on a GNU and a BSD userland" \
  "macos-latest ubuntu-latest selftest:invoked if:0 evals:0" "$CIGOT"

# A tag moves and a SHA does not, so a `uses:` pinned to a tag is an unreviewed third party running
# with the workflow's token. Every one carries its version in a trailing comment, which is how
# bats-core pins in `.github/workflows/scorecard.yml`. `/dev/null` is appended to the file list so
# grep always has a file argument: on an empty list GNU xargs would otherwise run grep with none
# and it would read stdin (LEARNINGS.md, zero-as-pass), and `$WFSCAN` carries that emptiness into
# the expected value regardless. grep's stderr is folded in so a tracked workflow file deleted
# from disk lands in the GOT string instead of scrolling past as terminal noise.
WFLIST=$(cd "$SRC" && git ls-files ".github/workflows/*.yml" ".github/workflows/*.yaml")
WFSCAN=$([ -n "$WFLIST" ] && echo scanned || echo "no workflow files matched")
UNPINNED=$(cd "$SRC" && printf '%s\n' "$WFLIST" /dev/null \
  | xargs grep -nE '^[[:space:]]*(-[[:space:]]+)?uses:' 2>&1 \
  | grep -vE 'uses:[[:space:]]*[^@[:space:]]+@[0-9a-f]{40}[[:space:]]+#[[:space:]]*[^[:space:]]' || true)
is "every github action is pinned to a commit sha" "scanned " "$WFSCAN $UNPINNED"

# --- shipped prose ----------------------------------------------------------
# This is a public repository. The patterns below are rhetoric, not information: antithesis,
# appeals to the point, and self-congratulation. Every one of them can be replaced by the fact it
# was decorating. Checked against the package source, not the throwaway install.
# A role prompt is an instruction, not a post-mortem. Dated incidents, task ids and reproduction
# stories are evidence for a human reading the repo; in a prompt they are tokens the model pays for
# on every stage. The rules, the rails they name and the output formats all stay.
NARRATIVE=$(cd "$SRC" && grep -rniE '[0-9]{4}-[0-9]{2}-[0-9]{2}|TASKS\.md T-[0-9]|reproduced (on |by )?[0-9]{4}|agentskills\.io' roles/ 2>/dev/null || true)
is "no role prompt carries an incident narrative" "" "$NARRATIVE"

FILLER='the whole point|that is the trick|is the whole |beautifully|elegantly|, it is one |extra steps|which is the point|the honest argument|is not a [a-z]+, it is'
HITS=$(cd "$SRC" && grep -rniE "$FILLER" README.md roles templates skills harness install.sh selftest.sh driver.sh evals 2>/dev/null | grep -v FILLER || true)
is "no shipped file carries rhetorical filler" "" "$HITS"

# --- teardown ---------------------------------------------------------------
cd /
# Never `[ -n "$KEEP" ] && echo ... || rm -rf "$T"`: a failed echo would run the rm and delete
# the scratch repo the operator asked to keep.
if [ -n "${KEEP:-}" ]; then echo "kept: $T"; else rm -rf "$T"; fi
echo
if [ "$FAIL" -ne 0 ]; then
  echo "harness selftest: FAILURES above."
elif [ "$SKIPPED" -gt 0 ]; then
  echo "harness selftest: all assertions that RAN passed, and $SKIPPED did not run. HARNESS_DRIVER=1 HARNESS_EVALS=1 runs the whole floor."
else
  echo "harness selftest: all assertions passed."
fi
exit "$FAIL"
