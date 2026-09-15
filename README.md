# harness

An autonomous task loop for a coding agent, installed into any git repository. One statically
linked binary reads a queue (`TASKS.md`), runs one task per fresh agent process through five
separated roles, and re-derives every `done` from the tree. No daemon: the agent is whatever
headless command `harness.toml` names. Runtime dependencies: `git`, `sh`, that agent CLI, and the
check command. Targets `x86_64` and `aarch64` on `linux-musl` and `apple-darwin`.

## Install

A tagged release publishes four static binaries and a `SHA256SUMS`:

```bash
curl -LO https://github.com/thmsmtylr/enallagi/releases/latest/download/harness-aarch64-apple-darwin
chmod +x harness-aarch64-apple-darwin
sudo mv harness-aarch64-apple-darwin /usr/local/bin/harness
```

Or from source: `cargo install --locked --git https://github.com/thmsmtylr/enallagi harness`. Then:

```bash
harness init                    # writes harness.toml, seeds the documents from defaults
$EDITOR harness.toml            # agent.preset, check.command, layout.spec
harness init                    # re-run: substitutes the edited answers
harness init --adapter claude   # optional: writes .claude/agents/ and the hook wiring
```

`harness init` prints the exact `git add` line for everything it wrote; `verdict` counts an
untracked path as work off the branch. `--dry-run` prints the plan without writing it.

## What a run does

`harness run --iterations <N>` (`-n <N>`) runs up to N iterations (default 3), each a pipeline
chosen by the queue's state:

| Pipeline | `when` | Stages |
| --- | --- | --- |
| `review` | `queue.reviewing` | **verify** |
| `task` | `queue.takeable` | **implement** → **verify** |
| `discover` | `!queue.takeable` | **scout** → **adjudicate** |

Every stage is a separate process spawned from a role prompt under `.enallagi/roles/`, turn-capped
per stage. `review` runs before `task`. `discover` ends the run after two consecutive rounds that
leave nothing takeable. Every iteration leaves exactly one `PROGRESS.md` entry: the launcher writes
one itself, and commits it, when no role did.

A run halts on a `STOP` file in the repo root; on `BUDGET_SECONDS` / `BUDGET_USD` /
`BUDGET_TOKENS` (or `--budget-seconds` / `--budget-usd` / `--budget-tokens`, which wins) at the
next stage boundary, where the dollar and token budgets need an `[agent.usage]` the preset fills
and halt when nothing was observed; on the adjudicator parking a fix at `needs-spec`; and on a
stage that could not start. `--dry-run` prints the plan and the probe output, spawns nothing and
runs no gates. `--frozen` refuses to re-vendor a skill whose hash has moved. `harness run` draws a
live view whenever stdout is a tty (`--no-tui` suppresses it) and `harness watch` attaches
read-only to a running loop's event log: three panes (queue, stages, output), `Tab` cycles focus,
`Up`/`Down` scroll the focused pane, `?` toggles help, `q` quits.

## What is enforced, and by what

Every rule below is a mechanism in `crates/harness/src/gates.rs`. `[[stage]].post` names the gates
run after a stage; a failed gate forces the task back to `ready` (or halts the run) and writes its reason next to the status.

| Rule | Gate | Runs after |
| --- | --- | --- |
| Only the verifier may set `done`, and an implementer that stopped short of `review` skips the rest of the iteration | `implementer-not-done` | implement |
| The work is committed, and the check is green on delta, before `done` is accepted | `verdict` | verify |
| Only the task's `scope:` globs are touched; a product task never edits the harness | `scope` | verify |
| A task id at the iteration's base commit is still in TASKS.md, or in DECISIONS.md; a lost heading halts the run | `queue-intact` | implement, verify, adjudicate |
| The check, on delta against `.check-baseline` | `check-delta` | wherever a stage's `post` names it |

`harness hook <name>` runs independently of the pipeline, wired into an adapter's hooks file where
the tool has one, reading its input from stdin.

| Hook | Fires on | Does |
| --- | --- | --- |
| `immutable` | pre-tool-use | blocks an edit to a file `test-hashes.json` covers |
| `one-writer` | pre-tool-use | refuses an edit from a session that is not the running loop's own (liveness: `<harness_dir>/loop.pid`) |
| `verify-done` | stop | reports the check's own state before the agent claims done |
| `skills` | prompt-submit | prints every declared skill's id, why it is relied on, and its gate |

The full rail list, including the judgment calls no mechanism checks, is `.enallagi/RAILS.md`.

## The queue

`TASKS.md` is a list of blocks. `harness tasks <cmd> [args] [file]` is the only parser.

```
## [T-###] one line, in the finding's own words
scope: src/thing.ts, src/thing.test.ts
blockedBy: none
status: ready
rows: none — harness
criteria:
  - objective, naming the command whose output changes when it is done
notes: the implementer's and the verifier's own words; output pasted, not summarised
```

| Command | Does |
| --- | --- |
| `list` | every block, reserialized |
| `ready` / `ready-unattended` | the first unblocked `ready` id, skipping `attended: true` |
| `ids-at <status>` | every id at that status |
| `block <id>` | one block's text |
| `field <id> <key>` | one field's value |
| `set-status <id> <status> <reason>` | rewrites a block's status and reason |
| `unblock` | drops a `blockedBy` id that is now `done` |
| `rejections [file]` | `REJECTED` lines out of `DECISIONS.md` (`file` defaults there) |
| `archive` | every `done` block to `DECISIONS.md`, oldest `PROGRESS.md` entries to `PROGRESS.archive.md` |

`file` after the other arguments defaults to `TASKS.md`. `status` is a block's first word; the rest
of the line is its reason. A run archives each iteration and on the way out, leaving a stub.

## Probes

`harness probe [names...]` prints one `PROBE <name> <count>` line per probe and a `FINDING` line
per shortfall. A probe that cannot run prints `PROBE <name> ERROR`, never a count of zero.

| Probe | Reports |
| --- | --- |
| `spec-untested` | an exit-criteria row with no test named in backticks, or a named test that does not exist |
| `queue-uncovered` | a spec row no task claims, or a queued row the spec does not have |
| `rail-unenforced` | a rail whose named enforcement is not a real gate, probe or hook, or is one that does not run |
| `hash-uncovered` | a rail naming `test-hashes.json` with a file the hash file does not cover |
| `check-unnamed` | the context file names a check other than `check.command` |
| `learning-unenforced` | a `LEARNINGS.md` rule naming no file, command or hook |
| `learning-ungated` | a dated rule with no eval behind it (`[seed]` entries exempt) |
| `skill-ungated` | a `[[skill]]` entry with `gate = "none"`, or a gate that names nothing real |
| `ponytail-ceiling` | a `ponytail:` marker in code with no dated kill line already naming its text |
| `rejection-stale` | a `REJECTED` note under a status other than `ready`, or a block parked at `needs-spec` |
| `queue-hygiene` | a duplicate id, a missing status, a blocker no block defines, an open block matching no scope |
| `friction-repeat` | two `PROGRESS.md` friction lines that are the same friction reworded, with neither a rule nor a dated kill line covering it |
| `check-red` | the forced check is failing |
| `litter` | a tracked, untracked or ignored path on no allowlist |
| `install-stale` | in this repo's own checkout: an installed file that has drifted from what `harness init` would write now |
| `verdict-flip` | a task forced `done` → `ready` by `verdict` more than once in one run |
| `rejection-repeat` | two rejections whose reasons are the same friction reworded |
| `stage-outlier` | a stage whose seconds or cost is over twice its role's median, in a role with 3+ runs |
| `turns-exhausted` | a stage that hit exactly its configured turn ceiling |
| `limit-repeat` | two rate limits on stages back-to-back in the run's own sequence |
| `driver` | off unless `driver_command` is set and `HARNESS_DRIVER=1`; whatever the built artifact reports |

The last five read `.enallagi/events.jsonl` and report `OFF` until it exists.

## harness.toml

Embedded in the binary as `harness.default.toml`; a repo's `harness.toml` deep-merges over it
(tables key by key, arrays whole). Unknown keys are refused per table.

**`[agent]`** takes `preset` (default `claude`), `command` (string list, default the preset's
own), `model` (optional), `usage` (a table of JSON paths `cost`, `input_tokens`, `output_tokens`,
`turns`, default the preset's own) and `rate_limit_pattern` (default `hit your session limit`).
`[agent.scout]`, `[agent.adjudicator]`, `[agent.implementer]`, `[agent.verifier]` and
`[agent.researcher]` take the same fields minus `rate_limit_pattern`.

**`[check]`** takes `command` (default `bun run check`), `force` (the same check uncached, default
`bun run check -- --force`) and `fail_name` (a regex whose capture group 1 is the failing test's
name, default `` \(fail\) (.+?)(?: \[[0-9.]+m?s\])?$ ``).

**`[layout]`** (selected fields; the rest are the `litter`/`scope` allowlists)

| Field | Type | Default |
| --- | --- | --- |
| `harness_dir` | string | `.enallagi` |
| `skills_dir` | string, optional | the preset's own |
| `spec`, `rows_heading`, `rows_end_heading` | string | `SPEC.md`, `## 11. Exit criteria`, `## 12.` |
| `context_file` | string | `AGENTS.md` |
| `pointer_files` | string list | `CLAUDE.md`, `GEMINI.md`, `QWEN.md`, `.github/copilot-instructions.md` |
| `driver_command` | string | empty (off) |
| `learnings_cap` | integer | `12` |
| `allowed_prefixes`, `docs`, `harness_files`, `harness_globs`, `harness_allow`, `machinery` | string lists | `litter`'s and `scope`'s allowlists |

**`[[pipeline]]`** (three shipped: `review`, `task`, `discover`) takes `name`, `when` (one of
`queue.takeable`, `queue.reviewing`, `queue.empty`, `task.attended`, `check.red`, `probe.<name>`,
any `!`-negated), `stages` (stage names, in order) and `end_after_dry_rounds` (default `0`, `2` on
`discover`).

**`[[stage]]`** (four shipped: `implement`, `verify`, `scout`, `adjudicate`)

| Field | Type | Default |
| --- | --- | --- |
| `name` | string | required |
| `role` xor `command` | string | one role name, or a literal command |
| `turns` | integer | `40` (`120`/`100`/`30`/`40` shipped) |
| `timeout` | `<n>s` \| `<n>m` \| `<n>h`, optional | none; required if the preset's `turn_cap` is `none` |
| `env` | table of string→string | `{}` |
| `post` | gate-name list | `[]` |

**`[[skill]]`** (seven shipped: `tdd`, `ponytail`, `debugging`, `review-received`,
`verify-before-done`, `review-requested`, `brainstorming`) and **`[[role]]`** (none shipped)

| Field | Type | `[[skill]]` | `[[role]]` |
| --- | --- | --- | --- |
| `id` / `name` | string, `^[a-z0-9-]+$` | `id`, required | `name`, required |
| `source` | string (`github:owner/repo`, `git+file://`, `path:`) | required | required |
| `path` | string, relative | the skill directory | the directory holding `<name>.md` |
| `rev` | string, optional | none | none |
| `gate` | string (`none`, or a real gate/probe/rail name) | required | — |
| `why` | string | required | — |

## Agent presets

`agent.preset` selects one of thirteen, or `custom` with an explicit `agent.command`: `claude`,
`codex`, `gemini`, `opencode`, `copilot`, `goose`, `aider`, `amp`, `cursor`, `kimi`, `qwen`, `omp`,
`pi`. Each is a file in `crates/harness/adapters/presets/` naming the headless command, the usage
fields the tool reports (`claude` reports cost, tokens and turns; `codex`, `amp` and `qwen` report
tokens) and its `turn_cap`: `flag` fills `{turns}` into the command, `config` needs the cap set in
the tool's own config, `time` fills `{timeout}` in place of a turn count, and `none` caps nothing,
so the stage needs its own `timeout`.

## Skills

`harness skills sync` vendors every `[[skill]]` into `layout.skills_dir` (or the preset's own) at
its pinned `rev` and writes `harness.lock`; `check` verifies the vendored copies against the lock
and writes nothing; `list` prints one line per skill with its id, source and locked commit or
content hash (or `unlocked`). `--frozen` (or `CI` set in the environment) refuses to re-fetch a
skill whose vendored hash no longer matches the lock. A `[[role]]` resolves the same way: fetched,
vendored to `<harness_dir>/roles/<name>.md`, pinned under `[[role]]` in `harness.lock`, committed
by the pipeline before its stage, and refused by `harness hook immutable`.

## Events

`harness events [--role <r>] [--task <t>] [--since <ts>] [--json]` reads `.enallagi/events.jsonl`,
one JSON object per line, written by every `harness run`. The kinds are `run.start`, `run.end`,
`stage.start`, `stage.output`, `stage.end` (with `seconds`, `exit`, `cost`, `input_tokens`,
`output_tokens`, `turns`), `gate`, `task.status`, `halt`, `limit`, `skill.resolved` and `probe`;
each carries the stage, task, gate or probe it names and its reason or result.

## Files

`harness init` writes, relative to the repo root (`.enallagi` is `layout.harness_dir`):

| Path | What |
| --- | --- |
| `harness.toml` | seeded once from the embedded defaults; never overwritten once present |
| `.enallagi/roles/{scout,adjudicator,implementer,verifier,researcher}.md` | the five role prompts, always resubstituted |
| `.enallagi/RAILS.md` | the rails, each naming its enforcement, always resubstituted |
| `.enallagi/.gitignore` | ignores the harness's own scratch state |
| `<skills_dir>/running-the-loop/{SKILL.md,references/task-block.md}` | the harness's own usage skill, in the preset's skill directory (`.claude/skills/` by default) |
| `TASKS.md`, `PROGRESS.md`, `LEARNINGS.md`, `DECISIONS.md`, `.check-baseline`, `AGENTS.md`, `SPEC.md` | seeded once: the queue, the append-only record, the rules, the archive, the inherited red, the context file every role reads first, the spec under the heading `layout.rows_heading` names |
| `CLAUDE.md`, `GEMINI.md`, `QWEN.md`, `.github/copilot-instructions.md` | one-line pointers to `AGENTS.md`, from `layout.pointer_files` |
| `evals/README.md` | the eval runner's own documentation |

`--adapter <preset>` adds the tool-specific parts: `.claude/agents/` and `.claude/settings.json` (hooks,
`Monitor` denied, commit and PR `attribution` empty) for `claude`; for a preset whose `hooks_file` is set
(`codex`, `gemini`, `copilot`, `cursor`, `qwen`), that file, merged; for any other preset, nothing.
`bun-turbo` is not an agent and not an `--adapter` value; see `adapters/README.md`.

## Evals

`harness eval [--gate <name>] [names...]` builds a fixture repo per eval package under `evals/`
(every one, with no names), runs the configured agent once with the eval's prompt, and checks its
assertion. `--gate <name>` runs the named eval with its rule present, again with the rule ablated,
and every other eval; the rule is accepted only when the gated eval fails without it and nothing
else regresses. An agent that does not run, or a fixture that cannot be built, is an error, never a
pass. Three ship: `scout`, `adjudicator`, `verifier`.

## Testing this package

```bash
cargo test --workspace                 # the crate's own floor
cargo clippy --all-targets -- -D warnings
cargo fmt --check
HARNESS_DRIVER=1 cargo test -p harness --test floor -- --include-ignored driver
./docs/demo.sh                         # one task ready -> done in a repo it creates and deletes
./docs/bootstrap.sh --check            # the record of this repo having built itself, derived from git
```

CI runs `cargo fmt`, `cargo clippy -D warnings` and `cargo test --workspace` on `ubuntu-latest` and `macos-latest`;
a `shell` job runs `bash -n`, `shellcheck` and `docs/bootstrap.sh --check` over `driver.sh`, `docs/*.sh`,
`adapters/bun-turbo/*.sh` and `evals/*.sh`; a `driver` job runs the ignored test above against the release binary.

## Not included

Parallel lanes (`harness worktree [N]` isolates one, fast-forwarded back). A held-out test suite.
A driver for your own artifact (`driver.sh` is the worked example for this one). `test-hashes.json`
(`hash-uncovered` reports its absence until you write it). Evals for the implementer and researcher
roles. The vendored skills: `harness skills sync` fetches them from `harness.lock` after a clone.

## License

MIT. See `LICENSE`; `NOTICE` credits the skills this harness declares and does not write.
