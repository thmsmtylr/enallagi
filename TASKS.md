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
archived: DECISIONS.md — full block at `git show 587dbfa:TASKS.md`

## [T-018] no verify stage when the implementer left the task anywhere but review
scope: crates/harness/src/gates.rs, crates/harness/src/pipeline.rs, crates/harness/tests/loop.rs, README.md
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show 9cea7ab:TASKS.md`

## [T-019] queue-hygiene checks the scope of open blocks, not of done ones
scope: crates/harness/src/probes/queue_hygiene.rs, crates/harness/tests/probes.rs, README.md
blockedBy: none
status: done
archived: DECISIONS.md — full block at `git show 9c0b4af:TASKS.md`

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
  recovered 2026-09-13: the block was lost at 9c0b4af, when T-019's verifier edit deleted its heading and its body merged into T-019's, which `harness run` then archived; restored verbatim from `git show ce92693:TASKS.md`.

## [T-026] install-stale .harness/RAILS.md:0 the installed copy differs from the source it was built from; re-run `harness init`
scope: .harness/RAILS.md, templates/RAILS.md
blockedBy:
status: done
probe: install-stale
rows: none — harness
command: `harness probe`
output: |
  PROBE install-stale 1
  FINDING install-stale .harness/RAILS.md:0 the installed copy differs from the source it was built from; re-run `harness init`
criteria:
  - the fix touches `.harness/RAILS.md`, a governing document: the adjudicator halts the run for a human on this block rather than promoting it
  - `harness probe` no longer emits a `FINDING install-stale` line for `.harness/RAILS.md`
notes: proposed from the output above on 2026-09-10. HALT 2026-09-10 — the fix changes `.harness/RAILS.md`, a governing document, so this is neither a kill nor a task. `diff templates/RAILS.md .harness/RAILS.md` shows the divergence is deliberate, not drift: the installed copy is the written rails, the template is the shipped skeleton that still carries the `__CONTEXT_FILE__` and `__SPEC__` placeholders and leaves the rail table commented out. `harness init`, which the finding recommends, would overwrite `.harness/RAILS.md` with that skeleton and destroy the rails text. A human decides which side is the source: either `templates/RAILS.md` is rewritten to say what `.harness/RAILS.md` says with the placeholders restored, so a re-init is safe, or `install-stale` stops calling a deliberately diverged install stale. Neither is the adjudicator's to write. One number in the block is the stale binary's: `cargo run -q -p harness -- probe` reports `PROBE install-stale 7`, not 1 — `.harness/RAILS.md` plus five `.harness/roles/*.md` and `.claude/skills/running-the-loop/SKILL.md`, all installed 2026-09-08 against sources T-014 rewrote 2026-09-09. Only the RAILS.md row halts: for the other six a re-init writes what the sources already say and destroys nothing. They are outside this block and unproposed.
  2026-09-13 operator, decided and done: `templates/RAILS.md` is the source. The installed copy was written 2026-09-08; T-014 swept the rationale prose out of the templates on 2026-09-09 and never reached the install, so the 16 extra lines were the old text, one paragraph of it duplicated. Measured before acting, on a throwaway clone: `harness init --adapter claude` rewrites 12 files, +77/-194 lines, and touches no document it lists as `kept` (harness.toml, SPEC.md, AGENTS.md, TASKS.md); the `green` rail keeps the substituted check command. Two clauses worth keeping were ported into `templates/RAILS.md` first: the hash check detects rather than prevents because it runs as the same principal as the lane, and the file that names the check is part of the gate. Then `harness init --adapter claude`; `cargo run -q -p harness -- probe` → `PROBE install-stale 0`, down from 7. `cargo test --workspace -q && cargo clippy --all-targets -q -- -D warnings && cargo fmt --all --check` → EXIT=0.

