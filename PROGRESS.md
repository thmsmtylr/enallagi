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

## 2026-09-08 — T-001 — landed
rows: `tests/roles.rs::a_declared_role_is_fetched_vendored_and_committed_before_its_stage`, `tests/roles.rs::a_role_whose_vendored_file_drifted_is_refused_under_frozen`
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 297 passed, 0 failed, at 2610220 before the commit
what happened: `[[role]]` parses and validates in config.rs, `Lock.role` pins it, and `roles::resolve` vendors `<path>/<name>.md` to `<harness_dir>/roles/<name>.md`. The skills loop was refactored into `skills::pin`, which both resolvers call, so the semantics match by construction. T-001 moved ready → review.
friction: the row name for the first test carries a clause ("and committed before its stage") that belongs to T-002's scope; the implementer has to decide how much of a row one task may satisfy. A row split at the task boundary would remove the judgment call.
next: the verifier takes T-001. T-002 (pipeline resolves and commits declared roles, immutable hook, scope gate) is blocked on it and extends `tests/roles.rs`.

## 2026-09-08 — T-002 — landed
rows: `tests/roles.rs::an_undeclared_role_falls_back_to_the_installed_or_embedded_file`, `tests/roles.rs::the_immutable_hook_refuses_an_edit_to_a_vendored_role`
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 301 passed, 0 failed, at 898d1eb before the commit
what happened: `role_spawn` resolves a declared role before reading its source, and one `chore(vendor): <ids>` commit carries skills and roles. `gates::scope` exempts a freshly vendored role and rejects a re-cut one; `hooks::immutable` refuses a locked role's file. T-002 moved ready → review.
friction: `roles.rs` sat on the scope line but needed no edit; a scope line that names a file the task never touches is noise the scope gate cannot tell from a forgotten edit.
next: the verifier takes T-002. Nothing else is at ready; the round's four exit rows all have tests once T-002 is done.
