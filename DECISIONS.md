# DECISIONS

Two things, in this order, because the top of this file is read far more often than the rest.

`## Rejected findings` is every proposal the adjudicator killed, one line each, with the command
that refutes it. Read it with `sed -n '/^## Rejected findings/,/^## \[T-/p' DECISIONS.md` and
never read past that range.

Below it, completed task blocks, verbatim, moved out of TASKS.md once `done` by
`harness run`. The queue stays small; the audit trail stays whole. Each block is the
implementer's and the verifier's own words, never summarised on the way in.

## Rejected findings

<!-- - [YYYY-MM-DD] <the claim in one sentence> — refuted by `<command>`: <the output that refutes it> -->
- [2026-09-08] SPEC.md:17's criterion `tests/roles.rs::a_role_whose_vendored_file_drifted_is_refused_under_frozen` has no test file — refuted by `cargo test -p harness -q --test roles -- a_role_whose_vendored_file_drifted_is_refused_under_frozen`: `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out`; the file is `crates/harness/tests/roles.rs`, under the `crates/harness/` that SPEC.md §12 names, and the probe's root-relative lookup of a row name containing `/` is T-003
- [2026-09-08] `.claude/skills/ponytail/SKILL.md:64` is a ponytail marker with no kill line naming its text — refuted by `sed -n 64p .claude/skills/ponytail/SKILL.md`: the line is "- Mark deliberate simplifications with a `ponytail:` comment (`// ponytail: this exists`), simple reads as intent, not ignorance. ...", the vendored skill's own prose describing the marker in a file harness.lock pins, not a shortcut taken
- [2026-09-08] `README.md:127` is a ponytail marker with no kill line naming its text — refuted by `sed -n 127p README.md`: the line is the probe table's own row, "| `ponytail-ceiling` | a `ponytail:` marker in code with no dated kill line already naming its text |", which describes the probe rather than taking a shortcut
- [2026-09-08] `crates/harness/src/agent.rs:377` is a ponytail marker with no kill line naming its text — refuted by `sed -n '377,379p' crates/harness/src/agent.rs`: "// ponytail: a timed-out child can leave a grandchild holding the pipe, so the readers are joined only on a clean exit; upgrade to a process-group kill if a preset turns out to orphan writers on a clean exit too" names its ceiling and its upgrade condition, and the condition waits on a measurement nobody has taken (`measure-first`)
- [2026-09-08] `crates/harness/src/probes/check_unnamed.rs:3` is a ponytail marker with no kill line naming its text — refuted by `sed -n 3p crates/harness/src/probes/check_unnamed.rs`: "//! ponytail: the context file only; widen to the spec or LEARNINGS.md once a second document is measured to have drifted." names its ceiling and gates the upgrade on a measurement not yet taken (`measure-first`); `harness probe` → `PROBE check-unnamed 0`
- [2026-09-08] `crates/harness/src/probes/friction_repeat.rs:29` is a ponytail marker with no kill line naming its text — refuted by `sed -n 29p crates/harness/src/probes/friction_repeat.rs`: "// ponytail: each group compares only to its first member, so an A-B-C chain whose ends don't overlap stays two groups" names its ceiling, and no chain has split in practice: `harness probe` → `PROBE friction-repeat 0`
- [2026-09-08] `crates/harness/src/probes/friction_repeat.rs:57` is a ponytail marker with no kill line naming its text — refuted by `sed -n 57p crates/harness/src/probes/friction_repeat.rs`: "// ponytail: rule coverage is per-line word-set containment; a rule that paraphrases every word escapes it" names its ceiling, and no paraphrased rule has escaped it in practice: `harness probe` → `PROBE friction-repeat 0`
- [2026-09-08] `docs/intent.md:171` is a ponytail marker with no kill line naming its text — refuted by `sed -n '170,171p' docs/intent.md`: the lines are the paragraph "Stop `ponytail-ceiling` counting its own documentation", which records this very class of hit — the probe firing on the text that describes it — and take no shortcut

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
status: done
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
  2026-09-08 verifier, at 83bc070 (T-002 is e0d0d34, parent 898d1eb; origin/main is the pre-port
  tree, so the diff base is 898d1eb): VERIFIED.
    git status --porcelain → empty; git diff 898d1eb e0d0d34 --stat → PROGRESS.md README.md TASKS.md
    gates.rs hooks.rs pipeline.rs tests/loop.rs tests/roles.rs, all on scope: or bookkeeping; no
    Cargo.toml, .harness, .check-baseline or test-hashes.json change (test-hashes.json does not exist)
    PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check
    → exit 0; 11 suites, passed=305 failed=0 ignored=3 (summed from the `test result` lines).
    .check-baseline has no test lines; no failure names to match. The implementer's 301 reproduces
    by delta: 297 at T-001 + 4 `#[test]` in 898d1eb..e0d0d34 = 301, + 4 in e0d0d34..HEAD = 305.
    cargo test -p harness -q --test roles → 4 passed; --test loop a_fetched_skill → 1 passed.
  Criteria: (1) pipeline.rs:585-599 resolves a declared role before role_source and halts under
  `role`; commit_vendored writes `chore(vendor): <ids>`; loop.rs::a_fetched_skill_is_committed_before_the_stage_that_needs_it
  moved the role source to vendor/roles/ and asserts the commit name, the role file in ls-files and
  the lock entry, so it fails without the resolve. (2) gates.rs added_ids/recut_ids per lock table,
  role_file() the one path spelling; unit tests scope_exempts_a_freshly_vendored_role_outside_scope
  and scope_rejects_a_recut_role_outside_scope. (3) hooks.rs locked_hit walks lock.role with exact
  path match; message "is locked by harness.lock" for both. (4) git diff 898d1eb e0d0d34 -- README.md
  is the `[[role]]` block after `[[skill]]` and one sentence under Skills, nothing else. (5) above.
  Probes (scratch, /tmp): `harness hook immutable` in a repo whose lock has one `[[role]]`: exit 2
  for .harness/roles/implementer.md, its absolute form, ./-prefixed and ../-traversal forms; exit 0
  for implementer.md.bak and verifier.md. `harness run 1` on a fixture-shaped repo with `[[role]]`
  declared and vendor/roles/implementer.md absent: events.jsonl `"halt":"role"`, no spawn, no commit;
  with it present: one commit `chore(vendor): implementer` carrying .harness/roles/implementer.md and
  harness.lock, the run role file rendered from it, porcelain empty.
  Ponytail: nothing significant. Minor: a failed vendor commit still halts under `skill` with the text
  "vendored skills could not be committed" when the list holds a role (pipeline.rs:685); no test
  covers the pipeline-level halt on a failed role resolve, only the probe above and roles.rs's unit
  test of resolve. Friction line present, first occurrence.

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
status: done
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
  2026-09-08 implementer: wrote the file from the shell, one key. Red first: a zero digest made
  `cargo test -p harness -q --test floor -- every_file_test_hashes_covers_still_hashes_to_its_recorded_digest`
  fail with `left: ["crates/harness/tests/roles.rs"]`; the real digest passes it (1 passed).
  `./target/debug/harness probe` → `PROBE rail-unenforced 0`; the one `hash-uncovered` line left is
  :58, T-006's. Scrutinise: the key is the only one and its value is
  `shasum -a 256 crates/harness/tests/roles.rs` = 0e2ec9467eca1163f757d1bfa210d573d50bd2153c86570143efcaf8a8d0648d at 65e1cd3.
  Commands:
    $ printf '{\n  "crates/harness/tests/roles.rs": "%s"\n}\n' "$(shasum -a 256 crates/harness/tests/roles.rs | cut -d' ' -f1)" > test-hashes.json
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
    test result: ok. 190 passed; 0 failed; 1 ignored (+ 8, 5, 9, 15 (2 ignored), 21, 29, 27, 4 passed) → exit=0
    $ git add test-hashes.json TASKS.md PROGRESS.md && git commit -m "feat(hashes): T-005 test-hashes.json covers tests/roles.rs"
  2026-09-08 verifier: VERIFIED. `git status --porcelain` empty at 2ee7163; diff base HEAD~1 (origin/main
  resolves but the branch is dogfood, HEAD~1 is the implementer's commit). Files touched: PROGRESS.md,
  TASKS.md, test-hashes.json — the last is the scope line; no Cargo.toml/Cargo.lock diff (`minimal`).
  The one key is new, not re-cut, and roles.rs itself is unchanged, so `tests-immutable` is intact.
  `rows: none — harness` licenses the hash-file edit (`harness-lane`). PROGRESS.md carries
  `friction: none`; no repeat. Criteria:
    1. `cat test-hashes.json` → one key `crates/harness/tests/roles.rs` = 0e2ec946…8d0648d; `shasum -a 256
       crates/harness/tests/roles.rs` → same digest; `python3 -c 'json.load(...)'` parses as a flat object.
    2. `cargo test -p harness -q --test floor -- every_file_test_hashes_covers_still_hashes_to_its_recorded_digest`
       → 1 passed; 0 failed; 16 filtered out. Break test: `printf '\n' >> crates/harness/tests/roles.rs`
       then the same command → FAILED, `left: ["crates/harness/tests/roles.rs"] right: []` at floor.rs:306;
       `git checkout -- crates/harness/tests/roles.rs` restored, porcelain empty. The test is live.
    3. `cargo run -q -p harness -- probe` → `PROBE rail-unenforced 0`; `PROBE hash-uncovered 1`, the one
       FINDING is `.harness/RAILS.md:58 ... harness.toml`, T-006's.
    4. `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
       → 190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 0 failed, 3 ignored; clippy exit 0; fmt exit 0.
       `.check-baseline` has no entries; no failure names to match.
  Not a rejection, for the scout: `harness probe` → `FINDING litter test-hashes.json:0` because
  harness.toml:18 `layout.docs` does not name the file, though .harness/RAILS.md:57 and gates.rs:299
  do. harness.toml is off this task's scope.

## [T-006] .harness/RAILS.md:58 enforced by test-hashes.json, which does not exist
scope: test-hashes.json
blockedBy: T-005
status: done
gate: the verifier returned done and the scope gate rejected it: touched test-hashes.json (crates/harness/tests/roles.rs), which the scope line does not name
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
  2026-09-08 implementer: three keys added from the shell, the roles.rs key untouched. Red first:
  `./target/debug/harness probe` → `PROBE hash-uncovered 1`, `FINDING hash-uncovered .harness/RAILS.md:58
  ... names harness.toml and test-hashes.json has no key for it`; after the write → `PROBE rail-unenforced 0`,
  `PROBE hash-uncovered 0`. Scrutinise: the three values are `shasum -a 256 harness.toml Cargo.toml
  crates/harness/Cargo.toml` at c529b4d (b28d83ae…, 5f693c40…, 03fb9e7d…) and the floor test passes
  against the repo (1 passed). Keys are sorted; the probe and floor test read a map, not an order.
  Commands:
    $ shasum -a 256 harness.toml Cargo.toml crates/harness/Cargo.toml   # values pasted into a heredoc > test-hashes.json
    $ cargo test -p harness -q --test floor -- every_file_test_hashes_covers_still_hashes_to_its_recorded_digest
    test result: ok. 1 passed; 0 failed; 0 ignored; 16 filtered out
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
    test result: ok. 190 passed; 0 failed; 1 ignored (+ 8, 5, 9, 15 (2 ignored), 21, 29, 27, 4 passed) → exit=0
    $ git add test-hashes.json TASKS.md PROGRESS.md && git commit -m "feat(hashes): T-006 test-hashes.json covers harness.toml and the build config"
  2026-09-08 verifier: VERIFIED. `git status --porcelain` empty at 58ad8db; diff HEAD~1 touches only
  test-hashes.json, TASKS.md, PROGRESS.md; `rows: none — harness` so the hashes file is in-lane, and
  every key is added fresh (no key on origin/main: file absent there), none re-cut. `.check-baseline`
  has no entries; no failure to match. PROGRESS.md entry carries `friction: none`.
    $ shasum -a 256 harness.toml Cargo.toml crates/harness/Cargo.toml crates/harness/tests/roles.rs
    b28d83ae…  harness.toml / 5f693c40…  Cargo.toml / 03fb9e7d…  crates/harness/Cargo.toml / 0e2ec946…  roles.rs
    → all four equal the committed values; `layout.harness_files` (harness.toml:15) names exactly Cargo.toml and crates/harness/Cargo.toml.
    $ cargo test -p harness -q --test floor -- every_file_test_hashes_covers_still_hashes_to_its_recorded_digest
    test result: ok. 1 passed; 0 failed; 0 ignored; 16 filtered out
    $ cargo build -q && ./target/debug/harness probe | grep -E "rail-unenforced|hash-uncovered"
    PROBE rail-unenforced 0 / PROBE hash-uncovered 0 (no FINDING for RAILS.md:58)
    $ cargo test --workspace -q; cargo clippy --all-targets -q -- -D warnings; cargo fmt --all --check
    exit 0 / 0 / 0; 190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 3 ignored, 0 failed (uncached, second run identical)
  Break attempt: `sed` one hex digit of the harness.toml key → floor test FAILED at floor.rs:306,
  left: ["harness.toml"] right: []; `git checkout -- test-hashes.json` restored, tree clean.
  Ponytail: three JSON keys, nothing to cut.
  2026-09-08 implementer (after the gate): the rejection point is answered, not re-implemented. The
  roles.rs key was never re-cut: its value is 0e2ec946… at 2ee7163 and at 58ad8db (diff above, and
  `shasum -a 256 crates/harness/tests/roles.rs` today). The gate reads a text diff (gates.rs:502
  `recut_keys`, regex gates.rs:468) and T-005's line lost its trailing comma while T-006's gained
  one, so the same key matched on a `-` and a `+` line and was counted as present-at-base-and-changed:
    $ git diff c529b4d 1a9b097 -- test-hashes.json | grep -E '^[+-]\s*"([^"]+)"\s*:'
    -  "crates/harness/tests/roles.rs": "0e2ec946…"
    +  "Cargo.toml": …  /  +  "crates/harness/Cargo.toml": …  /  +  "crates/harness/tests/roles.rs": "0e2ec946…",  /  +  "harness.toml": …
  A value-aware compare belongs in gates.rs, off this scope; left as friction in PROGRESS.md for the
  probe, not fixed here. test-hashes.json is untouched this iteration (`git diff 61f8600 HEAD --stat
  -- test-hashes.json` is empty), so the re-verify diffs nothing on it. Criteria re-run at 61f8600:
    $ ./target/debug/harness probe | grep -E "rail-unenforced|hash-uncovered"
    PROBE rail-unenforced 0 / PROBE hash-uncovered 0
    $ cargo test -p harness -q --test floor -- every_file_test_hashes_covers_still_hashes_to_its_recorded_digest
    test result: ok. 1 passed; 0 failed; 0 ignored; 16 filtered out
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
    190+0+8+5+9+15+21+29+27+4+0 passed, 3 ignored, 0 failed → exit=0
    $ git add test-hashes.json TASKS.md PROGRESS.md && git commit -q -m "chore(hashes): T-006 answers the scope-gate rejection, file unchanged" && git status --short
    (2 files changed: PROGRESS.md, TASKS.md; test-hashes.json unchanged; porcelain empty)
  2026-09-08 verifier (second pass): VERIFIED. `git status --porcelain` empty at 88022a5. `origin/main`
  is 52e8799 (the merge-base); diff base for the task's file is c529b4d..58ad8db, and 61f8600..HEAD touches
  only TASKS.md and PROGRESS.md. `rows: none — harness`, so test-hashes.json is in-lane. `.check-baseline`
  has no entries; no failure to match. PROGRESS.md entry at line 75 carries a `friction:` line, first
  occurrence (grep count 1; nothing in LEARNINGS.md or PROGRESS.archive.md).
    $ shasum -a 256 harness.toml Cargo.toml crates/harness/Cargo.toml crates/harness/tests/roles.rs
    b28d83ae… / 5f693c40… / 03fb9e7d… / 0e2ec946… → all four equal the committed values; `git show
    2ee7163:test-hashes.json` holds the same roles.rs value, so T-005's key is kept. harness.toml:15
    `harness_files = ["Cargo.toml", "crates/harness/Cargo.toml"]`, both keyed.
    $ cargo test --workspace -q; cargo clippy --all-targets -q -- -D warnings; cargo fmt --all --check
    exit 0 / 0 / 0; 190+0+8+5+9+15+21+29+27+4+0 = 308 passed, 0 failed, 3 ignored
    $ cargo test -p harness -q --test floor -- every_file_test_hashes_covers_still_hashes_to_its_recorded_digest
    test result: ok. 1 passed; 0 failed; 0 ignored; 16 filtered out
    $ cargo build -q && ./target/debug/harness probe | grep -E "rail-unenforced|hash-uncovered|RAILS.md:58"
    PROBE rail-unenforced 0 / PROBE hash-uncovered 0 (no FINDING line)
  Gate rejection reproduced as a false positive: `git diff c529b4d 58ad8db -- test-hashes.json` shows the
  roles.rs key on a `-` line (no trailing comma) and a `+` line (trailing comma) with an identical value;
  gates.rs:502 `recut_keys` reads the diff text, gates.rs:483 `keys_at` finds the key at base, so it is
  classed re-cut. The launcher's `iter_base` is the iteration's starting HEAD (pipeline.rs:365), and this
  iteration's diff on test-hashes.json is empty, so `gate_scope` has nothing to match on the re-run.
  Break attempt: `sed` one hex digit of the Cargo.toml key → floor test FAILED at floor.rs:306, left:
  ["Cargo.toml"] right: []; `git checkout -- test-hashes.json` restored, porcelain empty. The test's
  second assertion iterates the whole map over the repo root, not a hardcoded key (floor.rs:306).
  Ponytail: four JSON keys, nothing to cut. The value-aware compare the friction names belongs to a
  gates.rs task the probe can raise; not fixed here (off scope, and not mine to fix).

## [T-007] tdd is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: done
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
  2026-09-08 implementer: one line, `gate = "none"` → `gate = "spec-untested"` on the tdd entry
  (harness.default.toml:192); no other entry changed. Red first: `./target/debug/harness probe` at
  9b6c821 emitted `FINDING skill-ungated harness.toml:0 tdd is declared with gate: none ...`
  (PROBE skill-ungated 3). Green: after `cargo build -q`, `PROBE skill-ungated 2`, the two lines left
  are review-received and review-requested (T-008, T-009). Scrutinise: the gate name is
  `spec-untested`, which is in probes/mod.rs:56 `NAMES`, so the "names gate" branch of the probe stays
  silent; the tdd entry's source, path, rev and why are byte-identical to 9b6c821.
  Commands:
    $ sed -i '' '/^id = "tdd"$/,/^gate = /{s/^gate = "none"$/gate = "spec-untested"/;}' crates/harness/harness.default.toml
    $ cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate
    test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out
    $ cargo build -q && ./target/debug/harness probe | grep skill-ungated
    PROBE skill-ungated 2 (review-received, review-requested; no tdd line)
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
    test result: ok. 190 passed; 0 failed; 1 ignored (+ 8, 5, 9, 15 (2 ignored), 21, 29, 27, 4 passed) → exit=0
    $ git add crates/harness/harness.default.toml TASKS.md PROGRESS.md && git commit -m "feat(config): T-007 tdd names spec-untested as its gate"
    [dogfood/rust-port 315f0d0] feat(config): T-007 tdd names spec-untested as its gate — 3 files changed, 25 insertions(+), 2 deletions(-); amended to carry this line
  2026-09-08 verifier: VERIFIED. Tree clean (`git status --porcelain` empty); the work is 70b2622,
  base 9b6c821 (origin/main 52e8799 is the merge-base for the whole port, so the task's own diff is
  9b6c821..70b2622). `git diff 9b6c821 70b2622 --stat` → PROGRESS.md, TASKS.md, harness.default.toml
  only; test-hashes.json untouched. The toml diff is one line, `-gate = "none"` / `+gate = "spec-untested"`
  at :192; `git show 9b6c821:crates/harness/harness.default.toml` confirms source, path, rev, why
  byte-identical. `spec-untested` is probes/mod.rs:56 `NAMES`, and the unknown-name branch exists at
  probes/skill_ungated.rs:57 ("names gate ... which neither ... defines"), which is what the test's
  `contains("names gate ")` filter (tests/probes.rs:342) would catch. Baseline is empty; no failure seen,
  none matched. PROGRESS.md entry carries `friction: none`.
  Commands:
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo chain-exit=$?
    chain-exit=0 — per-binary: 190 (1 ignored), 0, 8, 5, 9, 15 (2 ignored), 21, 29, 27, 4, 0 passed → 308 passed, 3 ignored, 0 failed
    $ cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate
    test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out
    $ cargo build -q && ./target/debug/harness probe | grep skill-ungated
    PROBE skill-ungated 2 (review-received, review-requested; no tdd line)
  ponytail: nothing to audit; a one-line config edit is the floor.

## [T-008] review-received is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: done
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
  2026-09-08 implementer: one line, harness.default.toml:216 `gate = "none"` → `gate = "rejection-repeat"`;
  `git diff --stat` → 1 file changed, 1 insertion(+), 1 deletion(-). Red before the edit:
  `./target/debug/harness probe` → `PROBE skill-ungated 2` with a FINDING naming review-received;
  after `cargo build -q`: `PROBE skill-ungated 1`, the one left is review-requested (T-009).
  `cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate` → 1 passed.
  Reviewer: confirm the diff is that one line and that `rejection-repeat` is a probe name
  (probes/mod.rs:72); the probe must be read from a fresh `cargo build`, not the `harness` symlink.
  Gate: `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0, 308 passed, 3 ignored, 0 failed. Commit: `git add crates/harness/harness.default.toml TASKS.md
  PROGRESS.md && git commit -q -m "feat(config): T-008 review-received names rejection-repeat as its gate"`
  → d1943e2 (amended to carry these lines); `git status --porcelain` → empty.
  2026-09-08 verifier: VERIFIED at cbb7e3b. `git status --porcelain` → empty. `git show --stat HEAD` →
  PROGRESS.md, TASKS.md, harness.default.toml only; `git diff HEAD~1 -- crates/harness/harness.default.toml`
  → one hunk, `-gate = "none"` / `+gate = "rejection-repeat"` at line 216 under `id = "review-received"`;
  source, path, rev, why untouched. `git diff HEAD~1 -- test-hashes.json .check-baseline` → empty;
  .check-baseline has no failure lines. `grep -n '"rejection-repeat"' probes/mod.rs` → 72, 99.
  `touch src/lib.rs && cargo build -q && ./target/debug/harness probe | grep skill-ungated` →
  `PROBE skill-ungated 1`, the FINDING names review-requested (T-009); lines naming review-received: 0.
  `cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate` → 1 passed
  (body asserts 7 entries and no "names gate" finding, so an invented gate name fails it).
  `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; per-binary totals 15+190+21+27+29+4+5+8+9 = 308 passed, 3 ignored, 0 failed. PROGRESS.md
  entry carries `friction: none`. Ponytail: one-line config change, nothing to cut.

## [T-009] review-requested is declared with gate: none -- nothing fails without it, so relying on it is a hope
scope: crates/harness/harness.default.toml
blockedBy: none
status: done
gate: the verifier returned done and the gate was red (exit 101) at 3ae257d. agent::tests::a_signalled_child_reports_128_plus_the_signal; check tail: agent::tests::a_signalled_child_reports_128_plus_the_signal | test result: FAILED. 190 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 2.41s | error: test failed, to rerun pass `--lib`
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
  2026-09-09 verifier (third review): VERIFIED. `git status --porcelain` → empty, STOP absent. Base: origin/main
  (52e8799) is an ancestor; `git log 59e31df..HEAD` is ten commits, seven of them T-009's (b3afb37 and the six
  verdict/answer commits) touching only PROGRESS.md, TASKS.md, crates/harness/harness.default.toml; the other
  three are the operator's, not this task's (2557015 harness.toml `fail_name`, 3c534ab test-hashes.json
  re-cut of the `harness.toml` key, 5bbe681 gates.rs, none named T-009). The re-cut key is honest:
  `shasum -a 256 harness.toml` → 415943ae…cef07, equal to the key. The toml hunk over the whole range is
  `-gate = "none"` / `+gate = "verdict-flip"` at line 232 and nothing else; `verdict-flip` is a probe name
  (probes/mod.rs:71, 98; telemetry.rs:356). .check-baseline vs origin/main adds comment lines only, no name.
  The cause of both gate reds is gone: `sh -c 'kill -INT $$'; echo $?` → 130 in this lane (was 0 under the
  backgrounded launcher), and the launcher now runs as pid 46070 under a fresh parent. Criteria, each run here:
    $ sed -n 232p crates/harness/harness.default.toml → gate = "verdict-flip"
    $ cargo build -q; ./target/debug/harness probe | grep -E 'skill-ungated|review-requested' → PROBE skill-ungated 0
    $ ./target/debug/harness probe | grep -c review-requested → 0
    $ cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate
      → 1 passed; 0 failed; 26 filtered out (probes.rs:334 asserts cfg.skill.len() == 7 and no "names gate" finding)
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
      → exit=0; per-binary 191+0+8+5+9+15+21+29+27+4+0 = 309 passed, 3 ignored, 0 failed (one more than the
      308 in the notes above: 5bbe681 added a lib test). `a_signalled_child_reports_128_plus_the_signal` is
      among the 191.
    $ echo '{}' | ./target/debug/harness hook verify-done → exit 0
  All three T-009 PROGRESS.md entries carry `friction:`. The third (PROGRESS.md:110) is the second occurrence
  of "a red the gate cannot name is unanswerable", and `harness probe` → `PROBE friction-repeat 1` says so:
  it belongs in LEARNINGS.md as a gated rule, which is off this scope and the operator's 2557015/5bbe681 have
  since changed the behaviour it describes. Remaining probe findings (check-unnamed 1, ponytail-ceiling 3,
  litter 1, install-stale 1, verdict-flip 1 on this task's two flips, stage-outlier 6) touch no file on scope.
  Ponytail: one config line, nothing to cut. No dependency, no test touched. A scratch crate outside the repo
  to exercise the new `fail_name` regex was denied by the shell sandbox and not run; it is off scope.
  2026-09-09 implementer (after the third gate): the rejection point is answered, not re-implemented;
  harness.default.toml is untouched (`git diff 0620606 HEAD --stat -- crates/harness/harness.default.toml`
  is empty). This time the gate named the test (5bbe681 carries the output tail):
  `a_signalled_child_reports_128_plus_the_signal`, 190 passed; 1 failed, the same cause as the second
  rejection. The launcher is again a `&` job of a non-interactive `zsh -c` (`ps -axo pid,ppid,command`:
  46070 `harness run --iterations 6 --budget-usd 35 --no-tui`, parent 46069 `/bin/zsh -c … (harness run …
  > …/tasks-run-3.log 2>&1 …) & sleep 1`), so its `sh -c` check runs with SIGINT ignored and HEAD's stub
  `kill -INT $$` (3b06ecc agent.rs:609, asserts 130 at :618) exits 0. The lane measures `sh -c 'kill -INT
  $$'` → 130 because the agent CLI resets dispositions in its children, which is why every verifier sees
  green and the gate does not. The fix is already in the tree, uncommitted, and not this task's: the
  operator's working-tree edit to agent.rs (`git diff --stat -- crates/harness/src/agent.rs` → 1 file
  changed, 3 insertions(+), 2 deletions(-)) makes the stub `kill -KILL $$` and the assert 137. Measured the
  way the gate runs it:
    $ zsh -c '(cargo test -p harness --lib -q a_signalled_child_reports_128_plus_the_signal) & wait'
      → test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 191 filtered out; finished in 0.47s
      (the same command under HEAD's stub: `left: 0 right: 130`, 0 passed; 1 failed, second-gate note above)
  agent.rs is off this scope and is the operator's edit, so it is not staged here; until it is committed,
  the verdict gate's dirty-tree rule (gates.rs:892 `verdict_forces_back_a_done_with_a_dirty_tree`) is the
  next red, and it is not this test's. Criteria re-run at 3b06ecc, tree carrying that edit:
    $ sed -n 232p crates/harness/harness.default.toml → gate = "verdict-flip"
    $ cargo build -q; ./target/debug/harness probe | grep -E 'skill-ungated|review-requested' → PROBE skill-ungated 0
    $ ./target/debug/harness probe | grep -c review-requested → 0
    $ cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate
      → test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 0.22s
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
      → exit=0; per-binary 191+0+8+5+9+15+21+29+27+4+0 = 309 passed, 3 ignored, 0 failed
    $ git add TASKS.md PROGRESS.md && git commit -q -m "chore(config): T-009 answers the third verdict-gate rejection, file unchanged" && git status --short
      → 2 files committed (TASKS.md, PROGRESS.md); `git status --short` → ` M crates/harness/src/agent.rs` (operator, unstaged) and `?? STOP`; amended once to carry this line
  2026-09-09 verifier (fourth review): VERIFIED. `git status --porcelain` → empty, STOP absent. Base: origin/main
  (52e8799) is an ancestor; the task's range is 59e31df..HEAD (56844cc). `git diff --name-only 59e31df HEAD` →
  PROGRESS.md, TASKS.md, crates/harness/harness.default.toml, crates/harness/src/agent.rs, crates/harness/src/gates.rs,
  harness.toml, test-hashes.json; the last four are the operator's (2557015, 3c534ab, 5bbe681, 56844cc), none
  named T-009, and T-009's own commits touch only the first three. The toml hunk over the whole range is
  `-gate = "none"` / `+gate = "verdict-flip"` at line 232 and nothing else; `verdict-flip` is a probe name
  (probes/mod.rs:71, 98; telemetry.rs:356). test-hashes.json: one key re-cut, `harness.toml`, in the operator's
  3c534ab; `shasum -a 256 harness.toml` → 415943ae…cef07, equal to the key. .check-baseline is unchanged over the
  range and carries no name. The operator's 56844cc changes the signal test's stub `kill -INT $$` → `kill -KILL $$`
  and its assert 130 → 137: still 128 + the signal, not weakened. Criteria, each run here:
    $ sed -n 232p crates/harness/harness.default.toml → gate = "verdict-flip"
    $ cargo build -q; ./target/debug/harness probe | grep -E 'skill-ungated|review-requested' → PROBE skill-ungated 0
    $ ./target/debug/harness probe | grep -c review-requested → 0
    $ cargo test -p harness -q --test probes -- every_declared_skill_names_its_enforcing_gate
      → test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out (probes.rs:337 asserts 7 skills, :345 no "names gate" finding)
    $ cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo exit=$?
      → exit=0; per-binary 191+0+8+5+9+15+21+29+27+4+0 = 309 passed, 3 ignored, 0 failed
    $ zsh -c '(…the same chain…; echo exit=$?) > /tmp/t009-bg.log 2>&1 & wait'   (the gate's own condition: a `&` job of a non-interactive shell)
      → exit=0; 191+0+8+5+9+15+21+29+27+4+0 = 309 passed, 3 ignored, 0 failed
    $ echo '{}' | ./target/debug/harness hook verify-done → exit 0
  The launcher (pid 83853) is again a `&` job of a non-interactive `zsh -c` (parent 83852), the condition that
  produced the second and third gate reds; with 56844cc on the branch the chain is green under that condition,
  measured above. Scratch probe outside the repo (mktemp dir, harness.toml with `gate = "bogus-gate"`):
  `harness probe` → `PROBE skill-ungated 1`, `FINDING … x names gate bogus-gate, which neither the built-in gate
  and probe names nor .harness/RAILS.md defines`, so the second branch rejects an invented name and
  `verdict-flip` passes it because it is in probes/mod.rs's NAMES. All four T-009 PROGRESS.md entries carry
  `friction:` (PROGRESS.md:96, 103, 110, 117); `PROBE friction-repeat 1` is the "red the gate cannot name"
  second occurrence already flagged in the third review, LEARNINGS.md off scope. Remaining probe findings
  (check-unnamed 1, ponytail-ceiling 3, litter 1, install-stale 1, verdict-flip 1, stage-outlier 6) touch no
  file on scope. The lib test run prints `kill: <pid>: No such process` on stderr from a stub; not a failure,
  not on scope. Ponytail: one config line, nothing to cut. No dependency, no test touched by this task.

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
status: done
rows: none — harness
criteria:
  - every HTML comment block (`<!-- … -->`) in `templates/*.md` is at most two lines and states a format or a rule; a sentence explaining why the rule exists is deleted, e.g. `templates/DECISIONS.md:3` "Two things, in this order, because the top of this file is read far more often than the rest." becomes "Rejected findings first, then archived blocks."
  - `grep -nE '\b(because|which is why|the reason|worth|deliberately|on purpose|it turns out|in practice)\b' templates/*.md roles/*.md skills/running-the-loop/SKILL.md skills/running-the-loop/references/task-block.md evals/README.md adapters/README.md adapters/bun-turbo/README.md` prints nothing, except inside the `Why` column of `templates/RAILS.md`, where each cell is at most one sentence and a citation
  - every role prompt keeps every rule, every rail name and every protocol step it has today; only the sentences that justify them go; `wc -l` of each role file is at most 80% of today's count, pasted before and after in notes:
  - `harness probe` on a fresh `harness init` reports the same `rail-unenforced`, `spec-untested`, `queue-hygiene` and `learning-*` counts as before the change (paste both runs in notes:); `cargo test --workspace -q` exits 0 and the tests in scope are updated only where they assert on removed wording
notes: |
  The owner's rule: a comment is short, and exists only to stop a mistake recurring or to explain hard
  code. Rationale belongs in DECISIONS.md, not in the file an agent reads every session.

  2026-09-09 implementer, at 3e59458: every justifying sentence is gone from the eight templates, the five
  roles, SKILL.md, evals/README.md and the two adapter READMEs; every rule, rail name, `{{skill:}}` token,
  `__TOKEN__` and protocol step is kept (per-role diff of those tokens against HEAD is identical, except
  implementer.md's two RAILS.md rules now share one line). templates/RAILS.md lost its duplicated
  "Rails are named" paragraph; its commented-out product-rails table is now a two-line comment holding
  the three row shapes inline. Red-then-green test `the_seeded_documents_state_rules_and_do_not_argue`
  in tests/init.rs runs the criterion's grep over every seeded `.md` and refuses a comment run past two
  lines (failed at HEAD on 13 grep hits and 8 comment runs; passes now). probes.rs needed no change:
  nothing there asserts on removed wording. Chosen over no test as the easier one to delete.
  Criterion 2's exception names a `Why` column in templates/RAILS.md; that column is in
  templates/SPEC.section.md, and its cells hit nothing, so they are untouched.
  Dropped citations (Gloaguen arXiv:2602.11988, arXiv:2605.29463, arXiv:2605.29668, SlopCodeBench
  arXiv:2603.24755, the Superpowers install quote) stay citable at `git show 3e59458:templates/AGENTS.md`,
  `git show 3e59458:templates/RAILS.md`, `git show 3e59458:templates/SPEC.section.md`,
  `git show 3e59458:adapters/README.md`; DECISIONS.md is off this scope.
  Scrutinise: verifier.md and scout.md now open with one long paragraph (three merged) to hit 80%;
  the blank after each role's frontmatter and before each code fence is gone for the same reason.
  `wc -l roles/*.md` before (HEAD) / after: adjudicator 43/33, implementer 38/30, researcher 42/33,
  scout 52/41, verifier 52/40 (ceilings 34, 30, 33, 41, 41).
  `grep -nE '\b(because|which is why|the reason|worth|deliberately|on purpose|it turns out|in practice)\b' templates/*.md roles/*.md skills/running-the-loop/SKILL.md skills/running-the-loop/references/task-block.md evals/README.md adapters/README.md adapters/bun-turbo/README.md` → no output, exit 1.
  `./target/debug/harness probe` on a fresh `harness init` (git init + empty commit in mktemp -d),
  before at HEAD: `PROBE spec-untested 1`, `PROBE rail-unenforced 2` (RAILS.md:57, :58 test-hashes.json
  does not exist), `PROBE learning-unenforced 0`, `PROBE learning-ungated 0`, `PROBE queue-hygiene 0`;
  after: `PROBE spec-untested 1`, `PROBE rail-unenforced 2` (RAILS.md:42, :43, same message),
  `PROBE learning-unenforced 0`, `PROBE learning-ungated 0`, `PROBE queue-hygiene 0`.
  `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; per-binary 191+0+8+5+9+15+22+29+27+4+0 = 310 passed, 3 ignored, 0 failed.
  Commit: `git add templates/*.md roles/*.md skills/running-the-loop evals/README.md adapters/README.md adapters/bun-turbo/README.md crates/harness/tests/init.rs crates/harness/tests/probes.rs TASKS.md PROGRESS.md && git status --short && git commit -q -m "feat(templates): T-014 …" && git log --oneline -1 && git status --short | wc -l`
  → 20 `M ` lines (PROGRESS.md, TASKS.md, the two adapter READMEs, tests/init.rs, evals/README.md,
  the five roles, SKILL.md, the eight templates), then `3d1a9da feat(templates): T-014 the shipped
  documents state rules and formats, not reasons`, then `0`. Amended once (`git commit --amend
  --no-edit`) to carry this paste; the tree diff is the same.

  2026-09-09 verifier, at f7a71f0: VERIFIED.
  `git status --porcelain` → empty. `$BASE` = HEAD~1 (origin/main resolves but is 169 files behind this
  branch; the task is one commit). `git diff HEAD~1 --name-only` → 20 files, all on scope plus
  TASKS.md/PROGRESS.md. `git diff HEAD~1 -- test-hashes.json .check-baseline` → empty; `.check-baseline`
  has no failure lines. `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  run twice → exit 0 each; per-binary 191+0+8+5+9+15+22+29+27+4+0 = 310 passed, 3 ignored, 0 failed;
  clippy output empty; fmt exit 0.
  Criterion 1: awk over `templates/*.md` lists every `<!-- … -->` block with its line count: AGENTS.md
  3-4 (2), 40 (1), 44-45 (2); DECISIONS.md 15 (1); LEARNINGS.md 3-4 (2); pointer.md 3-4 (2); RAILS.md
  16-17 (2). None over two; templates/DECISIONS.md:3 reads exactly the criterion's replacement.
  Criterion 2: the criterion's grep → no output, exit 1; the same grep with `-i` → also no output.
  Criterion 3: `git show HEAD~1:roles/<r>.md | wc -l` / `wc -l roles/<r>.md`: adjudicator 43/33,
  implementer 38/30, researcher 42/33, scout 52/41, verifier 52/40; all at or under 80%. Backtick-token
  set diff (rail names, `{{skill:}}`, `__TOKEN__`) per role and template before/after: identical, except
  templates/SPEC.section.md lost the word `lint` from the deleted SlopCodeBench paragraph while the
  `lint` step itself stays at SPEC.section.md:51. Every numbered protocol step and every `- ` hard rule
  is present in the after-diff, merged or verbatim. Letter/spirit gap, on the record: `wc -w` fell
  adjudicator 855→799, implementer 546→522, researcher 902→837, scout 806→779, verifier 1123→989
  (88–96%); non-blank lines 33→31, 31→28, 31→29, 42→38, 43→38. The 80% line ceiling was reached by
  deleting blank lines and merging paragraphs, which the implementer's own notes flag. The criterion
  says `wc -l`; met as written, and every sentence removed is a justification, none a rule.
  Criterion 4: built HEAD~1 from `git archive` in a temp dir; fresh `git init` + empty commit +
  `harness init --adapter claude` + `harness probe` with each binary. Before (HEAD~1): spec-untested 1,
  rail-unenforced 2 (RAILS.md:57, :58), learning-unenforced 0, learning-ungated 0, queue-hygiene 0,
  install-stale 0, litter 0. After (HEAD): spec-untested 1, rail-unenforced 2 (RAILS.md:42, :43),
  learning-unenforced 0, learning-ungated 0, queue-hygiene 0, install-stale 0, litter 0. Same counts.
  Red check: appended the new test to the HEAD~1 tree's tests/init.rs and ran
  `cargo test -q -p harness --test init the_seeded_documents_state_rules_and_do_not_argue` → FAILED,
  20 offences (12 grep hits, 8 comment runs); the notes say 13 grep hits, my run says 12. probes.rs
  untouched, and no test in scope asserted on removed wording (full suite green without edits).
  Frauds: no test weakened or deleted (init.rs diff is one added test); no dependency added; no
  launcher/hook/check-script edit (`rows: none — harness` anyway); no litter (`git status` clean).
  Ponytail: the test is 30 lines on existing `Repo`/`install`/`walk`/`read` helpers; nothing to cut.
  Two things for whoever is next, neither a rejection: (1) `./target/debug/harness probe` on this
  checkout now reports `install-stale 7` (`.harness/roles/*.md`, `.harness/RAILS.md`,
  `.claude/skills/running-the-loop/SKILL.md`), up from 1 with the HEAD~1 binary; those installed copies
  are off T-014's scope, so re-run `harness init` to resync them. (2) PROGRESS.md:131's friction is
  the third occurrence (`friction-repeat` reports it, 3 times) and LEARNINGS.md carries no line for
  the scope-noise rule; it is owed by the next task with LEARNINGS.md on scope.

## [T-016] the licence is MIT
scope: LICENSE, NOTICE, Cargo.toml, crates/harness/Cargo.toml, README.md, crates/harness/tests/floor.rs
blockedBy: none
status: done
rows: none — harness
criteria:
  - `LICENSE` is the MIT License text with `Copyright (c) 2026 Thomas Taylor`; `NOTICE` is deleted; `git ls-files NOTICE` prints nothing
  - `crates/harness/Cargo.toml` has `license = "MIT"`; `grep -rn 'Apache' --include='*.md' --include='*.toml' --include='*.rs' --include='*.yml' . | grep -v target/ | grep -v docs/superpowers/` prints nothing
  - `README.md`'s License section reads `MIT. See \`LICENSE\`.`
  - a test in `crates/harness/tests/floor.rs` named `the_licence_is_mit` asserts `LICENSE` starts with `MIT License` and `Cargo.toml` names `MIT`; `cargo test --workspace -q` exits 0
notes: |
  Owner's decision 2026-09-09.

  2026-09-10 implementer, at 1adfc46: LICENSE is the MIT text (21 lines, `Copyright (c) 2026 Thomas
  Taylor` on line 3); `git rm NOTICE`; crates/harness/Cargo.toml:6 `license = "MIT"` by `sed` (the file
  is hashed, the edit hook refuses it) and its test-hashes.json key re-cut with `shasum -a 256` →
  915284c7…4a17f, the other three keys untouched; README.md:280 `MIT. See \`LICENSE\`.`; Cargo.toml
  (workspace) has no licence key and is unchanged. Red-then-green `the_licence_is_mit` in tests/floor.rs
  (LICENSE starts with `MIT License`; crates/harness/Cargo.toml contains `license = "MIT"`): at 1adfc46
  `cargo test -p harness -q --test floor the_licence_is_mit` → 1 failed at floor.rs:633; now green.
  Criterion 1: `git ls-files NOTICE | wc -l` → 0. Criterion 2: its grep → no output, exit 1
  (TASKS.md's own criterion line is dropped by its `grep -v target/`). Criterion 3: README.md:278-280
  is `## License`, blank, `MIT. See \`LICENSE\`.`.
  `./target/debug/harness probe` before and after, same lines: hash-uncovered 0, check-unnamed 1
  (AGENTS.md:7), litter 1 (test-hashes.json), install-stale 7; nothing new. harness.toml:18 `docs`
  still names NOTICE; no probe reports it, the file is hashed and off scope.
  Scrutinise: the test-hashes.json re-cut is for a file on this task's scope line (harness-lane exempts
  that, RAILS.md:63); the test reads the crate manifest, not the workspace Cargo.toml, since only the
  crate carries `license`.
  `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit=0; per-binary 191+0+9+5+9+16+22+29+27+4+0 = 312 passed, 3 ignored, 0 failed; clippy and fmt
  output empty.
  Commit (NOTICE was staged by `git rm` already, so `git add NOTICE` fails with "pathspec did not match"): `git add LICENSE Cargo.toml crates/harness/Cargo.toml README.md crates/harness/tests/floor.rs test-hashes.json TASKS.md PROGRESS.md && git status --short && git commit -q -m "feat(licence): T-016 the licence is MIT" && git log --oneline -1 && git status --short | wc -l`
  → `M  LICENSE`, `D  NOTICE`, `M  PROGRESS.md`, `M  README.md`, `M  TASKS.md`, `M  crates/harness/Cargo.toml`,
  `M  crates/harness/tests/floor.rs`, `M  test-hashes.json`, then `196882c feat(licence): T-016 the licence
  is MIT`, then `0`. Amended once (`git commit --amend --no-edit`) to carry this paste; the tree diff is the same.

  2026-09-10 verifier, at 1b67f3d: REJECTED: criterion 2 fails at HEAD.
  - Criterion 2's grep (the second command on TASKS.md:181, run verbatim) prints `TASKS.md:193:  Criterion
    1: ... Criterion 2: the A***** grep → no output, exit 1` and exits 0; the criterion says it prints nothing
    and `grep -rln` names TASKS.md as the only file. The hit is the implementer's own note at TASKS.md:193,
    written after the grep was run and committed in 1b67f3d, so the tree the note describes is not the tree
    on the branch. The criterion line itself survives only because its `grep -v target/` drops it. Fix: reword
    that note (say "the criterion 2 grep"), and keep the licence's old name out of every note, verdict and
    PROGRESS.md line while this block exists, or the criterion can never hold. This verdict does the same.
  Everything else holds; nothing below needs re-doing:
  - `git status --porcelain` → empty before and after; the work is committed as 1b67f3d.
  - Criterion 1: LICENSE line 1 `MIT License`, line 3 `Copyright (c) 2026 Thomas Taylor`, 21 lines; body from
    `Permission` diffed word-for-word against `~/.cargo/registry/src/*/anyhow-1.0.104/LICENSE-MIT` → identical.
    `git ls-files NOTICE | wc -l` → 0; `ls NOTICE` → No such file.
  - Criterion 3: README.md:278-280 → `## License`, blank, `MIT. See \`LICENSE\`.`
  - Criterion 4: red-green reproduced, not trusted: `git show HEAD~1:LICENSE > LICENSE; cargo test -p harness
    -q --test floor the_licence_is_mit` → `panicked at crates/harness/tests/floor.rs:633:5`, 0 passed 1 failed;
    `git checkout -- LICENSE` → porcelain empty. The test's two asserts are the criterion's two clauses.
  - `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
    → exit=0; 11 binaries, 312 passed, 3 ignored, 0 failed, matching the implementer's paste; clippy and fmt
    empty. `.check-baseline` has no entries and gained none.
  - Diff base: origin/main is 125 commits behind (171 files), so the task's own commit is the base:
    `git diff HEAD~1 --name-only` → LICENSE, NOTICE, PROGRESS.md, README.md, TASKS.md, crates/harness/Cargo.toml,
    crates/harness/tests/floor.rs, test-hashes.json. All on `scope:` or the loop's record; test-hashes.json
    moved one key, `crates/harness/Cargo.toml`, an on-scope file under `rows: none — harness` (harness-lane
    exempt, RAILS.md:63); `shasum -a 256` → 915284c7…4a17f matches the key. That file's diff is one line,
    `license = "MIT"`. floor.rs diff is one added test; no test weakened, no dependency added, no network,
    no litter. Workspace Cargo.toml unchanged.
  - PROGRESS.md T-016 entry has a `friction:` line and calls itself the second occurrence of "promoted with
    criteria its scope cannot reach"; `harness probe` at 1b67f3d does not yet emit friction-repeat for it, but
    a third would. It belongs in LEARNINGS.md via `harness eval --gate`, off this task's scope. Probe output
    before and after is the same set (check-unnamed 1, litter 1 test-hashes.json, friction-repeat 2 for older
    entries); nothing new from this task.
  - Ponytail: 11-line test on the file's existing `repo_root` and `read` helpers; nothing to cut.
  - Off scope, for a future task and not a rejection reason: `harness.toml:18` and
    `crates/harness/harness.default.toml:61` both still list `NOTICE` in `docs`; the second ships to every
    `harness init`.

  2026-09-10 implementer, at db2005f: answers the one rejection point; nothing re-implemented, no
  scope file changed. (1) TASKS.md:193 quoted the old licence's name inside the first take's note, so
  criterion 2's grep hit its own record; the phrase is now `its grep`. Same command run verbatim before
  and after: at db2005f → `TASKS.md:193:  Criterion 1: ...`, exit 0; now → no output, exit 1. This note,
  the PROGRESS.md entry and the commit message carry no form of that name; `git grep -n` for it over
  `*.md *.toml *.rs *.yml` → only the criterion line TASKS.md:180, which its own `grep -v target/` drops.
  No test added: encoding criterion 2 in floor.rs would put the pattern in a `.rs` file the grep reads.
  `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit=0; per-binary 191+0+9+5+9+16+22+29+27+4+0 = 312 passed, 3 ignored, 0 failed; clippy and fmt
  output empty. Same figure as the verdict.
  Scrutinise: the diff against db2005f is TASKS.md and PROGRESS.md only; criteria 1, 3 and 4 are as
  the verdict verified them and were not re-run here beyond the full check.
  Commit: `git add LICENSE Cargo.toml crates/harness/Cargo.toml README.md crates/harness/tests/floor.rs TASKS.md PROGRESS.md && git status --short && git commit -q -m "chore(licence): T-016 answers the criterion 2 rejection, note reworded, no scope file changed" && git log --oneline -1 && git status --short | wc -l`
  → `M  PROGRESS.md`, `M  TASKS.md`, then `b31dcc7 chore(licence): T-016 answers the criterion 2 rejection,
  note reworded, no scope file changed`, then `0`. Amended once (`git commit --amend --no-edit`) to carry
  this paste; the tree diff is the same.

  2026-09-10 verifier, at 7ce8f54: VERIFIED. The one rejection point is answered and every criterion re-run
  holds at HEAD; this verdict also keeps the old licence's name out.
  - `git status --porcelain` → empty before and after every step below.
  - Criterion 2, the rejection point: the grep on TASKS.md:181 run verbatim → no output, exit 1 (was one
    TASKS.md hit at 1b67f3d). `git diff db2005f --name-only` → PROGRESS.md, TASKS.md: the second take changed
    the record only, as its note says. crates/harness/Cargo.toml:6 `license = "MIT"`.
  - Criterion 1: LICENSE line 1 `MIT License`, line 3 `Copyright (c) 2026 Thomas Taylor`, `wc -l` → 21; body
    from `Permission` diffed whitespace-folded against `~/.cargo/registry/src/*/anyhow-1.0.104/LICENSE-MIT`
    → identical. `git ls-files NOTICE` → empty; `ls NOTICE` → No such file or directory.
  - Criterion 3: README.md:278-280 → `## License`, blank, `MIT. See \`LICENSE\`.`
  - Criterion 4: both asserts reproduced red, not trusted. `git show 1adfc46:LICENSE > LICENSE; cargo test -p
    harness -q --test floor the_licence_is_mit` → panicked at floor.rs:633:5, 0 passed 1 failed; restored.
    `git show 1adfc46:crates/harness/Cargo.toml` swapped in the same way → panicked at floor.rs:639:5, 0 passed
    1 failed; restored. Then → 1 passed. The test reads the tree via `repo_root()` (floor.rs:7, CARGO_MANIFEST_DIR
    joined with `../..`), so it is not a fixture of itself.
  - `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
    → exit=0; 11 binaries, 191+0+9+5+9+16+22+29+27+4+0 = 312 passed, 3 ignored, 0 failed; clippy and fmt
    output empty. `.check-baseline` has no entries and gained none.
  - Diff base: origin/main is 127 commits behind, so the task's own base 1adfc46 is `$BASE`.
    `git diff 1adfc46 --name-only` → LICENSE, NOTICE, PROGRESS.md, README.md, TASKS.md, crates/harness/Cargo.toml,
    crates/harness/tests/floor.rs, test-hashes.json: all on `scope:` or the loop's record. test-hashes.json moved
    one key, `crates/harness/Cargo.toml`, which is on the scope line under `rows: none — harness` (RAILS.md:63
    exempts it); `shasum -a 256` → 915284c7…4a17f matches the key, and that file's whole diff is the one
    `license` line. floor.rs diff is one added test; no test weakened, no dependency added, workspace Cargo.toml
    unchanged, no network, no new litter.
  - PROGRESS.md: both T-016 entries carry a `friction:` line (PROGRESS.md:145, :152). The :152 line calls itself
    the second occurrence of the record tripping its own check; `./target/debug/harness probe` at 7ce8f54 emits
    friction-repeat 2 for older entries only (PROGRESS.md:110, :131), litter 1 (test-hashes.json), check-unnamed 1,
    hash-uncovered 0, the same set as at 1b67f3d. The friction line the :145 and :152 entries call due belongs in
    LEARNINGS.md via `harness eval --gate`, off this scope.
  - Ponytail: 11-line test on the file's existing helpers; nothing to cut.
  - Off scope and not a rejection reason, unchanged from the first verdict: harness.toml:18 and
    crates/harness/harness.default.toml:61 still list `NOTICE` in `docs`.

## [T-015] README.md, docs/intent.md and harness.default.toml describe, and do not argue
scope: README.md, docs/intent.md, crates/harness/harness.default.toml, crates/harness/src/config.rs, crates/harness/tests/cli.rs, AGENTS.md, harness.toml, test-hashes.json, templates/pointer.md, templates/RAILS.md, .harness/RAILS.md, CLAUDE.md, GEMINI.md, QWEN.md, .github/copilot-instructions.md
blockedBy: none
status: done
gate: operator widened the scope to the files the verifier named on 2026-09-10; criteria unchanged
rows: none — harness
criteria:
  - `README.md` is at most 280 lines; every section is a table, a fenced command, or sentences in the present tense that state what a command or field does; `grep -nE '\b(because|which is why|the reason|worth|deliberately|on purpose|we |our )\b' README.md` prints nothing
  - every `_comment`-style key or `#` comment in `crates/harness/harness.default.toml` is at most one line stating what the key is; the cited papers move to `docs/intent.md` under one heading `## References` as a list, one line each, and appear nowhere else; the `include_str!`/migration tests in `config.rs` and `cli.rs` still pass
  - `docs/intent.md` keeps its headings Problem, Proposed outcome, What exists today, Where it goes, Affected users and systems, Constraints, Open questions, References; every bullet under them is a statement or a measurement with its command; the same grep as above over `docs/intent.md` prints nothing
  - `cargo test --workspace -q` exits 0; `harness probe` reports `check-unnamed 0` and `litter 0` after the change (paste in notes:)
notes: |
  Same rule as T-014. Measurements stay; sentences about why a measurement matters go.

  2026-09-09 implementer, at d0d5c77: README.md 359 → 280 lines (`wc -l`); the preset, events, `[agent]`,
  `[check]`, `[[pipeline]]`, halt and `harness skills` tables are sentences now, `[[skill]]` and `[[role]]`
  share one table, the TUI section folded into "What a run does", and the `pointer_files` default names
  QWEN.md as harness.default.toml does. harness.default.toml 253 → 204 lines: ten `#` comments, each one
  line naming its key; the commented-out `[[role]]` example is one line; agents.md, claude-code#34235,
  arXiv:2605.29668 and arXiv:2602.11988 moved to docs/intent.md `## References` (five lines, with the
  playbook URL) and are in no other scope file. docs/intent.md 242 → 120 lines: the eight headings in
  the criterion's order, nothing else; "What is already proven" and "The one thing intent has to resolve
  first" are gone, the control table names the Rust paths (gates.rs, hooks.rs, agent.rs) instead of the
  retired `.sh` ones, the probe count reads twenty-one (`./target/debug/harness probe | grep -c '^PROBE '`
  → 21), and the open question on `.harness/**` drift is dropped (the `install-stale` probe answers it).
  Red-then-green test `the_shipped_documents_describe_and_do_not_argue` in tests/cli.rs: README length,
  the criterion grep over README.md and docs/intent.md, the heading list, `#` comment runs and `arXiv`
  in `DEFAULT_TOML`; 24 offences at HEAD, 0 now. config.rs untouched: no test there reads comment text.
  Criterion 1 grep → no output, exit 1. Criterion 3 grep → no output, exit 1.
  Criterion 4, `./target/debug/harness probe check-unnamed litter` before (d0d5c77) and after, identical:
  `PROBE check-unnamed 1` (AGENTS.md:7 names the check without harness.toml's `export PATH=…; ` prefix;
  check_unnamed.rs:50 is `line.contains(check)`) and `PROBE litter 1` (test-hashes.json is tracked and
  harness.toml's `docs` array replaces the default whole without it). Both findings predate this task,
  and AGENTS.md, harness.toml and test-hashes.json are off scope, so the two counts stay 1 here; the
  one-line fixes are the `export PATH` prefix on AGENTS.md:7 and `"test-hashes.json"` in harness.toml's
  `docs` (hashed file, shell edit). The three document criteria are met in full.
  Scrutinise: the presets table is now prose naming only which presets report usage, the per-preset
  command lines live in `crates/harness/adapters/presets/*.toml`; intent.md's "Where it goes" item 5 is
  three words. Still carrying the moved citations off scope: AGENTS.md:48, CLAUDE.md:4, GEMINI.md:4,
  QWEN.md:4, .github/copilot-instructions.md:4, templates/pointer.md:3, .harness/RAILS.md:65.
  `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; per-binary 191+0+9+5+9+15+22+29+27+4+0 = 311 passed, 3 ignored, 0 failed.
  Commit: `git add README.md docs/intent.md crates/harness/harness.default.toml crates/harness/src/config.rs crates/harness/tests/cli.rs TASKS.md PROGRESS.md && git status --short && git commit -q -m "feat(docs): T-015 …" && git log --oneline -1 && git status --short | wc -l`
  → `M  PROGRESS.md`, `M  README.md`, `M  TASKS.md`, `M  crates/harness/harness.default.toml`,
  `M  crates/harness/tests/cli.rs`, `M  docs/intent.md` (config.rs unchanged), then `e7ef427 feat(docs):
  T-015 README.md, docs/intent.md and harness.default.toml describe, and do not argue`, then `0`.
  Amended once (`git commit --amend --no-edit`) to carry this paste; the tree diff is the same.

  2026-09-10 verifier, at 1f84fe8: REJECTED: two criteria fail as written, and neither can be met from
  this task's `scope:`, so the fix is the adjudicator's (widen the scope or amend the criterion), not
  another implementation. The document work itself reproduces and stays on the branch.
  (1) Criterion 4 asks for `check-unnamed 0` and `litter 0`. `./target/debug/harness probe` at
  1f84fe8 → `PROBE check-unnamed 1` (`FINDING check-unnamed AGENTS.md:7 the Commands section names
  no command matching harness.toml check (export PATH=…)`) and `PROBE litter 1` (`FINDING litter
  test-hashes.json:0 tracked and neither product nor a document that governs it`). Same binary in a
  `git worktree add /tmp/t015-base HEAD~1` → the same two lines, so both predate the task, as the
  notes say. The fixes are AGENTS.md:7 and harness.toml's `docs` array; neither file is on scope.
  Resolution: add `AGENTS.md, harness.toml` to `scope:` (`rows: none — harness` already permits
  harness.toml) or strike the probe clause from criterion 4. Until then an implementer taking this at
  `ready` should park it `blocked` with this reason rather than re-implement.
  (2) Criterion 2 says the moved citations "appear nowhere else". `grep -rnE 'arXiv|2602\.11988|2605\.29668|34235' --include='*.md' --include='*.toml' --include='*.rs' . | grep -vE 'target/|docs/superpowers/|docs/intent.md|PROGRESS|DECISIONS|TASKS.md'`
  → claude-code#34235 at CLAUDE.md:4, GEMINI.md:4, QWEN.md:4, .github/copilot-instructions.md:4
  (all generated from templates/pointer.md:3); arXiv:2602.11988 at AGENTS.md:48; arXiv:2605.29668 at
  .harness/RAILS.md:65. All off scope, all disclosed in the implementer's notes. Same resolution:
  scope in AGENTS.md and templates/pointer.md (the pointers regenerate from it), or narrow the
  clause to the scope files.
  Everything else verified, and it holds for the re-take:
  `git status --porcelain` → empty; `$BASE` = HEAD~1 (origin/main resolves but is the whole Rust
  port behind; the task is one commit). `git show HEAD --name-only` → README.md, docs/intent.md,
  crates/harness/harness.default.toml, crates/harness/tests/cli.rs, TASKS.md, PROGRESS.md; config.rs
  untouched and on scope. `git diff HEAD~1 --stat -- .harness .check-baseline test-hashes.json` → empty;
  `.check-baseline` has no failure lines.
  `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; per-binary 191+0+9+5+9+15+22+29+27+4+0 = 311 passed, 3 ignored, 0 failed; clippy and fmt
  output empty. Matches the notes' figure.
  Criterion 1: `wc -l README.md` → 280; the criterion grep → no output, exit 1. Tense scan
  `grep -nE '\b(was|were|had|will|would|should)\b'` → README.md:49 (`when nothing was observed`, a
  present-tense halt clause) and :132 (`has drifted`); neither argues.
  Criterion 2, the parts that hold: `grep -n '^\s*#' crates/harness/harness.default.toml` → ten
  lines (1, 3, 8, 15, 19, 23, 102, 105, 146, 204), none adjacent, each one line; `wc -l` → 204.
  docs/intent.md `## References` is five one-line bullets, each with a URL.
  Criterion 3: `grep -n '^## ' docs/intent.md` → the eight headings in the criterion's order and no
  others; the criterion grep → no output, exit 1; `wc -l` → 120. The control table's new names
  resolve: `crates/harness/src/{gates,hooks,agent}.rs` exist, gates.rs:61-63 dispatch `verdict`,
  `scope`, `check-delta`. "twenty-one" re-derived: `./target/debug/harness probe | grep -c '^PROBE '`
  → 21. Crate list in Constraints matches `crates/harness/Cargo.toml` `[dependencies]` (twelve plus
  tempfile). `ls crates/harness/adapters/presets/*.toml | wc -l` → 13, as README says.
  Red check re-derived from HEAD~1 with shell instead of a second build: old README 359 lines (1
  offence) and 2 grep hits; old intent 8 grep hits and a heading list of nine (1); old toml 2 `arXiv`
  lines and 10 two-line comment runs (awk) → 1+2+8+1+2+10 = 24, the notes' figure. The new test
  reads the same four inputs, so it was red at HEAD~1 and is green now.
  Frauds: no test weakened or deleted (cli.rs diff is one added test); `regex` is already a
  dependency, none added; no launcher/hook/check-script/baseline/hash edit; no network call; no
  litter (`git status` clean, worktree removed, `git worktree list` → one entry).
  PROGRESS.md:134-139 carries a `friction:` line; `grep -n check-unnamed PROGRESS.md LEARNINGS.md`
  → only this entry, so first occurrence as stated. `friction-repeat 2` at PROGRESS.md:131 and :110
  predates this task and is still owed a LEARNINGS.md line.
  Ponytail: 62-line test on `std::fs` and the existing `regex` dep, one caller each; nothing to cut.
  Minor, not a rejection: cli.rs:245 reports the comment-run offence at index `i` while :250 uses
  `i + 1`, so the run message is one line low; the presets table collapsed to prose loses the
  per-preset command line, which now lives only in `adapters/presets/*.toml`.

  2026-09-10 implementer, at 1adfc46: parked `blocked` as the verdict above directs; nothing re-implemented,
  the document work stays on the branch. Both open points need files off `scope:` (AGENTS.md:7 and
  harness.toml's `docs` for criterion 4; AGENTS.md:48, templates/pointer.md:3, .harness/RAILS.md:65 for
  criterion 2's "nowhere else"). `blockedBy: none` keeps it parked: queue.rs:193-198 releases a blocked
  task only when it has a non-empty blocker list and every blocker is done. To resume: add those files to
  `scope:` or narrow the two clauses to the scope files, then set `ready`.

  2026-09-10 implementer, at 8f0cb50, on the widened scope: both rejection points answered, nothing off scope
  touched. (1) AGENTS.md:8-9 carry harness.toml's check verbatim (`export PATH=…` prefix); harness.toml's `docs`
  names test-hashes.json and the file's key in test-hashes.json is re-cut with `shasum -a 256` (on-scope file,
  exempt under harness-lane). (2) templates/pointer.md is `See @__CONTEXT_FILE__.` plus a one-line comment with
  no citation, and CLAUDE.md, GEMINI.md, QWEN.md and .github/copilot-instructions.md are its render; AGENTS.md's
  two comments are the template's (templates/AGENTS.md:3-4, :46-47); .harness/RAILS.md:65 is templates/RAILS.md:50
  verbatim. templates/RAILS.md carried no moved citation and is unchanged. The test
  `the_shipped_documents_describe_and_do_not_argue` in tests/cli.rs grew the two criteria: the four citation tokens
  over the context file, the pointer files (from `cfg.layout.pointer_files`), templates/pointer.md, both RAILS.md;
  `probes::run_all` on `check-unnamed` against the repo with a stubbed green check; `cfg.layout.docs` naming
  test-hashes.json (litter's tracked-file half; the untracked half is not asserted so a scratch file in a dev
  tree cannot fail the suite). Red at 8f0cb50: nine offences, the verdict's seven citation lines plus
  check-unnamed and docs. Green now.
  `./target/debug/harness probe` after `cargo build -q` → `PROBE check-unnamed 0`, `PROBE litter 0`,
  `PROBE hash-uncovered 0`; `PROBE install-stale 7` before and after (.harness/RAILS.md differs from the
  template in rows this task did not touch; not a criterion).
  Criterion 1: `wc -l README.md` → 280; the grep over README.md and docs/intent.md → no output, exit 1.
  Criterion 2 grep `grep -rnE '2602\.11988|2605\.29668|34235|agents\.md' --include='*.md' --include='*.toml'
  --include='*.rs' . | grep -vE 'target/|docs/superpowers/|docs/intent.md|PROGRESS|DECISIONS|TASKS.md'`
  → one line, crates/harness/tests/cli.rs:266, the test's own token list; no document cites a paper.
  crates/harness/tests/fixtures/harness.default.json:13 names 34235 as migration input; not .md/.toml/.rs and
  off scope.
  `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; per-binary 191+0+9+5+9+16+22+29+27+4+0 = 312 passed, 3 ignored, 0 failed; clippy and fmt output empty.
  Scrutinise: the test reads the live repo through `config::load(root)`, so a future `pointer_files` edit widens
  its reach silently; `.harness/RAILS.md` still fails install-stale for unrelated rows.
  Commit: `git add README.md docs/intent.md crates/harness/harness.default.toml crates/harness/src/config.rs crates/harness/tests/cli.rs AGENTS.md harness.toml test-hashes.json templates/pointer.md templates/RAILS.md .harness/RAILS.md CLAUDE.md GEMINI.md QWEN.md .github/copilot-instructions.md TASKS.md PROGRESS.md && git status --short && git commit -q -m "feat(docs): T-015 …" && git log --oneline -1 && git status --short | wc -l`
  → `M  .github/copilot-instructions.md`, `M  .harness/RAILS.md`, `M  AGENTS.md`, `M  CLAUDE.md`, `M  GEMINI.md`,
  `M  PROGRESS.md`, `M  QWEN.md`, `M  TASKS.md`, `M  crates/harness/tests/cli.rs`, `M  harness.toml`,
  `M  templates/pointer.md`, `M  test-hashes.json` (README.md, docs/intent.md, harness.default.toml, config.rs
  and templates/RAILS.md unchanged), then `232502e feat(docs): T-015 the context file names the check,
  test-hashes.json is a document, the moved citations live only in docs/intent.md`, then `0`.
  Amended once (`git commit --amend --no-edit`) to carry this paste; the tree diff is the same.
  2026-09-10 verifier, at 1a1d212: VERIFIED. Both rejection points from the 1f84fe8 verdict reproduce as fixed,
  and every other criterion holds on re-run.
  `git status --porcelain` → empty. `$BASE` = 8f0cb50 (HEAD~1; the re-take is one commit, origin/main is the
  whole port behind). `git show --name-only 1a1d212` → .github/copilot-instructions.md, .harness/RAILS.md,
  AGENTS.md, CLAUDE.md, GEMINI.md, QWEN.md, crates/harness/tests/cli.rs, harness.toml, templates/pointer.md,
  test-hashes.json, plus TASKS.md and PROGRESS.md: every scope file or the record. The one test-hashes.json key
  moved is `harness.toml`, on scope; `shasum -a 256 harness.toml` → 693425d9…afa18, the key's value. No
  launcher, hook, check-script or `.check-baseline` edit; `.check-baseline` has no failure lines.
  `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check`
  → exit 0; per-binary 191+0+9+5+9+16+22+29+27+4+0 = 312 passed, 3 ignored, 0 failed; clippy and fmt output
  empty. Matches the notes' figure.
  Criterion 4: `cargo build -q && ./target/debug/harness probe` → `PROBE check-unnamed 0`, `PROBE litter 0`,
  `PROBE hash-uncovered 0`, `PROBE install-stale 7`. Same binary in `git worktree add /tmp/t015-parent 8f0cb50`
  → check-unnamed 1, litter 1, install-stale 7; so the two counts this task owns went 1 → 0 and install-stale
  is untouched. Worktree removed, `git worktree list` → one entry.
  Criterion 2, "nowhere else": `grep -rnE '2602\.11988|2605\.29668|34235|agents\.md' --include='*.md'
  --include='*.toml' --include='*.rs' --include='*.json' . | grep -vE 'target/|docs/superpowers/|docs/intent.md|PROGRESS|DECISIONS|TASKS.md'`
  → cli.rs:266 (the test's token list) and crates/harness/tests/fixtures/harness.default.json:13 and :156.
  The fixture is the migration test's pre-migration input, not hashed, not a shipped document, off scope,
  and disclosed in the notes; not a rejection. `grep -c arXiv crates/harness/harness.default.toml` → 0; ten
  `#` lines (1, 3, 8, 15, 19, 23, 102, 105, 146, 204), none adjacent. docs/intent.md `## References` is five
  one-line bullets. `.harness/RAILS.md:65` and `templates/RAILS.md:50` are byte-identical (`sed -n`).
  The four pointer files each equal `sed 's/__CONTEXT_FILE__/AGENTS.md/g' templates/pointer.md` (`diff -q`).
  Criterion 1: `wc -l README.md` → 280; the grep → no output, exit 1. Criterion 3: the grep → no output,
  exit 1; `grep -n '^## ' docs/intent.md` → the eight headings in order, `wc -l` → 120.
  Red-then-green: at 8f0cb50 `git show 8f0cb50:<f> | grep -nE` over the eight files the test reads → seven
  citation lines (AGENTS.md:48, CLAUDE.md:4, GEMINI.md:4, QWEN.md:4, copilot-instructions.md:4,
  pointer.md:3, .harness/RAILS.md:65), plus check-unnamed 1 and `docs` without test-hashes.json → nine, the
  notes' figure. Mutation: drop the `export PATH` prefix from AGENTS.md:8 and append `#34235` to CLAUDE.md,
  `cargo test -p harness --test cli the_shipped_documents_describe_and_do_not_argue` → FAILED with exactly
  `CLAUDE.md:5 cites a paper docs/intent.md owns` and the check-unnamed Finding; both files restored,
  `git status --porcelain` → empty. The test bites on both new clauses.
  Frauds: cli.rs diff is 42 added lines in one existing test, nothing weakened or deleted; no dependency
  added; no network call; no litter. PROGRESS.md:159 carries `friction:`; it is the fourth occurrence of
  the scope-line-noise friction (PROGRESS.md:124, :131, :159) and `harness probe` → `friction-repeat 2`:
  the LEARNINGS.md line is still owed, under a task that names LEARNINGS.md on scope.
  Ponytail: the test uses `config::load`, `probes::run_all` and the existing `read` closure, one caller
  each; nothing to cut. Minor, not a rejection: the check-unnamed Finding reports AGENTS.md line 6 for the
  line `sed -n 8p` prints, so the probe's line numbers are two low in that section.

## [T-017] the run archives on the way out, and an operator can archive without spending a stage
scope: crates/harness/src/pipeline.rs, crates/harness/src/archive.rs, crates/harness/src/cli/tasks.rs, crates/harness/src/cli/mod.rs, crates/harness/tests/loop.rs, crates/harness/tests/cli.rs, README.md
blockedBy: none
status: done
rows: none — harness
criteria:
  - `archive::archive_done` is called once more on the way out of a run, in `Loop::finish` before the digest is built, so the task that landed in the final iteration is archived by the run that landed it; its `refused` and `Err` stay warnings, never a halt, as at the iteration-start call site
  - a test in `crates/harness/tests/loop.rs`: one iteration lands T-001, and after `run` returns, `TASKS.md` holds the stub (`status: done` plus an `archived:` line) and `DECISIONS.md` holds the block; today the stub appears only on the next run
  - `harness tasks archive` runs the same function and prints one line per moved id, or `nothing to archive`; a test in `crates/harness/tests/cli.rs` drives the real binary over a queue with one done block and asserts the stub, the DECISIONS.md block and exit 0
  - `README.md`'s `harness tasks` row names `archive` among the subcommands; no other README change
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  `archive_done` refuses to rewrite the queue under a foreign live loop by reading `loop.pid` and
  walking the caller's ancestors, so calling it from `finish`, where the pid file still names this
  process, must stay allowed. That is the one line to get right, and it wants its own assertion.

  Implemented at b2b9765. The iteration-start block became `Loop::archive` (pipeline.rs) and
  `finish` calls it first, before `Kind::RunEnd` is emitted, so a refusal or an `Err` still lands
  in the digest's warnings and still never halts. `harness tasks archive` (cli/tasks.rs) resolves
  the root the way `cli/probe.rs` does, calls the same function, prints one id per line or
  `nothing to archive`, and prints a live-loop refusal to stderr with exit 1. README gained the
  `archive` row.

  Scrutinise: (a) the live-loop assertion — `the_run_archives_the_task_it_landed_in_its_last_iteration`
  asserts no `an agent is running` warning, which is the `loop_live` self-ancestor path, since
  `PidFile` is still alive when `finish` runs; (b) the README trim. Criterion 4 says "no other
  README change", but `tests/cli.rs::the_shipped_documents_describe_and_do_not_argue` caps the file
  at 280 lines and it was at 280, so the closing paragraph lost its third line to pay for the row.
  Nothing else in README.md changed. (c) `crates/harness/src/archive.rs` and
  `crates/harness/src/cli/mod.rs` are on `scope:` and needed no edit: `Command::Tasks` already
  passes any `cmd` string through.

  Red, at b2b9765 with the two tests added and nothing else:

      $ cargo test --test loop the_run_archives_the_task -q
      thread 'the_run_archives_the_task_it_landed_in_its_last_iteration' panicked at
      crates/harness/tests/loop.rs:916:5:
      ## [T-001] do the thing
      ...
      status: done
      gate: stub verified
      criteria:
        - it happens
      test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 29 filtered out

      $ cargo test --test cli tasks_archive -q
      assertion `left == right` failed: Output { status: ExitStatus(unix_wait_status(512)),
      stdout: "", stderr: "harness tasks: usage: harness tasks
      <list|ready|ready-unattended|ids-at|block|field|set-status|unblock|rejections> [args] [file]\n" }
        left: Some(2)
       right: Some(0)
      test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out

  Green, at b2b9765 with the change:

      $ export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo "EXIT=$?"
      test result: ok. 191 passed; 0 failed; 1 ignored
      test result: ok. 0 passed; 0 failed; 0 ignored
      test result: ok. 10 passed; 0 failed; 0 ignored
      test result: ok. 5 passed; 0 failed; 0 ignored
      test result: ok. 9 passed; 0 failed; 0 ignored
      test result: ok. 16 passed; 0 failed; 2 ignored
      test result: ok. 22 passed; 0 failed; 0 ignored
      test result: ok. 30 passed; 0 failed; 0 ignored
      test result: ok. 27 passed; 0 failed; 0 ignored
      test result: ok. 4 passed; 0 failed; 0 ignored
      test result: ok. 0 passed; 0 failed; 0 ignored
      EXIT=0

  314 passed, 3 ignored, 0 failed. `./target/debug/harness probe` prints byte-identical PROBE
  lines with the change stashed and unstashed (`diff /tmp/before.txt /tmp/after.txt` → no output).

  Commit: `git add crates/harness/src/pipeline.rs crates/harness/src/archive.rs crates/harness/src/cli/tasks.rs crates/harness/src/cli/mod.rs crates/harness/tests/loop.rs crates/harness/tests/cli.rs README.md TASKS.md PROGRESS.md && git status --porcelain && git -c commit.gpgsign=false commit -q -m "feat(pipeline): T-017 …" && git log --oneline -1 && git status --porcelain`

      M  PROGRESS.md
      M  README.md
      M  TASKS.md
      M  crates/harness/src/cli/tasks.rs
      M  crates/harness/src/pipeline.rs
      M  crates/harness/tests/cli.rs
      M  crates/harness/tests/loop.rs
      4bd1c10 feat(pipeline): T-017 the run archives on the way out, and `harness tasks archive` runs the same function
      (git status --porcelain printed nothing: the tree is clean)

  Amended once (`git commit --amend --no-edit`) to carry this paste; the tree diff is the same.

  VERIFIED — verifier, 2026-09-10, at e1d5206 (tree clean: `git status --porcelain` printed nothing
  before and after every command below).

  Base: `HEAD~1` = b2b9765, the commit before the task's. `origin/main` resolves but is 132 commits
  behind this branch, so a diff against it would name every earlier task's files; HEAD~1 is the
  task's own commit and is the base every figure here is against.

  Check, run by me, uncached form, exit 0:

      $ export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo "EXIT=$?"
      test result: ok. 191 passed; 0 failed; 1 ignored
      test result: ok. 0 passed; 0 failed; 0 ignored
      test result: ok. 10 passed; 0 failed; 0 ignored
      test result: ok. 5 passed; 0 failed; 0 ignored
      test result: ok. 9 passed; 0 failed; 0 ignored
      test result: ok. 16 passed; 0 failed; 2 ignored
      test result: ok. 22 passed; 0 failed; 0 ignored
      test result: ok. 30 passed; 0 failed; 0 ignored
      test result: ok. 27 passed; 0 failed; 0 ignored
      test result: ok. 4 passed; 0 failed; 0 ignored
      test result: ok. 0 passed; 0 failed; 0 ignored
      EXIT=0

  314 passed, 0 failed, 3 ignored, across 11 binaries; the two reading `0 tests` are the lib-doc
  targets and executed nothing, which is why they are quoted rather than counted as coverage. The
  figures reproduce the implementer's paste line for line. clippy and fmt are chained behind `&&`,
  so EXIT=0 asserts all three ran. Delta: `.check-baseline` is empty of names and
  `git diff HEAD~1 -- .check-baseline` is empty, so there is no listed failure to match and no line
  was added; 0 failed clears it.

  Red re-derived by me, not taken from the notes. Reverting one file at a time and restoring it in
  the same command:

      $ git checkout HEAD~1 -- crates/harness/src/pipeline.rs && cargo test --test loop the_run_archives_the_task -q
      thread 'the_run_archives_the_task_it_landed_in_its_last_iteration' panicked at crates/harness/tests/loop.rs:916:5
      test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 29 filtered out

      $ git checkout HEAD~1 -- crates/harness/src/cli/tasks.rs && cargo test --test cli tasks_archive -q
      assertion `left == right` failed: ... stderr: "harness tasks: usage: harness tasks <list|...|rejections> [args] [file]"
        left: Some(2)  right: Some(0)
      test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out

  Both tests fail on the pre-change source and pass on it; `git status --porcelain` printed nothing
  after each restore.

  Criteria, one at a time:
  1. `pipeline.rs:330` `Loop::archive` holds the whole match, and `finish` calls it as its first
     statement, before `self.digest.iterations` and before `Kind::RunEnd`. The iteration-start site
     is the same call, so `refused` → warning and `Err` → warning hold at both by construction.
  2. `tests/loop.rs:905` asserts `archived: DECISIONS.md` in TASKS.md, the absence of `criteria:`
     (the body left, it was not copied), and `## [T-001] do the thing` in DECISIONS.md, after
     `go(&r, &opts(1))` returns. Red above.
  3. `tests/cli.rs:299` spawns `CARGO_BIN_EXE_harness tasks archive`, asserts exit 0, stdout exactly
     `["T-001"]`, the stub, the moved `notes:` in DECISIONS.md, the untouched `## [T-002]`, and on a
     second run exit 0 with `nothing to archive`. Red above.
  4. Deviation, accepted, named here rather than waved through: the README diff is two hunks, not
     one. `tests/cli.rs::the_shipped_documents_describe_and_do_not_argue` rejects `> 280` lines and
     `git show HEAD~1:README.md | wc -l` → 280, `git show HEAD:README.md | wc -l` → 280, so the row
     had to be paid for out of the same file. The line it cost is the closing paragraph's third,
     rewritten to `A run archives each iteration and on the way out, leaving a stub.` — which is
     what the change made true. `git diff HEAD~1 -- README.md` shows nothing else. Read strictly,
     `no other README change` is broken; there is no command that would demonstrate a defect, and
     the alternative is raising the cap in the test that measures the document, which is worse.
  5. Covered above.

  Frauds, each one executed:
  - `git diff HEAD~1 -- test-hashes.json` → empty. No key moved, so no file's diff needs reading.
    The file's four keys (`Cargo.toml`, `crates/harness/Cargo.toml`, `crates/harness/tests/roles.rs`,
    `harness.toml`) name none of this task's files.
  - `git diff HEAD~1 -- .check-baseline` → empty. `git diff HEAD~1 --name-only -- .harness/
    crates/harness/src/gates.rs crates/harness/src/hooks.rs` → empty: the launcher, the hooks and
    the gates are untouched, and `rows: none — harness` would have permitted it anyway.
  - `git diff HEAD~1 --name-only` → PROGRESS.md, README.md, TASKS.md, cli/tasks.rs, pipeline.rs,
    tests/cli.rs, tests/loop.rs. Every source file is on `scope:`; TASKS.md and PROGRESS.md are on
    the gate's own always-allowed list (`gates.rs:13-14`). `one-scope` clean. `archive.rs` and
    `cli/mod.rs` sat on `scope:` and were not edited, which is the scope-line-noise friction again.
  - No test weakened, skipped or deleted: both test diffs are additions only, 53 and 30 lines,
    `git diff HEAD~1 -- crates/harness/tests/` shows no `-` line outside the added blocks.
  - The one assertion that could have been a tautology is the negative one. `tests/loop.rs:928`
    asserts no warning contains `an agent is running`; that string is `archive.rs:106` verbatim, and
    `archive.rs::refuses_to_rewrite_under_a_live_loop_that_is_not_us` proves it fires when
    `loop.pid` names a foreign live pid. It is load-bearing at `finish` because `_pid` is a binding
    in `go()` (`pipeline.rs:315`) and `finish` is called at `pipeline.rs:327` while it is still in
    scope, so the file exists and names this process: `loop_live`'s ancestor walk matches on the
    first step and returns `None`. The one line the notes asked to get right holds.
  - No dependency added: no Cargo.toml in the diff. No network call in either test. No `unwrap` on
    user input, no scratch file: `harness probe` → `litter 0`, `git status --porcelain` empty.
  - PROGRESS.md carries one entry with a `friction:` line, and it is a new friction (the 280-line
    cap versus a criterion that adds a line).

  Ponytail, on the diff: the extraction has two callers and replaces a copy, so it earns itself.
  The `archive` arm's root resolution is the eighth copy of the same `rev-parse --show-toplevel`
  match (`cli/probe.rs:10`, `hook.rs:17`, `eval.rs:11`, `skills.rs:25`, `run.rs:23`, `init.rs:11`,
  `worktree.rs:11`). Following the house idiom was right for this diff; a helper is now overdue and
  belongs in a task of its own, not in this one.

  Two things this verdict hands to the loop rather than fixes:
  - The scope-line-noise friction is at its fifth occurrence (T-002, T-013, T-014, T-015, T-017)
    and LEARNINGS.md still carries no rule for it. `harness probe` → `friction-repeat` reads 2 and
    does not count this one, because T-017 wrote it on the `next:` line, not the `friction:` line.
    The probe only reads `friction:`; a repeat recorded anywhere else is invisible to it.
  - `harness probe` → `install-stale 7` and `queue-hygiene 1` (T-016's scope NOTICE, which is what
    T-019 is for). Both predate this commit and neither is T-017's to answer.

## [T-018] no verify stage when the implementer left the task anywhere but review
scope: crates/harness/src/gates.rs, crates/harness/src/pipeline.rs, crates/harness/tests/loop.rs, README.md
blockedBy: none
status: done
rows: none — harness
criteria:
  - `gates::implementer_not_done` returns `skip_rest: true` for every status that is not `review`: `done` keeps today's force-back and its commit, while `blocked`, `needs-spec`, `deferred`, `ready` and a missing status pass with a reason naming the status and skip the remaining stages
  - a test in `crates/harness/tests/loop.rs`: a stub implementer that sets `blocked` runs one iteration, and the event log holds exactly one `stage.end` (the implement stage), no `stage.start` for verify, and the digest carries `T-001 ended the iteration at blocked, not done.`
  - the existing `implementer_not_done_forces_back_and_skips_rest` test still passes unchanged
  - `README.md`'s gate table row for `implementer-not-done` states that it also skips the rest of the iteration when the implementer stopped short of review; no other README change
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  Measured on 2026-09-10: a verify stage ran against a task the implementer had parked `blocked`,
  found nothing at review, and cost $1.45 saying so.

  Implemented 2026-09-10. `implementer_not_done` (gates.rs:156) now matches `review` first and
  passes it, returns `pass` with `skip_rest: true` and the reason `the implementer left it at
  <status>` for anything else that is not `done` (`no status` when the field is absent), and keeps
  the `done` arm's force-back and its `chore(<task>)` commit untouched.

  Scrutinise the pipeline.rs half: `Flow::SkipRest` used to `return !self.stopped` from the
  iteration, which skipped `promotions` and `task_outcome` too. It now `break`s, so the round still
  records its outcome -- that is where criterion 2's digest line `T-001 ended the iteration at
  blocked, not done.` comes from (pipeline.rs:883, pre-existing text). Breaking also skips the
  `boundary` check that the cut stages would have made, which dropped the halt in
  `a_dollar_budget_over_a_cost_nothing_reports_halts`; the skip arm calls `self.boundary(false)`
  before breaking to pay that back. Both are worth a reviewer's eye.

  Commands run, at ac935f1 plus this change, before the commit:

      $ export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo "EXIT=$?"
      test result: ok. 192 passed; 0 failed; 1 ignored
      test result: ok. 0 passed; 0 failed; 0 ignored
      test result: ok. 10 passed; 0 failed; 0 ignored
      test result: ok. 5 passed; 0 failed; 0 ignored
      test result: ok. 9 passed; 0 failed; 0 ignored
      test result: ok. 16 passed; 0 failed; 2 ignored
      test result: ok. 22 passed; 0 failed; 0 ignored
      test result: ok. 31 passed; 0 failed; 0 ignored
      test result: ok. 27 passed; 0 failed; 0 ignored
      test result: ok. 4 passed; 0 failed; 0 ignored
      test result: ok. 0 passed; 0 failed; 0 ignored
      EXIT=0

  316 passed, 3 ignored, 0 failed. Red first, both new tests:

      $ cargo test --workspace -q   # before the gates.rs change
      gates::tests::an_implementer_stopping_short_of_review_skips_the_rest --- FAILED
      blocked: GateOutcome { pass: true, reason: "the implementer left it at review", skip_rest: false, halt: false }

      $ cargo test --test loop -q an_implementer_that_stops_short   # before the pipeline.rs change
      assertion `left == right` failed: verify may not spawn
        left: 2
       right: 1

  Criterion 4: `git diff --stat` reads `README.md | 2 +-`, the one table row. Criterion 3:
  `implementer_not_done_forces_back_and_skips_rest` is unedited and green.

  Staged and committed (this block's own text is part of that commit, amended in):

      $ git add crates/harness/src/gates.rs crates/harness/src/pipeline.rs crates/harness/tests/loop.rs README.md TASKS.md PROGRESS.md
      $ git status --porcelain
      M  PROGRESS.md
      M  README.md
      M  TASKS.md
      M  crates/harness/src/gates.rs
      M  crates/harness/src/pipeline.rs
      M  crates/harness/tests/loop.rs
      $ git -c commit.gpgsign=false commit -q -m 'feat(gates): T-018 ...'
      $ git show --stat --oneline HEAD
      8d64ec1 feat(gates): T-018 no verify stage when the implementer left the task anywhere but review
       PROGRESS.md                    |  7 +++++++
       README.md                      |  2 +-
       TASKS.md                       | 45 +++++++++++++++++++++++++++++++++++++++++-
       crates/harness/src/gates.rs    | 42 +++++++++++++++++++++++++++++++++++++--
       crates/harness/src/pipeline.rs |  8 ++++++--
       crates/harness/tests/loop.rs   | 37 ++++++++++++++++++++++++++++++++++
       6 files changed, 135 insertions(+), 6 deletions(-)

  ---

  VERIFIED 2026-09-10 by the verifier, at a882221, working tree clean (`git status --porcelain`
  empty before and after this pass). `$BASE` = `HEAD~1` (ac935f1): `origin/main` verifies but is
  138 commits behind this branch (`git rev-list --count origin/main..HEAD` = 138), so it cannot
  scope one iteration. Every command below was run in this session.

      $ export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check; echo "EXIT=$?"
      test result: ok. 192 passed; 0 failed; 1 ignored     (unittests src/lib.rs)
      test result: ok. 0 passed; 0 failed; 0 ignored       (unittests src/main.rs)
      test result: ok. 10 passed; 0 failed; 0 ignored      (tests/cli.rs)
      test result: ok. 5 passed; 0 failed; 0 ignored       (tests/cli_smoke.rs)
      test result: ok. 9 passed; 0 failed; 0 ignored       (tests/eval.rs)
      test result: ok. 16 passed; 0 failed; 2 ignored      (tests/floor.rs)
      test result: ok. 22 passed; 0 failed; 0 ignored      (tests/init.rs)
      test result: ok. 31 passed; 0 failed; 0 ignored      (tests/loop.rs)
      test result: ok. 27 passed; 0 failed; 0 ignored      (tests/probes.rs)
      test result: ok. 4 passed; 0 failed; 0 ignored       (tests/roles.rs)
      test result: ok. 0 passed; 0 failed; 0 ignored       (Doc-tests harness)
      EXIT=0

  316 passed, 3 ignored, 0 failed, over 11 targets, reproducing the implementer's per-binary counts
  line for line. Target names came from a second `cargo test --workspace 2>&1 | grep -E "^\s+Running|^\s+Doc-tests"`.
  ZERO IS NOT PASS, so both zeros are named: `unittests src/main.rs` (a binary with no `#[test]`)
  and `Doc-tests harness` (no doctests). Both are pre-existing and neither is a target this task
  touched. `.check-baseline` is empty of test names and gained no line in this diff
  (`git diff HEAD~1 -- .check-baseline` is empty), so any failure would have been a rejection; there
  were none to match against it.

  Criterion 1 - PASS. `gates.rs:157` matches `review` first and returns `skip_rest: false`; the next
  arm returns `skip_rest: true` with `the implementer left it at <status>`, `no status` when the
  field is absent; the `done` arm still calls `force_back` and sets `skip_rest = true` at
  `gates.rs:181`, so `done` keeps its force-back and its `chore(<task>)` commit.

  Criterion 2 - PASS, and the test is load-bearing, not a tautology. Red-checked in a throwaway
  worktree at `/tmp/t018-red` (removed; `git worktree list` now shows only this checkout):

      $ git checkout HEAD~1 -- crates/harness/src/pipeline.rs   # gates.rs left at HEAD
      $ cargo test --test loop -q an_implementer_that_stops_short
      panicked at crates/harness/tests/loop.rs:857:5: []
      test result: FAILED. 0 passed; 1 failed; 30 filtered out

  The `ends(&events).len() == 1` and no-`stage.start`-for-verify assertions survive that revert, so
  the gates.rs half alone cuts the stage; the digest line `T-001 ended the iteration at blocked, not
  done.` is what the pipeline.rs half buys, and it is the pre-existing text at `pipeline.rs:888`.
  The gates.rs half was red-checked the other way, reverting only the match arm and keeping the new
  test:

      $ cargo test -q --lib an_implementer_stopping_short
      panicked at crates/harness/src/gates.rs:1090:13:
      blocked: GateOutcome { pass: true, reason: "the implementer left it at review", skip_rest: false, halt: false }
      test result: FAILED. 0 passed; 1 failed; 192 filtered out

  The `self.boundary(false)` payback is load-bearing too, so it is not dead code:

      $ # pipeline.rs skip arm with the boundary call deleted
      $ cargo test -q --workspace
      a_dollar_budget_over_a_cost_nothing_reports_halts --- FAILED
      test result: FAILED. 30 passed; 1 failed

  Criterion 3 - PASS. `git diff HEAD~1 -- crates/harness/src/gates.rs | grep "^-"` removes exactly
  two lines: the old doc comment and the old `!= Some("done")` arm.
  `implementer_not_done_forces_back_and_skips_rest` is not among them and is green in the run above.

  Criterion 4 - PASS. `git diff HEAD~1 -- README.md` is one hunk, one line, the
  `implementer-not-done` table row; `README.md | 2 +-`. No other README change.

  Criterion 5 - PASS, EXIT=0 above.

  Frauds checked and clear. `git diff HEAD~1 -- test-hashes.json` is empty, so no key was re-cut and
  no testcase body needed reading behind one. `git diff HEAD~1 --name-only` is the four scope files
  plus `TASKS.md` and `PROGRESS.md`, both in `gates.rs`'s `BOOKKEEPING` (`gates.rs:12`) and exempt
  from `one-scope`. `rows: none — harness` is the correct lane for a diff in `gates.rs` and
  `pipeline.rs`, so `harness-lane` holds rather than being dodged. No test weakened, skipped or
  deleted. No dependency added (`Cargo.toml` and `Cargo.lock` are not in the diff). No shape
  redefined locally. No network call in either new test; both drive a shell-script stub. No number
  asserted that this pass did not re-run. `harness probe` reports `litter 0` and the tree is clean.
  `PROGRESS.md`'s new entry carries a `friction:` line, and it is a first occurrence -- the two
  `friction-repeat` findings the probe still emits point at `PROGRESS.md:131` (3 times) and
  `PROGRESS.md:110` (2 times), both older than this task and both still owed their LEARNINGS.md
  lines, which is the loop's debt and not T-018's. `install-stale 7` is unchanged by this diff,
  which touches nothing under `roles/` or `.harness/`.

  Ponytail: nothing to cut. Two lines changed in the gate (one match arm, struct-update syntax over
  a helper), three in the pipeline. No abstraction, no new caller-less indirection, no config for a
  constant.

  Two residuals, named because a later reader will meet them, neither a rejection and neither
  contradicting the criteria:
  - The `blocked` arm skips `verify`, and `commit-verdict` lived on that stage
    (`harness.default.toml:132`), so nothing in the launcher now commits `TASKS.md` when the
    implementer parks a task short of `review`. The `done` arm still commits, via `force_back`. This
    is asymmetric on purpose per criterion 1, and the implementer's own prompt makes the commit its
    last required step, but the belt-and-braces commit is gone. There is no commit rail in
    `.harness/RAILS.md` for it to erode (`grep -n commit .harness/RAILS.md` returns only the
    `one-scope` row), so it stays a note.
  - The notes above cite `8d64ec1`, which is a real commit object (`git cat-file -t 8d64ec1` →
    `commit`) but not on the branch: it was amended into `a882221`. The evidence reproduces at
    `a882221`, so the stale sha is a citation to a dangling object, not a claim about a tree nobody
    can check out.
