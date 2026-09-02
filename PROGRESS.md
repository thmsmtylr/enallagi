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
rows: the four gate rows and the two probe rows
check: `./selftest.sh` -> all assertions that ran passed, 1 skipped (driver, HARNESS_DRIVER unset).
what happened: `evals/run.sh --gate <name>` decides a candidate rule on three conditions — fixes its
case, fails without itself, regresses nothing. `learning-ungated` reports dated rules with no eval
and a library over its cap. Both are grounded in fetched sources, cited in README and RAILS.md.
friction: the new rail named `evals/run.sh`, which no installed repo has, so `rail-unenforced` went
from 2 to 3. A rail must name enforcement that exists where the rail is read, not where it was
written. Raised as T-003 rather than widened into T-002's scope.
next: T-003, install the gate into target repos.

## 2026-09-02 — T-003 — landed
rows: none — harness
check: `./selftest.sh` -> all assertions that ran passed, 1 skipped.
what happened: `install.sh` now writes `evals/run.sh` and `evals/README.md` into the target repo, so
the gate exists where the rules are written. The rail names the probe and the verifier, not the
gate command: nothing runs the gate automatically and `rail-unenforced` reported that correctly.
friction: installing a new directory into a target repo made the `litter` probe report it, because
`allowedPrefixes` did not know it existed. Any change that adds a directory to an install has to
change the allowlist in the same task.
next: verify, then teardown.

## 2026-09-02 — round 3 verified — T-001, T-002, T-003
rows: all six
check: probes.sh -> spec-untested 0, queue-uncovered 0, learning-ungated 0, check-red 0, litter 0.
what happened: the friction-to-eval link is closed. A repeated friction produces a candidate rule; the
gate decides it on three conditions; the probe reports rules that skipped the gate and a library over
its cap. Every condition is cited to a fetched source, and the one condition with no precedent in
those sources (ablation) is marked as ours.
friction: none new.
next: teardown.
