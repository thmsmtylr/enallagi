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
archived: DECISIONS.md — full block at `git show ce92693:TASKS.md`

## [T-016] the licence is MIT
scope: LICENSE, NOTICE, Cargo.toml, crates/harness/Cargo.toml, README.md, crates/harness/tests/floor.rs
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show a6e102c:TASKS.md`

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
