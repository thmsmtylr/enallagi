# Setting up a repository

This guide takes a repository with a test suite from install to a merged pull request.
The example is a node repository whose tests run under `node --import tsx --test`.

## 1. Install

Install the binary as the [README](../README.md#install) shows.
Runtime needs `git`, `sh` and your agent CLI.

## 2. Run `enallagi init`

The repository's `package.json` names its test script:

```json
  "test": "node --import tsx --test src/*.test.ts"
```

Run init at the repository root:

```bash
enallagi init
```

One pass does everything a lane needs, and `--issue <url>` queues the issue the loop will work:

```bash
enallagi init --issue https://github.com/owner/repo/issues/12
```

Init detects the test runner from the files the repository carries.
Each detected key is printed with the `file:line` that decided it.
Four of the six lines read:

```text
  detected: check.command = "npm test" (package.json:4)
  detected: check.fail_name = '^✖ (.+?) \([0-9.]+m?s\)$' (package.json:4)
  detected: layout.test_file_suffix_re = '\.test\.[cm]?[jt]sx?$' (package.json:4)
  detected: layout.source_root = "src" (src/date.ts:1)
```

A tree that matches no runner, or more than one, is asked four questions instead: `agent.preset`,
`check.command`, `check.fail_name` and `layout.source_root`, each defaulting to what detection
found. `--check`, `--fail-name`, `--source-root` and `--preset` answer a question without asking
it, and `--yes` takes every default. A stdin that is not a terminal never asks.

```bash
enallagi init --yes
```
The seven runners and their `fail_name` patterns are in [configuration.md](configuration.md#checkfail_name).

Init then writes `.enallagi/enallagi.toml` and renders the role prompts, `RAILS.md`, the loop's
skill and the Commands section of `AGENTS.md` from the answers, in the same pass. It writes the
hook and instruction files for the configured agent, `--adapter` overriding the preset, vendors
the declared skills when stdin is a terminal (`--sync` forces it, `--frozen` forbids it), commits
`.enallagi/` in its own repository, appends the issue as a `proposed` block, and prints what
`enallagi probe` reports. `PROBE install-stale 0` is the state it leaves.

The block stays `proposed` until the first run: the adjudicator writes `scope:` and `criteria:`
and sets it `ready` before anything else happens. To queue one by hand instead, append a block to
`.enallagi/TASKS.md`:

```markdown
## [T-001] the date parser drops a timezone
scope: src/date.ts, src/date.test.ts
blockedBy:
status: ready
rows: none — harness
criteria:
  - `npm test` passes with a case for `+10:00` that fails on the current parser
notes:
```

- `scope:` lists every file the task may touch, comma-separated.
- Each criterion names a command whose output fails until the work is done.
- `rows:` is a token the queue reads, never prose. Write `none — harness` or a spec row's exact name.
- A spec row name carries no comma, since `rows:` splits on commas.

`.enallagi/` is its own git repository, excluded from yours, and the verdict gate fails a task
while a file there carries an uncommitted edit. Init commits what it wrote; a block you append is
yours to commit, with `git -C .enallagi commit -am "queue T-001"`.

Edit `.enallagi/enallagi.toml` later and the rendered copies name the old values; re-run
`enallagi init` and it re-renders them, or `enallagi probe install-stale` names each stale file
and `enallagi run` refuses to start.

## 3. Run

```bash
enallagi run --pr-per-task --iterations 1
```

The adjudicator decides the proposed block, the implementer commits and sets it to `review`, the
verifier returns `done` or a rejection, and a landed task is pushed to its own branch and opened
as a pull request. `pr` branches off the upstream default branch, carries only the commits naming
the task, and reads the target's contribution guide before it pushes. The pull request is merged
by a person, never by the binary.

```bash
enallagi tasks list
enallagi skills list
enallagi probe install-stale
```

## 4. Read a rejection

A rejected task goes back to `ready` with the verifier's reasons in `notes:`.

```bash
enallagi tasks rejections
enallagi events --task T-001
```

`tasks rejections` prints every open rejection with its reasons, and `tasks block T-001` one block.
`events --task` prints each stage and gate the task went through.
Answer each point in the criteria or the scope, commit, and run again.
