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
scope: README.md, docs/intent.md, crates/harness/harness.default.toml, crates/harness/src/config.rs, crates/harness/tests/cli.rs
blockedBy: none
status: review
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

## [T-016] the licence is MIT
scope: LICENSE, NOTICE, Cargo.toml, crates/harness/Cargo.toml, README.md, crates/harness/tests/floor.rs
blockedBy: none
status: ready
rows: none — harness
criteria:
  - `LICENSE` is the MIT License text with `Copyright (c) 2026 Thomas Taylor`; `NOTICE` is deleted; `git ls-files NOTICE` prints nothing
  - `crates/harness/Cargo.toml` has `license = "MIT"`; `grep -rn 'Apache' --include='*.md' --include='*.toml' --include='*.rs' --include='*.yml' . | grep -v target/ | grep -v docs/superpowers/` prints nothing
  - `README.md`'s License section reads `MIT. See \`LICENSE\`.`
  - a test in `crates/harness/tests/floor.rs` named `the_licence_is_mit` asserts `LICENSE` starts with `MIT License` and `Cargo.toml` names `MIT`; `cargo test --workspace -q` exits 0
notes: |
  Owner's decision 2026-09-09.
