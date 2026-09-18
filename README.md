# enallagi

**The fractional harness that could.**

An autonomous task loop for a coding agent, in one static binary.

## Highlights

- Reads a queue of tasks and works them one at a time.
- Runs each task in a fresh agent process, never a long-lived session.
- Splits the work across five roles that cannot grade each other.
- Re-derives every `done` from the tree instead of trusting a claim.
- Runs each lane in its own git worktree and fast-forwards on success.
- Wraps any headless agent CLI, with thirteen presets shipped.
- Pins every vendored skill by commit and hash.

## Install

```bash
curl -LO https://github.com/thmsmtylr/enallagi/releases/latest/download/enallagi-aarch64-apple-darwin
chmod +x enallagi-aarch64-apple-darwin
sudo mv enallagi-aarch64-apple-darwin /usr/local/bin/enallagi
```

Runtime needs `git`, `sh` and your agent CLI.

Releases carry four targets. Pick `x86_64` or `aarch64`, and `apple-darwin` or `unknown-linux-musl`.

From source:

```bash
cargo install --locked --git https://github.com/thmsmtylr/enallagi enallagi
```

Releasing is one procedure, stated in
[.github/workflows/release.yml](.github/workflows/release.yml).

## From install to a landed task

[docs/setup.md](docs/setup.md) walks the same path on a real repository, step by step.

```bash
enallagi init                     # seeds the documents and writes .enallagi/enallagi.toml
$EDITOR .enallagi/enallagi.toml   # set agent.preset and check.command
enallagi init                     # re-run to apply the edited answers
```

Turn an issue into a task:

```bash
enallagi issue owner/repo#12   # appends a proposed block to .enallagi/TASKS.md
$EDITOR .enallagi/TASKS.md     # fill scope: and criteria:, then set status: ready
```

The block quotes the issue in `notes:`. It stays `proposed` until you write what done means.

Or write a task into `.enallagi/TASKS.md` by hand:

```markdown
## [T-001] the date parser drops a timezone
scope: src/date.ts, src/date.test.ts
blockedBy:
status: ready
rows: none — harness
criteria:
  - `npm test -- date` passes with a case for `+10:00`
notes:
```

Run it:

```bash
enallagi run --pipeline task --iterations 1   # one task, in this checkout
enallagi worktree 1                          # or one task, in its own worktree
enallagi watch                               # attach read-only to the live loop
enallagi pr T-001 --push                     # one pull request for the landed task
```

## How a run works

Each iteration takes the first `ready` task whose blockers are `done`.

- The implementer edits only the paths on the task's `scope:` line.
- It commits, writes its notes, and sets the task to `review`.
- The verifier reads the criteria and returns `done` or a rejection.
- Gates then re-run behind that verdict and can overturn it.

A `done` the gates disagree with goes back to `ready`. A question the criteria do not answer
becomes `needs-spec` and halts the run.

`enallagi run --pipeline task` runs only the pipeline named. Repeat the flag to name more than one.
With none named, every pipeline in `.enallagi/enallagi.toml` is eligible.

The pipeline, gate, probe, key and file tables are in [docs/reference.md](docs/reference.md).
Changing a skill, a pipeline or a stage is in [docs/pipeline.md](docs/pipeline.md).

## Configuration

`.enallagi/enallagi.toml` holds the whole configuration.
Every key and its default is in [docs/configuration.md](docs/configuration.md).

`enallagi init` writes only the keys whose value differs from the embedded defaults.
A key left out takes its default.

A re-run names every key that still equals one.
`enallagi init --prune-defaults` deletes those keys.

A first `enallagi init` also detects the test runner from the files the repository already carries.
Seven runners ship: cargo, node, bun, vitest, jest, pytest and go.
Detection writes `check.command`, `check.fail_name` and the `layout` test keys.

Each detected value is printed with the `file:line` it came from.
A tree that matches no runner, or more than one, keeps the defaults and names the candidates.

- `agent.preset` names the CLI to drive, or `custom` with your own `agent.command`.
- `agent.model` and `agent.effort` set defaults. A role or a task can override both.
- `agent.dangerously_skip_permissions` adds the preset's bypass flag to every lane. Default `false`.
- `enallagi run --dangerously-skip-permissions` does the same for one run.
  A preset that declares no bypass flag refuses the run.
- `run.start` records the choice as `permissions_skipped`, and `enallagi events` prints it.
- `check.command` is the single command that decides green.
- `[[skill]]` declares a skill to vendor, with its `rev` and the gate that enforces it.

Budgets come from the environment. Set `BUDGET_USD`, `BUDGET_SECONDS`, or `BUDGET_TOKENS`.
`BUDGET_TOKENS` counts every token lane `[agent.usage]` names, cached reads and cache writes included.

## Commands

- `init` installs into a repository. `eject` removes it and leaves no trace.
- `run` drives the pipelines in place. `worktree` drives them in an isolated checkout.
- `watch` attaches to a live loop. `events` queries the log.
- `tasks` reads and edits the queue. `base` prints the commit a task was queued against.
- `issue` appends a GitHub issue to the queue as a `proposed` task.
- `probe` reports findings. `gate` runs one gate. `hook` is the agent's lifecycle entry point.
- `skills` resolves declared skills. `eval` runs the evals. `pr` builds a pull request.

`enallagi pr --push` reads the target's contribution guide first.
A sentence there that refuses or conditions generated changes stops the push.

Pass `--policy-read` once you have read it. The description file records either outcome.

Run `enallagi <command> --help` for the flags.

## License

MIT
