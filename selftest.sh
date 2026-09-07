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
bad() {
  echo "FAIL  $1"
  [ $# -gt 1 ] && echo "      $2"
  FAIL=1
}
is() { [ "$2" = "$3" ] && ok "$1" || bad "$1" "want [$2] got [$3]"; }
# A gate asserts what it EXECUTED. An assertion that did not run prints `skip`, never `ok`, and the
# summary says how many — otherwise an exit-criteria row reads green on a run that drove nothing
# (LEARNINGS.md, zero-as-pass; TASKS.md T-001, rejected 2026-09-02 for exactly this).
skip() {
  echo "skip  $1"
  [ $# -gt 1 ] && echo "      $2"
  SKIPPED=$((SKIPPED + 1))
  return 0
}

mkdir -p "$T/src" && cd "$T" || exit 2
git init -q && echo 'export const x = 1' >src/schema.ts
git add -A && git -c user.email=t@t -c user.name=t commit -qm init

# --- install ----------------------------------------------------------------
SKILLS=".claude/skills"
OUT=$("$SRC/install.sh" "$T" 2>&1)
RC=$?
is "install exits 0 on a fresh repo" "0" "$RC"
[ "$RC" -eq 0 ] || {
  printf '%s\n' "$OUT" | sed 's/^/      /'
  exit 1
}
echo "$OUT" | grep -q 'every token substituted' && ok "no __TOKEN__ survives substitution" ||
  bad "no __TOKEN__ survives substitution"

python3 - <<'PY'
import json
c = json.load(open('harness.json'))
c.update({'check': './src/fakecheck.sh', 'checkForce': './src/fakecheck.sh',
          'agentCommand': ['./src/fakeagent.sh', '{prompt}', '{turns}']})
json.dump(c, open('harness.json', 'w'), indent=2)
PY
printf '#!/usr/bin/env bash\necho "fake agent ran"\n' >src/fakeagent.sh && chmod +x src/fakeagent.sh
printf '#!/usr/bin/env bash\nexit 0\n' >src/fakecheck.sh && chmod +x src/fakecheck.sh
"$SRC/install.sh" "$T" >/dev/null 2>&1
is "re-install is idempotent" "0" "$?"
git add -A && git -c user.email=t@t -c user.name=t commit -qm harness

# --- the launcher -----------------------------------------------------------
.harness/loop.sh --selftest >/tmp/hs-loop.$$ 2>&1
is "loop.sh --selftest passes" "0" "$?"
is "loop.sh --selftest asserts something" "0" "$(grep -c FAIL /tmp/hs-loop.$$)"
rm -f /tmp/hs-loop.$$

DRY_RUN=1 .harness/loop.sh 1 2>&1 | grep -q 'DRY_RUN would spawn' &&
  ok "a dry iteration plans implement and verify" || bad "a dry iteration plans implement and verify"
DRY_RUN=1 .harness/loop.sh 1 2>&1 | grep -q 'via ./src/fakeagent.sh' &&
  ok "the launcher spawns the configured agent, not a hardcoded one" ||
  bad "the launcher spawns the configured agent, not a hardcoded one"
DRY_RUN=1 .harness/loop.sh 1 2>&1 | grep -q '.harness/roles/implementer.md' &&
  ok "the implement stage points the agent at its role file" ||
  bad "the implement stage points the agent at its role file"

# one command per role: a verifier on a different model from the implementer
python3 -c "
import json; c = json.load(open('harness.json'))
c['agentCommand'] = {'default': ['./src/fakeagent.sh', '{prompt}', '{turns}'],
                     'verifier': ['./src/fakeverifier.sh', '{prompt}', '{turns}']}
json.dump(c, open('harness.json', 'w'), indent=2)"
printf '#!/usr/bin/env bash\necho "fake verifier ran"\n' >src/fakeverifier.sh && chmod +x src/fakeverifier.sh
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

printf '\n[NEEDS CLARIFICATION] which store?\n' >>SPEC.md
.harness/loop.sh 1 2>&1 | grep -q 'HALT: SPEC.md carries' &&
  ok "a bare clarification marker halts the loop" || bad "a bare clarification marker halts the loop"
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
grep -q 'AGENTS.md' .github/copilot-instructions.md &&
  ok "copilot-instructions.md is a pointer at it" || bad "copilot-instructions.md is a pointer at it"
is "the context file stays short" "under" \
  "$(if [ "$(wc -l <AGENTS.md)" -lt 80 ]; then echo under; else wc -l <AGENTS.md; fi)"
grep -q '^name: running-the-loop' "$SKILLS/running-the-loop/SKILL.md" &&
  ok "the project skill is valid agentskills.io frontmatter" ||
  bad "the project skill is valid agentskills.io frontmatter"

# one parser, and the launcher holds none of its own
is "the launcher parses no task blocks itself" "0" \
  "$(grep -cE '## \\\[T-|awk .*TASKS|sed .*TASKS' .harness/loop.sh)"
is "tasks.py answers the queue against fixture files" "0" \
  "$(
    python3 .harness/tasks.py --selftest >/dev/null 2>&1
    echo $?
  )"
# shellcheck disable=SC2016 # the backticks are a literal markdown fence in the fixture, not a
# command substitution -- this assertion checks that tasks.py ignores a heading inside a fence.
printf '# TASKS\n\n```\n## [T-900] the example in the docs\nblockedBy:\nstatus: ready\n```\n\n## [T-901] the real one\nblockedBy: none\nstatus: ready\n' >/tmp/fence.$$.md
is "a heading inside a code fence is not a task" "T-901" \
  "$(python3 .harness/tasks.py ready-unattended /tmp/fence.$$.md)"
rm -f /tmp/fence.$$.md
# `list` is one line per task instead of the file's several hundred, and watch.sh:11 is its
# only other caller. File order IS priority -- ready_unattended takes the first `ready` in file
# order -- so the listing must never sort, and the fenced template block must never appear.
# shellcheck disable=SC2016 # the backticks are a literal markdown fence in the fixture again
printf '# TASKS\n\n```\n## [T-900] the template block, which is not a task\nstatus: ready\n```\n\n---\n\n## [T-902] out of file order by id, and file order is what wins\nstatus: done\n\n## [T-901] short\nstatus: ready\n' >/tmp/list.$$.md
is "tasks.py list prints one line per task, in file order, title cut at 44" \
  "T-902  out of file order by id, and file order is w  → done|T-901  short  → ready" \
  "$(python3 .harness/tasks.py list /tmp/list.$$.md | paste -sd'|' -)"
rm -f /tmp/list.$$.md
is "the launcher is split into sourced modules" "queue agent gates" \
  "$(for m in queue agent gates; do [ -f ".harness/lib/$m.sh" ] && printf '%s ' "$m"; done | sed 's/ $//')"

P=$(.harness/hooks/probes.sh 2>&1)
is "probes.sh exits 0 (every probe ran)" "0" "$?"
is "no probe errored" "0" "$(printf '%s\n' "$P" | grep -c 'PROBE .* ERROR')"
is "the row parser reads the seeded criteria table" "1" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE spec-untested //p')"
[ -x evals/run.sh ] && ok "the write-path gate installs where the rules are written" ||
  bad "the write-path gate installs where the rules are written"
is "a fresh install has two unenforced rails, both wanting test-hashes.json" "2" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE rail-unenforced //p')"
is "the seeded criterion is untested and no task in flight names it" "1" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE queue-uncovered //p')"
is "harness-immutable names loop.sh and no test-hashes.json covers it" "1" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE hash-uncovered //p')"
is "the seeded PROGRESS.md repeats no friction" "0" \
  "$(printf '%s\n' "$P" | sed -n 's/^PROBE friction-repeat //p')"
printf '%s\n' "$P" | grep -q '^PROBE driver OFF' &&
  ok "with no driverCommand the driver says so rather than scoring zero" ||
  bad "with no driverCommand the driver says so rather than scoring zero"
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
echo '- [2026-09-02] the check cache served a green nobody ran -> always run `./selftest.sh` uncached.' >>LEARNINGS.md
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
  echo "- [2026-09-0$n] a rule that cost a run -> do the other thing (evals/cache-green)." >>LEARNINGS.md
done
is "a learnings file over its cap is reported" "1" \
  "$(.harness/hooks/probes.sh 2>&1 | grep -c 'against a cap of 12')"
cp "$T/learnings.bak" LEARNINGS.md && rm -f "$T/learnings.bak" && rm -rf evals

# --- a reworded friction is the same friction --------------------------------
# The exact-match key this replaced never collided, so five sightings of one friction sat in the
# record and the probe read 0 (TASKS.md [T-045]). The firing pair below is two of those five, taken
# verbatim from this package's own PROGRESS.archive.md:428 and :477 -- a FOURTH and a FIFTH sighting
# of the same thing, worded differently. The pair under it is the guard, and it is the closest
# measured NON-repeat in the same record (:1253 and :1351): two different frictions that open with
# the same eleven words. Both are asserted because either alone passes on a broken probe. Measured
# against this fixture, 2026-09-06: at a threshold of 0.4 or 0.3 the guard pair reads as a repeat of
# itself and the count is 2; at 0.9, and with the exact text match this replaced, it is 0; at 0.05
# every friction here folds into one group and the count is 1 again, but the finding then says
# `recorded 5 times` and quotes the guard -- which is the case only the second assertion catches.
cp PROGRESS.md "$T/progress.bak"
cat >>PROGRESS.md <<'FRICTION'

## fixture — a friction
friction: FOURTH sighting of a check firing on the prose that documents it, and the first where the
next: nothing

## fixture — the same one, reworded
friction: FIFTH sighting of a check firing on the prose that documents it - and the first where the
next: nothing

## fixture — a different friction that shares an opening
friction: none new. One thing worth the next lane's time, not a rule: `.harness/hooks/probes.sh`
next: nothing

## fixture — and another, sharing the same opening
friction: none new. One thing worth the next lane's time: the selftest assertion deliberately does
next: nothing
FRICTION
FR=$(.harness/hooks/probes.sh 2>&1)
is "a reworded repeat of one friction is reported" "1" \
  "$(printf '%s\n' "$FR" | sed -n 's/^PROBE friction-repeat //p')"
is "and two frictions that merely share words are not collapsed into it" "1" \
  "$(printf '%s\n' "$FR" | grep -c '^FINDING friction-repeat PROGRESS.md:.*recorded 2 times.*FIFTH sighting')"
cp "$T/progress.bak" PROGRESS.md && rm -f "$T/progress.bak"

# --- the PROGRESS.md rollover -----------------------------------------------
for n in 1 2 3 4 5 6; do printf '\n## fixture entry %s\nfriction: none\nnext: nothing\n' "$n" >>PROGRESS.md; done
git add -A && git commit -qm progress >/dev/null 2>&1
# a done block whose notes quote another block inside a fence, and a ready block after it: the
# archive has to move the first and leave the second exactly where the launcher can take it.
# Both files are restored afterwards: the ceiling fixture below appends kill lines to DECISIONS.md
# and reads them as inside `## Rejected findings`, which an archived block at the end would close.
cp TASKS.md "$T/tasks.bak" && cp DECISIONS.md "$T/decisions.bak"
python3 - <<'SEED'
s = open('TASKS.md').read().rstrip() + """

## [T-401] done, and its notes quote a block
scope: src/a.ts
blockedBy:
status: done
notes: |
```
## [T-402] the quoted block, not a task
blockedBy:
status: ready
```

## [T-403] the next real task
scope: src/b.ts
blockedBy:
status: ready
"""
open('TASKS.md', 'w').write(s)
SEED
git add -A >/dev/null && git commit -qm 'seed a done block' >/dev/null
PROGRESS_MAX=12 PROGRESS_KEEP=4 .harness/archive-done.sh >/dev/null 2>&1
is "a done block is archived to DECISIONS.md and the queue keeps its stub" "DECISIONS.md 1 1" \
  "$(python3 .harness/tasks.py field T-401 archived | cut -d' ' -f1) $(grep -c '^## \[T-401\]' DECISIONS.md) $(grep -c '^status: done$' <<<"$(python3 .harness/tasks.py block T-401)")"
is "the quoted block is not a task to the launcher, and the real next task survives" "T-403 0" \
  "$(python3 .harness/tasks.py ids-at ready | grep -E 'T-40[0-9]' | tr '\n' ' ' | sed 's/ $//') $(python3 .harness/tasks.py list | grep -c 'T-402')"
cp "$T/tasks.bak" TASKS.md && cp "$T/decisions.bak" DECISIONS.md && rm -f "$T/tasks.bak" "$T/decisions.bak"
git add -A >/dev/null && git commit -qm 'archive fixture undone' >/dev/null
is "the oldest entry moves out of PROGRESS.md" "1" "$(grep -c 'fixture entry 1' PROGRESS.archive.md 2>/dev/null || echo 0)"
is "and is gone from the file the loop reads" "0" "$(grep -c 'fixture entry 1' PROGRESS.md)"
is "the newest entry stays" "1" "$(grep -c 'fixture entry 6' PROGRESS.md)"
is "the entry format the next iteration needs stays" "1" "$(grep -c '^## Entry format' PROGRESS.md)"
# the header keeps the entry-format fence, so the check is on what follows the archive note
is "the split lands on an entry heading, never inside one" "## fixture entry 6" \
  "$(awk '/^<!-- Entries before this point/{f=1; next} f && NF {print; exit}' PROGRESS.md)"
git add -A && git commit -qm rolled >/dev/null 2>&1

# --- one checkout, one writer ------------------------------------------------
# Liveness is `.harness/loop.pid`: loop.sh writes its pid on start and removes it on exit, and a
# writer is "not the lane" exactly when that pid is alive and not among the writer's own ancestors.
# The fixture is a background sleep standing in for a loop: a sibling of this shell, never an
# ancestor of it. The allow assertions are the ones that matter -- a hook that refuses every write
# is an outage, and an operator resolving a needs-spec halt is working because the loop stopped.
WRITE='{"tool_input":{"file_path":"TASKS.md"}}'
sleep 30 &
FAKELOOP=$!
echo "$FAKELOOP" >.harness/loop.pid
REFUSED=$(printf '%s' "$WRITE" | .harness/hooks/one-writer.sh 2>&1)
RRC=$?
is "a write from a session that is not the live lane is refused" "2 T-001" \
  "$RRC $(printf '%s\n' "$REFUSED" | grep -o 'T-001' | head -1)"
[ "$RRC" -eq 2 ] || printf '%s\n' "$REFUSED" | sed 's/^/      /'
OWN=$(bash -c 'echo $$ >.harness/loop.pid; printf %s "$0" | .harness/hooks/one-writer.sh 2>&1; echo "rc=$?"' "$WRITE" | tail -1)
is "a write from under the loop itself -- its own lane -- is allowed" "rc=0" "$OWN"
echo "$FAKELOOP" >.harness/loop.pid
{ kill "$FAKELOOP" && wait "$FAKELOOP"; } >/dev/null 2>&1
ALLOWED=$(printf '%s' "$WRITE" | .harness/hooks/one-writer.sh 2>&1)
is "a pid file left by a loop that is gone is not a live loop" "0 " "$? $ALLOWED"
rm -f .harness/loop.pid

# --- the driver, the only probe that does not read text ---------------------
printf '#!/usr/bin/env bash\necho "FINDING the artifact answered but wrote nothing to the store"\n' >src/fakedriver.sh
chmod +x src/fakedriver.sh
python3 -c "
import json; c = json.load(open('harness.json'))
c['driverCommand'] = '\$HARNESS_ROOT/src/fakedriver.sh'
json.dump(c, open('harness.json', 'w'), indent=2)"
"$SRC/install.sh" "$T" >/dev/null 2>&1
D=$(HARNESS_DRIVER=1 .harness/hooks/probes.sh 2>&1)
DRC=$?
is "a configured driver runs and exits 0" "0" "$DRC"
is "the driver's shortfall is one FINDING" "1" "$(printf '%s\n' "$D" | sed -n 's/^PROBE driver //p')"
printf '%s\n' "$D" | grep -q '^FINDING driver .*wrote nothing to the store' &&
  ok "the driver's FINDING line reaches the scout verbatim" ||
  bad "the driver's FINDING line reaches the scout verbatim"
# `env -u`, so this assertion still means what it says under `HARNESS_DRIVER=1 ./selftest.sh`
D=$(env -u HARNESS_DRIVER .harness/hooks/probes.sh 2>&1)
printf '%s\n' "$D" | grep -q '^PROBE driver OFF' &&
  ok "configured but HARNESS_DRIVER unset is still off" || bad "configured but HARNESS_DRIVER unset is still off"
# a driver that cannot reach the artifact records no score and the scout proposes nothing from it
printf '#!/usr/bin/env bash\necho "connection refused" >&2\nexit 7\n' >src/fakedriver.sh
D=$(HARNESS_DRIVER=1 .harness/hooks/probes.sh 2>&1)
DRC=$?
is "a driver that cannot reach the artifact fails the whole probe run" "1" "$DRC"
printf '%s\n' "$D" | grep -q '^PROBE driver ERROR' &&
  ok "an unreachable artifact is ERROR, never a count of zero" || bad "an unreachable artifact is ERROR, never a count of zero"
rm -f src/fakedriver.sh

# the example driver ships with every install: driverCommand needs somewhere to point, and a
# skeleton that finds nothing is the honest starting state
is "an example driver installs into the harness directory" "0" \
  "$(
    [ -x .harness/driver.example.sh ] && .harness/driver.example.sh >/dev/null 2>&1
    echo $?
  )"

# --- the driver's --unlabelled: what reached the branch without going through the queue -------
# Not gated on HARNESS_DRIVER, because it drives nothing. `--unlabelled` reads `git log` on the repo
# it is run from, so it is asserted against two hand-built histories rather than a round. This is
# the whole of the reading's reachability: `drive()` does not call it (T-076), so if these two go,
# nothing exercises it.
ulog() { # $1 = fixture dir under $T, $2.. = commit subjects, oldest first
  local d="$T/$1" s
  shift
  rm -rf "$d" && mkdir -p "$d" || return 2
  (
    cd "$d" || exit 2
    git init -q
    for s in "$@"; do
      echo "$s" >>f && git add f &&
        git -c user.email=t@t -c user.name=t commit -qm "$s" >/dev/null
    done
    "$SRC/driver.sh" --unlabelled
  )
}
UL_FIRE=$(ulog ul-mixed "feat: T-001 the work" "tidy the thing" "verify: T-001 verdict")
UL_QUIET=$(ulog ul-clean "feat: T-001 the work" "verify: T-001 verdict")
# Three facts in one string: the firing case found exactly one, it named the commit by subject
# rather than merely counting, and a round whose every commit names a task is silent.
is "the driver names a commit on the round that no task claims" "1 tidy the thing 0" \
  "$(printf '%s\n' "$UL_FIRE" | grep -c '^FINDING ') \
$(printf '%s\n' "$UL_FIRE" | sed -n 's/^FINDING [^:]*: [0-9a-f]* //p') \
$(printf '%s\n' "$UL_QUIET" | grep -c '^FINDING ')"
rm -rf "$T/ul-mixed" "$T/ul-clean"

# --- this package's own driver, against the installed artifact ---------------
# HARNESS_DRIVER gates it out of the ordinary run: it installs four throwaway repos and drives four
# loop iterations, ~54s (`time ./driver.sh` on the four-mode driver, on this tree at 195eb0e,
# 2026-09-06).
# `./selftest.sh` stays fast; `HARNESS_DRIVER=1 ./selftest.sh` asserts the worked example still
# reaches the artifact and still reports what it found.
if [ -n "${HARNESS_DRIVER:-}" ]; then
  DOUT=$("$SRC/driver.sh" 2>&1)
  DRC=$?
  # What is asserted is the INSTRUMENT: it reached the artifact (rc 0) and everything it said was a
  # finding — never a verdict, a pass or a fail. Deliberately NOT "it found at least one thing":
  # that would assert the harness stays broken, and the first hole it found (a lane that never
  # commits still reaching done) was closed by `gate_verdict` the same day, at which point the
  # probe correctly went quiet.
  DN=$(printf '%s\n' "$DOUT" | grep -v '^FINDING ' | grep -c . || true)
  is "the package driver reports shortfalls as FINDING lines" "0 0" "$DRC $DN"
  # One FINDING is one proposed block, so a finding the operator cannot open is a task nobody can
  # act on. Every sha the driver prints has to resolve HERE: `drive()` works in a `mktemp -d` that
  # is `rm -rf`'d before the probe prints, so a sha read out of that sandbox names nothing.
  DSHA=$(printf '%s\n' "$DOUT" | sed -n 's/^FINDING [^:]*: \([0-9a-f][0-9a-f]*\) .*/\1/p' |
    while read -r s; do git -C "$SRC" rev-parse -q --verify "$s^{commit}" >/dev/null || echo "$s"; done)
  is "every commit the package driver names resolves in this checkout" "" "$DSHA"
else
  skip "the package driver reports shortfalls as FINDING lines" \
    "HARNESS_DRIVER is unset, so nothing drove the artifact. Not a pass."
  skip "every commit the package driver names resolves in this checkout" \
    "HARNESS_DRIVER is unset, so nothing drove the artifact. Not a pass."
fi

# --- the gate, on delta -----------------------------------------------------
gate() {
  .harness/hooks/check-gate.sh >/dev/null 2>&1
  echo $?
}
printf '#!/usr/bin/env bash\necho "(fail) a thing that broke"\nexit 1\n' >src/fakecheck.sh
is "red and not on the baseline is a rejection" "2" "$(gate)"
echo "a thing that broke" >>.check-baseline
is "red and recorded on the baseline is forgiven" "0" "$(gate)"
printf '#!/usr/bin/env bash\necho "(fail) a thing that broke"\necho "(fail) something new"\nexit 1\n' >src/fakecheck.sh
is "one forgiven and one new failure is still a rejection" "2" "$(gate)"
printf '#!/usr/bin/env bash\necho boom >&2\nexit 1\n' >src/fakecheck.sh
is "red it cannot name forgives nothing, it fails closed" "2" "$(gate)"
printf '#!/usr/bin/env bash\nexit 0\n' >src/fakecheck.sh
is "green is green" "0" "$(gate)"

# --- the scope gate, end to end --------------------------------------------
# in_scope is asserted in loop.sh --selftest; this is the gate around it, which rewrites TASKS.md.
git config user.email t@t && git config user.name t
LANE_LOG="${TMPDIR:-/tmp}/harness-lane-$$.log"
cat >src/fakelane.sh <<'LANE'
#!/usr/bin/env bash
case "$1" in
  *"roles/implementer.md"*)
    date +%s%N > src/allowed.ts
    # .check-baseline is the one file where the DIRECTION of the edit is the question: emptied is
    # a failure cleared, appended is a red check made green by hand
    if [ "${SNEAK:-}" = .check-baseline ] && [ -z "${GROW:-}" ]; then : > .check-baseline
    elif [ -n "${SNEAK:-}" ]; then date +%s%N > "$SNEAK"; fi
    # SELF_DONE: the implementer writes the verdict on its own work, which only the verifier may
    if [ -n "${SELF_DONE:-}" ]; then sed -i.bak 's/^status: ready/status: done/' TASKS.md; else sed -i.bak 's/^status: ready/status: review/' TASKS.md; fi
    rm -f TASKS.md.bak
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
  SNEAK="$1" NO_COMMIT="${NO_COMMIT:-}" GROW="${GROW:-}" SELF_DONE="${SELF_DONE:-}" BUDGET_USD="${BUDGET_USD:-}" .harness/loop.sh 1 >"$LANE_LOG" 2>&1
  git add -A >/dev/null 2>&1 && git commit -qm "whatever the lane left" >/dev/null 2>&1
  awk '/^## \[T-101\]/{f=1} f&&/^status:/{print $2; exit}' TASKS.md
}
is "a lane inside its scope keeps its done verdict" "done" "$(lane '' 'none — harness')"
is "a lane that never committed is forced back to ready" "ready" "$(NO_COMMIT=1 lane '' 'none — harness')"
is "an implementer that marks its own task done is forced back to ready" "ready" "$(SELF_DONE=1 lane '' 'none — harness')"
is "and the verify stage is skipped, since nothing is at review" "0" "$(grep -c '^=== Iteration 1: verify' "$LANE_LOG")"
is "a lane that leaves its scope is forced back to ready" "ready" "$(lane src/sneaky.ts 'none — harness')"
# the message is the line a human acts on, so it is asserted, not only the status it produced
is "and the rejection names the file it is rejecting" "1" \
  "$(grep -c 'SCOPE FAILED -- touched src/sneaky.ts' "$LANE_LOG")"
rm -f src/sneaky.ts
# each baseline lane starts from a baseline with a line in it: the allowed edit empties it, so a
# second lane on the emptied file would be no edit at all and prove nothing
echo "a thing that broke" >>.check-baseline
is "a harness edit a harness task declared is allowed" "done" \
  "$(lane .check-baseline 'none — harness' .check-baseline)"
echo "a thing that broke" >>.check-baseline
# shellcheck disable=SC2016 # literal backticks: this is the `rows:` field's own text, quoted the
# way TASKS.md writes it.
is "the same edit under a product task is forced back to ready" "ready" \
  "$(lane .check-baseline '`src/thing.test.ts::a name copied from your suite`' .check-baseline)"
echo "a thing that broke" >>.check-baseline
is "a line added to .check-baseline is rejected even under a harness task" "ready" \
  "$(GROW=1 lane .check-baseline 'none — harness' .check-baseline)"
is "and the rejection says the baseline only shrinks" "1" "$(grep -c 'SCOPE FAILED --.*baseline only ever shrinks' "$LANE_LOG")"
: >.check-baseline
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
cat >src/wtlane.sh <<'WTLANE'
#!/usr/bin/env bash
P=$(dirname "$(git rev-parse --git-common-dir)")
case "$1" in
  *"roles/implementer.md"*)
    printf '%s %s\n' "$(git -C "$P" rev-parse HEAD)" "$(git -C "$P" status --porcelain | grep -c . || true)" > wt-observed.txt
    case "$PWD" in
      "$P"/.harness/worktrees/*) W=under-harness ;;
      "$P"/*) W=inside-repo ;;
      *) W=beside-the-repo ;;
    esac
    { printf '%s\n' "$W"; git -C "$P" status --porcelain | tr '\n' ';'; printf '\n'; } > wt-where.txt
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
scope: src/allowed.ts, wt-observed.txt, wt-where.txt
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
# Where the checkout LANDS, read from inside the lane while it runs. A sibling of the repository
# put a lane's tree in the operator's home directory (T-077); under the harness directory it is
# covered by __HARNESS_DIR__/.gitignore, and the second reading is the one that matters -- an
# un-ignored directory inside the repo is exactly the untracked path gate_verdict counts as
# "the lane left its work off the branch", which cost T-004 a VERIFIED verdict on 2026-09-04.
is "a lane's worktree lands under the harness directory, not beside the repository" \
  "under-harness" "$(sed -n 1p wt-where.txt 2>/dev/null)"
is "and the parent's git status is empty while that worktree exists" \
  "" "$(sed -n 2p wt-where.txt 2>/dev/null)"
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
cat >src/rulestub.sh <<'STUB'
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
cat >src/evalobeys.sh <<'OBEYS'
#!/usr/bin/env bash
# the verifier's rule: uncommitted source means the work is not on the branch. Reject to ready.
sed -i.bak 's/^status: review/status: ready/' TASKS.md && rm -f TASKS.md.bak
OBEYS
cat >src/evalbreaks.sh <<'BREAKS'
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
cp -R "$SRC" "$T/pkgcopy" 2>/dev/null
rm -f "$T/pkgcopy/harness.json" "$T/pkgcopy/harness.default.json"
is "with no agent configured the evals refuse rather than report" "2" \
  "$(
    "$T/pkgcopy/evals/run.sh" verifier >/dev/null 2>&1
    echo $?
  )"
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
  "$(
    (
      cd "$FIX" || exit 99
      "$SRC/docs/bootstrap.sh" --check >/dev/null 2>&1
    )
    echo $?
  )"
fixcommit 'chore(dogfood): install the harness'
fixcommit 'verify: T-003 REJECTED — a skipped assertion printed ok'
fixcommit 'chore: strip the round-3 dogfood instance'
# three rounds read out of a repository whose history was written a moment ago, so the record is
# derived rather than transcribed; and --check passes on this package's own history
FIXROUNDS=$(cd "$FIX" && "$SRC/docs/bootstrap.sh" | grep -c '^round ')
FIXRC=$(
  cd "$FIX" && "$SRC/docs/bootstrap.sh" --check >/dev/null 2>&1
  echo $?
)
SELFRC=$(
  cd "$SRC" && ./docs/bootstrap.sh --check >/dev/null 2>&1
  echo $?
)
is "the bootstrap record is derived from git" "3 0 0" "$FIXROUNDS $FIXRC $SELFRC"
rm -rf "$FIX"

# --- the sixty-second demo --------------------------------------------------
# README.md opens with `docs/demo.sh`, so the first thing a stranger runs is covered by the floor
# like everything else. The assertion reads the STAGE SEQUENCE out of the demo's own stdout in
# order and unsorted -- an iteration that printed a digest without an implement stage, or printed
# them the other way round, is not the run the README describes. Exit status alone would go green
# on a demo that printed nothing (LEARNINGS.md, zero-as-pass).
# ponytail: this reads the demo's own stdout, so a demo.sh that printed a canned transcript and
# ran nothing would still pass. Seven mutations of the real script fail it (T-074 notes); a
# side-effect reading under a TMPDIR this assertion owns is the upgrade if that stops holding.
DEMO_OUT=$("$SRC/docs/demo.sh" 2>&1)
DEMO_RC=$?
DEMO_SEEN=$(printf '%s\n' "$DEMO_OUT" |
  grep -oE '=== Iteration 1: implement T-001|=== Iteration 1: verify T-001|=== digest|T-001 +status: done' |
  tr '\n' '|')
# "it must leave the machine as it found it" is the criterion with nothing else behind it, so it is
# read here too. The demo prints the directory it made; an extraction that found no path reads
# `nopath`, never `gone` -- otherwise a demo that printed nothing at all would pass this half.
DEMO_TMP=$(printf '%s\n' "$DEMO_OUT" | sed -n 's/^== install into a throwaway repo  (\(.*\))$/\1/p')
if [ -z "$DEMO_TMP" ]; then
  DEMO_LEFT=nopath
elif [ -d "$DEMO_TMP" ]; then
  DEMO_LEFT=left
else
  DEMO_LEFT=gone
fi
is "docs/demo.sh drives one loop iteration end to end and deletes what it made" \
  "0|=== Iteration 1: implement T-001|=== Iteration 1: verify T-001|=== digest|T-001  status: done|gone" \
  "$DEMO_RC|$DEMO_SEEN$DEMO_LEFT"

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
  HASHED=$(
    cd "$SRC" && python3 - <<'PY'
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

  SHOUT=$(cd "$SRC" && printf '%s\n' "$SHLIST" | xargs shellcheck 2>&1)
  SHRC=$?
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
  BARE=$(cd "$SRC" && printf '%s\n' "$SHLIST" "$RCLIST" |
    xargs grep -nE '#[[:space:]]*shellcheck[[:space:]][^#]*disable[=]|^[[:space:]]*disable[=]' |
    grep -vE 'disable=[A-Z0-9,]+ +# *[^ ]' || true)
  is "every shellcheck suppression names its reason" "scanned " "$SCANNED $BARE"
else
  skip "every shipped script passes shellcheck at full severity" "shellcheck is not installed"
  skip "every shellcheck suppression names its reason" "shellcheck is not installed"
fi

# --- the shfmt floor --------------------------------------------------------
# The package SOURCE again, never the scratch install, for the same reason as the shellcheck floor
# above. No flags on purpose: shfmt reads `.editorconfig` only when it is given none, and
# `.editorconfig` (`[*.sh] indent_style = space, indent_size = 2`) is where this repo's flag set is
# recorded -- so this run and `.github/workflows/ci.yml:74`, which also passes no flags, cannot
# drift apart. `$FMTSCANNED` carries the file list's own emptiness into the expected value: `xargs`
# on empty input exits 0, so a glob that matched nothing would otherwise read green (LEARNINGS.md,
# zero-as-pass). Skips when shfmt is absent -- it is on neither GitHub runner image, which is why
# ci.yml installs it.
if command -v shfmt >/dev/null 2>&1; then
  FMTLIST=$(cd "$SRC" && git ls-files "*.sh" | grep -v "^\.harness/")
  FMTSCANNED=$([ -n "$FMTLIST" ] && echo scanned || echo "no .sh files matched")

  FMTOUT=$(cd "$SRC" && printf '%s\n' "$FMTLIST" | xargs shfmt --diff 2>&1)
  FMTRC=$?
  is "every shipped script is shfmt clean" "scanned 0" "$FMTSCANNED $FMTRC"
  [ "$FMTRC" -eq 0 ] || printf '%s\n' "$FMTOUT" | grep -E '^--- ' | sed 's/^/      /'
else
  skip "every shipped script is shfmt clean" "shfmt is not installed"
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
# One job's own block, never the whole file. Literal two spaces, not `[[:space:]]{2}`: ERE interval
# expressions are not portable across the awk on macOS and the one on ubuntu, and this matrix exists
# because of exactly that class of gap. An absent or renamed job yields an empty block, so every
# token derived from it reads as its own absence rather than as a silent pass (LEARNINGS.md,
# zero-as-pass).
# `[a-z#]`, so the block ends at the comment that introduces the NEXT job as well as at the job key
# itself: every comment inside a job in this file is indented deeper than two spaces, and every
# comment at exactly two is a separator between jobs.
job_block() { # $1 = a workflow file, $2 = a job key
  awk -v k="$2" '$0 == "  " k ":" { f = 1; next } f && /^  [a-z#]/ { f = 0 } f' "$1"
}
CI=".github/workflows/ci.yml"
if [ -f "$SRC/$CI" ]; then
  FLOORJOB=$(job_block "$SRC/$CI" floor)
  CIOS=$(printf '%s\n' "$FLOORJOB" |
    awk '/^[[:space:]]*os:/ { f = 1; next } f && /^[[:space:]]*-[[:space:]]/ { print $2; next } f { f = 0 }' |
    sort | tr '\n' ' ')
  CISELF=$(printf '%s\n' "$FLOORJOB" | grep -E '^[^#]*\./selftest\.sh' | grep -cvE '^[^#]*name:' || true)
  if [ "${CISELF:-0}" -gt 0 ]; then CISELF=invoked; else CISELF=absent; fi
  CIIF=$(printf '%s\n' "$FLOORJOB" | grep -cE '^[[:space:]]*(-[[:space:]]+)?if:' || true)
  # `continue-on-error: true` is the sibling of `if:` -- the step runs, its failure is a warning,
  # the job stays green. Counted at any depth for the same reason `if:` is.
  CICOE=$(printf '%s\n' "$FLOORJOB" | grep -cE '^[[:space:]]*(-[[:space:]]+)?continue-on-error:' || true)
  CIGOT="${CIOS}selftest:$CISELF if:${CIIF:-?} coe:${CICOE:-?} evals:$(grep -cE '^[^#]*HARNESS_EVALS' "$SRC/$CI" || true)"
else
  CIGOT="no $CI"
fi
is "ci runs the floor on a GNU and a BSD userland" \
  "macos-latest ubuntu-latest selftest:invoked if:0 coe:0 evals:0" "$CIGOT"

# --- the one assertion that reaches the artifact, off this machine -----------
# `the package driver reports shortfalls as FINDING lines` is gated behind HARNESS_DRIVER, so until
# ci.yml carried a job that sets it, the only assertion in this file that touches the installed
# package ran when the author remembered a flag and never otherwise. A gate asserts what it
# EXECUTED, and one nothing ever executes is not a gate. This reads ci.yml for the job that runs it.
#
# Four tokens, each an ablation the row cares about: the job itself deleted, the variable no longer
# usefully set (the floor still runs and every assertion still prints ok, minus the one that
# matters), the floor invocation replaced by something cheaper, and the job switched off by an
# `if:` while every other token still reads as expected. The `if:` count is the floor assertion's
# own idiom five lines up and it is here for the verdict that put it there (`aff0b33`, T-004
# rejected because an `if:` on the floor job leaves the row green): a job that never runs asserts
# nothing, and `the package driver reports shortfalls as FINDING lines` would print `skip` on every
# CI run with this row green. The scorecard job's legitimate `if:` is a different job's block and
# is not read here.
#
# The variable is read by VALUE, not by presence: `HARNESS_DRIVER: ''` is the token present and the
# feature off, because selftest.sh:277 gates on `[ -n "${HARNESS_DRIVER:-}" ]`. `[^[:space:]'"]`
# after the optional opening quote is what rejects the empty string. The `^[^#]*` and `-v name:`
# exclusions are the floor assertion's, for its two reasons: ci.yml's own prose names both tokens,
# and a step `name:` mentioning the floor is a label rather than an invocation.
#
# Asserted FIRING, not merely passing on a file that happens to be right: the same function runs
# over three fixture workflows written here -- no driver job at all; a driver job that runs the
# floor with `HARNESS_DRIVER: ''`; and a driver job that is correct in every other token and
# carries `if: false`. Three assertions this round were rejected for passing vacuously (93e567a,
# aff0b33, 74eaa47), so the expected value carries all four readings and each is worthless without
# the others -- present on the real file proves nothing if the function cannot report absence, and
# absent on a fixture proves nothing if it reports absence on everything. Every token has both of
# its readings somewhere in the expected value; the `ci-off.yml` arm differs from the real file's
# in the `if:` token and in nothing else.
#
# HARNESS_EVALS is deliberately not read here: the floor assertion above already counts it over the
# WHOLE file, which is the depth that catches it set at workflow top level, and that count staying
# 0 is criterion 2 of this task. Duplicating it at job depth would be strictly weaker.
driver_job() { # $1 = a workflow file. The job, the variable, the floor invocation, and the `if:`.
  local blk self
  blk=$(job_block "$1" driver)
  self=$(printf '%s\n' "$blk" | grep -E '^[^#]*\./selftest\.sh' | grep -cvE '^[^#]*name:' || true)
  printf 'job:%s driver:%s selftest:%s if:%s coe:%s' \
    "$([ -n "$blk" ] && echo present || echo absent)" \
    "$(printf '%s\n' "$blk" | grep -qE "^[^#]*HARNESS_DRIVER:[[:space:]]*['\"]?[^[:space:]'\"]" && echo set || echo unset)" \
    "$([ "${self:-0}" -gt 0 ] && echo invoked || echo absent)" \
    "$(printf '%s\n' "$blk" | grep -cE '^[[:space:]]*(-[[:space:]]+)?if:' || true)" \
    "$(printf '%s\n' "$blk" | grep -cE '^[[:space:]]*(-[[:space:]]+)?continue-on-error:' || true)"
}
cat >"$T/ci-nojob.yml" <<'YML'
jobs:
  floor:
    steps:
      - run: ./selftest.sh
YML
cat >"$T/ci-novar.yml" <<'YML'
jobs:
  driver:
    steps:
      - name: the floor, with the driver reaching the artifact
        env:
          HARNESS_DRIVER: ''
        run: ./selftest.sh
YML
cat >"$T/ci-off.yml" <<'YML'
jobs:
  driver:
    if: false
    steps:
      - env:
          HARNESS_DRIVER: '1'
        run: ./selftest.sh
YML
cat >"$T/ci-soft.yml" <<'YML'
jobs:
  driver:
    steps:
      - env:
          HARNESS_DRIVER: '1'
        continue-on-error: true
        run: ./selftest.sh
YML
is "ci runs the floor with the driver reaching the artifact" \
  "job:present driver:set selftest:invoked if:0 coe:0 | job:absent driver:unset selftest:absent if:0 coe:0 | job:present driver:unset selftest:invoked if:0 coe:0 | job:present driver:set selftest:invoked if:1 coe:0 | job:present driver:set selftest:invoked if:0 coe:1" \
  "$([ -f "$SRC/$CI" ] && driver_job "$SRC/$CI" || echo "no $CI") | $(driver_job "$T/ci-nojob.yml") | $(driver_job "$T/ci-novar.yml") | $(driver_job "$T/ci-off.yml") | $(driver_job "$T/ci-soft.yml")"
rm -f "$T/ci-nojob.yml" "$T/ci-novar.yml" "$T/ci-off.yml" "$T/ci-soft.yml"

# A tag moves and a SHA does not, so a `uses:` pinned to a tag is an unreviewed third party running
# with the workflow's token. Every one carries its version in a trailing comment, which is how
# bats-core pins in `.github/workflows/scorecard.yml`. `/dev/null` is appended to the file list so
# grep always has a file argument: on an empty list GNU xargs would otherwise run grep with none
# and it would read stdin (LEARNINGS.md, zero-as-pass), and `$WFSCAN` carries that emptiness into
# the expected value regardless. grep's stderr is folded in so a tracked workflow file deleted
# from disk lands in the GOT string instead of scrolling past as terminal noise.
#
# Asserted FIRING, not merely passing on a repo that happens to be clean: `unpinned_uses` runs a
# SECOND time over a scratch workflow written into the throwaway install, carrying a tag pin and a
# bare SHA with no trailing comment beside one correctly pinned line. Both halves are in one
# expected value because each is worthless alone -- nothing reported here proves nothing if the
# scan cannot report at all, and two reported there proves nothing if it reports on everything.
# That is the shape two assertions were already rejected for this round (`74eaa47`, `5a4d129`).
# The fixture is `git add`ed and scanned through the SAME function, so the `git ls-files` discovery
# glob is exercised by both and the two scans cannot drift apart; it is un-added and deleted
# afterwards, and only `workflows/` goes -- `.github/` itself holds install.sh's
# copilot-instructions.md, which an assertion above reads.
unpinned_uses() { # $1: a git work tree. Prints every `uses:` in its workflows that is not SHA-pinned.
  (
    cd "$1" || return
    printf '%s\n' "$(git ls-files ".github/workflows/*.yml" ".github/workflows/*.yaml")" /dev/null |
      xargs grep -nE '^[[:space:]]*(-[[:space:]]+)?uses:' 2>&1 |
      grep -vE 'uses:[[:space:]]*[^@[:space:]]+@[0-9a-f]{40}[[:space:]]+#[[:space:]]*[^[:space:]]' || true
  )
}
WFLIST=$(cd "$SRC" && git ls-files ".github/workflows/*.yml" ".github/workflows/*.yaml")
WFSCAN=$([ -n "$WFLIST" ] && echo scanned || echo "no workflow files matched")
UNPINNED=$(unpinned_uses "$SRC")
mkdir -p "$T/.github/workflows"
cat >"$T/.github/workflows/pin-fixture.yml" <<'YML'
jobs:
  fixture:
    steps:
      - uses: actions/checkout@v7.0.1
      - uses: ossf/scorecard-action@2d1146689b8cda280b9bc96326124645441f03bc
      - uses: step-security/harden-runner@e14015d583714f6e62063499dc959a02595150a1 # v2.21.1
YML
git -C "$T" add -f .github/workflows/pin-fixture.yml
FIXBAD=$(unpinned_uses "$T" | grep -c 'pin-fixture' || true)
FIXOK=$(unpinned_uses "$T" | grep -c 'harden-runner' || true)
git -C "$T" rm -qf --cached .github/workflows/pin-fixture.yml
rm -rf "$T/.github/workflows"
is "every github action is pinned to a commit sha" "scanned unpinned:2 pinned:0 " \
  "$WFSCAN unpinned:$FIXBAD pinned:$FIXOK $UNPINNED"

# --- the declared skills and what enforces each ------------------------------
# The role prompts name skills and continue when one is missing, so a skill is a soft dependency
# and the only thing that makes one non-optional is a gate that runs without it. `skills` in
# harness.default.json records what is relied on and what enforces it; a `gate` naming an
# enforcement that does not exist is a failure, not a warning, or the key documents a check nobody
# runs. Read from the package default -- the key belongs there and never in harness.json, which
# gate_scope classifies as the harness set -- and checked against the throwaway install's own
# probes and rails, the two files a gate may name. The expected value carries the COUNT, so an
# entry deleted to make this pass is the thing that fails it (LEARNINGS.md, zero-as-pass), as does
# a missing or empty list.
SKILLGATE=$(
  HD="$SRC/harness.default.json" python3 - <<'PY'
import json, os, re
skills = json.load(open(os.environ['HD'])).get('skills')
if not isinstance(skills, list) or not skills:
    print('harness.default.json declares no skills list')
    raise SystemExit
known = open('.harness/hooks/probes.sh').read() + open('.harness/RAILS.md').read()
bad = []
for entry in skills:
    absent = [f for f in ('name', 'why', 'gate') if not entry.get(f)]
    if absent:
        bad.append('%s lacks %s' % (entry.get('name', '?'), ','.join(absent)))
    # word-boundary, not substring: a one-letter gate matches some word in every file otherwise
    elif entry['gate'] != 'none' and not re.search(r'\b%s\b' % re.escape(entry['gate']), known):
        bad.append('%s names gate %s, which neither probes.sh nor RAILS.md defines'
                   % (entry['name'], entry['gate']))
print('; '.join(bad) if bad else '%d skills, every gate exists' % len(skills))
PY
)
is "every declared skill names its enforcing gate" "7 skills, every gate exists" "$SKILLGATE"

# --- the same question, asked of the installed repo by the probe ------------
# The gate above reads the package default; this asserts the probe that reports the same shortfall
# to the scout, out of the installed `harness.json`, and asserts it FIRING -- a probe asserted only
# by "it ran" reports nothing and still passes, the shape two assertions were already rejected for
# this round (`74eaa47`, `5a4d129`). Both halves in one string because each is worthless alone: the
# named FINDING on the `gate: none` fixture proves it can report at all, and 0 on the fixture whose
# every gate exists proves it discriminates rather than reporting on everything.
# `harness.json` is restored afterwards -- the headless-lane assertion below counts the names in it.
cp harness.json "$T/harness.bak"
skills_fixture() { python3 -c "
import json, sys
c = json.load(open('harness.json'))
c['skills'] = json.loads(sys.argv[1])
json.dump(c, open('harness.json', 'w'), indent=2)" "$1"; }
skills_fixture '[{"name": "fixture-gated", "why": "w", "gate": "ponytail-ceiling"},
                 {"name": "fixture-hope", "why": "w", "gate": "none"}]'
UNGATED=$(.harness/hooks/probes.sh 2>&1)
FIRED=$(printf '%s\n' "$UNGATED" | sed -n 's/^PROBE skill-ungated //p')
NAMED=$(printf '%s\n' "$UNGATED" | grep -c '^FINDING skill-ungated .*fixture-hope is declared with gate: none')
skills_fixture '[{"name": "fixture-gated", "why": "w", "gate": "ponytail-ceiling"}]'
QUIET=$(.harness/hooks/probes.sh 2>&1 | sed -n 's/^PROBE skill-ungated //p')
is "a skill with no enforcement is reported" "1 1 0" "$FIRED $NAMED $QUIET"
cp "$T/harness.bak" harness.json && rm -f "$T/harness.bak"

# --- a ceiling the adjudicator has already killed ----------------------------
# The scout proposes from a FINDING line, so a marker whose kill is already written down comes back
# every round until the probe reads the kills: 22 blocks in one 2026-09-04 round, 21 killed, every
# one of them `ponytail-ceiling` (TASKS.md [T-070]). Matched on the marker's own TEXT, never on the
# task id beside it -- an id is spent once and the same marker returns under a new one -- and never
# on the line number alone, because a marker that MOVED is the same marker (`gates.sh:42` came back
# as `:44`, `:65` as `:92`). The second fixture below is that moved case: the kill line names a
# line the marker no longer sits on and quotes the marker itself, which is the only thing that can
# still identify it.
# Two assertions, because either one alone passes on a broken probe. The first shows a written kill
# suppressing; the SECOND shows the probe still reporting the marker nobody killed, since a probe
# that reports nothing is an outage and not a clean tree (LEARNINGS.md, zero-as-pass). That third
# marker is named twice more, in the two shapes that are not kills -- an UNDATED line inside the
# section, and a dated line under an archived block BELOW it -- so the second assertion fails a
# probe that takes either for a kill. Measured: without those two lines, dropping the `DATED`
# filter and ignoring the section boundary both passed.
# `DECISIONS.md` is restored afterwards -- it is tracked here and the lane gates below read the tree.
CEIL=$(.harness/hooks/probes.sh 2>&1)
BEFORE=$(printf '%s\n' "$CEIL" | sed -n 's/^PROBE ponytail-ceiling //p')
CREFS=$(printf '%s\n' "$CEIL" | sed -n 's/^FINDING ponytail-ceiling \([^ ]*\) .*$/\1/p')
KILLED=$(printf '%s\n' "$CREFS" | sed -n 1p) # killed at the path:line it still sits on
MOVED=$(printf '%s\n' "$CREFS" | sed -n 2p)  # killed at a line it has since moved off
LIVE=$(printf '%s\n' "$CREFS" | sed -n 3p)   # no kill line anywhere, so still a finding
MTEXT=$(sed -n "${MOVED##*:}p" "${MOVED%:*}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
STILL0=$(printf '%s\n' "$CEIL" | grep -cE "^FINDING ponytail-ceiling $LIVE ")
cp DECISIONS.md "$T/decisions.bak"
{
  printf -- "- [2026-09-04] the marker at \`%s\` — refuted by a re-run: settled.\n" "$KILLED"
  printf -- "- [2026-09-04] the marker at \`%s\`, since moved — refuted by a re-run: \`%s\`.\n" \
    "${MOVED%:*}:1" "$MTEXT"
  printf -- "- the marker at \`%s\` — undated, so nothing decided it and it is not a kill.\n" "$LIVE"
  printf -- "\n## [T-000] an archived block, below the section\n"
  printf -- "- [2026-09-04] the marker at \`%s\` — dated, but out of the section.\n" "$LIVE"
} >>DECISIONS.md
AFTER=$(.harness/hooks/probes.sh 2>&1)
LEFT=$(printf '%s\n' "$AFTER" | sed -n 's/^PROBE ponytail-ceiling //p')
GONE=$(printf '%s\n' "$AFTER" | grep -cE "^FINDING ponytail-ceiling ($KILLED|$MOVED) ")
STILL=$(printf '%s\n' "$AFTER" | grep -cE "^FINDING ponytail-ceiling $LIVE ")
# two of BEFORE are killed above, so LEFT is BEFORE less two -- never a literal count, which read
# a marker added elsewhere in the package as this probe breaking
is "a ceiling whose kill is already written down is not re-proposed" "$((BEFORE - 2)) 0" "$LEFT $GONE"
is "a ceiling with no kill line is still reported" "1 1" "$STILL0 $STILL"
cp "$T/decisions.bak" DECISIONS.md && rm -f "$T/decisions.bak"

# --- the skill contract on a headless lane -----------------------------------
# The role prompts name skills and a headless lane may ignore them; the only thing that reaches one
# every turn is a hook the install writes into the repo. `--adapter claude` writes it as
# `UserPromptSubmit`, not `SessionStart`: SessionStart fires once and decays as the context grows
# (TASKS.md [T-041] criterion 2, measured 2026-09-03 against `claude -p`).
# One assertion, three facts in one string, because each is worthless alone: the hook exits 0 (it
# reports and never blocks -- refusal is the gates' job), `settings.json` registers it on the
# per-prompt event, and its stdout carries every `name` in the installed `harness.json`. The COUNT
# is in the expected value, so a `skills` list emptied to make this pass is the thing that fails it
# (LEARNINGS.md, zero-as-pass).
# It `skip`s only when the package ships no claude adapter to install -- never when the adapter ran
# and wrote nothing. "The install produced no hook" is the failure this assertion exists to catch,
# and a skip there would report the hole as an absence of a check.
if [ ! -d "$SRC/adapters/claude" ]; then
  skip "the skill hook fires on a headless lane" "the package ships no claude adapter"
else
  "$SRC/install.sh" "$T" --adapter claude >/dev/null 2>&1
  if [ -x .harness/hooks/skill-hook.sh ]; then
    HOOKOUT=$(CLAUDE_PROJECT_DIR="$T" .harness/hooks/skill-hook.sh </dev/null 2>&1)
    HOOKRC=$?
  else
    HOOKOUT=""
    HOOKRC="--adapter claude wrote no .harness/hooks/skill-hook.sh"
  fi
  SKILLHOOK=$(
    HOOKOUT="$HOOKOUT" python3 - <<'PY'
import json, os
names = [s.get('name', '') for s in (json.load(open('harness.json')).get('skills') or [])]
missing = [n for n in names if n not in os.environ['HOOKOUT']]
print('the installed harness.json declares no skills' if not names
      else 'absent from the hook: %s' % ' '.join(missing) if missing
      else '%d names' % len(names))
PY
  )
  EVENT=$(python3 -c "
import json
try:
    hooks = json.load(open('.claude/settings.json')).get('hooks', {})
except Exception as e:
    print('no readable .claude/settings.json: %s' % e)
else:
    print('UserPromptSubmit' if 'UserPromptSubmit' in hooks else 'no UserPromptSubmit hook in settings.json')")
  is "the skill hook fires on a headless lane" "0 UserPromptSubmit 7 names" "$HOOKRC $EVENT $SKILLHOOK"
fi

# --- the installed instance drifting from the source it was built from -------
# `install-stale` can only run where the source tree lives beside the instance -- the package's own
# checkout -- so the fixture is that layout: the source dirs install.sh reads, installed into
# themselves. A repository that merely INSTALLED the harness has no `harness/**` and the probe
# returns nothing there, which is why the assertions above cannot ask this of "$T".
# `check` is neutralised in the fixture's harness.json before the install: probes.sh runs
# `checkForce`, and this package's own is `./selftest.sh`, so the fixture would re-enter this file.
# Three facts in one string because each is worthless alone. 0 on the freshly installed tree is
# what stops the probe being a permanent finding nobody reads; 2 after one installed file is edited
# and another deleted is what stops it being decoration; the two named FINDINGs are what prove it
# reports the file that drifted rather than a count.
S="$T/selfhost"
mkdir -p "$S" && cd "$S" && git init -q
for p in install.sh harness.default.json harness.json harness roles templates skills evals; do
  cp -R "$SRC/$p" "$S/$p"
done
python3 -c "
import json
c = json.load(open('harness.json'))
c.update({'check': 'true', 'checkForce': 'true', 'driverCommand': ''})
json.dump(c, open('harness.json', 'w'), indent=2)"
"$SRC/install.sh" "$S" >/dev/null 2>&1
FRESH=$(.harness/hooks/probes.sh "$S" 2>&1 | sed -n 's/^PROBE install-stale //p')
printf '\n# edited in the instance and not in the source\n' >>.harness/lib/queue.sh
rm -f .harness/roles/scout.md
STALE=$(.harness/hooks/probes.sh "$S" 2>&1)
DRIFT=$(printf '%s\n' "$STALE" | sed -n 's/^PROBE install-stale //p')
EDITED=$(printf '%s\n' "$STALE" | grep -c '^FINDING install-stale \.harness/lib/queue\.sh:0 .*differs from the source')
DELETED=$(printf '%s\n' "$STALE" | grep -c '^FINDING install-stale \.harness/roles/scout\.md:0 .*does not have it')
is "an installed file that drifted from its source is reported" "0 2 1 1" \
  "$FRESH $DRIFT $EDITED $DELETED"
cd "$T" && rm -rf "$S"

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
