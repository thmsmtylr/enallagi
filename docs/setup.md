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

Init seeds `.enallagi/` and writes `.enallagi/enallagi.toml`.
It detects the test runner from the files the repository carries.
Each detected key is printed with the `file:line` that decided it.
Four of the six lines read:

```text
  detected: check.command = "npm test" (package.json:4)
  detected: check.fail_name = '^✖ (.+?) \([0-9.]+m?s\)$' (package.json:4)
  detected: layout.test_file_suffix_re = '\.test\.[cm]?[jt]sx?$' (package.json:4)
  detected: layout.source_root = "src" (src/date.ts:1)
```

A tree that matches no runner, or more than one, keeps the defaults.
Init then names the candidates it found.
The seven runners and their `fail_name` patterns are in [configuration.md](configuration.md#checkfail_name).

## 3. Set the keys detection cannot

Open `.enallagi/enallagi.toml` and set these by hand:

- `agent.preset`, when your agent CLI is not `claude`.
- `check.command`, when detection found no runner or more than one.
- `check.fail_name`, for the same reason.
- `layout.source_root`, when the code does not live under `src`.

Every key and its default is in [configuration.md](configuration.md).

## 4. Re-run `enallagi init` after every config edit

Init renders the role prompts, `RAILS.md` and the loop's skill with the configured check.
An edit to `enallagi.toml` leaves those copies naming the old values.

```bash
enallagi init
enallagi probe install-stale
```

`enallagi probe install-stale` prints `PROBE install-stale 0` once every copy matches.
Skipped, it names each stale file, and `enallagi run` refuses to start.

Init keeps `.enallagi/AGENTS.md` as you left it.
After a check change, edit its Commands section to name the new check.
The `check-unnamed` probe reports it until you do.

## 5. Pick the adapter

`--adapter` writes the hook and instruction files for one agent CLI:

```bash
enallagi init --adapter claude
```

The presets are listed in `enallagi init --help`.

## 6. Vendor the skills

```bash
enallagi skills sync
```

Sync fetches every `[[skill]]` at its pinned `rev` and records it in `.enallagi/harness.lock`.
Replacing or removing a skill is in [pipeline.md](pipeline.md#skills).

## 7. Queue a task

Turn an issue into a `proposed` block, or edit the `T-001` placeholder init seeded:

```bash
enallagi issue owner/repo#12
```

The block stays `proposed` until you fill `scope:` and `criteria:` and set `status: ready`.

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

## 8. Commit every instance edit

`.enallagi/` is its own git repository, excluded from yours.
The verdict gate fails a task while a file there carries an uncommitted edit.

```bash
git -C .enallagi add enallagi.toml TASKS.md SPEC.md AGENTS.md
git -C .enallagi commit -m "queue T-001"
```

## 9. Run one task

```bash
enallagi run --pipeline task --iterations 1
```

The implementer commits and sets the task to `review`.
The verifier returns `done` or a rejection.

## 10. Read a rejection

A rejected task goes back to `ready` with the verifier's reasons in `notes:`.

```bash
enallagi tasks block T-001
enallagi events --task T-001
```

`tasks block` prints the block with its notes.
`events --task` prints each stage and gate the task went through.
Answer each point in the criteria or the scope, commit, and run again.

## 11. Open the pull request

```bash
enallagi pr T-001 --push
```

`pr` branches off the upstream default branch and carries only the commits naming the task.
It reads the target's contribution guide before it pushes.
The pull request is merged by a person, never by the binary.
