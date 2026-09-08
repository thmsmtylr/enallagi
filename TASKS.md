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
probe: spec-untested
rows: `tests/roles.rs::a_declared_role_is_fetched_vendored_and_committed_before_its_stage`, `tests/roles.rs::a_role_whose_vendored_file_drifted_is_refused_under_frozen`, `tests/roles.rs::an_undeclared_role_falls_back_to_the_installed_or_embedded_file`, `tests/roles.rs::the_immutable_hook_refuses_an_edit_to_a_vendored_role`
command: `harness probe`
output: |
  PROBE spec-untested 4
  FINDING spec-untested SPEC.md:16 no file tests/roles.rs for criterion tests/roles.rs::a_declared_role_is_fetched_vendored_and_committed_before_its_stage
  PROBE queue-uncovered 2
  FINDING queue-uncovered SPEC.md:16 tests/roles.rs::a_declared_role_is_fetched_vendored_and_committed_before_its_stage is untested and no open task names it (no file tests/roles.rs for criterion tests/roles.rs::a_declared_role_is_fetched_vendored_and_committed_before_its_stage)
criteria:
  - `probes::spec_untested::untested_rows` resolves a row name that contains `/` under `layout.source_root` first (`<source_root>/<name>`) and falls back to the name verbatim only when that path does not exist; a name with no `/` resolves exactly as today
  - a new test in `crates/harness/tests/probes.rs`: with `source_root` set, a row `tests/x.rs::t` whose file `<source_root>/tests/x.rs` declares `fn t(` yields no `spec-untested` finding, and the same row with that file absent still yields one; `cargo test -p harness -q --test probes` passes
  - `harness probe` emits no `spec-untested` line for SPEC.md:16, SPEC.md:17, SPEC.md:18 or SPEC.md:19 (today it emits all four)
  - `git diff --stat <parent>..HEAD -- SPEC.md crates/harness/src/probes/queue_uncovered.rs` is empty: the row names stay as SPEC.md §12 writes them, and `queue_uncovered` already reuses `untested_rows`
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  proposed from the output above on 2026-09-08. Not adjudicated. T-001 carried this row and is done, so no open block names it.
  2026-09-08 adjudicator: promoted. Re-run reproduced all four `spec-untested` lines. The claim as
  worded is false — `cargo test -p harness -q --test roles` runs 4 tests and passes — so the task is
  the probe's path rule, not the row: SPEC.md §12 says rows live under `crates/harness/`, harness.toml
  sets `source_root` to exactly that, and spec_untested.rs:17 takes any name with a `/` as
  repo-root-relative. T-004 was the same defect at SPEC.md:17 and was killed rather than carried twice.
  2026-09-08 implementer: `untested_rows` (spec_untested.rs:17-22) now builds `<source_root>/<name>`
  first and takes the name verbatim only when it has a `/` and that path is absent; a name with no
  `/` resolves as before. New test `a_row_with_a_slash_resolves_under_source_root_before_the_repo_root`
  in tests/probes.rs, red before (left: 1, right: 0), green after. Scrutinise: the seeded row
  `src/thing.test.ts` still counts as untested under default `source_root = "src"` because
  `src/src/thing.test.ts` does not exist, which is the fallback branch doing its job. `harness` on PATH
  is a symlink to target/release, so `cargo build --release` was needed for `harness probe` to show it.
  `./target/debug/harness probe` and `harness probe` (after release build) both → `PROBE spec-untested 0`,
  `PROBE queue-uncovered 0`. `git diff --stat HEAD -- SPEC.md crates/harness/src/probes/queue_uncovered.rs`
  → empty. `cargo test -p harness -q --test probes` → 27 passed. Check →
  `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  exit 0 (190+8+5+9+15+21+29+27+4 passed, 3 ignored, 0 failed) at a55bcf6 before the commit.
  Committed: `git add crates/harness/src/probes/spec_untested.rs crates/harness/tests/probes.rs TASKS.md PROGRESS.md`
  then `git commit -m "feat(probes): T-003 a row with a slash resolves under source_root first"`
  (first commit e71e483 missed TASKS.md on a bad edit anchor; amended in place, nothing pushed).
  2026-09-08 verifier: VERIFIED. Base `HEAD~1` (a55bcf6; origin/main is 82 commits behind this
  branch, so the parent is the only base that isolates the task). `git status --porcelain` → empty
  before and after. `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; 190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 3 ignored, 0 failed, same figures on two
  runs; `.check-baseline` has no lines and nothing failed, so the delta is empty. Red-green: with
  spec_untested.rs reverted to a55bcf6, `cargo test -p harness -q --test probes` → 26 passed, 1 failed,
  `a_row_with_a_slash_resolves_under_source_root_before_the_repo_root` (left: 1, right: 0), and
  `./target/debug/harness probe` at the parent emits `PROBE spec-untested 4` with FINDING lines at
  SPEC.md:16, :17, :18, :19; restored, `cargo build -q && ./target/debug/harness probe` →
  `PROBE spec-untested 0`, `PROBE queue-uncovered 0`. `git diff --stat HEAD~1..HEAD -- SPEC.md
  crates/harness/src/probes/queue_uncovered.rs` → empty. `git diff HEAD~1 --name-only` → the two
  scope files plus PROGRESS.md and TASKS.md, both exempt at gates.rs:13-14. No test-hashes.json in the
  tree, no Cargo.toml/Cargo.lock change, no launcher/hook/check edit, and PROGRESS.md's `rows:` reads
  `none — harness`. The test diff is additive only; the new test asserts 1 with the file absent and 0
  with it present, so the failure path is covered. The comment's citation (SPEC.md §12) checks out at
  SPEC.md:24. PROGRESS.md entry carries a `friction:` line, first occurrence (grep `target/release`
  → 1 hit). Ponytail: 6-line diff, one extra `exists` call per slashed row, nothing to cut.

## [T-005] .harness/RAILS.md:57 enforced by test-hashes.json, which does not exist
scope: test-hashes.json
blockedBy: none
status: ready
probe: rail-unenforced
rows: none — harness
command: `harness probe`
output: |
  PROBE rail-unenforced 2
  FINDING rail-unenforced .harness/RAILS.md:57 enforced by test-hashes.json, which does not exist
criteria:
  - `test-hashes.json` exists at the repo root as a flat JSON object mapping a repo-relative path to the lowercase hex SHA-256 of that file's bytes, with one key per file the SPEC.md §11 rows name under `crates/harness/`: today exactly `crates/harness/tests/roles.rs`, its value equal to `shasum -a 256 crates/harness/tests/roles.rs`
  - `cargo test -p harness -q --test floor -- every_file_test_hashes_covers_still_hashes_to_its_recorded_digest` passes against the real file (today it passes vacuously, the file being absent)
  - `harness probe` emits no `rail-unenforced` line for `.harness/RAILS.md:57` or `.harness/RAILS.md:58`; the `hash-uncovered` line for :58 is T-006's and may remain
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  proposed from the output above on 2026-09-08. Not adjudicated.
  2026-09-08 adjudicator: promoted. Reproduced. hooks.rs:92 (`hashed_hit`) and gates.rs:299 already
  read this file, README.md:350 says it stays unwritten until you write it, and RAILS.md:57 names
  it, so the fix is the file and not the row; .harness/RAILS.md is off scope. The PreToolUse hook
  refuses the edit tool on `test-hashes.json` only once it exists (hooks.rs:93-96), so creating it
  is unobstructed. Keys for `harness.toml` and the build config are T-006, not here.

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