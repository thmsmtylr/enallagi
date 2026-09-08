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
