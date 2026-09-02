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

## 2026-09-02 — T-001 — landed
rows: `selftest.sh::no role prompt carries an incident narrative`, `selftest.sh::the trimmed role prompts still pass their evals`
check: `HARNESS_DRIVER=1 HARNESS_EVALS=1 ./selftest.sh` -> 79 ok, 0 skip, 0 FAIL, rc=0.
what happened: removed dated incidents, task-id cross-references, reproduction stories and the
duplicated skills paragraph from the five role prompts. 4671 -> 4244 words. Added a grep that fails
on any of those patterns returning, and a second assertion that runs the three evals against a real
agent, since a prompt trim is a behaviour change no static check can see.
friction: none.
next: teardown.
