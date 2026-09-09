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
archived: DECISIONS.md — full block at `git show c3deeca:TASKS.md`

## [T-014] the templates, roles and skill carry no rationale prose: a comment states a rule or a format, nothing else
scope: templates/*.md, roles/*.md, skills/running-the-loop/**, evals/README.md, adapters/README.md, adapters/bun-turbo/README.md, crates/harness/tests/init.rs, crates/harness/tests/probes.rs
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show 11e8ea6:TASKS.md`

## [T-015] README.md, docs/intent.md and harness.default.toml describe, and do not argue
scope: README.md, docs/intent.md, crates/harness/harness.default.toml, crates/harness/src/config.rs, crates/harness/tests/cli.rs, AGENTS.md, harness.toml, test-hashes.json, templates/pointer.md, templates/RAILS.md, .harness/RAILS.md, CLAUDE.md, GEMINI.md, QWEN.md, .github/copilot-instructions.md
blockedBy: none
status: ready
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
