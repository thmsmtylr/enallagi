# CHANGELOG

One line per user-visible change, newest first. `## Unreleased` is what is on main and not yet
tagged. The release procedure is stated in `.github/workflows/release.yml`.

## Unreleased

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
