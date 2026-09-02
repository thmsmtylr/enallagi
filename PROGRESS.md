# PROGRESS

The loop's own record, one entry per iteration, newest last. Required by the `one-row` rail: write
it at the end of every iteration; re-read its **tail** — with SPEC.md and `git log --oneline -20` —
at the start of the next.

TASKS.md holds the task's record in its `notes:`. This file holds the run's: what a fresh process
needs in order not to repeat the last one. Per-bug accuracy falls 58.9% → 36.5% when an agent
inherits its own prior state rather than a clean one, and multi-turn degradation averages 39% —
a short written handoff is the mitigation, and a long one is the thing it mitigates.

**Keep entries short and keep the newest `next:` true.** This file grows without bound and the
loop reads only its tail; anything a future iteration must not lose belongs in LEARNINGS.md or in
the task's own `notes:`, not buried here.

## Entry format

```
## <date> — <task id> — <landed | BLOCKED | rejected>
rows: <the exit-criteria rows this iteration turned green, or none>
check: <the exact command and its result>
what happened: <two or three sentences>
friction: <one thing that cost time and a rule or check could prevent, or none>
next: <what the following iteration inherits>
```

`BLOCKED` carries a written reason and stops the loop. That is a success, not a failure
(`blocked-is-allowed`). A row is never recorded green without the exact command and its pasted
output.

`friction:` is the harness loop's only intake. The first occurrence of something is evidence and
stays here. The **second** occurrence of the same thing becomes a line in LEARNINGS.md — a rule
per surprise is how a harness rewrites its operating system every week and gets worse.

---

## 2026-09-02 — T-001 — landed
rows: `selftest.sh::the package driver reports shortfalls as FINDING lines`, `selftest.sh::an example driver installs into the harness directory`
check: `./selftest.sh` -> all assertions passed. `HARNESS_DRIVER=1 ./selftest.sh` -> all assertions passed.
what happened: wrote `driver.sh`, this package's own driver, and `harness/driver.example.sh`, the
skeleton every install now carries. The driver installs the harness into four throwaway repos and
drives one loop iteration through each with a lane that is sloppy in exactly one way, then reads the
task's status and the working tree back out. It found one real hole on its first run: a lane that
never commits still reaches `done`, because nothing the launcher runs looks at `git status`.
friction: a bash branch ending in `[ test ] && action` exits non-zero when the test is false; the
launcher reads a non-zero lane as a halt, so the verify stage never ran and the driver reported
nothing. Cost one debug cycle to find. Use `if ... fi` for the last statement of a branch.
next: T-002 (worktree isolation). The scope-gate message bug noted in T-001's `notes:` needs its own
block — it is `harness/loop.sh` and was out of T-001's scope.

## 2026-09-02 — T-001 — landed (second attempt, after a rejection)
rows: `selftest.sh::the package driver reports shortfalls as FINDING lines`, `selftest.sh::an example driver installs into the harness directory`
check: `HARNESS_DRIVER=1 ./selftest.sh` -> 50 ok, 0 skip, all assertions passed, rc=0.
what happened: the verifier rejected the first attempt because `./selftest.sh` printed `ok` for an
assertion it had skipped, which would have shown an exit-criteria row green on a run that drove
nothing. Added `skip()` beside `ok()`/`bad()`, a skip count, and a summary line that names the flag.
friction: the gated-assertion pattern reintroduced zero-as-pass one level up from the probe that
exists to prevent it. A skipped check that prints like a passing one is the failure the whole
harness is built around, and it took a verifier pass to catch.
next: T-002 (worktree isolation).

## 2026-09-02 — T-002 — landed
rows: `selftest.sh::a worktree lane leaves the parent checkout untouched`, `selftest.sh::a lane that cannot fast forward is left for a human`
check: `./selftest.sh` -> all assertions that ran passed, 1 skipped (the driver, HARNESS_DRIVER unset).
what happened: added `harness/worktree.sh`. It composes rather than complicating `loop.sh` — the
loop needed no change at all. Six assertions: isolation while the lane runs, the fast-forward back,
removal after merge, and the three-way refusal when the parent has moved.
friction: none.
next: T-003 (evals for the role prompts).

## 2026-09-02 — T-003 — landed
rows: `selftest.sh::the eval runner passes a role that obeys its rule`, `selftest.sh::the eval runner fails a role that breaks its rule`
check: `./selftest.sh` -> all assertions that ran passed, 1 skipped. `./evals/run.sh` -> three PASS, rc=0.
what happened: added `evals/` — a runner and one eval per queue-gating role. All three prompts hold
under a live agent: the scout transcribed findings and promoted nothing, the adjudicator killed an
unanchored block unread and wrote its line to DECISIONS.md, and the verifier rejected a task whose
implementation was never committed. That last one is the hole `driver.sh` found in T-001, so the
prompt that closes it is now held to its word by a test.
friction: none new. The repeat from T-001 (a check that can be skipped must not print like one that
passed) hit again in an eval fixture and is now a LEARNINGS.md line, per the two-occurrence rule.
next: the three skipped items are in. Remaining: T-004 for the two out-of-scope notes T-001 and T-002
left, then strip this dogfood install back out so the package ships as a starter harness.
