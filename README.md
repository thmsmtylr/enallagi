# enallagi

enallagi is an autonomous task loop for a coding agent: one static binary that reads a queue of
tasks out of a markdown file, runs each one through separate implement and verify agent processes,
and re-derives every `done` from the tree.

## Install

```bash
curl -LO https://github.com/thmsmtylr/enallagi/releases/latest/download/enallagi-aarch64-apple-darwin
chmod +x enallagi-aarch64-apple-darwin
sudo mv enallagi-aarch64-apple-darwin /usr/local/bin/enallagi
```

Each release attaches `x86_64` and `aarch64` builds for `linux-musl` and `apple-darwin`, and a
`SHA256SUMS`. From source: `cargo install --locked --git https://github.com/thmsmtylr/enallagi enallagi`.
Runtime dependencies: `git`, `sh`, the agent CLI `enallagi.toml` names, and the check command.
A release is cut by [`.github/workflows/release.yml`](.github/workflows/release.yml), which states
the procedure; the changes per release are in [`CHANGELOG.md`](CHANGELOG.md).

## From install to a landed task

```bash
enallagi init                     # seeds .enallagi/ from the embedded defaults
$EDITOR .enallagi/enallagi.toml   # agent.preset, check.command, layout.spec
enallagi init                     # re-run: substitutes the edited answers
enallagi init --adapter claude    # optional: the claude plugin's agents and hooks
```

Write the exit criteria into `.enallagi/SPEC.md` under the heading `layout.rows_heading` names, then
a first task into `.enallagi/TASKS.md`. A task is a block; `enallagi tasks` is the only parser:

```
## [T-001] one line, in the finding's own words
scope: src/thing.ts, src/thing.test.ts
blockedBy: none
status: ready
rows: none
criteria:
  - objective, naming the command whose output changes when it is done
notes: the implementer's and the verifier's own words; output pasted, not summarised
```

Commit what `init` wrote outside `.enallagi/`: the `verdict` gate counts an untracked path as work
off the branch, so a file left untracked fails the first verdict.

```bash
enallagi probe                    # what the tree says about itself
enallagi tasks ready              # the first unblocked ready id
enallagi run --dry-run            # the plan and the probe output; spawns nothing
enallagi run --iterations 1       # one iteration: implement, then verify
```

The implementer edits only the files on `scope:`, commits them, and sets `status: review`. The
verifier re-runs the check and either promotes the task to `done` or rejects it to `ready` with its
reasons in `notes:`. `enallagi pr T-001` then replays the task's product commits onto a branch cut
from the upstream default branch, runs the check there, and writes `.enallagi/pr/T-001.md`.

## How a run works

`enallagi run --iterations <N>` (`-n <N>`, default 3) runs up to N iterations, each a pipeline the
queue's state selects:

| Pipeline | `when` | Stages |
| --- | --- | --- |
| `review` | `queue.reviewing` | **verify** |
| `task` | `queue.takeable` | **implement** → **verify** |
| `discover` | `!queue.takeable` | **scout** → **adjudicate** |

Every stage is a separate process spawned from a role prompt under `.enallagi/roles/`, turn-capped
per stage. `review` runs before `task`. `discover` ends the run after two dry rounds. Roles commit
product files; the launcher commits the harness directory as `<stage> T-### at <product sha>`, read
back by `enallagi base T-###` as a task's diff base, and writes a `PROGRESS.md` entry when no role
did.

A run halts on a `STOP` file in the repo root; on `BUDGET_SECONDS` / `BUDGET_USD` / `BUDGET_TOKENS`
(or `--budget-seconds` / `--budget-usd` / `--budget-tokens`, which wins) at the next stage boundary,
where the dollar and token budgets need an `[agent.usage]` the preset fills; on the adjudicator
parking a fix at `needs-spec`; and on a stage that could not start. `--frozen` refuses to re-vendor a
skill whose hash has moved. A run draws a live view whenever stdout is a tty (`--no-tui` suppresses
it), and `enallagi watch` attaches read-only to a running loop's event log.

`[[stage]].post` names the gates run after a stage. A failed gate forces the task back to `ready`, or
halts the run, and writes its reason beside the status.

| Gate | Refuses | Runs after |
| --- | --- | --- |
| `implementer-not-done` | a `done` from anyone but the verifier, or an implementer that stopped short of `review` | implement |
| `verdict` | a `done` whose work is uncommitted, or whose check is red on delta | verify |
| `scope` | a file outside the task's `scope:` globs, and a product task editing the harness | verify |
| `queue-intact` | a task id at the iteration's base commit that is in neither TASKS.md nor DECISIONS.md | implement, verify, adjudicate |
| `check-delta` | a check failure that is not already in `.check-baseline` | wherever a stage's `post` names it |

`enallagi probe` reports what no gate can refuse: a spec row with no test, a rail whose enforcement
is not real, a rule with no eval, a stage over twice its role's median cost, a duplicate task id. It
prints one `PROBE <name> <count>` line per probe and a `FINDING` line per shortfall; a probe that
cannot run prints `PROBE <name> ERROR`, never a count of zero. The rails themselves, including the
judgment calls no mechanism checks, are `.enallagi/RAILS.md`.

`enallagi hook <name>` runs outside the pipeline, wired into an adapter's hooks file, reading the
tool's JSON on stdin: `immutable` blocks an edit to a file `test-hashes.json` covers, `one-writer`
refuses an edit from a session that is not the running loop's own, `verify-done` reports the check's
state before the agent claims done, and `skills` prints every declared skill, its use and its gate.

## Configuration

`.enallagi/enallagi.toml` deep-merges over the embedded `harness.default.toml` (tables key by key,
arrays whole). Unknown keys are refused per table.

**`[agent]`** takes `preset` (default `claude`), `command` (string list, default the preset's own),
`model`, `effort`, `usage` (JSON paths for `cost`, `input_tokens`, `output_tokens`, `turns`) and
`rate_limit_pattern` (default `hit your session limit`). `[agent.scout]`, `[agent.adjudicator]`,
`[agent.implementer]`, `[agent.verifier]` and `[agent.researcher]` take the same fields minus
`rate_limit_pattern`, and a task's own `model:` and `effort:` lines win over both.

`preset` selects one of thirteen headless CLIs, or `custom` with an explicit `agent.command`:
`claude`, `codex`, `gemini`, `opencode`, `copilot`, `goose`, `aider`, `amp`, `cursor`, `kimi`,
`qwen`, `omp`, `pi`. Each is a file in `crates/harness/adapters/presets/` naming the command, the
usage fields the tool reports, and its `turn_cap`: `flag` fills `{turns}` into the command, `config`
needs the cap set in the tool's own config, `time` fills `{timeout}` instead, and `none` caps
nothing, so the stage needs its own `timeout`.

**`[check]`** takes `command` (default `bun run check`), `force` (the same check uncached) and
`fail_name` (a regex whose capture group 1 is the failing test's name).

**`[layout]`** takes `harness_dir` (default `.enallagi`), `skills_dir`, `spec`, `rows_heading`,
`rows_end_heading`, `context_file` (default `AGENTS.md` beside the queue), `pointer_files`,
`driver_command`, `learnings_cap`, and the allowlists `litter` and `scope` read.

**`[[pipeline]]`** takes `name`, `when` (`queue.takeable`, `queue.reviewing`, `queue.empty`,
`task.attended`, `check.red`, `probe.<name>`, any `!`-negated), `stages` and `end_after_dry_rounds`.
**`[[stage]]`** takes `name`, `role` xor `command`, `turns` (default 40), `timeout`, `env` and
`post`. **`[[skill]]`** and **`[[role]]`** take `id`/`name`, `source` (`github:owner/repo`,
`git+file://`, `path:`), `path` and `rev`, and a skill also takes `gate` and `why`.
`enallagi skills sync` vendors each one at its pinned revision and writes `harness.lock`; `check`
verifies the vendored copies and writes nothing.

## Files

`enallagi init` writes, relative to the repo root, with `.enallagi` as `layout.harness_dir`:

| Path | What |
| --- | --- |
| `.enallagi/roles/{scout,adjudicator,implementer,verifier,researcher}.md` | the five role prompts, always resubstituted |
| `.enallagi/RAILS.md`, `.enallagi/.gitignore` | the rails, each naming its enforcement; the ignore file for the harness's own scratch state |
| `<skills_dir>/running-the-loop/` | the loop's own usage skill, in the preset's skill directory |
| `.enallagi/{enallagi.toml,TASKS.md,PROGRESS.md,LEARNINGS.md,DECISIONS.md,.check-baseline,SPEC.md,evals/README.md}` | seeded once and never overwritten: the config, the queue, the append-only record, the rules, the archive, the inherited red, the spec, the eval runner's documentation |
| `.enallagi/AGENTS.md`; `CLAUDE.md`, `GEMINI.md`, `QWEN.md`, `.github/copilot-instructions.md` | seeded once: the context file every role reads first, and one-line pointers to it for a preset whose argv carries no `{context_file}` |

`--adapter <preset>` adds the tool-specific parts: for `claude`, the plugin's `agents/` and
`hooks/hooks.json`, which every lane loads with `--plugin-dir`; for a preset whose `hooks_file` is
set (`codex`, `gemini`, `copilot`, `cursor`, `qwen`), that file, merged with one already there.

A fresh `.enallagi/` is its own git repository, and it and every untracked file init writes outside
it go in one `# >>> harness` block of `git rev-parse --git-path info/exclude`, never `.gitignore`.
A root `TASKS.md` or a `.harness/` keeps its layout: `init` lists each file with its new path, and
`--move` moves them and installs nothing. `--dry-run` writes nothing.
`enallagi eject` removes `.enallagi/`, each still-untracked path and the exclude block, and refuses
while a lane worktree or a live loop exists, naming each; `--keep-record <dir>` moves the record out.

`enallagi events` reads `.enallagi/events.jsonl`, one JSON object per line, written by every run:
`run.start`, `run.end`, `stage.start`, `stage.output`, `stage.end` (with `seconds`, `exit`, `cost`,
`input_tokens`, `output_tokens`, `turns`), `gate`, `task.status`, `halt`, `limit`, `skill.resolved`
and `probe`. `enallagi eval` builds a fixture repo per package under `evals/`, runs the configured
agent once with the eval's prompt and checks its assertion; `--gate <name>` runs the named eval with
a candidate rule present, again with it ablated, and every other eval, accepting the rule only when
the gated eval fails without it and nothing else regresses.

`enallagi worktree [N]` runs a lane in its own git worktree, product and harness directory both
fast-forwarded back or neither. `.enallagi/test-hashes.json` is not shipped: `hash-uncovered`
reports its absence until it is written. The vendored skills are fetched by `enallagi skills sync`
from `harness.lock` after a clone. `adapters/README.md` covers `bun-turbo`, which is a check
adapter and not an `--adapter` value.

## License

MIT. See `LICENSE`; `NOTICE` credits the skills this harness declares and does not write.
