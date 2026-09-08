# harness

An autonomous task loop for a coding agent, installed into any git repository. One statically
linked binary: it reads a queue (`TASKS.md`), runs one task per fresh agent process through five
separated roles, and re-derives every `done` from the tree rather than from what the agent said.
No daemon, no service, no vendor lock: the agent is whatever headless command `harness.toml` names.
Runtime dependencies: `git`, `sh`, that agent CLI, and the check command `harness.toml` names.

Targets `x86_64` and `aarch64` on `linux-musl` and `apple-darwin`.

## Install

A tagged release publishes four static binaries and a `SHA256SUMS`:

```bash
curl -LO https://github.com/<repo>/releases/latest/download/harness-aarch64-apple-darwin
chmod +x harness-aarch64-apple-darwin
sudo mv harness-aarch64-apple-darwin /usr/local/bin/harness
```

Or from source: `cargo install --path crates/harness`.

Then, in the repository to run it on:

```bash
harness init                    # writes harness.toml, seeds the documents from defaults
$EDITOR harness.toml            # agent.preset, check.command, layout.spec -- the three that matter
harness init                    # re-run: substitutes the edited answers
harness init --adapter claude   # optional: writes .claude/agents/ and the hook wiring
```

`harness init` prints the exact `git add` line for everything it wrote. Commit it: `verdict`
counts an untracked path as work off the branch. `--dry-run` prints the plan without writing it.
What gets written and what is only ever seeded once is in Files, below.

## What a run does

`harness run [N]` runs up to N iterations (default 3), each a pipeline chosen by the queue's state:

| Pipeline | `when` | Stages |
| --- | --- | --- |
| `task` | `queue.takeable` | **implement** → **verify** |
| `discover` | `!queue.takeable` | **scout** → **adjudicate** |

Every stage is a separate process spawned from a role prompt under `.harness/roles/`, turn-capped
per stage. `discover` ends the run after two consecutive rounds that leave nothing takeable.

Halts: a `STOP` file in the repo root; `BUDGET_SECONDS` / `BUDGET_USD` / `BUDGET_TOKENS` at the
next stage boundary (the dollar and token budgets need an `[agent.usage]` the preset's output can
fill, and the run halts if the budget is set and nothing was observed); the adjudicator halting on
a fix that needs a human (`needs-spec`); a stage that could not start.

`harness run` draws a TUI whenever stdout is a tty; `--no-tui` suppresses it, `--dry-run` prints the
plan and the probe output and spawns nothing — it runs no gates — `--frozen` refuses rather than
re-vendors a skill whose hash has moved.

## What is enforced, and by what

Every claim below is a mechanism in `crates/harness/src/gates.rs`, not a sentence in a prompt.
`[[stage]].post` names the gates that run after a stage; a gate that fails forces the task back to
`ready` (or halts the run, for the adjudicator) and writes the reason next to the status.

| Rule | Gate | Runs after |
| --- | --- | --- |
| Only the verifier may set `done` | `implementer-not-done` | implement |
| The work is committed, and the check is green on delta, before `done` is accepted | `verdict` | verify |
| Only the task's `scope:` globs are touched; a product task never edits the harness | `scope` | verify |
| The check, on delta against `.check-baseline` | `check-delta` | wherever a stage's `post` names it |

Independent of the pipeline: `harness hook <name>`, wired into an adapter's hooks file where the
tool has one, reading its input from stdin.

| Hook | Fires on | Does |
| --- | --- | --- |
| `immutable` | pre-tool-use | blocks an edit to a file `test-hashes.json` covers |
| `one-writer` | pre-tool-use | refuses an edit from a session that is not the running loop's own (liveness: `<harness_dir>/loop.pid`) |
| `verify-done` | stop | reports the check's own state before the agent claims done |
| `skills` | prompt-submit | prints every declared skill's id, why it is relied on, and its gate |

The full rail list, including the judgment calls no mechanism checks, is `.harness/RAILS.md`.

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

`file` after the other arguments defaults to `TASKS.md`. `status` is a block's first word;
anything after it is the reason. `done` blocks archive to `DECISIONS.md`, leaving a stub the
launcher still reads.

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
| `queue-hygiene` | a duplicate id, a missing status, a blocker no block defines, a `done` block matching no scope |
| `friction-repeat` | two `PROGRESS.md` friction lines that are the same friction reworded, with no rule covering it |
| `check-red` | the forced check is failing |
| `litter` | a tracked, untracked or ignored path on no allowlist |
| `install-stale` | in this repo's own checkout: an installed file that has drifted from what `harness init` would write now |
| `verdict-flip` | a task forced `done` → `ready` by `verdict` more than once in one run |
| `rejection-repeat` | two rejections whose reasons are the same friction reworded |
| `stage-outlier` | a stage whose seconds or cost is over twice its role's median, in a role with 3+ runs |
| `turns-exhausted` | a stage that hit exactly its configured turn ceiling |
| `limit-repeat` | two rate limits on stages back-to-back in the run's own sequence |
| `driver` | off unless `driver_command` is set and `HARNESS_DRIVER=1`; whatever the built artifact reports |

The last five read `.harness/events.jsonl` and report `OFF` until it exists.

## harness.toml

Embedded in the binary as `harness.default.toml`; a repo's `harness.toml` deep-merges over it
(tables key by key, arrays whole). Unknown keys are refused per table.

**`[agent]`**

| Field | Type | Default |
| --- | --- | --- |
| `preset` | string | `claude` |
| `command` | string list | the preset's own `argv` |
| `model` | string, optional | none |
| `usage` | table (`cost`, `input_tokens`, `output_tokens`, `turns`, each a JSON path) | the preset's own |
| `rate_limit_pattern` | string | `hit your session limit` |

`[agent.scout]`, `[agent.adjudicator]`, `[agent.implementer]`, `[agent.verifier]`,
`[agent.researcher]` take the same fields minus `rate_limit_pattern`, to run one role on a
different preset, command or model.

**`[check]`**

| Field | Type | Default |
| --- | --- | --- |
| `command` | string | `bun run check` |
| `force` | string | `bun run check -- --force` |
| `fail_name` | regex, capture group 1 | `` \(fail\) (.+?)(?: \[[0-9.]+m?s\])?$ `` |

**`[layout]`** (selected fields; the rest are the `litter`/`scope` allowlists)

| Field | Type | Default |
| --- | --- | --- |
| `harness_dir` | string | `.harness` |
| `skills_dir` | string, optional | the preset's own |
| `spec`, `rows_heading`, `rows_end_heading` | string | `SPEC.md`, `## 11. Exit criteria`, `## 12.` |
| `context_file` | string | `AGENTS.md` |
| `pointer_files` | string list | `CLAUDE.md`, `GEMINI.md`, `.github/copilot-instructions.md` |
| `driver_command` | string | empty (off) |
| `learnings_cap` | integer | `12` |
| `allowed_prefixes`, `docs`, `harness_files`, `harness_globs`, `harness_allow`, `machinery` | string lists | `litter`'s and `scope`'s allowlists |

**`[[pipeline]]`** (two shipped, `task` and `discover`)

| Field | Type | Default |
| --- | --- | --- |
| `name` | string | required |
| `when` | predicate | `queue.takeable`, `queue.empty`, `task.attended`, `check.red`, `probe.<name>`, any `!`-negated |
| `stages` | string list | stage names, in order |
| `end_after_dry_rounds` | integer | `0` (`2` on `discover`) |

**`[[stage]]`** (four shipped: implement, verify, scout, adjudicate)

| Field | Type | Default |
| --- | --- | --- |
| `name` | string | required |
| `role` xor `command` | string | one role name, or a literal command |
| `turns` | integer | `40` (`120`/`100`/`30`/`40` shipped) |
| `timeout` | `<n>s` \| `<n>m` \| `<n>h`, optional | none — required if the preset's `turn_cap` is `none` |
| `env` | table of string→string | `{}` |
| `post` | gate-name list | `[]` |

**`[[skill]]`** (seven shipped)

| Field | Type | Default |
| --- | --- | --- |
| `id` | string, `^[a-z0-9-]+$` | required |
| `source` | string (`github:owner/repo`) | required |
| `path` | string, relative | required |
| `rev` | string, optional | none |
| `gate` | string (`none`, or a real gate/probe/rail name) | required |
| `why` | string | required |

Shipped: `tdd`, `ponytail`, `debugging`, `review-received`, `verify-before-done`,
`review-requested`, `brainstorming`.

**`[[role]]`** (none shipped)

| Field | Type | Default |
| --- | --- | --- |
| `name` | string, `^[a-z0-9-]+$` | required |
| `source` | string (`github:owner/repo`, `git+file://`, `path:`) | required |
| `path` | string, relative; the file is `<path>/<name>.md` | required |
| `rev` | string, optional | none |

## Agent presets

`agent.preset` selects one of thirteen (`crates/harness/adapters/presets/*.toml`), or `custom`
with an explicit `agent.command`.

| Preset | Headless command | Usage reported | Turn cap |
| --- | --- | --- | --- |
| `claude` | `claude -p {prompt} --output-format stream-json --verbose --max-turns {turns} --dangerously-skip-permissions` | cost, tokens, turns | flag |
| `codex` | `codex exec {prompt} --json --dangerously-bypass-approvals-and-sandbox` | tokens | none |
| `gemini` | `gemini -p {prompt} --output-format stream-json --yolo` | — | config |
| `opencode` | `opencode run {prompt} --format json --auto` | — | config |
| `copilot` | `copilot -p {prompt} --allow-all-tools --max-autopilot-continues {turns}` | — | flag |
| `goose` | `goose run -t {prompt} --output-format stream-json --max-turns {turns}` | — | flag |
| `aider` | `aider --message {prompt} --yes-always` | — | none |
| `amp` | `amp -x {prompt} --stream-json --dangerously-allow-all` | tokens | none |
| `cursor` | `agent -p {prompt} --output-format stream-json --force` | — | none |
| `kimi` | `kimi -p {prompt} --output-format stream-json --yolo` | — | config |
| `qwen` | `qwen -p {prompt} --output-format stream-json --yolo --max-session-turns {turns}` | tokens | flag |
| `omp` | `omp -p {prompt} --mode json --yolo --max-time {timeout}` | — | time |
| `pi` | `pi -p {prompt} --mode json` | — | none |

`turn_cap`: `flag` fills `{turns}` into the command; `config` needs the cap set in the tool's own
config; `time` fills `{timeout}` in place of a turn count; `none` caps nothing, so the stage needs
its own `timeout`.

## Skills

`harness skills check|sync|list [--frozen]` operates on the `[[skill]]` list, vendoring each into
`layout.skills_dir` (or the preset's own) at the pinned `rev` and recording it in `harness.lock`.

| Command | Does |
| --- | --- |
| `list` | one line per skill: id, source, and the locked commit or content hash, or `unlocked` |
| `sync` | vendors every skill at its pinned `rev`, writing `harness.lock` |
| `check` | verifies the vendored copy against `harness.lock`; writes nothing |

`--frozen` (or `CI` set in the environment) refuses to re-fetch a skill whose vendored hash no
longer matches the lock, rather than silently re-vendoring it. `harness hook skills` prints every
declared skill's id, why it is relied on, and its gate at the start of a turn. A `[[role]]` resolves
the same way: fetched, vendored to `<harness_dir>/roles/<name>.md`, pinned under `[[role]]` in
`harness.lock`, committed by the pipeline before its stage, and refused by `harness hook immutable`.

## Events

`harness events [--role <r>] [--task <t>] [--since <ts>] [--json]` reads `.harness/events.jsonl`,
one JSON object per line, written by every `harness run`.

| Kind | Carries |
| --- | --- |
| `run.start` | `config_sha256`, `pipeline` |
| `run.end` | `halts`, `landed`, `promoted`, `killed`, `warnings` |
| `stage.start` | `stage`, `role`, `command`, `task` |
| `stage.output` | `stage`, `chunk` |
| `stage.end` | `stage`, `task`, `seconds`, `exit`, `cost`, `input_tokens`, `output_tokens`, `turns` |
| `gate` | `gate`, `task`, `pass`, `reason` |
| `task.status` | `task`, `from`, `to`, `reason`, `by` |
| `halt` | `halt`, `reason` |
| `limit` | `stage`, `matched`, `sleep_seconds`, `attempt` |
| `skill.resolved` | `id`, `commit`, `result` |
| `probe` | `name`, `count`, `error` |

## TUI

`harness run` draws a live view whenever stdout is a tty (`--no-tui` suppresses it); `harness
watch` attaches read-only to a running loop's own event log from a second terminal. Three panes —
queue, stages, output. `Tab` cycles focus, `Up`/`Down` scroll the focused pane, `?` toggles a help
overlay, `q` quits.

## Files

`harness init` writes, relative to the repo root (`.harness` is `layout.harness_dir`):

| Path | What |
| --- | --- |
| `harness.toml` | seeded once from the embedded defaults; never overwritten once present |
| `.harness/roles/{scout,adjudicator,implementer,verifier,researcher}.md` | the five role prompts, always resubstituted |
| `.harness/RAILS.md` | the rails, each naming its enforcement, always resubstituted |
| `.harness/.gitignore` | ignores the harness's own scratch state |
| `<skills_dir>/running-the-loop/{SKILL.md,references/task-block.md}` | the harness's own usage skill, in the preset's skill directory (`.claude/skills/` by default) |
| `TASKS.md`, `PROGRESS.md`, `LEARNINGS.md`, `DECISIONS.md`, `.check-baseline` | seeded once: the queue, the append-only record, the rules, the archive, the inherited red |
| `AGENTS.md` | the context file every role reads first, seeded once |
| `SPEC.md` | seeded once, under the heading `layout.rows_heading` names |
| `CLAUDE.md`, `GEMINI.md`, `.github/copilot-instructions.md` | one-line pointers to `AGENTS.md`, from `layout.pointer_files` |
| `evals/README.md` | the eval runner's own documentation |

`--adapter <preset>` adds more; see Adapters below.

## Evals

`harness eval [--gate <name>] [names...]` builds a fixture repo per eval package under `evals/`,
runs the configured agent once with the eval's prompt, and checks its assertion. With no names,
every eval in `evals/` runs. `--gate <name>` runs the named eval with its rule present, again with
the rule ablated, and every other eval; a rule is accepted only when the gated eval fails without
it and nothing else regresses. An agent that does not run, or a fixture that cannot be built, is
an error and never a pass. Three ship: `scout`, `adjudicator`, `verifier`.

## Adapters

`harness init --adapter <preset>` names one of the thirteen agent presets and adds the parts that
are tool-specific: `.claude/agents/` and `.claude/settings.json` hook wiring for `claude`; for a
preset whose `hooks_file` is set (`codex`, `gemini`, `copilot`, `cursor`, `qwen`), that file,
merged with one that already exists; for any other preset, nothing. `bun-turbo` is not an agent
and not an `--adapter` value — see `adapters/README.md`.

## Testing this package

```bash
cargo test --workspace                 # the crate's own floor
cargo clippy --all-targets -- -D warnings
cargo fmt --check
HARNESS_DRIVER=1 cargo test -p harness --test floor -- --include-ignored driver
./docs/demo.sh                         # one task ready -> done in a repo it creates and deletes
./docs/bootstrap.sh --check            # the record of this repo having built itself, derived from git
```

CI runs `cargo fmt`, `cargo clippy -D warnings` and `cargo test --workspace` on `ubuntu-latest` and
`macos-latest`; a `shell` job runs `bash -n` and `shellcheck` at full severity over what remains
(`driver.sh`, `docs/*.sh`, `adapters/bun-turbo/*.sh`, `evals/*.sh`) plus `docs/bootstrap.sh
--check`; a `driver` job runs the ignored test above against the release binary.

## Not included

Parallel lanes (`harness worktree [N]` isolates one, fast-forwarded back). A held-out test suite.
A driver for your own artifact (`driver.sh` is the worked example for this one).
`test-hashes.json` (`hash-uncovered` reports its absence until you write it). Evals for the
implementer and researcher roles.

## License

Apache-2.0. See `LICENSE` and `NOTICE`.
