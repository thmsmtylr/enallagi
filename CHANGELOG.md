# CHANGELOG

One line per user-visible change, newest first. `## Unreleased` is what is on main and not yet
tagged. The release procedure is stated in `.github/workflows/release.yml`.

## Unreleased

## v0.3.0-beta.4 (2026-09-28)

- `enallagi pr` builds its branch in a worktree under the harness directory, as a lane does, so a check that finds its tools in a parent directory, as npm finds `node_modules`, runs there
- `check.tally` names the counts in a runner's summary with the regex groups `passed` and `failed`; the agent's check proposal writes one and init verifies it, and a runner with no count records no tally instead of a tally of zero
- `enallagi init` and the `check-unnamed` finding name `enallagi init --propose-check` when no runner is detected
- `title-length` leaves an imported issue's title alone while the block is `proposed`; the adjudicator writes the task's title at promotion
- the adjudicator runs an issue's `gh issue view` alone and without asking for network, since a lane already runs it outside the sandbox
- a `claude` lane loads no MCP server: `--strict-mcp-config` keeps the operator's own out
- `branch-protection` is silent on a GitHub repository the operator cannot administer
- the verifier may run a mutation a criterion names: an `Edit`, the command alone, an `Edit` back, and a clean `git diff --quiet`; the implementer makes one the same way, never with `sed -i` or a chained command a lane cannot run unattended
- the context file, rails, roles and loop skill name the baseline at `.enallagi/.check-baseline`, where it lives, instead of a bare `.check-baseline`
- the context file, and the implementer and verifier roles an existing install re-renders, say the tool's result carries the check's exit status, so a lane never chains `echo $?` onto it, which the sandbox holds for approval

## v0.3.0-beta.3 (2026-09-27)

- the README and docs describe the agent's check proposal, the `claude` preset's lane sandbox, the committed `.gitignore` line, and the issue block's placeholders

## v0.3.0-beta.2 (2026-09-26)

- an imported issue's scope reads `<written by the adjudicator at promotion>` instead of a made-up path, and `enallagi run` and `enallagi worktree` refuse a `ready` block still carrying either import placeholder
- the `scope` gate counts widening from a block's promotion, not from its proposal, so the scope the adjudicator writes is not a widening
- the context file tells a lane to run the check alone in its shell call, the one form a sandboxed lane runs outside its sandbox
- a Claude lane runs sandboxed with edits accepted instead of stopping at every permission prompt: the check runs outside the sandbox, commits to the product and state repositories are allowed, and writes outside the lane, network and the deny list stay blocked
- `enallagi init` commits `/.enallagi/` to the product's `.gitignore`, since linters read it and never `info/exclude`; `enallagi eject` commits its removal
- an imported issue is a `probe: issue` block with a `command:` and `output:`, which the adjudicator promotes instead of killing as unanchored
- the check's output has its ANSI escape sequences removed before `fail_name` reads it
- `enallagi init` ends on `Next: set check.command ...` when no check is set, instead of pointing at `enallagi run`
- `enallagi worktree` commits uncommitted edits in the harness directory's own repository before branching, instead of refusing; uncommitted product work is still refused
- `enallagi init` asks the configured agent for the check when no runner preset matches, and writes it only after running it green, then red naming a planted failing test; a refused proposal is sent back once, and `--propose-check` asks when stdin is not a terminal

## v0.3.0-beta.1 (2026-09-26)

- beta: the first release that installs with one command (`install.sh`), turns an issue into a task (`enallagi issue`, `init --issue`), runs a named pipeline (`run --pipeline`), and measures itself (`events --summary`, `tasks landed`); the lines under v0.2.1 to v0.2.6 are what it adds since v0.2.0

## v0.2.6 (2026-09-26)

- `enallagi tasks landed` prints each done task with its commits and one of `no branch`, `built <branch>` or `pushed <url>`, and `--built` lists only the branches waiting on a push
- `enallagi pr` writes a `pushed:` line into its description, and `--push` on an already built branch pushes it as it stands and updates that line

## v0.2.5 (2026-09-26)

- `enallagi events --summary` prints one row per task (stages, verify rounds, done verdicts, rejections, overturns, seconds, cost and the four token lanes) and a footer with totals and the false-completion rate, overturned done verdicts over all done verdicts; `--json` prints the same as one object per task

## v0.2.4 (2026-09-26)

- every event in `events.jsonl` carries `sha`, the product HEAD it was written at, and `run.start` carries `binary`, the crate version and the commit the binary was built from
- `enallagi run` prints the binary's version and commit when it starts, and `binary predates HEAD` when HEAD has moved past the commit it was built from

## v0.2.3 (2026-09-26)

- `curl -fsSL https://raw.githubusercontent.com/thmsmtylr/enallagi/main/install.sh | sh` installs the release binary for the machine, checked against `SHA256SUMS`; `install.ps1` runs it inside WSL
- a release is created as a draft and published only after every asset is attached

## v0.2.2 (2026-09-26)

- the `harness-immutable` rail in RAILS.md and SPEC.md names only `.enallagi/enallagi.toml`, the one file `init` hashes, where it also named a build config, preloads and a check script that nothing hashed

## v0.2.1 (2026-09-26)

- every `cargo test` step in CI runs with `--no-fail-fast`, so a failing test binary no longer hides the failures in the binaries after it, and a floor test fails a step without the flag
- the repository-identity test passes on a machine with a global git identity, where it failed on every developer machine and passed only in CI

## v0.2.0 (2026-09-25)

- a new crate version on main is the release: `release.yml` tags it and builds that tag in the same run, so cutting one is merging the pull request that bumps `crates/harness/Cargo.toml` and moves these lines
- `friction-repeat` counts a `## Earned rules` line in DECISIONS.md as cover, so a friction the adjudicator closed stops being reported
- block titles are 72 characters or fewer, and `probe title-length` reports one past it from `queue.title_cap_from` on
- `enallagi review` queues one block per open thread, folds the same finding raised twice on the same lines into one, and a landed block replies on its thread with the sha and resolves it
- a `task/` branch main already carries makes no `gh pr list` call
- the commit-verdict gate reads a `deferred:` line before prose, and a quoted line, code span, path or test name never trips it
- a task at `review` keeps its lane branch when origin moves
- the branch-protection probe gives the host tool ten seconds, then reports `unknown`
- `enallagi pr` commits its own record and leaves operator edits in the state repository alone
- a merge git refused is reported in git's words, not as an aborted conflict
- a `STOP` directory ends no rate-limit wait; only a `STOP` file does
- `turns-exhausted` reports only a stage whose preset was handed the turn cap
- `queue-uncovered` reports a criterion whose cargo filter cannot select the test it names
- `litter` reads the state repository's tracked-and-ignored paths too
- `enallagi init` is one pass: it asks for the four keys detection cannot decide (`--yes`, `--check`, `--fail-name`, `--source-root`, `--preset` answer without asking), renders every copy from the answers, writes the configured preset's adapter, vendors skills at a terminal (`--sync`, `--frozen`), commits the state, takes `--issue <url>`, and ends with a probe; it seeds no placeholder task and no placeholder spec row
- a tree no runner preset matches still gets `layout.source_root` and `layout.test_glob` from its `src/` and `tests/`
- `enallagi eject` keeps the harness directory under `$XDG_DATA_HOME/enallagi/ejected` unless `--delete` is passed
- a `done` the scope gate refuses returns to `ready` with the reason on the block, and a second refusal in one run halts it
- `triage` spends one dry round on an undecided block, then the round goes to `discover`
- the cold-start eval names the probes that reported `OFF` and fails when every probe did
- `enallagi worktree` refuses a lane while the parent checkout has uncommitted work, naming the repository and the porcelain lines
- a task block's `model:` and `effort:` reach the lane that implements it, and `run --dry-run` prints both on every stage line
- the crate, binary, config file and environment variables are named `enallagi`; the `harness` names are read as a fallback
- the scope gate exempts a `Cargo.lock` beside an in-scope `Cargo.toml`, and a file the iteration's range adds under a locked skill id
- `enallagi watch` attaches to the lane whose loop is alive, from the checkout the operator stands in
- a claude lane runs only the plugins the harness pins: the preset's `--settings` names every plugin the operator enables false
- `probe plain-record` reports a commit subject, a task note or a printed line that comments instead of recording
- `enallagi pr T-###` builds a branch off the upstream default branch from the commits naming the task, runs the check there, and pushes and opens the pull request under `--push`
- `enallagi eject` removes the harness directory, the untracked entry points init wrote and the exclude block, and refuses while a lane worktree or a live loop exists
- `enallagi base T-###` prints the product commit a task was queued against
- `enallagi worktree` runs a lane in its own worktree of the product and the harness directory, and fast-forwards both branches or neither
- the harness directory is its own git repository, committed once per stage, and the gates read it at its own base
- `enallagi init --move` relocates a root or `.harness` install, and init hides what it writes in one `git info/exclude` block rather than `.gitignore`
- the harness directory defaults to `.enallagi`, with `.harness` read as a legacy fallback
- the claude adapter installs as a plugin under the harness directory, loaded with `--plugin-dir`, its skills named `harness:<id>`

## v0.1.0 (2026-09-15)

- the first release: `init`, `run`, `watch`, `probe`, `gate`, `hook`, `skills`, `tasks`, `eval`, `events` and `worktree`, and four static binaries with a `SHA256SUMS` on a tag push
