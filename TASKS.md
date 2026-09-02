# TASKS

The queue. One block per task, in the order they were written. The loop takes the first `ready`
block whose blockers are all `done` and which is not `attended: true`.

Statuses: `proposed` (the scout's, inert to the loop) · `ready` · `blocked` · `review`
(the implementer's last act) · `done` (only the verifier's, and the launcher re-runs the gate
behind it) · `needs-spec` (the contract does not answer a question the task hit).

Finished blocks archive to DECISIONS.md — `.harness/archive-done.sh` moves them at the top of each
iteration, leaving a stub with the fields the loop still reads. Keep this file small: every
process an iteration spawns re-reads all of it.

## Block format

```
## [T-###] one line, in the finding's own words
scope: src/thing.ts, src/thing.test.ts
blockedBy:
status: ready
rows: <the exit-criteria rows this turns green, character for character, or `none — harness`>
criteria:
  - <objective, and naming the command whose output changes when it is done>
notes: <what a reviewer should scrutinise; the implementer's and the verifier's own words>
```

`T-###` above is written unnumbered on purpose: `## [T-<digits>]` is the exact shape
`loop.sh`'s `ready_unattended` scans for, and it does not know this one is inside a code fence —
a numbered example here is a task the loop will take. `blockedBy:` is empty, `none`, or a
comma-separated list of ids. `attended: true` marks a task
needing a human credential and no launcher auto-selects it.

---

## [T-001] the driver has nothing to drive
scope: driver.sh, harness/driver.example.sh, selftest.sh, README.md
blockedBy: none
status: done
rows: `selftest.sh::the package driver reports shortfalls as FINDING lines`, `selftest.sh::an example driver installs into the harness directory`
criteria:
  - `./driver.sh` installs this package into a throwaway repo, drives one request through the
    installed loop, and prints one `FINDING ` line per shortfall it observes in the tree afterwards.
    It exits 0 whenever it reached the artifact, whatever it found, and non-zero only when it could
    not reach it.
  - `HARNESS_DRIVER=1 driverCommand=./driver.sh` on a fresh install makes `probes.sh` print
    `PROBE driver <n>` instead of `PROBE driver OFF`.
  - `install.sh` writes a runnable `driver.example.sh` into the harness directory, and it exits 0
    finding nothing until someone fills in its three sections.
  - `./selftest.sh` declares both rows' assertions and passes.
notes: |
  `driver.sh` drives four modes through a real install: a lane that never commits, one that writes
  no PROGRESS.md entry, one that edits outside its scope, and one that reaches done with the floor
  red. It judges the PERSISTENT EFFECT — the task's `status:` and `git status --porcelain` read back
  from the tree — never what the loop printed.
  Evidence, 2026-09-02 at f6b7215:
    ./selftest.sh                    -> harness selftest: all assertions passed.
    HARNESS_DRIVER=1 ./selftest.sh   -> harness selftest: all assertions passed.
    ./driver.sh; echo $?             -> 1 FINDING line, exit 0
  The one FINDING it reports today is real and is the point of the task:
    "a lane left its implementation uncommitted and the task still reached done with 2 dirty
     path(s). Nothing the launcher runs reads the working tree; only verifier.md step 0 does, and a
     prompt is not a gate."
  Scrutinise: (a) that the four modes differ in exactly one behaviour each, so a FINDING names one
  cause; (b) that `driver.sh` exits non-zero ONLY when install fails, never on a finding.
  OUT OF SCOPE, noted and not touched (`one-scope`): `harness/loop.sh`'s scope-gate message prints
  `touchedsrc/sneaky.ts` — `${out_of# }` strips the separating space. Needs its own block.

  REJECTED 2026-09-02, verifier, at 0f7ce72.
  Reproduce: `./selftest.sh | grep "the package driver"` ->
    "ok    the package driver reports shortfalls as FINDING lines (skipped: HARNESS_DRIVER unset)"
  The default run prints `ok` for an assertion it did not execute. A gate asserts what it EXECUTED,
  never merely that nothing failed (LEARNINGS.md, zero-as-pass) — and the exit-criteria row named by
  that assertion reads green on a run where nothing drove anything. This is the exact defect the
  `driver` probe was built to avoid one level down, reintroduced one level up.
  To fix: a skipped assertion must not print `ok`. Print it as a skip, count skips separately, and
  say so in the summary line so a reader cannot mistake the run for complete.
  Everything else verified: tree clean, every changed file on the scope line or bookkeeping, no test
  weakened (the one deleted line is the `env -u HARNESS_DRIVER` strengthening), both row assertions
  present and spelled as SPEC.md spells them, `driver.sh` exits non-zero only when install fails.

  Rejection addressed 2026-09-02. The one point was: a skipped assertion printed `ok`.
  `selftest.sh` now has a `skip()` beside `ok()`/`bad()`; the driver assertion calls it with the
  reason; the summary line counts skips and names the flag that runs them. Evidence:
    ./selftest.sh                  -> "skip  the package driver reports shortfalls as FINDING lines"
                                      "all assertions that RAN passed, and 1 did not run."
    HARNESS_DRIVER=1 ./selftest.sh -> 50 ok, 0 skip, "all assertions passed.", rc=0

  VERIFIED 2026-09-02, second pass, at 241e29b. Commands run: `git status --porcelain` (0 lines),
  `./selftest.sh` (the rejection point, now "skip"), `HARNESS_DRIVER=1 ./selftest.sh` (50 ok, 0 skip,
  rc=0), `git diff --name-only HEAD~3 HEAD` (every file on the scope line or bookkeeping),
  `.harness/hooks/check-gate.sh` (rc=0). Noted, not a rejection: the gate passes on a run carrying a
  labelled skip. The skip is named in the summary and `HARNESS_DRIVER=1` is the complete floor.

## [T-002] a lane cannot be isolated from the checkout it runs in
scope: harness/worktree.sh, selftest.sh, README.md
blockedBy: none
status: done
rows: `selftest.sh::a worktree lane leaves the parent checkout untouched`, `selftest.sh::a lane that cannot fast forward is left for a human`
criteria:
  - `.harness/worktree.sh [iterations]` creates a git worktree on its own branch, runs `loop.sh`
    inside it, and the parent checkout's working tree and HEAD are unchanged while it runs.
  - On success it fast-forwards the parent branch to the lane branch and removes the worktree.
  - When the parent branch has moved and the merge cannot fast-forward, it leaves the branch and the
    worktree in place, prints what a human has to do, and exits non-zero. It never forces a merge.
  - `./selftest.sh` declares both rows' assertions and passes.
notes: |
  `.harness/worktree.sh [iterations]`: `git worktree add -b lane/<stamp>`, run `loop.sh` in there,
  then `git merge --ff-only` and `git worktree remove`. Three refusals, each deliberate — a detached
  parent HEAD, a lane with uncommitted work (a terminated iteration leaves exactly that, and
  `implementer.md` says finish it, never restart it), and a parent that moved so the merge would be
  a merge commit. None of them is forced; the last one prints the three commands a human chooses
  between.
  Evidence, 2026-09-02: `./selftest.sh` -> six new assertions, all ok.
  Scrutinise: the isolation assertion does not read the parent AFTERWARDS, which would prove
  nothing. The fixture lane records the parent's HEAD and `git status --porcelain` count from INSIDE
  the worktree while it works, commits that recording, and the assertion reads it back after the
  merge — so it is evidence about the moment the lane was running.
  OUT OF SCOPE, noted and not touched (`one-scope`): `install.sh`'s header comment lists the files
  it writes into the harness directory and now misses `worktree.sh` and `driver.example.sh`.

  VERIFIED 2026-09-02 at 74b7f64. `git status --porcelain` 0 lines; `git diff --name-only HEAD~1 HEAD`
  is the scope line plus bookkeeping; both row assertions declared in `selftest.sh`; `git merge` is
  `--ff-only` and nowhere else, `worktree remove` runs only after a successful merge and never with
  `--force`, the branch delete is `-d` and not `-D`; `.harness/hooks/check-gate.sh` rc=0.

## [T-003] nothing tests a role prompt
scope: evals/**, selftest.sh, README.md
blockedBy: none
status: review
rows: `selftest.sh::the eval runner passes a role that obeys its rule`, `selftest.sh::the eval runner fails a role that breaks its rule`
criteria:
  - `evals/run.sh` runs one eval per directory under `evals/`: each carries a `setup.sh` that builds
    a fixture repo, a `prompt.txt` that is the request, and an `assert.sh` that exits 0 when the role
    obeyed its rule. It prints `EVAL <name> PASS|FAIL` per eval and exits non-zero if any failed.
  - It spawns the agent through `harness.json`'s `agentCommand`, in the fixture repo, one fresh
    process per eval — the same isolation the loop's stages get.
  - With no agent configured for evals it refuses and says so, rather than reporting a pass.
  - At least one eval exists per queue-gating role: scout, adjudicator, verifier.
  - `./selftest.sh` declares both rows' assertions and passes.
notes: |
  `evals/run.sh` plus three evals, one per queue-gating role. Each runs in a throwaway repo with the
  harness freshly installed and one fresh agent process — the same isolation the loop's stages get.
  Evidence, 2026-09-02:
    ./evals/run.sh                  -> EVAL adjudicator PASS / EVAL scout PASS / EVAL verifier PASS, rc=0
                                       (a real `claude -p` per eval, from harness.json's agentCommand)
    ./selftest.sh                   -> the three runner assertions ok
    EVAL_AGENT=<an agent that does nothing> ./evals/run.sh verifier -> EVAL verifier FAIL, rc=1
  Scrutinise: the runner is asserted in `selftest.sh` with stubs and never spawns a real agent there;
  the evals themselves need one. And `evals/verifier/setup.sh` asserts its own edit landed — a
  fixture whose replace matches nothing would measure a state it never built and pass.
