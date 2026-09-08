# TASKS

<!-- One block per task. status: proposed | ready | review | done | blocked | needs-spec | deferred -->

## [T-001] the [[role]] table, its lock entries, and a resolver that fetches and vendors a role
scope: crates/harness/src/config.rs, crates/harness/src/skills.rs, crates/harness/src/roles.rs, crates/harness/src/lib.rs, crates/harness/harness.default.toml, crates/harness/tests/roles.rs
blockedBy: none
status: ready
rows: `tests/roles.rs::a_declared_role_is_fetched_vendored_and_committed_before_its_stage`, `tests/roles.rs::a_role_whose_vendored_file_drifted_is_refused_under_frozen`
criteria:
  - `config::Config` gains `pub role: Vec<RoleDecl>` (`name`, `source`, `path` default ".", `rev`), deserialised from `[[role]]`, with `deny_unknown_fields`; `validate` refuses a duplicate name, a name outside `[a-z0-9-]+`, a non-relative or `..` path, and a `[[role]]` whose name no `[[stage]]` uses
  - `skills::Lock` gains `pub role: Vec<LockEntry>` (default empty, so an existing lock still parses) and `write_lock` writes it sorted by id
  - a new `roles.rs` exposes `resolve(root, cfg, names: &[String], opts: &skills::ResolveOpts, events) -> Result<Vec<ResolvedRole>, SkillError>` reusing the skills module's fetch, cache and vendor functions (refactor them to take a destination, do not copy them); the destination is `<harness_dir>/roles/<name>.md`; `cached` / `fetched` / `refused` semantics identical to skills, refusal under `frozen` returns `SkillError::Unresolved` after a `SkillResolved`-shaped event with the role's name (reuse the kind; add no new event kind)
  - `cargo test -p harness --test roles` passes the two named tests; the tests build a `path:` source and a `git+file://` bare repo like `skills.rs`'s tests do and never reach the network
  - `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check` are clean
notes: |
  Read `crates/harness/src/skills.rs` first: `resolve`, `cached`, `fetch`, `vendor`, `read_lock`, `write_lock`.
  The lock's `[[role]]` table reuses `LockEntry`. Comments: short, only where they guard a mistake.

## [T-002] the pipeline resolves and commits declared roles, and the immutable hook covers them
scope: crates/harness/src/pipeline.rs, crates/harness/src/hooks.rs, crates/harness/src/roles.rs, crates/harness/src/gates.rs, crates/harness/tests/roles.rs, crates/harness/tests/loop.rs, README.md
blockedBy: T-001
status: ready
rows: `tests/roles.rs::an_undeclared_role_falls_back_to_the_installed_or_embedded_file`, `tests/roles.rs::the_immutable_hook_refuses_an_edit_to_a_vendored_role`
criteria:
  - before rendering a role stage, `pipeline` calls `roles::resolve` for the stage's role when a `[[role]]` declares it; the vendored file and `harness.lock` are committed in the same `chore(skills): vendor …` commit the pipeline already makes for skills (rename the message to `chore(vendor): …` and update the existing loop test that asserts it); an undeclared role takes the existing `role_source` path unchanged
  - `gates::scope` exempts `<harness_dir>/roles/<name>.md` for a role whose lock entry was added this iteration, and treats a re-cut `[[role]]` lock entry exactly like a re-cut `[[skill]]` entry
  - `hooks::immutable` refuses an edit to `<harness_dir>/roles/<name>.md` when `harness.lock` has a `[[role]]` entry for `name`, with the same message shape it uses for a locked skill
  - `README.md` gains a `[[role]]` row in the `harness.toml` table and one sentence under Skills saying roles resolve the same way; no other README change
  - `cargo test -p harness` passes the two named tests plus every existing test; clippy and fmt clean
notes: |
  `pipeline.rs::commit_vendored_skills` and `skills_dirty` are the places to extend. Keep the halt on a
  failed commit. Comments: short, only where they guard a mistake.
