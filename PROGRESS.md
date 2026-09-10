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

## 2026-09-09 — T-013 — landed
rows: none — harness (the gates.rs ponytail marker is lifted, not killed)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 191+0+8+5+9+15+21+29+27+4+0 = 309 passed, 3 ignored, 0 failed, at 9880c5a before the commit
what happened: `gates::skills_dir_for` delegates to `init::skills_root`, which already wraps `skills::skills_dir` and owns the `custom`-preset arm; its own three-branch lookup and the marker are gone, signature kept for hooks.rs:186. `./target/debug/harness probe` → `PROBE ponytail-ceiling 2` (was 3), no line for gates.rs; the TASKS.md:62 line is T-013's own quoted output and leaves at archive. T-013 moved ready → review.
friction: skills.rs sat on the scope line and needed no edit; second occurrence of T-002's "a scope file the task never touches is noise the scope gate cannot tell from a forgotten edit". Due a LEARNINGS.md line, and LEARNINGS.md is off this task's scope.
next: the verifier takes T-013. T-014, T-015 and T-016 are ready with no blockers. Whoever next holds LEARNINGS.md on scope owes the scope-line-noise rule.

## 2026-09-09 — T-014 — landed
rows: none — harness (the shipped templates, roles, skill and READMEs state rules and formats only)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 191+0+8+5+9+15+22+29+27+4+0 = 310 passed, 3 ignored, 0 failed, at 3e59458 before the commit
what happened: every justifying sentence is out of templates/*.md, roles/*.md, the running-the-loop skill, evals/README.md and the adapter READMEs; template comments are at most two lines, the roles are at 30–41 lines (all under 80% of HEAD), and the criterion grep prints nothing. A fresh-init probe reports the same five counts before and after (spec-untested 1, rail-unenforced 2, learning-* 0, queue-hygiene 0). One test added to tests/init.rs encodes the grep and the comment ceiling; probes.rs was untouched. T-014 moved ready → review.
friction: probes.rs sat on the scope line and needed no edit; third occurrence of T-002's and T-013's "a scope file the task never touches is noise the scope gate cannot tell from a forgotten edit". Still owed a LEARNINGS.md line, and LEARNINGS.md is off this task's scope.
next: the verifier takes T-014. T-015 (README.md, docs/intent.md, harness.default.toml) and T-016 (MIT) are ready with no blockers; the citations T-014 dropped from the templates are named in its notes for T-015's `## References` list if wanted.

## 2026-09-09 — T-015 — landed
rows: none — harness (README.md, docs/intent.md and harness.default.toml describe; the criterion grep is empty on both documents)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 191+0+9+5+9+15+22+29+27+4+0 = 311 passed, 3 ignored, 0 failed, at d0d5c77 before the commit
what happened: README.md 359 → 280 lines, harness.default.toml's ten comments are one line each with its four citations moved to docs/intent.md `## References`, and docs/intent.md keeps exactly the eight named headings. One red-then-green test in tests/cli.rs encodes the three document criteria (24 offences at HEAD). `./target/debug/harness probe check-unnamed litter` reads 1 and 1 before and after: AGENTS.md:7 lacks harness.toml's `export PATH` prefix and harness.toml's `docs` array omits test-hashes.json, both off scope. T-015 moved ready → review.
friction: criterion 4 asks for `check-unnamed 0` and `litter 0` from a scope that cannot reach either finding (AGENTS.md, harness.toml); an adjudicator running the named probe before promoting would have seen both at 1 at d0d5c77. First occurrence.
next: the verifier takes T-015. T-016 (MIT licence) is ready with no blockers. AGENTS.md:7 and harness.toml's `docs` list each want a one-line edit under a task that names them.

## 2026-09-10 — T-016 — landed
rows: none — harness (the licence is MIT; NOTICE is gone)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 191+0+9+5+9+16+22+29+27+4+0 = 312 passed, 3 ignored, 0 failed, at 1adfc46 before the commit
what happened: T-015 was first at ready, but its verdict says both open criteria need AGENTS.md, harness.toml or templates/pointer.md, none on scope, and directs the next lane to park it; it is `blocked` with that reason and nothing was re-implemented. T-016: LICENSE is the MIT text, NOTICE is removed, crates/harness/Cargo.toml reads `license = "MIT"` with its test-hashes.json key re-cut (on-scope file, exempt under harness-lane), README's License section is one line, and `the_licence_is_mit` in tests/floor.rs was red at 1adfc46 and is green. `./target/debug/harness probe` reads the same before and after: check-unnamed 1, litter 1 (test-hashes.json), hash-uncovered 0. T-015 moved ready → blocked; T-016 moved ready → review.
friction: a task promoted with criteria its scope cannot reach costs an implement round and a verify round before the queue learns it, and a third lane to park it; second occurrence (the T-015 entry above is the first). Due a LEARNINGS.md line, and LEARNINGS.md is off this task's scope: the adjudicator runs every probe a criterion names before promoting, and every file the fix touches is on the scope line.
next: the verifier takes T-016. T-015 waits at blocked for a scope edit (AGENTS.md, harness.toml, templates/pointer.md) or a narrowed criterion, then `ready`. Nothing else is at ready, so the loop ends on dry rounds unless the scout proposes.

## 2026-09-10 — T-016 — landed
rows: none — harness (re-review after the criterion 2 rejection; no scope file changed)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 191+0+9+5+9+16+22+29+27+4+0 = 312 passed, 3 ignored, 0 failed, at db2005f before the commit
what happened: the verdict's one point reproduced: the first take's note at TASKS.md:193 quoted the old licence's name, so criterion 2's repo-wide grep hit the task's own record. The phrase is reworded, the grep is empty (exit 1), and this entry and the commit avoid the name. T-016 moved ready → review.
friction: a criterion that greps the whole repo for a word is failed by any note, verdict or PROGRESS.md line that quotes the word; second occurrence of the task's own record tripping its check (T-013's ponytail-ceiling line at TASKS.md:62 was the first, 2026-09-09 entry). Due a LEARNINGS.md line, off this task's scope: a repo-wide grep criterion excludes TASKS.md, PROGRESS.md and DECISIONS.md, or the adjudicator writes it so the record can quote it.
next: the verifier takes T-016; the diff against db2005f is TASKS.md and PROGRESS.md only. T-015 waits at blocked for a scope edit or a narrowed criterion. Nothing else is at ready, so the loop ends on dry rounds unless the scout proposes.

## 2026-09-10 — T-015 — landed
rows: none — harness (re-take on the widened scope; check-unnamed 0 and litter 0, the moved citations live only in docs/intent.md)
check: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 191+0+9+5+9+16+22+29+27+4+0 = 312 passed, 3 ignored, 0 failed, at 8f0cb50 before the commit
what happened: AGENTS.md's Commands lines carry harness.toml's check verbatim, harness.toml's `docs` names test-hashes.json with the key re-cut, the pointer template and its four renders drop the claude-code citation, AGENTS.md's comments and .harness/RAILS.md:65 match their templates. The cli.rs document test grew both criteria (red: nine offences at 8f0cb50, the verdict's list). `./target/debug/harness probe` → check-unnamed 0, litter 0 (were 1 and 1). T-015 moved ready → review.
friction: templates/RAILS.md sat on the scope line and needed no edit; fourth occurrence of the scope-line-noise friction (T-002, T-013, T-014), still owed its LEARNINGS.md line, and LEARNINGS.md is off this task's scope.
next: the verifier takes T-015; the diff against 8f0cb50 is the ten scope files plus TASKS.md and PROGRESS.md. Nothing else is at ready, so the loop ends on dry rounds unless the scout proposes.

## 2026-09-10 — T-017 — landed
rows: none — harness (the run archives the task it landed, and `harness tasks archive` runs the same function)
check: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 191+0+10+5+9+16+22+30+27+4+0 = 314 passed, 3 ignored, 0 failed, at b2b9765 before the commit
what happened: the iteration-start archive block became `Loop::archive` and `finish` calls it first, before `Kind::RunEnd`, so a refusal or an `Err` is still a warning and never a halt; `loop.pid` still names this process there, so the live-loop refusal does not fire and the test asserts that. `harness tasks archive` resolves the root the way `cli/probe.rs` does and prints one id per line or `nothing to archive`. Two red-then-green tests (loop.rs, cli.rs). `./target/debug/harness probe` prints byte-identical PROBE lines with the change stashed and unstashed. T-017 moved ready → review.
friction: criterion 4's "no other README change" collides with `tests/cli.rs`'s 280-line README cap, which the file was already at: adding the table row cost the closing paragraph its third line. A criterion that adds a line to a length-capped file says what pays for it. First occurrence.
next: the verifier takes T-017; the diff against b2b9765 is five scope files plus TASKS.md and PROGRESS.md (`archive.rs` and `cli/mod.rs` were on scope and needed no edit — fifth occurrence of the scope-line-noise friction, still owed its LEARNINGS.md line). T-018, T-019 and T-020 are ready with no blockers; T-019's fix is exactly the one `queue-hygiene` finding still open at TASKS.md:72.

## 2026-09-10 — T-018 — landed
rows: none — harness (the implement stage's gate ends the iteration at any status short of review, and the round still records its outcome)
check: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0; per-binary 192+0+10+5+9+16+22+31+27+4+0 = 316 passed, 3 ignored, 0 failed, at ac935f1 before the commit
what happened: `implementer_not_done` matches `review` first and passes it; every other non-`done` status passes with `the implementer left it at <status>` and `skip_rest: true`, and the `done` arm keeps its force-back and commit. `Flow::SkipRest` in `iteration` became a `break` instead of an early `return`, so `promotions` and `task_outcome` still run — that is where the digest's `T-001 ended the iteration at blocked, not done.` comes from. Two red-then-green tests (gates.rs unit over the five statuses plus the missing one, loop.rs end-to-end). T-018 moved ready → review.
friction: the `break` dropped the budget halt in `a_dollar_budget_over_a_cost_nothing_reports_halts`, because the stages it cuts are where `boundary` is checked; the skip arm now calls `self.boundary(false)` before breaking. A gate that shortens an iteration silently shortens every per-stage check the iteration owed. First occurrence.
next: the verifier takes T-018; the diff against ac935f1 is the four scope files plus TASKS.md and PROGRESS.md. T-019 (queue-hygiene on open blocks only) and T-020 (one PROGRESS.md entry per iteration) are ready with no blockers.

## 2026-09-10 — T-019 — landed
rows: none — harness (the scope check reads open blocks; a done block's scope is history)
check: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → EXIT=0; per-binary 192+0+10+5+9+16+22+31+28+4+0 = 317 passed, 3 ignored, 0 failed, at 4df3a61 before the commit
what happened: the scope loop in `queue_hygiene.rs` guards on a four-status allowlist (`ready`, `review`, `blocked`, `needs-spec`) instead of `== done`, so `done`, `proposed`, `deferred` and every archived stub fall through; the finding reads "is open and" and the module doc no longer claims the done case. The duplicate-id, missing-status and dangling-`blockedBy` checks sit above the guard and are untouched. One red-then-green test in tests/probes.rs covers both halves in one queue. `./target/debug/harness probe`, rebuilt each way: `PROBE queue-hygiene 1` with the change stashed (`T-016 is done and its scope NOTICE matches no file`), `PROBE queue-hygiene 0` with it applied. T-019 moved ready → review.
friction: none
next: the verifier takes T-019; the diff against 4df3a61 is the three scope files plus TASKS.md and PROGRESS.md. T-020 (one PROGRESS.md entry per iteration) is the last ready block, with no blockers; after it the queue is dry unless the scout proposes.
