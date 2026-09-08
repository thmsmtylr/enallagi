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
