# harness

An autonomous task loop for a coding agent, installed into any git repository. It reads a queue
(`TASKS.md`), runs one task per fresh agent process through four separated roles, and re-derives
every `done` from the tree rather than from what the agent said. Bash, git and python3; no daemon,
no service, no vendor lock: the agent is whatever headless command `harness.json` names.

Runs on macOS (bash 3.2, BSD userland) and Linux (bash 5, GNU). CI exercises both.

## Install

```bash
git clone <this> ~/harness
~/harness/install.sh /path/to/repo       # first run: seeds harness.json and the documents from defaults
$EDITOR /path/to/repo/harness.json       # check, spec, agentCommand -- the three that matter
~/harness/install.sh /path/to/repo       # second run: substitutes your answers; re-syncs the check AGENTS.md names
```

`install.sh` prints the exact `git add` line for everything it wrote. Commit it: the launcher
counts every untracked path as work off the branch. Then:

```bash
.harness/hooks/probes.sh                 # what the tree says about itself
.harness/loop.sh 1                       # one iteration, attended, watch it work
```

Re-running `install.sh` is how you upgrade. Documents with content are never overwritten; the
harness directory and skills always are. `--dry-run` prints what would be written. `--adapter
claude` wires the hook scripts into `.claude/settings.json`, merging into one that already exists.

## What a run does

`.harness/loop.sh [N]` runs up to N iterations. Each iteration is one of two shapes, every stage a
separate process with a role prompt from `.harness/roles/`:

| Queue state | Stages |
| --- | --- |
| a task is `ready`, unblocked, not `attended: true` | **implementer** → **verifier** → gates → archive |
| nothing takeable | **scout** (probe output → `proposed` blocks) → **adjudicator** (`ready` or killed) |

Two consecutive scout rounds that leave nothing takeable end the run. `touch STOP` halts before
the next stage. `BUDGET_SECONDS` and `BUDGET_USD` halt at the next stage boundary; the dollar
budget requires an `agentCommand` whose output `costSed` can read (for Claude Code, add
`--output-format json`), and the loop halts if it is set and no cost was observed.

The digest at the end lists what landed, what was promoted or killed, every halt and every warning.
`.harness/run.log` holds one tab-separated row per spawned stage: time, iteration, role, task,
seconds, exit code, cost.

## What is enforced, and by what

Every claim below is a mechanism in the launcher, not a sentence in a prompt. A `done` the tree
cannot support is forced back to `ready` and the reason is written next to the status.

| Rule | Mechanism |
| --- | --- |
| Only the verifier may set `done` | `loop.sh`: a task at `done` after the implement stage is forced back and the verify stage skipped |
| The work is on the branch | `gate_verdict`: any untracked or modified path fails the verdict |
| The check is green, on delta | `gate_verdict` → `check-gate.sh`: red on anything not already in `.check-baseline` fails |
| `.check-baseline` only shrinks | `gate_scope`: a line added to it in the iteration's commits fails, under any task |
| Only the files on `scope:` | `gate_scope`: the iteration's commits diffed against the task's globs |
| A product task never edits the harness | `gate_scope`: hooks, launcher, baseline or `harness.json` touched under a task not marked `rows: none — harness` fails |
| A queue the launcher cannot read is not a pass | `tasks.py` refuses a duplicate id or an unterminated code fence; both gates fail closed on it |
| One checkout, one writer | `loop.pid` while the loop runs; `archive-done.sh` will not rewrite the queue under it, and `one-writer.sh` (Claude adapter) refuses an Edit/Write from any session not under that loop |
| A rate limit is a notice, not a word | `agent.sh`: retried only when a reset time parses out of the output, at most twice |

Ceilings, stated: a `PreToolUse` hook never sees a `sed -i` from Bash, so `one-writer.sh` and
`immutable.sh` make a bypass deliberate rather than impossible. `loop.pid` can be reused after a
crash until the next loop overwrites it. The verifier's judgment calls (`citable`,
`measure-first`, `minimal`) are prompts, and the full table with each rail's enforcement is
`.harness/RAILS.md`.

## The queue

`TASKS.md` is a list of blocks. `harness/tasks.py` is the only parser; `tasks.py list`,
`ready-unattended`, `ids-at <status>`, `field <id> <key>`, `set-status`, `unblock`.

```
## [T-001] one line naming the defect
scope: src/a.ts, src/b.ts       # globs the implementer may touch
blockedBy: none                 # or T-nnn, T-mmm
status: ready                   # proposed | ready | review | done | blocked | needs-spec | deferred
rows: none — harness            # or the exit-criteria row this turns green
criteria:
  - a command someone else can run
notes: |
  the implementer's and verifier's own words; output pasted, not summarised
```

`status` is its first word; anything after it is the reason. A fenced code block is documentation,
not a task. `done` blocks are moved to `DECISIONS.md` by `archive-done.sh` at the top of each
iteration, leaving a stub the launcher can still read.

## Probes

`.harness/hooks/probes.sh` prints one `PROBE <name> <count>` line per probe and a `FINDING` line
per shortfall. The scout may propose only from a `FINDING` line. A probe that errors prints
`PROBE <name> ERROR`; it never reports 0 for a check that did not run.

| Probe | Reports |
| --- | --- |
| `spec-untested` | an exit-criteria row in `spec` with no test named in backticks, or a named test that does not exist |
| `queue-uncovered` | a row no task in the queue claims |
| `rail-unenforced` | a rail whose named enforcement is in no check, hook or hash |
| `hash-uncovered` | a harness file `test-hashes.json` does not cover |
| `check-unnamed` | the context file names no command matching `harness.json` `check` |
| `learning-unenforced` | a `LEARNINGS.md` rule naming no file, command or hook |
| `learning-ungated` | a dated rule with no `evals/run.sh --gate` behind it |
| `skill-ungated` | a declared skill with `gate: none` |
| `ponytail-ceiling` | a `ponytail:` marker in source with no dated kill line in `DECISIONS.md` |
| `rejection-stale` | a `REJECTED` note under a status other than `ready`, or a `needs-spec` block |
| `queue-hygiene` | duplicate ids, missing fields, dangling `blockedBy` |
| `friction-repeat` | the same `friction:` twice in `PROGRESS.md` (token overlap ≥ 0.5) with no rule covering it |
| `check-red` | the check is failing |
| `litter` | a tracked or untracked path on no allowlist |
| `install-stale` | in this package's own checkout only: an installed file that differs from what `install.sh` would write |
| `driver` | with `driverCommand` set and `HARNESS_DRIVER=1`: whatever the driver reports after reaching the artifact |

## harness.json

Every key is substituted into the installed files as `__SCREAMING_SNAKE__`; a value that is also
read by a Python heredoc is additionally rendered as `__KEY_JSON__`, so quotes in it are characters.
Defaults come from `harness.default.json`; a key you omit takes the default.

| Key | Meaning |
| --- | --- |
| `agentCommand` | the headless agent, as a word list with `{prompt}` and `{turns}`; or an object keyed by role (`default`, `scout`, `adjudicator`, `implementer`, `verifier`) |
| `check` / `checkForce` | the command that is "done"; cannot be empty, cannot contain `"` or a newline |
| `failNameSed` | one `sed -n` expression that extracts a failing test's name from the check's output |
| `spec`, `rowsHeading`, `rowsEndHeading` | where the exit-criteria table lives |
| `contextFile`, `pointerFiles` | the agent context file and the one-line pointers other tools read |
| `harnessDir`, `skillsDir` | where the launcher and the skill are installed |
| `driverCommand` | a script that exercises the built artifact; empty means off |
| `rateLimitPattern`, `costSed` | how the launcher recognises a rate-limit notice and reads a cost |
| `allowedPrefixes`, `docs`, `harnessAllow`, `machinery` | the `litter` probe's allowlists |
| `sourceRoot`, `testFileSuffixRe`, `testDeclPatterns`, `sourceExt` | how tests and source are recognised |
| `skills` | skills the role prompts may name, each with the gate that enforces it |
| `learningsCap` | the size `LEARNINGS.md` may not exceed |

## Files

| Path | What |
| --- | --- |
| `.harness/loop.sh` | the launcher |
| `.harness/lib/{queue,agent,gates}.sh` | the queue surface, the agent spawner and run log, the gates |
| `.harness/tasks.py` | the queue parser |
| `.harness/hooks/` | `probes.sh`, `check-gate.sh`, `immutable.sh`, `one-writer.sh`, `verify-done.sh` |
| `.harness/roles/` | the five role prompts |
| `.harness/archive-done.sh` | moves done blocks to `DECISIONS.md`, rolls `PROGRESS.md` |
| `.harness/worktree.sh` | one lane in its own git worktree under `.harness/worktrees/`, fast-forwarded back |
| `.harness/watch.sh` | a status board for a running loop |
| `.harness/RAILS.md` | the rails, each naming its enforcement |
| `TASKS.md`, `PROGRESS.md`, `LEARNINGS.md`, `DECISIONS.md`, `.check-baseline` | the queue, the append-only record, the rules, the archive, the inherited red |
| `AGENTS.md` + pointers | the context file every role reads first |
| `evals/run.sh` | the write-path gate for a new `LEARNINGS.md` rule |

## Evals

`evals/run.sh [name...]` builds a fixture repo per eval, runs the configured agent once with the
eval's prompt, and runs its `assert.sh`. `--gate <name>` runs the eval with the rule, again with
the rule ablated, and every other eval; a rule is accepted only when its eval fails without it and
nothing else regresses. An agent that does not run, or a fixture that cannot be built, is `ERROR`
and never a pass; a directory with no evals is refused. Three evals ship with this package
(`scout`, `adjudicator`, `verifier`); `install.sh` installs the runner only.

## Adapters

`--adapter claude` installs the role prompts as `.claude/agents/`, the hook scripts, and the
`PreToolUse` / `UserPromptSubmit` / `Stop` hook wiring plus a deny list into
`.claude/settings.json`. `--adapter bun-turbo` is this author's own check script for a
bun+turbo monorepo and expects that layout.

## Testing this package

```bash
./selftest.sh                          # the floor: every gate, probe and installer path, in a throwaway install
HARNESS_DRIVER=1 ./selftest.sh         # plus driver.sh reaching the installed artifact
HARNESS_EVALS=1 ./selftest.sh          # plus the evals against a real agent (needs a credential)
./docs/demo.sh                         # one task ready -> done in a repo it creates and deletes
./docs/bootstrap.sh --check            # the record of this package having built itself, derived from git
```

CI runs the floor on `ubuntu-latest` and `macos-latest` with shellcheck at full severity and
shfmt; a `continue-on-error` or an `if:` on either job fails the selftest that reads the workflow.

## Not included

Parallel lanes (`worktree.sh` isolates one). A held-out test suite. A driver for your artifact
(`driver.sh` is the worked example for this package). `test-hashes.json` (`hash-uncovered` reports
its absence until you write it). Evals for the implementer and researcher roles.

## License

Apache-2.0. See `LICENSE` and `NOTICE`.
