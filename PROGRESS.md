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

## 2026-09-08 — T-003 — landed
rows: none — harness (the probe's path rule; the four §11 rows already had tests at T-002)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 308 passed, 3 ignored, 0 failed, at a55bcf6 before the commit
what happened: `untested_rows` tries `<source_root>/<name>` before taking a slashed name verbatim, so the SPEC.md §12 convention and the probe agree; `harness probe` → `PROBE spec-untested 0` (was 4). One test added to tests/probes.rs. T-003 moved ready → review.
friction: `harness` on PATH is a symlink to `target/release/harness`, so a criterion phrased as "`harness probe` emits no line" is only observable after `cargo build --release`; the debug build a test run produces does not update it. A criterion naming `./target/debug/harness probe` or a note in the dogfood skill would remove the surprise.
next: the verifier takes T-003. T-005 (test-hashes.json) is the next ready task with no blockers; T-006 waits on it. T-007..T-009 and T-013 are ready too.

## 2026-09-08 — T-005 — landed
rows: none — harness (`tests-immutable` now has its reference file)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 308 passed, 3 ignored, 0 failed, at 65e1cd3 before the commit
what happened: `test-hashes.json` exists with one key, `crates/harness/tests/roles.rs`, cut with `shasum -a 256` from the shell. The floor test fails on a zero digest and passes on the real one; `./target/debug/harness probe` → `PROBE rail-unenforced 0`. T-005 moved ready → review.
friction: none
next: the verifier takes T-005. T-006 unblocks once it is done and adds the `harness.toml`, `Cargo.toml` and `crates/harness/Cargo.toml` keys from the shell (the edit tool is refused on the file now). T-007..T-009 and T-013 remain ready.

## 2026-09-08 — T-006 — landed
rows: none — harness (`harness-immutable` now has keys for every file it names)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 308 passed, 3 ignored, 0 failed, at c529b4d before the commit
what happened: `test-hashes.json` gained keys for `harness.toml`, `Cargo.toml` and `crates/harness/Cargo.toml`, cut with `shasum -a 256` from the shell; the roles.rs key kept its value. `./target/debug/harness probe` → `PROBE hash-uncovered 0` (was 1) and `PROBE rail-unenforced 0`. T-006 moved ready → review.
friction: none
next: the verifier takes T-006. T-007..T-009 (one `gate =` line each in harness.default.toml) and T-013 (fold `skills_dir_for` into `skills::skills_dir`) are ready with no blockers.

## 2026-09-08 — T-006 — landed
rows: none — harness (re-review after a scope-gate rejection; the file did not change)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 308 passed, 3 ignored, 0 failed, at 61f8600 before the commit
what happened: the gate's "re-cut crates/harness/tests/roles.rs" was a false positive: the value is identical at 2ee7163 and 58ad8db, but `recut_keys` (gates.rs:502) matches keys on a text diff and the line gained a trailing comma when the keys before it were added. Nothing to re-implement; test-hashes.json is untouched this iteration, the criteria re-run green, and the answer is in the task's notes. T-006 moved ready → review.
friction: the scope gate counts a `test-hashes.json` key as re-cut when only its line's punctuation changed (gates.rs:502 reads `git diff` text, not values); a compare of the key's value at base and HEAD would remove the false rejection. gates.rs is off this task's scope, so it stays a finding.
next: the verifier takes T-006 again; the diff against 61f8600 carries only TASKS.md and PROGRESS.md. T-007..T-009 and T-013 are ready with no blockers.

## 2026-09-08 — T-007 — landed
rows: none — harness (the tdd skill now names the probe that fails without it)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 308 passed, 3 ignored, 0 failed, at 9b6c821 before the commit
what happened: the tdd entry in harness.default.toml reads `gate = "spec-untested"`; nothing else in the file changed. `./target/debug/harness probe` → `PROBE skill-ungated 2` (was 3), the two left being T-008's and T-009's. T-007 moved ready → review.
friction: none
next: the verifier takes T-007. T-008 and T-009 are the same one-line shape (`rejection-repeat`, `verdict-flip`) on the same file, each rebuilt with `cargo build -q` before the probe is read; T-013 (fold `skills_dir_for` into `skills::skills_dir`) is ready too.

## 2026-09-08 — T-008 — landed
rows: none — harness (the review-received skill now names the probe that fails without it)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 308 passed, 3 ignored, 0 failed, at 0d590d0 before the commit
what happened: the review-received entry in harness.default.toml reads `gate = "rejection-repeat"`; nothing else in the file changed. `./target/debug/harness probe` → `PROBE skill-ungated 1` (was 2), the one left being T-009's. T-008 moved ready → review.
friction: none
next: the verifier takes T-008. T-009 is the same one-line shape (`verdict-flip`, harness.default.toml:232) on the same file, rebuilt with `cargo build -q` before the probe is read; T-013 (fold `skills_dir_for` into `skills::skills_dir`) is ready too.

## 2026-09-09 — T-009 — landed
rows: none — harness (the review-requested skill now names the probe that fails without it)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 308 passed, 3 ignored, 0 failed, at 59e31df before the commit
what happened: the review-requested entry in harness.default.toml reads `gate = "verdict-flip"`; nothing else in the file changed. The edit was sitting uncommitted in the tree from a lane cut off before its commit; this lane finished it rather than restarting. `./target/debug/harness probe` → `PROBE skill-ungated 0` (was 1). T-009 moved ready → review.
friction: a lane was terminated after the edit and before the commit, leaving a one-line change with the task still at `ready`; the next lane had to infer from `git diff` which task owned it. Second occurrence of the LEARNINGS.md `[seed]` "uncommitted is lost" entry's shape, already a rule; nothing new to add.
next: the verifier takes T-009. `skill-ungated` is at 0. T-013 (fold `skills_dir_for` into `skills::skills_dir`), T-014, T-015 and T-016 are ready with no blockers.

## 2026-09-09 — T-009 — landed
rows: none — harness (re-review after a verdict-gate rejection; the file did not change)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit 0, 308 passed, 3 ignored, 0 failed, at a0980d0 before the commit
what happened: the gate forced T-009 back to ready with "no failure could be named" after a 3-second red at 6fe5898 whose output the launcher did not keep. The full check, the verify-done hook and the lib tests three times are all green at a0980d0, and every criterion re-runs green; nothing to re-implement, the answer is in the task's notes. T-009 moved ready → review.
friction: a red the gate cannot name is unanswerable: `fail_name` in harness.toml matches `test … FAILED`, which `cargo test -q` never prints (it prints `<name> --- FAILED`), and `verdict` (gates.rs:243-246) drops the check output from the reason it writes. Either `fail_name` matching the `failures:` list or the output's tail in the reason would have let this lane answer the rejection instead of re-running blind. First occurrence; harness.toml and gates.rs are off this task's scope.
next: the verifier takes T-009 again; the diff against a0980d0 carries only TASKS.md and PROGRESS.md. T-013, T-014, T-015 and T-016 are ready with no blockers.

## 2026-09-09 — T-009 — BLOCKED
rows: none — harness (the file did not change; the second verdict-gate rejection is answered in the task's notes)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 3 ignored, 0 failed, at 0620606 before the commit
what happened: BLOCKED: no `done` can pass the verdict gate under this launcher process. `harness run` (pid 44909) was started as `( harness run … ) &` from a non-interactive `zsh -c`, which leaves SIGINT ignored in every child; the gate's check inherits it and `agent::tests::a_signalled_child_reports_128_plus_the_signal` (agent.rs:607) gets exit 0 for `kill -INT $$` instead of 130, 2.4 s in, and `cargo test -q` prints `<name> --- FAILED`, which `fail_name` cannot name. Reproduced both ways in the task's notes; the same command in the foreground under the launcher's own env is green, 308 passed. Start the launcher in the foreground, or make the test use a signal background jobs do not ignore (agent.rs:609, `kill -TERM $$` → 143); both are off T-009's scope. STOP is already in the tree (07:58, operator session), so the run halts here. T-009 moved ready → review with every criterion re-run green.
friction: second occurrence: a red the gate cannot name is unanswerable: `fail_name` in harness.toml matches `test … FAILED`, which `cargo test -q` never prints (it prints `<name> --- FAILED`), and `verdict` (gates.rs:243-246) drops the check output from the reason it writes. Either `fail_name` matching the `failures:` list or the output's tail in the reason would have named `a_signalled_child_reports_128_plus_the_signal` two rounds ago instead of two lanes re-running blind. Due a LEARNINGS.md line; harness.toml, gates.rs and LEARNINGS.md are off this task's scope.
next: restart `harness run` in the foreground (not as a `&` job of a non-interactive shell), then the verifier takes T-009 at review and the gate runs with SIGINT at default. T-013, T-014, T-015 and T-016 are ready with no blockers; T-013 has gates.rs on scope and is the first place the check output's tail can reach the verdict reason.

## 2026-09-09 — T-009 — landed
rows: none — harness (the file did not change; the third verdict-gate rejection is answered in the task's notes)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 191+0+8+5+9+15+21+29+27+4+0 = 309 passed, 3 ignored, 0 failed, at 3b06ecc before the commit
what happened: the gate named the red this time (`a_signalled_child_reports_128_plus_the_signal`, 190 passed; 1 failed): the launcher (pid 46070) is again a `&` job of a non-interactive `zsh -c` (parent 46069), so its check runs with SIGINT ignored and HEAD's `kill -INT $$` stub exits 0 instead of 130. The operator's uncommitted agent.rs edit (`kill -KILL $$`, asserts 137) passes as a background job of a non-interactive shell, 1 passed; it is off scope and not staged here. Every T-009 criterion re-runs green; T-009 moved ready → review.
friction: second occurrence of the launcher started as `( harness run … ) &` from a non-interactive shell, which ignores SIGINT in the gate's check. The uncommitted agent.rs edit removes the test's dependence on the inherited disposition, so once it lands there is no rule left to gate; no LEARNINGS.md line is proposed.
next: commit the agent.rs edit (operator's; off every queued task's scope) before the verifier takes T-009, or the verdict gate rejects on a dirty tree. STOP is in the tree, so the run halts here. T-013, T-014, T-015 and T-016 are ready with no blockers.
