# CHANGELOG

One line per user-visible change, newest first. `## Unreleased` is what is on main and not yet
tagged. The release procedure is stated in `.github/workflows/release.yml`.

## Unreleased

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
