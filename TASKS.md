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

## [T-001] one agent command for every role
scope: harness/loop.sh, install.sh, harness.default.json, selftest.sh, README.md
blockedBy: none
status: review
rows: `selftest.sh::a role with its own agent command is spawned with it`, `selftest.sh::a role with no agent command falls back to the default`
criteria:
  - `agentCommand` in harness.json accepts an object keyed by role (`default`, `scout`,
    `adjudicator`, `implementer`, `verifier`) as well as the existing word list. A word list keeps
    working unchanged.
  - Each stage spawns the command for its own role, falling back to `default`.
  - `DRY_RUN=1 .harness/loop.sh 1` prints which command each stage would spawn.
  - `./selftest.sh` declares both rows' assertions and passes.
notes: |
  install.sh emits one __AGENT_COMMAND_<ROLE>__ token per known role, filling any the config does
  not name with default, so loop.sh can reference all five and the leftover-token check still
  applies. loop.sh selects with a case, no eval.
  Evidence 2026-09-02: `DRY_RUN=1 .harness/loop.sh 1` -> "as role verifier via ./src/fakeverifier.sh"
  and "as role implementer via ./src/fakeagent.sh" with one config naming only default and verifier.
  Scrutinise: __AGENT_BINARY__ is now a pipe-joined alternation of every configured binary, because
  archive-done.sh passes it to `pgrep -f`, which takes an ERE.

## [T-002] the loop reports no cost and enforces no budget
scope: harness/loop.sh, harness.default.json, selftest.sh, README.md
blockedBy: none
status: review
rows: `selftest.sh::every spawned stage appends one record to the run log`, `selftest.sh::the loop stops before a stage that would exceed the budget`
criteria:
  - Every spawned stage appends one tab-separated record to `.harness/run.log`: ISO timestamp,
    iteration, role, task id, seconds, exit code, and the agent's reported cost where the output
    carries one.
  - `BUDGET_SECONDS` and `BUDGET_USD` halt the loop before the next stage when the run total has
    reached either. Zero or unset means no limit.
  - The digest prints total wall clock, total cost where known, and per-role totals.
  - `./selftest.sh` declares both rows' assertions and passes.
notes: |
  Wall clock is portable and always recorded; cost is not, so it is extracted from the agent's own
  output with `costSed` and left empty when nothing matches. Budgets are checked at stage
  boundaries, never inside a stage.
  Evidence 2026-09-02: one iteration with a lane reporting `{"total_cost_usd": 0.5}` wrote two
  records; with `BUDGET_USD=0.4` the run halted after the first and the second stage never spawned
  (one record, and `HALT: the run has spent` in the output).
  Scrutinise: `.harness/run.log` had to become git-ignored. Written into the repo it was staged by
  any lane running `git add -A`, which then failed the scope gate for a file the lane did not write,
  and made the parent checkout dirty during a worktree run. install.sh writes
  `.harness/.gitignore`, never the repository's own.

## [T-003] the shipped prose is written for an audience that is not the reader
scope: README.md, roles/*.md, templates/*.md, skills/**, harness/*.sh, harness/hooks/*.sh, install.sh, selftest.sh, driver.sh, evals/**
blockedBy: none
status: review
rows: `selftest.sh::no shipped file carries rhetorical filler`
criteria:
  - `./selftest.sh` greps every shipped file for a written list of rhetorical patterns and fails on
    a hit. The list covers antithesis ("is not X, it is Y"), appeals to the point ("the whole
    point", "that is the trick"), and self-congratulation ("beautifully", "elegantly").
  - Every hit in the current tree is rewritten to state the fact, keeping every citation, measured
    figure and file reference intact.
  - No claim, source or number is lost in the rewrite: `git diff` shows prose changes only.
notes: |
  16 hits in the first scan. Each was replaced by the fact it was decorating, and where the fact was
  a measurement the citation moved with it: the roles table's "one agent with extra steps" became
  SpecBench's 43-48pp visible-versus-held-out gap (arXiv:2605.21384), and loop.sh's "this is the
  whole trick" became ChainSWE's 58.9% -> 36.5% (arXiv:2607.27283).
  The check greps the package source, not the throwaway install, so it covers what ships.
  Scrutinise: the pattern list was narrowed after the first draft. `is not the ` and `and that is`
  matched sentences carrying real information ("a testcase existing is not the check"), so the
  automated list now covers unambiguous filler only and the ambiguous cases were rewritten by hand.
  Evidence 2026-09-02: `./selftest.sh` -> "no shipped file carries rhetorical filler" ok.
