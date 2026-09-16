# Intent: what the harness is for

Author: Thomas Taylor. Status: accepted 2026-09-04. Format: Stage 1 of the AI-native SDLC playbook
(References).

## Problem

The playbook describes a loop in which every stage commits an artifact the next stage reads, and
human judgment concentrates at gates. It does not ship one: each play is a set of instructions a
platform team executes by hand, and the two mechanisms that close the loop, "an independent
confidence gate between stages, a deterministic check or an adversarial reviewing agent" and a
detection script that "invokes Claude without a person in the path", are named in Stage 6 and left
as an exercise.

This repository is that exercise, narrowed to the half of the loop where the gates live.

## Proposed outcome

**A runnable implementation of Stages 3 through 6 of the AI-native SDLC, proven by building
itself.**

The harness takes an accepted specification and runs Build, Test, Deploy and Maintain without a
person in the invocation path. The playbook's controls are code:

| Playbook control | Enforced here by |
| --- | --- |
| "the agent that wrote the code has no way to approve it" | five role prompts with separated authority; the implementer may set `review` and never `done` (`roles/`) |
| "a fresh context window once the session believes the work is done" | one process per stage, no shared conversation (`crates/harness/src/agent.rs`) |
| "verification before a task is reported done" | the `verdict` gate re-runs the check behind the verifier's verdict and forces `done` back to `ready` (`crates/harness/src/gates.rs`) |
| "a hook that blocks edits to test files during a fix task" | `test-hashes.json` plus `enallagi hook immutable` (`crates/harness/src/hooks.rs`) |
| "evals … run on any change to CLAUDE.md, skills or hooks" | `enallagi eval --gate <name>`: a rule is accepted only when its eval fails without it |
| "a deterministic script watches … and invokes Claude when a control band is breached" | `enallagi probe`: twenty-one deterministic analyses, no model, emitting `FINDING` lines that are the queue's only input |

The probes watch the repository's own invariants: an untested exit criterion, a rail naming
enforcement that does not exist, a rule with no eval behind it. The loop has a control band before
there is a production to measure.

**Proof of the outcome is self-hosting.** The package was built by installing itself into its own
repository and running the loop. `docs/bootstrap.sh` derives that record from `git log` and nothing
else, and `docs/bootstrap.sh --check` fails on a history with no verifier rejection.

## What exists today

Measured 2026-09-08 against the Rust binary that replaced the bash package:

- `cargo test -p enallagi -q 2>&1 | grep 'test result'` → 285 passed / 0 failed / 3 ignored, summed
  across the crate's `test result:` lines (lib, main, seven integration files, doctests).
- `target/release/enallagi probe`, run after `enallagi init` into a fresh, otherwise-empty git repo →
  exit 0. `spec-untested 1`, `queue-uncovered 1`, `rail-unenforced 2`, `hash-uncovered 1`,
  `skill-ungated 3`, `check-red 1`, all from the seeded templates' own unfilled placeholders
  (`src/thing.test.ts` named and absent, `test-hashes.json` unwritten, three `[[skill]]` entries
  still carrying `gate = "none"`, no check command configured). Every other text probe reports 0;
  the five telemetry probes and `driver` report OFF.
- `./driver.sh` → `EXIT=0`, no `FINDING` lines. The four adversarial lane modes (work left
  uncommitted, no PROGRESS.md entry, an edit outside `scope:`, a red floor) are caught, driven
  through the binary.

Against the playbook's six stages:

| Stage | State |
| --- | --- |
| 1 Plan | **out of scope.** See Constraints. |
| 2 Design | **out of scope.** `SPEC.md` is written by a human and accepted by a human. |
| 3 Build | built: roles, `one-scope`, task-block criteria as the committed plan, hooks as guardrails |
| 4 Test | built: delta against `.check-baseline`, the ablation gate |
| 5 Deploy | **partial.** The CI matrix exists, and `enallagi pr` builds one pull-request branch per landed task; the PR review loop and the release gate do not |
| 6 Maintain | built: twenty-one probes, deterministic, no model in the detection path |

## Where it goes

In the order a stranger can verify each one:

1. **Capability findings.** `driver` on. The measure is a `FINDING` line about the installed
   artifact rather than about the queue.
2. **Stage 5.** The playbook's PR review loop and hooks-as-approval-gates: a `REVIEW.md`, findings
   the launcher reads as a machine-readable tally, and a release gate that asks rather than blocks.
3. **A threat model.** The `FINDING`-to-queue path is an injection surface into an unattended loop.
4. **Trajectory probes.** Every probe but `driver` reads text at rest; none reads what the agent
   did.
5. **A thirty-second demo.**

## Affected users and systems

One repository and one author. No adoption goal, no niche, no monetisation. The audience is a senior
engineer reading the repository cold.

## Constraints

- **Plan and Design stay human.** The harness starts where an accepted specification exists. This
  is a refusal, recorded so its absence is not read as an oversight.
- **Every claim carries the command that produces it** (`citable`). No number lands that was not run
  and stamped (`measure-first`).
- **Self-hosting is the validation.** Anything the loop cannot demonstrate on this repository is
  not evidence.
- **No parallel lanes, no daemon, no durable-execution engine, no hosting.** Recorded refusals.
- **Stack.** One Rust binary, statically linked, targets `x86_64` and `aarch64` on `linux-musl` and
  `apple-darwin`. Runtime dependencies: `git`, the agent CLI named in the configuration, and the check
  command named in the configuration. Crate dependencies: `clap`, `serde`, `serde_json`, `toml`,
  `ratatui`, `crossterm`, `globset`, `regex`, `sha2`, `jiff`, `thiserror`, `anyhow`; `tempfile` in
  tests. Git is invoked as a subprocess, not linked. Supersedes "Bash and `python3` only"
  (2026-09-04 to 2026-09-07).
- **Vendor-neutral.** The agent is any CLI with a headless mode. No adapter is the default; the
  Claude Code adapter is one of several, each a settings file.

## Open questions

- Does the `driver` probe's finding stream stay inside the same queue as the paperwork probes, or
  does a capability finding deserve a different lane? Currently they are indistinguishable to the
  scout.
- Does Stage 5's review loop run as `claude-code-action` in CI, or as a sixth role inside the loop?
  The former is the playbook's answer; the latter keeps the whole loop in one place and one process
  model.

## References

- The AI-native SDLC playbook, claude.com, 21 Aug 2026: https://claude.com/blog/the-ai-native-sdlc-playbook
- AGENTS.md, an open context-file format read by 20+ tools and present in 60,000+ repositories (agents.md, 2026): https://agents.md
- Claude Code does not read AGENTS.md natively, "not planned": https://github.com/anthropics/claude-code/issues/34235
- GRASP: a capacity-bounded skill library where an add at capacity requires a remove; the ungated baseline regressed below no skills (arXiv:2605.29668): https://arxiv.org/abs/2605.29668
- Gloaguen et al.: context files raise inference cost by over 20% and cost about 3% of success rate when LLM-generated (arXiv:2602.11988): https://arxiv.org/abs/2602.11988
