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

## [T-016] the licence is MIT
scope: LICENSE, NOTICE, Cargo.toml, crates/harness/Cargo.toml, README.md, crates/harness/tests/floor.rs
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show a6e102c:TASKS.md`

## [T-017] the run archives on the way out, and an operator can archive without spending a stage
scope: crates/harness/src/pipeline.rs, crates/harness/src/archive.rs, crates/harness/src/cli/tasks.rs, crates/harness/src/cli/mod.rs, crates/harness/tests/loop.rs, crates/harness/tests/cli.rs, README.md
blockedBy: none
status: ready
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

## [T-018] no verify stage when the implementer left the task anywhere but review
scope: crates/harness/src/gates.rs, crates/harness/src/pipeline.rs, crates/harness/tests/loop.rs, README.md
blockedBy: none
status: ready
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

## [T-019] queue-hygiene checks the scope of open blocks, not of done ones
scope: crates/harness/src/probes/queue_hygiene.rs, crates/harness/tests/probes.rs, README.md
blockedBy: none
status: ready
rows: none — harness
criteria:
  - the scope-matches-a-file check in `crates/harness/src/probes/queue_hygiene.rs` applies to a block at `ready`, `review`, `blocked` or `needs-spec` and is skipped for `done` and for an archived stub; the duplicate-id, missing-status and dangling-`blockedBy` checks stay unchanged for every block
  - a test in `crates/harness/tests/probes.rs`: a queue with a `done` block whose scope names a deleted file yields no finding, and a `ready` block whose scope names a file that does not exist yields one finding naming the pattern
  - the probe's module doc names what it checks and no longer claims the done case
  - `README.md`'s probe table row for `queue-hygiene` matches the new behaviour; no other README change
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  Measured on enallagi.ai 2026-09-09: fourteen findings, every one a done block whose scoped files a
  later cleanup deleted. A done block's scope is history; an open block whose scope names nothing is
  a task nobody can take, which is the defect worth reporting.

## [T-020] every iteration leaves exactly one PROGRESS.md entry
scope: crates/harness/src/pipeline.rs, roles/verifier.md, .harness/roles/verifier.md, crates/harness/tests/loop.rs, README.md
blockedBy: none
status: ready
rows: none — harness
criteria:
  - an iteration of the `review` pipeline ends with no `wrote no PROGRESS.md entry` warning in the digest, and `PROGRESS.md` grows by exactly one entry
  - an iteration of the `task` pipeline still grows `PROGRESS.md` by exactly one entry: no duplicate entry from a second role writing one
  - two tests in `crates/harness/tests/loop.rs`, one per pipeline, asserting the entry count and the absence of the warning; count entries by lines matching `^## ` in `PROGRESS.md`
  - whichever mechanism is chosen, state it in one line in `README.md` where the pipelines are described; if a role prompt changes, the source under `roles/` and the installed copy under `.harness/roles/` say the same thing and `harness probe` reports `install-stale 0`
  - `cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: |
  The warning fires on every review-pipeline iteration today, because the verifier writes no entry and
  the launcher expects one. Either the record gains the verdict round's line or the launcher stops
  asking for one; the criteria fix the outcome, not the mechanism.
