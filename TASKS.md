# TASKS

<!-- One block per task. status: proposed | ready | review | done | blocked | needs-spec | deferred -->

## [T-001] the [[role]] table, its lock entries, and a resolver that fetches and vendors a role
scope: crates/harness/src/config.rs, crates/harness/src/skills.rs, crates/harness/src/roles.rs, crates/harness/src/lib.rs, crates/harness/harness.default.toml, crates/harness/tests/roles.rs
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show ec81489:TASKS.md`

## [T-002] the pipeline resolves and commits declared roles, and the immutable hook covers them
scope: crates/harness/src/pipeline.rs, crates/harness/src/hooks.rs, crates/harness/src/roles.rs, crates/harness/src/gates.rs, crates/harness/tests/roles.rs, crates/harness/tests/loop.rs, README.md
blockedBy: T-001
status: done
archived: DECISIONS.md — full block at `git show b1cc210:TASKS.md`

## [T-003] no file tests/roles.rs for criterion tests/roles.rs::a_declared_role_is_fetched_vendored_and_committed_before_its_stage
scope: crates/harness/src/probes/spec_untested.rs, crates/harness/tests/probes.rs
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show 8baf510:TASKS.md`

## [T-005] .harness/RAILS.md:57 enforced by test-hashes.json, which does not exist
scope: test-hashes.json
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show f342583:TASKS.md`

## [T-006] .harness/RAILS.md:58 enforced by test-hashes.json, which does not exist
scope: test-hashes.json
blockedBy: T-005
status: done
archived: DECISIONS.md — full block at `git show 7559ba6:TASKS.md`

## [T-007] tdd is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show 054a985:TASKS.md`

## [T-008] review-received is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show 116893e:TASKS.md`

## [T-009] review-requested is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: review
gate: the verifier returned done and the gate was red at d86c4f8. no failure could be named
gate: the verifier returned done and the gate was red at 6fe5898. no failure could be named
probe: skill-ungated
rows: none — harness
command: `harness probe`
output: |
  PROBE skill-ungated 3
  FINDING skill-ungated harness.toml:0 review-requested is declared with gate: none -- nothing fails without it, so relying on it is a hope
criteria:
  - the `[[skill]]` entry `id = "review-requested"` in crates/harness/harness.default.toml reads `gate = "verdict-flip"`; its `source`, `path`, `rev` and `why` are unchanged, and no other entry changes
  - `harness probe` emits no `skill-ungated` line naming `review-requested` (today: 1)
  - `cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate` passes
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  proposed from the output above on 2026-09-08. Not adjudicated.
  2026-09-08 adjudicator: promoted. Reproduced. `verdict-flip` (probes/telemetry.rs:95) reports a
  verdict that flips between passes over the same task, which is what a verifier reading the diff
  for whatever stands out looks like from the event log, and a checklist is what stops it. The
  embedded default is the entry the probe reads here; harness.toml is off scope.
  2026-09-09 implementer: one line, harness.default.toml:232 `gate = "none"` → `gate = "verdict-flip"`;
  `git diff --stat` → 1 file changed, 1 insertion(+), 1 deletion(-). The edit was already in the tree
  uncommitted when this lane started (a prior lane cut off mid-flight); finished, not restarted.
  Red is the previous iteration's recorded probe at 0d590d0: `PROBE skill-ungated 1`, the FINDING
  naming review-requested. After `cargo build -q`: `./target/debug/harness probe | grep skill-ungated`
  → `PROBE skill-ungated 0`, no line names review-requested.
  `cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate` → 1 passed.
  Reviewer: confirm the diff is that one line and that `verdict-flip` is a probe name
  (probes/mod.rs:71, 98; telemetry.rs:95); the probe must be read from a fresh `cargo build`, not the
  `harness` symlink.
  Gate: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; per-binary 190+8+5+9+15+21+29+27+4 = 308 passed, 3 ignored, 0 failed.
  Commit: `git add crates/harness/harness.default.toml TASKS.md PROGRESS.md && git commit -q -m
  "feat(config): T-009 review-requested names verdict-flip as its gate"` → 6ae2827 (amended to carry
  these lines); `git status --porcelain` → empty.
  2026-09-09 verifier: VERIFIED. `git status --porcelain` → empty; the change is on the branch at b3afb37
  (6ae2827 in the note above is the pre-amend SHA). Base: origin/main is an ancestor, so the task diff is
  `git diff HEAD~1`: PROGRESS.md, TASKS.md, crates/harness/harness.default.toml; the toml hunk is exactly
  `-gate = "none"` / `+gate = "verdict-flip"` at line 232, 1 insertion 1 deletion, source/path/rev/why
  untouched. test-hashes.json and .check-baseline are not in the diff; their delta against origin/main is
  T-005/T-006 (2ee7163, 58ad8db), already done. `verdict-flip` is a probe name (probes/mod.rs:71, 98).
  `cargo build -q` then `./target/debug/harness probe | grep skill-ungated` → `PROBE skill-ungated 0`;
  `grep -c review-requested` over the probe output → 0.
  `cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate` → 1 passed,
  0 failed; the test asserts 7 skills and no "names gate" finding, so the name is checked against the
  probe list, not merely against "none".
  `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; per-binary 190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 3 ignored, 0 failed; .check-baseline is
  empty, no failure to match. PROGRESS.md entry carries `friction:`; it cites the LEARNINGS.md `[seed]`
  uncommitted-is-lost rule (LEARNINGS.md:24) as its second occurrence, already a rule, no repeat to file.
  Ponytail: one config line, nothing to cut. Scratch-dir probe with a bogus gate name was not run (shell
  call denied); the second branch of skill_ungated.rs is covered by the test above.
  2026-09-09 implementer (after the gate): the rejection point is answered, not re-implemented. The
  gate's red at 6fe5898 lasted 3 s (events.jsonl seq 118 stage.end 07:46:52 → seq 120 task.status
  07:46:55) and named nothing; `check_delta` keeps `report.output` but `verdict` (gates.rs:243-246)
  writes only `unforgiven` into the reason, so the output is gone. It does not reproduce at a0980d0,
  the same tree plus that one TASKS.md commit:
    $ echo '{}' | ./target/debug/harness hook verify-done; echo exit=$?   → exit=0, 14.8 s wall
    $ cargo test -p harness --lib -q  (×3)   → 190 passed; 0 failed; 1 ignored, each run
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
    190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 3 ignored, 0 failed → exit=0
  Two things in the launcher would have let the red be named, both off this scope (harness.toml,
  gates.rs): harness.toml:7 `fail_name = '^test (\S+) \.\.\. FAILED$'` cannot match `cargo test -q`
  output, which prints `<name> --- FAILED` and a `failures:` list (a scratch crate with one failing test, `cargo test -q` → `boom --- FAILED`), never a `test … FAILED` line; and the verdict gate
  drops the check output instead of writing its tail into the reason. Left as friction in PROGRESS.md.
  harness.default.toml is untouched this iteration (`git diff a0980d0 HEAD --stat -- crates/harness/harness.default.toml`
  is empty). Criteria re-run at a0980d0:
    $ sed -n 232p crates/harness/harness.default.toml   → gate = "verdict-flip"
    $ ./target/debug/harness probe | grep skill-ungated   → PROBE skill-ungated 0
    $ ./target/debug/harness probe | grep -c review-requested   → 0
    $ cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate
    test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out
    $ git add crates/harness/harness.default.toml TASKS.md PROGRESS.md && git commit -q -m "chore(config): T-009 answers the verdict-gate rejection, file unchanged" && git status --short
    (2 files changed: PROGRESS.md, TASKS.md; harness.default.toml unchanged; porcelain empty)
  2026-09-09 verifier (re-review): VERIFIED. `git status --porcelain` → empty. Base: origin/main
  (52e8799) is an ancestor; the task's own range is 59e31df..HEAD (6ba5ca7), `--name-only` → PROGRESS.md,
  TASKS.md, crates/harness/harness.default.toml; the toml hunk is `-gate = "none"` / `+gate = "verdict-flip"`
  at line 232 and nothing else. test-hashes.json and .check-baseline are not in that range (their delta vs
  origin/main is 773cd0d, 2ee7163, 58ad8db: T-005/T-006, done). `verdict-flip` is a probe name
  (probes/mod.rs:71, 98; telemetry.rs:356). Criteria, each run here:
    $ cargo build -q; ./target/debug/harness probe | grep -E "skill-ungated|review-requested"  → PROBE skill-ungated 0
    $ ./target/debug/harness probe | grep -c review-requested  → 0
    $ cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate
      → 1 passed; 0 failed; 26 filtered out (asserts cfg.skill.len() == 7 and no "names gate" finding)
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check
      → exit 0, twice; per binary 190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 3 ignored, 0 failed both runs.
  .check-baseline has no names, so no failure to match. On the earlier gate red at 6fe5898: the verdict gate ran
  check_delta (gates.rs:545, `sh -c` of harness.toml [check].command) and went red in 3 s (events seq 118 07:46:52
  → seq 121 07:46:55), which is the lib binary's 2.4 s plus a cached compile, not a full pass (~13 s); consistent
  with one lib test failing under `cargo test -q`, whose `<name> --- FAILED` line the `fail_name` regex
  (harness.toml:7) cannot capture. It does not reproduce: `cargo test -p harness --lib -q` ×3 → 190 passed;
  0 failed; 1 ignored each run, plus the two full chains above. The implementer's answer names this correctly and
  leaves it as a first-occurrence friction in PROGRESS.md:103; harness.toml and gates.rs are off this scope, so
  it is not this task's to fix. Both PROGRESS.md entries carry `friction:`; the first cites LEARNINGS.md's
  `[seed]` uncommitted-is-lost rule (LEARNINGS.md:24) as its second occurrence, already a rule, `PROBE
  friction-repeat 0`. Probe findings that remain (check-unnamed 1, litter 1 on test-hashes.json, install-stale 1,
  ponytail-ceiling 3, stage-outlier 5) predate this range and touch no file on its scope. Ponytail: one config
  line, nothing to cut. No dependency added, no test touched.
  2026-09-09 implementer (after the second gate): the rejection point is answered, not re-implemented;
  harness.default.toml is untouched (`git diff 0620606 HEAD --stat -- crates/harness/harness.default.toml` is
  empty). The red is reproduced and named, and it is not T-009's. The launcher (pid 44909) was started as
  `( harness run … ) &` from a non-interactive `zsh -c` (its parent 44908 in `ps -axo pid,ppid,command`). A
  background job of a non-interactive shell inherits SIGINT and SIGQUIT ignored (POSIX XCU 2.11 Signals and
  Error Handling, https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html#tag_18_11,
  2026-09-09); the disposition survives exec, so the verdict gate's `sh -c` check (gates.rs:551-555), the lib
  test binary and the stub agent all run with SIGINT ignored, and
  `agent::tests::a_signalled_child_reports_128_plus_the_signal` (agent.rs:607-619, stub `kill -INT $$`,
  asserts 130) gets 0. Measured here, both ways:
    $ sh -c 'kill -INT $$'; echo $?                                          → 130
    $ zsh -c '(sh -c "kill -INT \$\$"; echo $?) & wait'                       → 0   (bash -c: 0 too)
    $ zsh -c '(sh -c "kill -TERM \$\$"; echo $?) & wait'                      → 143
    $ cargo test -p harness --lib -q a_signalled_child_reports_128_plus_the_signal   → 1 passed (foreground)
    $ zsh -c '(cargo test -p harness --lib -q a_signalled_child_reports_128_plus_the_signal) & wait'
      → assertion `left == right` failed  left: 0  right: 130; test result: FAILED. 0 passed; 1 failed
  The lib binary is the first `cargo test --workspace -q` runs and takes 2.4 s, which is the gate's 3 s red
  both times (events.jsonl seq 324 07:56:17 → seq 325 07:56:20; seq 119 → 120 on the first). `cargo test -q`
  prints `<name> --- FAILED`, which harness.toml:7 `fail_name = '^test (\S+) \.\.\. FAILED$'` cannot match,
  so `unforgiven` is empty and the reason reads "no failure could be named". The same command under the
  launcher's own environment (`ps eww -p 44909` → env file, `env -i` + `/bin/sh -c`, stdin from the tool and
  from /dev/null), run in the foreground: exit 0, 14 s, 308 passed, 0 failed, both times. So the red is
  deterministic under this launcher process and independent of the task: no `done` can pass the verdict gate
  until `harness run` is started in the foreground (or by anything that resets SIGINT before exec), or the test
  stops depending on the inherited disposition, e.g. agent.rs:609 `kill -TERM $$` and 143, which background
  jobs do not ignore (measured above). agent.rs, harness.toml and gates.rs are off this scope. STOP is in the
  tree (written 07:58 by the operator session), so the run halts at this boundary (pipeline.rs:741) and no
  verify round is spent on a gate that cannot go green. Criteria re-run at 0620606:
    $ sed -n 232p crates/harness/harness.default.toml                          → gate = "verdict-flip"
    $ cargo build -q; ./target/debug/harness probe | grep -E 'skill-ungated|review-requested'   → PROBE skill-ungated 0
    $ ./target/debug/harness probe | grep -c review-requested                 → 0
    $ cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate
      → test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
      → exit=0; per-binary 190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 3 ignored, 0 failed
    $ git add TASKS.md PROGRESS.md && git commit -q -m "chore(config): T-009 answers the second verdict-gate rejection, file unchanged" && git status --short
      → 2 files committed (TASKS.md, PROGRESS.md), harness.default.toml unchanged; `git status --short` → ?? STOP (STOP is the operator's halt marker, untracked); amended once to carry this line

## [T-013] ponytail-ceiling crates/harness/src/gates.rs:451 marker with no dated kill line naming its text
scope: crates/harness/src/gates.rs, crates/harness/src/skills.rs
blockedBy: none
status: ready
probe: ponytail-ceiling
rows: none — harness
command: `harness probe`
output: |
  PROBE ponytail-ceiling 8
  FINDING ponytail-ceiling crates/harness/src/gates.rs:451 // ponytail: unify with skills::skills_dir (Task 8) once it resolves the same sources.
criteria:
  - one function resolves the skills directory: `gates.rs` no longer carries its own three-source lookup in `skills_dir_for`, and every gates.rs use goes through `skills::skills_dir` (change its signature if the preset lookup belongs inside it; `config::validate` refuses an unknown preset at config.rs:472, so a lookup after `load` cannot miss)
  - the marker at gates.rs:451 is gone and `harness probe` emits no `ponytail-ceiling` line for `crates/harness/src/gates.rs`
  - `cargo test -p harness -q` passes; no file under `crates/harness/tests` changes and no `#[test]` in gates.rs or skills.rs is deleted or weakened (call sites may be updated)
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  proposed from the output above on 2026-09-08. Not adjudicated.
  2026-09-08 adjudicator: promoted as a lift, not a kill line. The marker's own condition, "once it
  resolves the same sources", holds: `skills::skills_dir` (skills.rs:119-127) reads
  `layout.skills_dir`, then the preset's `skills_dir`, then `<harness_dir>/skills`, the same three
  in the same order as gates.rs:452-463. DECISIONS.md is off scope: nothing to record when the
  shortcut goes.
## [T-014] the templates, roles and skill carry no rationale prose: a comment states a rule or a format, nothing else
scope: templates/*.md, roles/*.md, skills/running-the-loop/**, evals/README.md, adapters/README.md, adapters/bun-turbo/README.md, crates/harness/tests/init.rs, crates/harness/tests/probes.rs
blockedBy: none
status: ready
rows: none — harness
criteria:
  - every HTML comment block (`<!-- … -->`) in `templates/*.md` is at most two lines and states a format or a rule; a sentence explaining why the rule exists is deleted, e.g. `templates/DECISIONS.md:3` "Two things, in this order, because the top of this file is read far more often than the rest." becomes "Rejected findings first, then archived blocks."
  - `grep -nE '\b(because|which is why|the reason|worth|deliberately|on purpose|it turns out|in practice)\b' templates/*.md roles/*.md skills/running-the-loop/SKILL.md skills/running-the-loop/references/task-block.md evals/README.md adapters/README.md adapters/bun-turbo/README.md` prints nothing, except inside the `Why` column of `templates/RAILS.md`, where each cell is at most one sentence and a citation
  - every role prompt keeps every rule, every rail name and every protocol step it has today; only the sentences that justify them go; `wc -l` of each role file is at most 80% of today's count, pasted before and after in notes:
  - `harness probe` on a fresh `harness init` reports the same `rail-unenforced`, `spec-untested`, `queue-hygiene` and `learning-*` counts as before the change (paste both runs in notes:); `cargo test --workspace -q` exits 0 and the tests in scope are updated only where they assert on removed wording
notes: |
  The owner's rule: a comment is short, and exists only to stop a mistake recurring or to explain hard
  code. Rationale belongs in DECISIONS.md, not in the file an agent reads every session.

## [T-015] README.md, docs/intent.md and harness.default.toml describe, and do not argue
scope: README.md, docs/intent.md, crates/harness/harness.default.toml, crates/harness/src/config.rs, crates/harness/tests/cli.rs
blockedBy: none
status: ready
rows: none — harness
criteria:
  - `README.md` is at most 280 lines; every section is a table, a fenced command, or sentences in the present tense that state what a command or field does; `grep -nE '\b(because|which is why|the reason|worth|deliberately|on purpose|we |our )\b' README.md` prints nothing
  - every `_comment`-style key or `#` comment in `crates/harness/harness.default.toml` is at most one line stating what the key is; the cited papers move to `docs/intent.md` under one heading `## References` as a list, one line each, and appear nowhere else; the `include_str!`/migration tests in `config.rs` and `cli.rs` still pass
  - `docs/intent.md` keeps its headings Problem, Proposed outcome, What exists today, Where it goes, Affected users and systems, Constraints, Open questions, References; every bullet under them is a statement or a measurement with its command; the same grep as above over `docs/intent.md` prints nothing
  - `cargo test --workspace -q` exits 0; `harness probe` reports `check-unnamed 0` and `litter 0` after the change (paste in notes:)
notes: |
  Same rule as T-014. Measurements stay; sentences about why a measurement matters go.

## [T-016] the licence is MIT
scope: LICENSE, NOTICE, Cargo.toml, crates/harness/Cargo.toml, README.md, crates/harness/tests/floor.rs
blockedBy: none
status: ready
rows: none — harness
criteria:
  - `LICENSE` is the MIT License text with `Copyright (c) 2026 Thomas Taylor`; `NOTICE` is deleted; `git ls-files NOTICE` prints nothing
  - `crates/harness/Cargo.toml` has `license = "MIT"`; `grep -rn 'Apache' --include='*.md' --include='*.toml' --include='*.rs' --include='*.yml' . | grep -v target/ | grep -v docs/superpowers/` prints nothing
  - `README.md`'s License section reads `MIT. See \`LICENSE\`.`
  - a test in `crates/harness/tests/floor.rs` named `the_licence_is_mit` asserts `LICENSE` starts with `MIT License` and `Cargo.toml` names `MIT`; `cargo test --workspace -q` exits 0
notes: |
  Owner's decision 2026-09-09.
