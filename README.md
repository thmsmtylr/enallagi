# enallagi

**The fractional harness that could.**

An autonomous task loop for a coding agent, in one static binary.

## Highlights

- Reads a queue of tasks and works them one at a time.
- Runs each task in a fresh agent process, never a long-lived session.
- Splits the work across five roles that cannot grade each other.
- Re-derives every `done` from the tree instead of trusting a claim.
- Runs each lane in its own git worktree and fast-forwards on success.
- Wraps any headless agent CLI. Thirteen presets ship.
- Pins every vendored skill by commit and hash.
- No daemon and no service. Runtime needs are `git`, `sh`, and your agent CLI.

## Install

```bash
curl -LO https://github.com/thmsmtylr/enallagi/releases/latest/download/enallagi-aarch64-apple-darwin
chmod +x enallagi-aarch64-apple-darwin
sudo mv enallagi-aarch64-apple-darwin /usr/local/bin/enallagi
```

Releases carry four targets. Pick `x86_64` or `aarch64`, and `apple-darwin` or `unknown-linux-musl`.

From source:

```bash
cargo install --locked --git https://github.com/thmsmtylr/enallagi enallagi
```

Releasing is one procedure, stated in
[.github/workflows/release.yml](.github/workflows/release.yml).

## From install to a landed task

```bash
enallagi init                     # seeds the documents and writes .enallagi/enallagi.toml
$EDITOR .enallagi/enallagi.toml   # set agent.preset and check.command
enallagi init                     # re-run to apply the edited answers
```

Write a task into `.enallagi/TASKS.md`:

```
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
enallagi run --iterations 1   # one task, in this checkout
enallagi worktree 1           # or one task, in its own worktree
enallagi watch                # attach read-only to the live loop
enallagi pr T-001 --push      # one pull request for the landed task
```

## How a run works

Each iteration takes the first `ready` task whose blockers are `done`.

- The implementer edits only the paths on the task's `scope:` line.
- It commits, writes its notes, and sets the task to `review`.
- The verifier reads the criteria and returns `done` or a rejection.
- Gates then re-run behind that verdict and can overturn it.

A `done` the gates disagree with goes back to `ready`. A question the criteria do not answer
becomes `needs-spec` and halts the run.

## Configuration

`.enallagi/enallagi.toml` holds the whole configuration.

- `agent.preset` names the CLI to drive, or `custom` with your own `agent.command`.
- `agent.model` and `agent.effort` set defaults. A role or a task can override both.
- `check.command` is the single command that decides green.
- `check.timeout` bounds it and defaults to `30m`. A check that runs past it halts the run.
- `layout.*` says where the documents live.
- `[[skill]]` declares a skill to vendor, with its `rev` and the gate that enforces it.

Budgets come from the environment. Set `BUDGET_USD`, `BUDGET_SECONDS`, or `BUDGET_TOKENS`.

## Commands

- `init` installs into a repository. `eject` removes it and leaves no trace.
- `run` drives the pipelines in place. `worktree` drives them in an isolated checkout.
- `watch` attaches to a live loop. `events` queries the log.
- `tasks` reads and edits the queue. `base` prints the commit a task was queued against.
- `probe` reports findings. `gate` runs one gate. `hook` is the agent's lifecycle entry point.
- `skills` resolves declared skills. `eval` runs the evals. `pr` builds a pull request.

Run `enallagi <command> --help` for the flags.

## License

MIT
