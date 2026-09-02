# PROGRESS

The loop's own record, one entry per iteration, newest last. Required by the `one-row` rail: write
it at the end of every iteration; re-read its **tail** — with SPEC.md and `git log --oneline -20` —
at the start of the next.

TASKS.md holds the task's record in its `notes:`. This file holds the run's: what a fresh process
needs in order not to repeat the last one. Per-bug accuracy falls 58.9% → 36.5% when an agent
inherits its own prior state rather than a clean one, and multi-turn degradation averages 39% —
a short written handoff mitigates this; a long one reproduces it.

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
stays here. The **second** occurrence of the same thing becomes a line in LEARNINGS.md — one rule per surprise rewrites the operating manual every week, which costs more than the friction it removes.

---

## 2026-09-02 — T-001, T-002 — landed
rows: all four
check: `HARNESS_DRIVER=1 HARNESS_EVALS=1 ./selftest.sh` -> 83 ok, 0 skip, 0 FAIL, rc=0.
what happened: TASKS.md is parsed once, in harness/tasks.py, with 15 assertions of its own. The
launcher went 632 -> 252 lines and sources queue.sh, agent.sh and gates.sh. Fence-aware parsing
means a task heading inside a code block is documentation, which is the class of bug that made the
template's own example a takeable task.
friction: moving gate_verdict out of loop.sh made `rail-unenforced` report that nothing runs
check-gate.sh — correctly, because the probe's WIRED list named loop.sh and knew nothing about
lib/. A probe that decides what counts as "the harness" has to be told when the harness moves.
Second: the gated floor now takes about 50 minutes, most of it three live-agent evals run in
series. That is too slow to be a routine check and it is why both agent-spawning sections are
behind flags. Running the evals in parallel is the obvious fix and is not done.
next: teardown.
