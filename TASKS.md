# TASKS

<!-- One block per task. status: proposed | ready | review | done | blocked | needs-spec | deferred -->

## [T-001] the [[role]] table, its lock entries, and a resolver that fetches and vendors a role
scope: crates/harness/src/config.rs, crates/harness/src/skills.rs, crates/harness/src/roles.rs, crates/harness/src/lib.rs, crates/harness/harness.default.toml, crates/harness/tests/roles.rs
blockedBy: none
status: done
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
  2026-09-08 implementer, at 2610220: the cache/fetch/vendor/lock cycle moved into `skills::pin`
  (`Pin { id, source, rev, file }` + a vendor closure); `skills::resolve` and `roles::resolve` are
  both that loop, so cached/fetched/refused and the `SkillResolved` event are identical by
  construction. `cached` and `fetch` take a `Pin` now, not a `SkillDecl`. Choices a reviewer should
  scrutinise: (1) a role's source file is `<path>/<name>.md` under the fetched root, mirroring
  `<path>/SKILL.md` -- `path` naming the file itself is not supported; (2) `config::validate`
  skips `MissingRole` for a role a `[[role]]` declares, since the file cannot exist before the
  first fetch (`a_declared_role_is_not_missing_before_it_is_fetched`); (3) `Lock.role` is
  `skip_serializing_if = "Vec::is_empty"` so a skills-only lock round-trips byte-for-byte;
  (4) the resolver's `BadId`/`BadPath`/`Undeclared` messages still say "skill" -- validate is the
  first lock and reports role-specific errors. The row name's "and committed before its stage"
  clause is T-002's pipeline work; this test asserts fetch, vendor, lock and events.
  Commands run:
    cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check
    exit=0, 11 suites, passed=297 failed=0 (`grep "test result"` over the output)
    cargo test -p harness --test roles -q
    test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
    git add crates/harness/src/config.rs crates/harness/src/skills.rs crates/harness/src/roles.rs crates/harness/src/lib.rs crates/harness/harness.default.toml crates/harness/tests/roles.rs TASKS.md PROGRESS.md
    git commit -m "feat(roles): T-001 the [[role]] table, its lock entries, and a resolver that fetches and vendors a role"
  2026-09-08 verifier, at 1a3d46c: VERIFIED.
    git status --porcelain -> empty; the task commit is 41acc08, parent 2610220.
    Base: origin/main (52e8799) is 68 commits behind this branch and predates the Rust port, so the
    diff verified is the iteration's own commit, 2610220..41acc08.
    cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check
    exit 0; 11 suites; 186+0+6+5+9+15+21+27+26+2+0 = 297 passed, 0 failed, 3 ignored. .check-baseline
    is empty, so delta is zero failures against zero lines.
    cargo test -p harness --test roles -q -> 2 passed; 0 failed.
    git show --stat 41acc08: config.rs, skills.rs, roles.rs, lib.rs, harness.default.toml, tests/roles.rs
    plus TASKS.md and PROGRESS.md; every file is on scope. No Cargo.toml or Cargo.lock change. No
    test-hashes.json in the tree. No event kind added (events.rs untouched). No http/github in
    tests/roles.rs: the git source is a bare clone under a tempdir.
    Criteria: RoleDecl has deny_unknown_fields and path "." by default (config.rs:229-247); validate
    refuses duplicate, bad name, bad path and unused (config.rs:395-416), each asserted by
    a_role_declaration_is_refused_when_malformed_or_unused. Lock.role defaults empty and write_lock
    sorts it (skills.rs:48-50, 84). roles::resolve reuses skills::pin, cached and fetch; only the
    single-file vendor is new, and skills' vendor copies a directory, so nothing was duplicated.
    Test bodies read: the first asserts source, rev, commit, sha256, the vendored path and body, the
    fetched-then-cached event pair and a byte-identical lock on the second pass; the second asserts
    Unresolved naming the role, the refused event, the tampered file left alone, and the refetch.
    Probe, a scratch crate at /tmp/roleprobe-67970 depending on the harness crate by path (outside
    the tree; the session's policy denied its deletion): 4 passed, 0 failed -- two roles resolve
    and land in the lock sorted by id; the repo's own skills-only harness.lock round-trips through
    read_lock/write_lock byte for byte with no [[role]]; frozen with no lock entry returns Unresolved
    and writes neither the file nor a lock; an absolute role path is refused by validate with
    "role implementer: path `/etc` must be relative".
    Open, for T-002's verifier: the row a_declared_role_is_fetched_vendored_and_committed_before_its_stage
    names a commit and a stage ordering its body does not assert. T-002 owns pipeline.rs and
    tests/roles.rs; its verdict should reject unless that test body grows the commit assertion.
    Minor, not blocking: roles::resolve's BadId, BadPath and Undeclared errors still read "skill"
    (roles.rs:33-44); validate reports the role-specific message first.
    PROGRESS.md entry carries a friction line; first occurrence of that friction in the file.

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
