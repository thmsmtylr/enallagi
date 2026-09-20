# Reference

The tables the README links. Each names the source file that decides it.

## Pipelines

`enallagi run` takes the first pipeline whose `when` holds, in the order `enallagi.toml` lists them.
The defaults are in `crates/harness/harness.default.toml`.

| Pipeline | `when` | Stages |
| --- | --- | --- |
| `review` | `queue.reviewing` | **verify** |
| `task` | `queue.takeable` | **implement** → **verify** → **adjudicate** |
| `discover` | `!queue.takeable` | **scout** → **adjudicate** |

`discover` ends the run after two dry rounds.

## Gates

`[[stage]].post` names the gates run after a stage. A failed gate sends the task back to `ready` or
halts the run. The names are matched in `crates/harness/src/gates.rs`.

| Gate | Refuses | Runs after |
| --- | --- | --- |
| `implementer-not-done` | a `done` from anyone but the verifier, or an implementer that stopped short of `review` | implement |
| `commit-verdict` | a verdict whose new notes defer a finding and add no `proposed` block, or a failed commit of TASKS.md | verify |
| `verdict` | a `done` whose work is uncommitted, or whose check is red on delta | verify |
| `scope` | a file outside the task's `scope:` globs, or a product task editing the harness | verify |
| `queue-intact` | a task id at the iteration's base commit that is in neither TASKS.md nor DECISIONS.md | implement, verify, adjudicate |
| `check-delta` | a check failure that is not already in `.check-baseline` | wherever a stage's `post` names it |
| `commit-round` | a failed commit of TASKS.md and DECISIONS.md | adjudicate |
| `adjudicator-halt` | an adjudicator output line opening with `halt` and naming a task id, and halts the run | adjudicate |
| `dry-round` | a round that leaves no ready unattended task, counted toward `end_after_dry_rounds` | adjudicate |

## Probes

`enallagi probe` prints one `PROBE <name> <count>` line per probe and a `FINDING` line per shortfall.
A probe that cannot run prints `PROBE <name> ERROR`, never a count of zero.
The list is `registry()` in `crates/harness/src/probes/mod.rs`.

| Probe | Reports |
| --- | --- |
| `spec-untested` | an exit-criteria row whose test does not exist |
| `queue-uncovered` | a spec row no task claims, or a claimed row the spec lacks |
| `rail-unenforced` | a rail whose enforcement does not exist, or exists and nothing runs |
| `hash-uncovered` | a file a rail names with no key in `test-hashes.json` |
| `check-unnamed` | a context file whose Commands section omits the configured check |
| `learning-unenforced` | a LEARNINGS.md entry naming no file, command or hook |
| `learning-ungated` | a dated rule naming no eval, or a library past its cap |
| `skill-ungated` | a `[[skill]]` whose `gate` is `none` |
| `ponytail-ceiling` | a `ponytail:` marker no kill line names |
| `rejection-stale` | a block whose last verdict is REJECTED and whose status is not `ready` |
| `queue-hygiene` | a repeated id, a missing status, an undefined blocker, or a scope entry matching nothing |
| `friction-repeat` | a friction recorded twice that no rule or kill line covers |
| `check-red` | a check that exits non-zero, with its first failing test |
| `litter` | a tracked file outside the source root and the allowlists |
| `plain-record` | a commit subject, note or printed line that comments instead of recording |
| `install-stale` | an installed file that differs from what `enallagi init` writes now |
| `contribution-policy` | a guide sentence that refuses or conditions generated changes |
| `verdict-flip` | a task flipped from `done` to `ready` more than once in one run |
| `rejection-repeat` | one rejection reason repeated across tasks |
| `stage-outlier` | a stage over twice its role's median time or cost |
| `turns-exhausted` | a stage that used its whole turn cap |
| `limit-repeat` | a rate limit hit in consecutive stages |
| `driver` | a shortfall the built artifact reports when `layout.driver_command` runs it |

## Configuration

`.enallagi/enallagi.toml` deep-merges over `crates/harness/harness.default.toml`.
Tables merge key by key and arrays merge whole.

Unknown keys are refused per table.
The fields are declared in `crates/harness/src/config.rs`.

Every table and key, with its default, is in [configuration.md](configuration.md).

A task's own `model:` and `effort:` lines win over `[agent]` and `[agent.<role>]`.

`when` takes any of these, each negated by a leading `!`:

- `queue.takeable`
- `queue.reviewing`
- `queue.empty`
- `task.attended`
- `check.red`
- `probe.<name>`

A skill or role `source` is `github:owner/repo`, `git+file://` or `path:`.

## Files

`enallagi init` writes these, relative to the repo root, with `.enallagi` as `layout.harness_dir`.
The plan is built in `crates/harness/src/init.rs`.

| Path | What |
| --- | --- |
| `.enallagi/roles/{scout,adjudicator,implementer,verifier,researcher}.md` | the five role prompts, always resubstituted |
| `.enallagi/RAILS.md`, `.enallagi/.gitignore` | the rails, each naming its enforcement, and the ignore file for scratch state |
| `<skills_dir>/running-the-loop/` | the loop's own usage skill, in the preset's skill directory |
| `.enallagi/{enallagi.toml,TASKS.md,PROGRESS.md,LEARNINGS.md,DECISIONS.md,.check-baseline,SPEC.md,evals/README.md}` | seeded once and never overwritten |
| `.enallagi/AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `QWEN.md`, `.github/copilot-instructions.md` | seeded once: the context file, and one-line pointers to it |
