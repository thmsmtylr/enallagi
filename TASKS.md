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
status: ready
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
notes:

## [T-002] a lane cannot be isolated from the checkout it runs in
scope: harness/worktree.sh, selftest.sh, README.md
blockedBy: none
status: ready
rows: `selftest.sh::a worktree lane leaves the parent checkout untouched`, `selftest.sh::a lane that cannot fast forward is left for a human`
criteria:
  - `.harness/worktree.sh [iterations]` creates a git worktree on its own branch, runs `loop.sh`
    inside it, and the parent checkout's working tree and HEAD are unchanged while it runs.
  - On success it fast-forwards the parent branch to the lane branch and removes the worktree.
  - When the parent branch has moved and the merge cannot fast-forward, it leaves the branch and the
    worktree in place, prints what a human has to do, and exits non-zero. It never forces a merge.
  - `./selftest.sh` declares both rows' assertions and passes.
notes:

## [T-003] nothing tests a role prompt
scope: evals/**, selftest.sh, README.md
blockedBy: none
status: ready
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
notes:
