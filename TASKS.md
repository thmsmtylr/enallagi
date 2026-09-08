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
status: ready
probe: rail-unenforced
rows: none — harness
command: `harness probe`
output: |
  PROBE rail-unenforced 2
  FINDING rail-unenforced .harness/RAILS.md:58 enforced by test-hashes.json, which does not exist
  PROBE hash-uncovered 1
  FINDING hash-uncovered .harness/RAILS.md:58 `harness-immutable` names harness.toml and test-hashes.json does not exist, so the rail covers nothing
criteria:
  - `test-hashes.json` gains a key, with the file's SHA-256 as T-005 formats it, for `harness.toml` and for each file `layout.harness_files` in harness.toml names (`Cargo.toml`, `crates/harness/Cargo.toml`); every key T-005 wrote keeps its value
  - `cargo test -p harness -q --test floor -- every_file_test_hashes_covers_still_hashes_to_its_recorded_digest` passes
  - `harness probe` emits no `rail-unenforced` or `hash-uncovered` line for `.harness/RAILS.md:58`
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  proposed from the output above on 2026-09-08. Not adjudicated. Same missing file as T-005; the adjudicator may fold them.
  2026-09-08 adjudicator: promoted, blocked on T-005, which creates the file. Not folded: a kill
  line needs a refutation and none exists until T-005 lands. Once the file exists the PreToolUse
  hook refuses the edit tool on it (hooks.rs:93-96, "the reference itself"), so re-cut it from a
  shell (`shasum -a 256` into a redirect); a key added fresh needs no scope entry (gates.rs:300),
  and the verifier reads the diff.

## [T-007] tdd is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: ready
probe: skill-ungated
rows: none — harness
command: `harness probe`
output: |
  PROBE skill-ungated 3
  FINDING skill-ungated harness.toml:0 tdd is declared with gate: none -- nothing fails without it, so relying on it is a hope
criteria:
  - the `[[skill]]` entry `id = "tdd"` in crates/harness/harness.default.toml reads `gate = "spec-untested"`; its `source`, `path`, `rev` and `why` are unchanged, and no other entry changes
  - `harness probe` emits no `skill-ungated` line naming `tdd` (today: 1)
  - `cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate` passes (a gate name the probe does not know fails it)
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  proposed from the output above on 2026-09-08. Not adjudicated. harness.toml declares no `[[skill]]`; the entry the probe reads is the embedded default.
  2026-09-08 adjudicator: promoted. Reproduced. `spec-untested` is what fails when the test a
  criterion names is never written, which is the half of tdd a probe can see; nothing observes
  ordering, and that stays a hope the `why` line already states. The embedded default is the entry
  the probe reads here, so it is the file; harness.toml is off scope.

## [T-008] review-received is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: ready
probe: skill-ungated
rows: none — harness
command: `harness probe`
output: |
  PROBE skill-ungated 3
  FINDING skill-ungated harness.toml:0 review-received is declared with gate: none -- nothing fails without it, so relying on it is a hope
criteria:
  - the `[[skill]]` entry `id = "review-received"` in crates/harness/harness.default.toml reads `gate = "rejection-repeat"`; its `source`, `path`, `rev` and `why` are unchanged, and no other entry changes
  - `harness probe` emits no `skill-ungated` line naming `review-received` (today: 1)
  - `cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate` passes
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  proposed from the output above on 2026-09-08. Not adjudicated.
  2026-09-08 adjudicator: promoted. Reproduced. `rejection-repeat` (probes/telemetry.rs:135) reports
  the same rejection reason recurring across a task's verdicts, which is what a rejected task that
  did not answer its points looks like from the event log; it is the one probe that fails without
  this skill. The embedded default is the entry the probe reads here; harness.toml is off scope.

## [T-009] review-requested is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: ready
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