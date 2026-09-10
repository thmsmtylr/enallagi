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

## [T-024] friction-repeat PROGRESS.md:131 the same friction is recorded 3 times and LEARNINGS.md carries no rule for it
scope: LEARNINGS.md, evals/**
blockedBy:
status: ready
probe: friction-repeat
rows: none — harness
command: `harness probe`
output: |
  PROBE friction-repeat 2
  FINDING friction-repeat PROGRESS.md:131 the same friction is recorded 3 times and LEARNINGS.md carries no rule for it: probes.rs sat on the scope line and needed no edit; third occurrence of T-002's and T-013'
criteria:
  - `evals/scope-noise/` holds `setup.sh`, `prompt.txt`, `assert.sh` and `ablate.sh`, the four-file shape `evals/README.md` names; `evals/verifier/` is the only existing eval with all four, so copy it
  - `cargo run -q -p harness -- eval --gate scope-noise` prints a line beginning `GATE scope-noise ACCEPT`. A `REJECT` is a result, not a failure: paste it and move this block to `blocked` rather than reshaping the rule until it passes
  - LEARNINGS.md carries one new dated entry for the scope-line friction, naming `evals/scope-noise` and naming a file, a command or a hook; `cargo run -q -p harness -- probe` still prints `PROBE learning-unenforced 0` and `PROBE learning-ungated 0`
  - `cargo run -q -p harness -- probe` prints no `FINDING friction-repeat` line for `PROGRESS.md:131`
  - `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: promoted 2026-09-10. The friction is T-002's, restated at T-013 and T-014: a scope file the task never touches is noise the scope gate cannot tell from a forgotten edit; PROGRESS.md:131 closes with "Whoever next holds LEARNINGS.md on scope owes the scope-line-noise rule", and this block is that holder. Every criterion runs `cargo run -q -p harness -- …`, never the `harness` on PATH: that is a stale `target/release` build predating T-019 and it answers a different question (see T-023's kill line in DECISIONS.md). The cap is not binding — LEARNINGS.md holds 5 entries against `learnings_cap = 12` (crates/harness/harness.default.toml:103) — so nothing has to be removed to add this one. Rule coverage is per-line word-set containment (friction_repeat.rs:57), so the entry has to carry the friction's own words or the probe stays loud. The scope glob is `evals/**` rather than `evals/scope-noise/**` because a directory that does not exist yet matches no file and queue-hygiene calls that an open block nobody can take; the criteria pin the directory instead. Touch no eval but the new one.

## [T-025] friction-repeat PROGRESS.md:110 the same friction is recorded 2 times and LEARNINGS.md carries no rule for it
scope: LEARNINGS.md, evals/**
blockedBy:
status: ready
probe: friction-repeat
rows: none — harness
command: `harness probe`
output: |
  PROBE friction-repeat 2
  FINDING friction-repeat PROGRESS.md:110 the same friction is recorded 2 times and LEARNINGS.md carries no rule for it: second occurrence: a red the gate cannot name is unanswerable: `fail_name` in harness.toml
criteria:
  - `evals/unnamed-red/` holds `setup.sh`, `prompt.txt`, `assert.sh` and `ablate.sh`, the four-file shape `evals/README.md` names; copy `evals/verifier/`, the only existing eval with all four
  - `cargo run -q -p harness -- eval --gate unnamed-red` prints a line beginning `GATE unnamed-red ACCEPT`. A `REJECT` is a result, not a failure: paste it and move this block to `blocked` rather than reshaping the rule until it passes
  - LEARNINGS.md carries one new dated entry for that friction, naming `evals/unnamed-red` and naming a file, a command or a hook; `cargo run -q -p harness -- probe` still prints `PROBE learning-unenforced 0` and `PROBE learning-ungated 0`
  - `cargo run -q -p harness -- probe` prints no `FINDING friction-repeat` line for `PROGRESS.md:110`
  - `export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check` exits 0
notes: promoted 2026-09-10. The friction is that a red the verdict gate cannot name is unanswerable: two lanes re-ran blind before the failing test was named (PROGRESS.md:110). The mechanism is already repaired — harness.toml:7 now carries a `fail_name` that matches both `… --- FAILED` and `… ... FAILED`, which is what `cargo test -q` actually prints — so this block owes the rule that keeps it repaired, not the repair; harness.toml and gates.rs are off scope and stay off it. Every criterion runs `cargo run -q -p harness -- …`, never the `harness` on PATH, which is a stale `target/release` build (see T-023's kill line in DECISIONS.md). T-024 also holds LEARNINGS.md on scope: take them one at a time, one checkout one writer (LEARNINGS.md, seed entry 3). Cap is not binding: 5 entries against `learnings_cap = 12`. The scope glob is `evals/**` rather than `evals/unnamed-red/**` because a directory that does not exist yet matches no file and queue-hygiene calls that an open block nobody can take; the criteria pin the directory instead. Touch no eval but the new one.

## [T-026] install-stale .harness/RAILS.md:0 the installed copy differs from the source it was built from; re-run `harness init`
scope: .harness/RAILS.md, templates/RAILS.md
blockedBy:
status: proposed
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

