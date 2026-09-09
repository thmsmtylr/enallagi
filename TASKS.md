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
status: done
archived: DECISIONS.md — full block at `git show e28d6ae:TASKS.md`

## [T-013] ponytail-ceiling crates/harness/src/gates.rs:451 marker with no dated kill line naming its text
scope: crates/harness/src/gates.rs, crates/harness/src/skills.rs
blockedBy: none
status: done
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
  2026-09-09 implementer: `skills_dir_for` (gates.rs:457) is now one expression over
  `init::skills_root(cfg, presets.get(&cfg.agent.preset))`, which calls `skills::skills_dir` for a
  known preset and owns the `custom` arm (config.rs:465 lets `custom` through validate with no
  preset file, so the lookup can miss); the marker and the three-branch body are gone. Signature
  kept: hooks.rs:186 (off scope) calls it. skills.rs is on scope and needed no edit. No new test:
  a pure refactor, guarded by the existing `skills_dir_falls_back_from_layout_to_preset_to_harness_dir`
  (gates.rs:1337), which ran red-then-green against the probe: before, `./target/debug/harness probe`
  → `FINDING ponytail-ceiling crates/harness/src/gates.rs:456 …`; after → `PROBE ponytail-ceiling 2`,
  both lines DECISIONS.md:418 and TASKS.md:62 (this block's own quoted output; leaves at archive).
  Scrutinise: gates → init is a new module edge; the alternative was deriving Default on Preset
  (agent.rs, off scope). `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` → exit=0,
  191+0+8+5+9+15+21+29+27+4+0 = 309 passed, 3 ignored, 0 failed, at 9880c5a before the commit.
  Commit: `git add crates/harness/src/gates.rs crates/harness/src/skills.rs TASKS.md PROGRESS.md && git commit -q -m "feat(gates): T-013 skills_dir_for delegates to init::skills_root, marker gone"`
  → `3 files changed, 25 insertions(+), 12 deletions(-)` (gates.rs, TASKS.md, PROGRESS.md; skills.rs unchanged), `git status --porcelain` empty.
  2026-09-09 verifier: VERIFIED at 4ba2e15. `git status --porcelain` → empty. Base `origin/main` =
  52e8799 is the merge-base; the iteration's own diff is `git diff HEAD~1 --name-only` → PROGRESS.md,
  TASKS.md, crates/harness/src/gates.rs, all on scope or protocol files; test-hashes.json and
  crates/harness/tests untouched. `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → per-binary 191+0+8+5+9+15+21+29+27+4+0 = 309 passed, 3 ignored, 0 failed; clippy exit 0; fmt
  exit 0. `.check-baseline` carries no names, and no failure appeared, so there is nothing to
  match. Criterion 1: `skills_dir_for` (gates.rs:457) is one call to `init::skills_root`
  (init.rs:367), whose `Some` arm is `skills::skills_dir` (skills.rs:119, layout → preset →
  `<harness_dir>/skills`) and whose `None` arm is layout → `<harness_dir>/skills`, which is what the
  deleted body did for a preset absent from the map; `agent::presets()` builds `custom` inline at
  agent.rs:137 rather than storing it, so `None` is that arm. Criterion 2: `cargo build -q` then
  `./target/debug/harness probe | grep ponytail-ceiling` → `PROBE ponytail-ceiling 2`, the two
  lines DECISIONS.md:418 and TASKS.md:62 (this block's quoted output); no gates.rs line. Criterion 3:
  `git diff HEAD~1 -- crates/harness/src/gates.rs` touches only lines 456-467, the function body;
  `skills_dir_falls_back_from_layout_to_preset_to_harness_dir` (gates.rs:1337) still asserts all
  three sources and ran in the 191. Ponytail: pure deletion, no new dependency; the
  `to_string_lossy().into_owned()` is the cost of keeping the String signature for hooks.rs:186,
  which is off scope. Comment cites config.rs:465, which is the `custom` branch of validate.
  PROGRESS.md entry carries `friction:` and names it as a second occurrence of the T-002
  scope-line-noise friction: that line is due in LEARNINGS.md, off this task's scope, and
  `friction-repeat` will emit it until it lands.
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
