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
rows: `selftest.sh::a role with its own agent command is spawned with it`, `selftest.sh::a role with no agent command falls back to the default`
check: `./selftest.sh` -> all assertions that ran passed, 1 skipped (driver, HARNESS_DRIVER unset).
what happened: agentCommand now takes an object keyed by role as well as a word list. install.sh
renders one token per role; loop.sh selects by stage. No change to the stage prompts or the gates.
friction: none.
next: T-002, run log and budget.
