# Reference

The tables the README links. Each names the source file that decides it.

## Pipelines

`enallagi run` takes the first pipeline whose `when` holds, in the order `enallagi.toml` lists them.
The defaults are in `crates/harness/harness.default.toml`.

| Pipeline | `when` | Stages |
| --- | --- | --- |
| `review` | `queue.reviewing` | **verify** |
| `task` | `queue.takeable` | **implement** → **verify** → **adjudicate** |
| `triage` | `queue.proposed` | **adjudicate** |
| `discover` | `!queue.takeable` | **scout** → **adjudicate** |

A queue holding a `proposed` block and nothing takeable takes `triage`, so a block the operator
queued is decided before the scout files more.
`discover` ends the run after two dry rounds.
`triage` has one: a round that leaves its proposed block undecided hands the next round to
`discover`, and a round that leaves takeable work reopens it.

Before selecting a task, an iteration reads the review comments on open pull requests
from its own `task/` branches.
Each unresolved thread lands as one `proposed` block carrying its thread id, and the same
finding raised twice on the same lines is one block.
When that block lands, the launcher replies on the thread with the sha and resolves it.
No host tool, no such branch, or a failed host call is a warning, never a halt.

`enallagi audit` reads review findings, frictions and rejections, and writes proposed learnings to DECISIONS.md: a model names each class, and code checks each citation.
`enallagi run` runs it once when a round ends, after its last iteration.
It never accepts what it proposes.
A learning that overlaps a standing entry adds its instances to that entry.

The adjudicator decides each proposed learning.
It promotes one under `## Earned rules` only with three or more instances and a passing `enallagi eval --gate`.
Otherwise it writes a dated `killed: YYYY-MM-DD` line under the entry, which keeps its class from being proposed again.
A learning undecided for `queue.proposed_rounds` rounds moves to `## Expired findings` as a dated line.
The run digest counts and names the learnings proposed, promoted, killed and expired.

The audit stamps each earned rule with a `promoted:` line naming the commit that added it.
An instance merged into the rule whose line that commit did not hold makes the rule ineffective.
The next audit hands an ineffective rule back to the auditor, which proposes a revision like any learning.
The rule gains a dated `revised:` line, and a promoted revision replaces it and keeps those lines.
A rule that recurs after two revisions moves to `## Expired findings` as a dated line.

`enallagi audit --harness <dir>...` reads `DECISIONS.md` in each install directory given.
It reports each rule that two or more installs earned under `## Earned rules`.
A rule killed in any install is left out.
Each report names every install's dated line and prints the rule as a `[seed]` line.
It only reports: it writes to no install and to no file in this repository.
A person adds the line to `templates/LEARNINGS.md` in a pull request.

`enallagi events --prefix` prints one row per role stage in `events.jsonl`.
Each row carries the stage's three input token lanes.
`handed_bytes` prints `-`: no event records the size of the prompt a stage was given.
`repaid` is input plus cache creation, which every stage pays in full.
`cached` is the cache read, which a warm cache re-reads at a discount.
A stage that logged no usage prints `-`.
The first line names the window the log covers, and no figure is scaled past it.
`--json` prints one object per row, then one `total` object per role.

`enallagi skills --cost` prints one row per declared skill.
Each row carries its vendored bytes and two stage counts.
`loaded` counts the role stages whose role file names the skill.
`used` counts the stages whose transcript shows a `Skill` tool call for it.
A skill that no stage loading it ever used is named as unproven in this log.
`--cost` takes no subcommand and no `--frozen`.

## Gates

`[[stage]].post` names the gates run after a stage. A failed gate sends the task back to `ready` or
halts the run. The names are matched in `crates/harness/src/gates.rs`.

| Gate | Refuses | Runs after |
| --- | --- | --- |
| `implementer-not-done` | a `done` from anyone but the verifier, or an implementer that stopped short of `review` | implement |
| `commit-verdict` | a verdict whose `deferred:` line or new prose defers a finding and adds no `proposed` block, or a failed commit of TASKS.md | verify |
| `verdict` | a `done` whose work is uncommitted, or whose check is red on delta | verify |
| `scope` | a file outside the task's `scope:` globs, or a product task editing the harness | verify |
| `queue-intact` | a task id at the iteration's base commit that is in neither TASKS.md nor DECISIONS.md | implement, verify, adjudicate |
| `commit-identity` | a commit in the iteration's range whose author is not the repository's configured `user.email` | implement, verify |
| `check-delta` | a check failure that is not already in `.check-baseline` | wherever a stage's `post` names it |
| `commit-round` | a failed commit of TASKS.md and DECISIONS.md | adjudicate |
| `adjudicator-halt` | an adjudicator output line opening with `halt` and naming a task id, and halts the run | adjudicate |
| `dry-round` | a round that leaves no ready unattended task, counted toward `end_after_dry_rounds` | adjudicate |
| `install-stale` | an installed file that drifted from what `enallagi init` writes now, read by spawning the binary | verify |

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
| `learning-standing` | a proposed learning still undecided, with its age and the rounds left before it expires |
| `skill-ungated` | a `[[skill]]` whose `gate` is `none` |
| `ponytail-ceiling` | a `ponytail:` marker no kill line names |
| `rejection-stale` | a block whose last verdict is REJECTED and whose status is not `ready` |
| `title-length` | a block title past 72 characters, from `queue.title_cap_from` on, except an imported issue still `proposed`; the id and the count |
| `queue-hygiene` | a repeated id, a missing status, an undefined blocker, or a scope entry matching nothing |
| `friction-repeat` | a friction recorded twice that no LEARNINGS.md rule, `## Earned rules` line or kill line covers, and an earned rule whose class recurred after its `promoted:` commit |
| `check-red` | a check that exits non-zero, with its first failing test |
| `litter` | a tracked path the repository treats as disposable, or an untracked path on no allowlist |
| `plain-record` | a commit subject, note or printed line that comments instead of recording |
| `install-stale` | an installed file that differs from what `enallagi init` writes now |
| `contribution-policy` | a guide sentence that refuses or conditions generated changes |
| `upstream-drift` | the checkout's branch and its upstream each carrying commits the other lacks |
| `branch-protection` | what this checkout and a host tool can establish about the default branch |
| `review-merged` | a block at `status: review` whose id a product commit on the remote's default branch names |
| `context-behind` | a Commands line the context template renders that the installed context file lacks |
| `verdict-flip` | a task flipped from `done` to `ready` more than once in one run |
| `rejection-repeat` | one rejection reason repeated across tasks |
| `stage-outlier` | a stage over twice its role's median time or cost |
| `turns-exhausted` | a stage that used its whole turn cap |
| `limit-repeat` | a rate limit hit in consecutive stages |
| `driver` | a shortfall the built artifact reports when `layout.driver_command` runs it |

`litter` reads a tracked path as disposable when `layout.machinery` names a part of it.
The repository's own ignore rules covering a path git still tracks reads the same way.
`layout.strict_prefixes = true` widens the tracked arm to every path outside `layout.allowed_prefixes`.

`branch-protection` is advisory, and silent on a GitHub repository whose `permissions.admin` reads `false`.
Its host-free leg counts the non-merge first-parent commits on the remote's default branch.
Its host leg quotes what a host tool answered, or `unknown` when that tool exited non-zero.

## Configuration

`.enallagi/enallagi.toml` deep-merges over `crates/harness/harness.default.toml`.
Tables merge key by key and arrays merge whole.

Unknown keys are refused per table.
The fields are declared in `crates/harness/src/config.rs`.

Every table and key, with its default, is in [configuration.md](configuration.md).

`[pr] per_task = true` hands the operator one pushed branch per landed task.
The checkout's own branch then follows its own tracking ref and carries nothing else.
With `per_task = false` a lane fast-forwards the checkout's branch and pushes nothing.

That split needs the harness documents in a repository of their own.
When they share the product repository, `by_branch` is false even under `per_task = true`.
Lane commits then fast-forward into the checkout, exactly as they do under `per_task = false`.

A task's own `model:` and `effort:` lines win over `[agent]` and `[agent.<role>]`.

`when` takes any of these, each negated by a leading `!`:

- `queue.takeable`
- `queue.reviewing`
- `queue.proposed`
- `queue.empty`
- `task.attended`
- `check.red`
- `probe.<name>`

A skill or role `source` is `github:owner/repo`, `git+file://` or `path:`.

## Files

`enallagi init` writes these, relative to the repo root, with `.enallagi` as `layout.harness_dir`.
The plan is built in `crates/harness/src/init.rs`.

`enallagi eject` removes them and moves the harness directory, the run's record, to
`$XDG_DATA_HOME/enallagi/ejected/<repository>-<stamp>` (`~/.local/share/...` when unset);
`--keep-record <path>` names the place instead, and `--delete` is the one way to remove it.

Init commits one line to the product's `.gitignore`, and eject commits its removal.
Those two commits are what an install leaves in the product's history.

| Path | What |
| --- | --- |
| `.enallagi/roles/{scout,adjudicator,implementer,verifier,researcher}.md` | the five role prompts, always resubstituted |
| `.enallagi/RAILS.md`, `.enallagi/.gitignore` | the rails, each naming its enforcement, and the ignore file for scratch state |
| `<skills_dir>/running-the-loop/` | the loop's own usage skill, in the preset's skill directory |
| `.enallagi/{enallagi.toml,TASKS.md,PROGRESS.md,LEARNINGS.md,DECISIONS.md,.check-baseline,SPEC.md,evals/README.md}` | seeded once and never overwritten |
| `.enallagi/AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `QWEN.md`, `.github/copilot-instructions.md` | seeded once: the context file, and one-line pointers to it |
| `.gitignore` | `/.enallagi/` appended and committed, so the product's linters skip the harness directory |
