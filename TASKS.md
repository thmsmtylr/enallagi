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
status: review
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
  2026-09-08 implementer, at 898d1eb: `role_spawn` calls `roles::resolve` for a declared role
  BEFORE `role_source`, so the vendored file lands at the path `role_source` already reads first and
  the undeclared path is untouched. `commit_vendored_skills`/`skills_dirty` became
  `commit_vendored`/`vendored_dirty` over one `(id, path, result)` list of skills then roles; the
  message is `chore(vendor): <ids>`. `gates::scope` builds `added_ids`/`recut_ids` per lock table
  and `role_file(cfg, id)` is the one place that spells `<harness_dir>/roles/<id>.md` (the hook uses
  it too). `hooks::immutable`'s outer text is now "is locked by harness.lock" so a role hit does not
  read "locked role ... is a locked skill". A reviewer should scrutinise: (1) a failed role resolve
  halts under the name `role`, a failed skill resolve still under `skill`; (2) the fallback row's
  test was green before any code changed -- it guards the branch that skips `roles::resolve`, and
  fails if every role is resolved; (3) `a_fetched_skill_is_committed_before_the_stage_that_needs_it`
  now declares a `[[role]]` too, so it covers the shared commit and the scope exemption end to end.
  Commands run:
    cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check
    exit 0, 11 suites, passed=301 failed=0 (`grep "test result"` over the output)
    cargo test -p harness -q --test roles
    test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
    git add crates/harness/src/pipeline.rs crates/harness/src/hooks.rs crates/harness/src/roles.rs crates/harness/src/gates.rs crates/harness/tests/roles.rs crates/harness/tests/loop.rs README.md TASKS.md PROGRESS.md
    git commit -m "feat(pipeline): T-002 a declared role is resolved and committed before its stage, and the lock covers it"
