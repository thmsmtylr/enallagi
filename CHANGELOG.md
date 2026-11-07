# CHANGELOG

One line per user-visible change, newest first. `## Unreleased` is what is on main and not yet
tagged. The release procedure is stated in `.github/workflows/release.yml`.

## Unreleased

- a role's `writes:` entry that is absolute, climbs out with `..` or resolves outside the checkout is dropped, so it never widens what the lane may write
- on Linux the unfiltered sandbox bridge marks itself undumpable, so its lane cannot ptrace it or read its `/proc` entries
- the launcher's `status`, `add`, `diff`, `commit` and `stash` refuse a gitlink to a repository inside the checkout, whose `core.fsmonitor` or filter a lane could set to run outside the sandbox; a lane cannot write `.git/modules/`
- a confined lane inherits only an allowlist of the launcher's env (`PATH`, `HOME`, locale, `XDG_*`, `ENALLAGI_*`, the agent keys) plus the names in the new `[sandbox] env`; `GH_TOKEN`, `GITHUB_TOKEN`, `[respond] token_env` and every other credential stay with the launcher; `harness.default.toml` gains the `[sandbox]` table
- the security gate reads its scanner pins from the binary, never from `harness.lock`, which a lane can write; `[[tool]]` entries are ignored and init no longer writes them
- `docs/demo.sh` runs one spec from decompose to a dark merge against the GitHub twin, with Kilo and Greptile scripted; `merge = "auto"` counts an accepted spec not yet decomposed as open work; holdout sets resolve through the main worktree, so a respond lane's `holdout` gate runs them
- under `[pr] merge = "auto"` each iteration merges an `open` ticket's pull request when its spec has holdouts, its base is the default branch, its head is the origin tip, every check passed, no thread is unresolved, `respond: clean at` names the head and the holdouts pass there; otherwise the digest names why it was left open; the GitHub twin serves `pr merge`
- `queue.proposed_rounds` counts loop iterations, not state commits: each loop state commit carries an `Iteration: <run>/<n>` trailer, so a respond round no longer ages a filed finding by several rounds
- `enallagi watch` keeps one stage and output view per run: `l` focuses the next lane (newest by default), and the queue pane names the lane branch of each claimed ticket
- under `writers > 1` the launcher also starts a reader lane (`run --task <id> --pipeline review`) per `review` ticket no live claim holds, outside the `writers` count; a pinned verifier verifies only its ticket
- a writer lane's implementer prompt lists the commits other lanes landed on the parent branch since its base (short sha, subject, files), each once per run
- under `writers > 1` a ticket a lane filed whose id another lane landed first is renamed to the next free id at landing, and the old id is rewritten in it and in the lane's own ticket
- the GitHub twin serves `ReplyThread` and `ResolveThread` in place of `SettleThread`, records each call's `GH_TOKEN` under `tokens`, resolves the thread ids a script round's `resolve` names, and posts a scripted finding with no `path` as a summary row only
- a new finding with the same author, path and words as one the ticket rejected is not routed: the launcher replies with the rejection's URL, resolves its thread, records `settled: F-<n> <url> for <new url>` and starts no round
- the respond loop stops: inside `[respond] quiet` (default `10m`) after a push the poll judges nothing, and the launcher sleeps to the window's end when nothing else can run; new in-scope findings after `[respond] max_rounds` (default `3`) set `ready-for-human` and `respond: max_rounds`; no new finding, green checks and every thread resolved write `respond: clean at <sha>`; each push asks every `[respond] rereview` reviewer again
- after a respond push the launcher replies once per answered finding, marked `<!-- enallagi:F-<n> -->` (a thread reply, or one comment listing the rest by URL), writes each reply's URL as `settled:`, and at the next poll resolves only the answered threads still unresolved; `[respond] token_env` (default `ENALLAGI_GH_TOKEN`) names the variable passed to `gh` as `GH_TOKEN` for these writes
- a returned ticket's implementer writes one `answer: F-<n> accept <test>` or `answer: F-<n> reject <concept>: <command>` per finding and the verifier re-runs each; a new `findings` gate after `claims` forces back a `done` with a finding unanswered or answered twice, an accept no changed test file holds, or a reject with no command or no `ran:` line; after the push each reject is appended to `.out-of-scope/<concept>.md`
- a `ready-for-agent` ticket carrying `pr:` is taken first, and its round runs in a worktree on `origin/<branch>`; after verify the launcher fast-forward pushes it to that branch, writes `pushed:` and sets the ticket `open`, refusing a range with a state commit or a moved remote head; the checkout's branch never moves
- the poll routes each new finding on an `open` ticket's pull request: inside the ticket's `scope:` or unanchored, it is appended as a `finding:` entry and the ticket returns to `ready-for-agent` with `rounds:` bumped; outside it, it becomes a `needs-triage` ticket and its thread is answered with that id and resolved; a URL or key any ticket holds (`wontfix` included), or an `.out-of-scope/` `## Prior requests` line, is never routed again
- the review read (`enallagi review` and the poll) takes review bodies, plain comments and the head's failed checks beside threads, each one finding with its author; a bot table maps Kilo, Greptile, Copilot, Cursor Bugbot, CodeRabbit and Claude logins to vendors, keys each summary by its marker so an in-place edit is one finding, and drops a summary while its bot has an open thread or counts zero; the loop's own `<!-- enallagi:` replies are never findings
- under `[pr] per_task` an opened pull request sets its ticket `open` with `pr:` and `branch:`; each iteration polls every `open` ticket with one light read, sets `done` on merge and `wontfix` on close, and re-reads reviews only when the head, newest comment or review, or check rollup changed (`polled:`); a blocker at `open` frees its dependent; pull requests are no longer found through local `task/` branches
- `enallagi pr` and `tasks landed` skip a state commit (a subject ending ` at <sha>`) that names the ticket, so an in-tree install's queue commits never reach a pull request
- under `[parallel] writers = N` (N > 1), `enallagi worktree` runs up to N lanes at once, each pinned to a ticket its claim holds (`run/claims.json`, no two overlapping scopes), with the budget left split across the slots, and falls back to one unpinned lane when nothing can be claimed
- under `writers > 1` lanes land one at a time: a lane whose parent moved is rebased and its check re-run, and a conflict or a red check sends its ticket back to `ready-for-agent` with an `integrate` gate event
- `enallagi run --task T-NNN` takes only that ticket, ends once it is neither takeable nor at review, skips the scout, adjudicator and decomposer stages, and names the ticket in the implementer prompt
- `init` writes `.enallagi/.gitattributes` with `PROGRESS.md merge=union`, so two lanes appending progress entries merge without conflict
- `[parallel] writers`, default `1`; `0` is refused at load
- `enallagi watch` scrolls the focused pane by `PgUp`/`PgDn`, `Home`/`End` and the mouse wheel, opens the output pane on the event feed (one line per stage and gate event and per tool call), and `r` shows the raw stream
- `STOP` is never committed to the state repository (it joins the harness `.gitignore`, and every state commit drops it), `init` untracks one already tracked, and a `STOP` in the main checkout's harness directory halts every lane before its next stage
- `[pr] merge = "human" | "auto"`, default `human`; `auto` opens a pull request per ticket and refuses to start without a holdout set for open work, a USD or token budget, untracked holdouts and the sandbox on
- a lane whose transcript or diff names a holdout set's path sets its ticket to `ready-for-human` and halts the run, after every stage whatever its `post` list
- a new `holdout` gate, last on verify, runs the executable files in `.enallagi/holdouts/<spec>/` in a scratch worktree and sends a `done` back with only `holdouts: <k> of <n> red`; a second red for one ticket halts the run
- a role's frontmatter declares `writes:` (checkout-relative paths, replacing the checkout and git dirs as its writable set) and `hosts:` (proxy entries for its spawns, `*` for any name), enforced by the sandbox; `tools:` is gone from the shipped roles and refused at load; the auditor writes nothing and the researcher writes only `REFERENCES.md` and reaches any host
- each runner preset gains `hosts` (its registries) and `write` (its caches), and init writes the detected runner's into `[sandbox] hosts` and `[sandbox] write`
- a red check's output ends with one `sandbox refused <host>:<port> (<reason>)` line per proxy refusal during that check, so the gate's tail names it
- the adjudicator no longer re-runs a `probe: issue` ticket's `gh issue view` (a lane has no GitHub credentials); it judges the state `enallagi issue` recorded in `output:`
- a stage's cost and tokens are read from its last `"type":"result"` line when it has one, so claude 2.1.300's trailing `session_state_changed` line no longer halts a run under `BUDGET_USD`
- under `[sandbox] mode = "native"` no lane or check reads or writes `<harness_dir>/holdouts/` (its own instance's and the parent's) or `~/.cache/enallagi/holdouts/`, a renamed parent included, and no lane reads `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.netrc` or `~/.config/gh`; init's state-repository `.gitignore` gains `holdouts/`
- `[sandbox] mode` defaults to `"native"` and every lane carries its preset's bypass flag; `agent.dangerously_skip_permissions` and `run --dangerously-skip-permissions` are gone (the key is refused at load, naming `[sandbox] mode`), and so are the claude preset's own sandbox settings, its allow list and the context file's run-the-check-alone line
- on Linux, `[sandbox] mode = "native"` runs every lane and check under `bwrap` (a `bwrap` on PATH outside the product) with no network but its own loopback: the proxy port and `[sandbox] ports` are bridged out over Unix sockets under `~/.cache/enallagi/run/`, writes follow the macOS rules as bind mounts, and a seccomp filter fails `socket(AF_UNIX)` and `io_uring_setup`; a run refuses to start without bwrap or user namespaces, naming the fix
- on macOS, `[sandbox] mode = "native"` runs every lane and check under `sandbox-exec` with a deny-default profile (Codex's base, Apache-2.0): reads anywhere, writes only to the lane's checkout, both repositories' git dirs, a per-spawn `TMPDIR`, the preset's new `write` paths and `[sandbox] write`; connects only to the proxy and `[sandbox] ports`; no repository's `config`, `hooks/` or worktree pointers writable; a run already inside a sandbox wraps nothing and records `nested`; `run.start` records `seatbelt`; a run refuses to start when the sandbox is unavailable
- under `[sandbox] mode = "native"` each run starts a loopback CONNECT proxy whose allowlist is GitHub's API and release hosts, the configured presets' new `hosts` and `[sandbox] hosts` (`name` or `*.suffix`, optional `:port`, 443 by default); it refuses other methods, IP literals, unlisted hosts and names resolving to loopback, link-local or unspecified addresses with `403`, each logged as `sandbox.denied`; every spawn gets `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY` in both cases
- new `[sandbox]` table (`mode` = `"off"` | `"native"`, `hosts`, `ports`, `write`), `off` by default and any other mode refused at load; `run.start` records `sandbox` in place of `permissions_skipped`, and `enallagi events` prints `sandbox=<mode>`
- every command run from inside a nested instance repo (`run`, `probe`, `skills`, `audit`, `eval`, `eject`, `init`, `hook`, `worktree`) reads the product around it, as `tasks check` and `tasks archive` do
- `enallagi skills --cost` takes `--since <ts>` and `--until <ts>`, counting only role stages that started in that window
- skill `running-the-loop` drops its role table and points to the roles directory; its ticket reference names every `## Harness` key, `verdict:`, `spec:`, `standards:`, `ran:`, `exit:` and `deferred:` included
- `enallagi eval --gate`: an ablated role is copied over the adapter's agent file too, so the ablated agent never reads the full role
- new eval `decomposer`: an accepted spec's drafts pass `tasks check` with `spec.md` unchanged; no ablation, the role is `to-tickets` plus `tasks check`
- new pipeline `decompose` (`when = "spec.undecomposed"`, after `review`): a `spec.md` at `**Status:** ready-for-agent` with no tickets is broken into drafts by the new `decomposer` role, numbered and committed by the `tickets` gate, and triaged by the adjudicator in the same round; `tickets` now commits the round
- new gate `tickets` (`enallagi gate tickets <spec>` by hand): a spec's drafts that pass `tasks check` become `T-NNN-<slug>.md` tickets at `needs-triage` with `probe: decompose` and mapped blockers; a failing check halts and leaves the drafts; a ticket file's bare `- [ ]` lines read as its criteria
- new `enallagi tasks check <spec>`: checks a spec's `NN-<slug>.md` drafts (shape, no `## Harness` of their own, blockers, no cycle, a free start, every exit-criterion test named, nothing else changed, `spec.md` untouched), one line per problem, exit 2 on any; from inside a nested instance repo it reads the product around it; the queue skips drafts
- `enallagi eval --gate verifier-findings` builds its ablated arm again; a test fails when any eval's `ablate.sh` anchor misses its role
- the adjudicator triages with Pocock's `triage`: a kill records its line in `.out-of-scope/<concept>.md` instead of DECISIONS.md `## Rejected findings`, which stays read-only history the probes still honour; a `probe: decompose` ticket is judged against its spec; a ticket needing a person goes to `ready-for-human`
- new eval `verifier-filing`: the verifier files a defect outside the criteria as one `needs-triage` ticket; its ablation removes only that rule
- the verifier reviews along `code-review`'s Spec and Standards axes and runs `ponytail-review`; the `verdict` gate sends a `done` back to `ready-for-agent` unless `spec:` and `standards:` lines sit under its verdict; a new command that writes files or calls a model is verified in a copy of the harness directory
- the implementer loads `codebase-design` by its own token, `writing-for-agents` and `domain-modeling` when its scope names a doc, skill, role, glossary or ADR, and updates the doc stating a changed behaviour in the same commit
- the probes read exit-criteria rows from every `specs/*/spec.md`, each finding citing its own spec; a one-ticket stage's `__SPEC__` is that ticket's `spec.md`; the default rows heading is `### Exit criteria`, ending at `## Out of Scope`
- a skill whose frontmatter holds `disable-model-invocation: true` renders inline in the role prompt for every preset, under `## Skill: <id>` naming its vendored directory
- the lane prompt describes the tracker (specs layout, global ids, states, `## Comments`, `## Harness`), and a new install's context file says a lane has no user to answer
- the default skills are thirteen, each id its upstream directory name: Pocock's `to-tickets`, `triage`, `tdd`, `diagnosing-bugs`, `codebase-design`, `writing-for-agents`, `domain-modeling` and `code-review` at v1.3.2, ponytail and `ponytail-review` at v5.3.3; `debugging`, `review-received` and `verify-before-done` are renamed, `brainstorming` and `review-requested` are gone
- the `security` gate runs osv-scanner on a lockfile or manifest a `done` task changed, at HEAD and at base, and sends the ticket back to `ready-for-agent` for an advisory the base did not carry
- `enallagi init` records a full-history secrets baseline in `security/gitleaks-baseline.json` once, which the `security` gate passes to gitleaks; a fetch failure at init is a warning
- a new `security` gate runs gitleaks with `--redact` over a `done` task's commits; a leak sets the ticket to `ready-for-human` and halts the run. Init seeds `gitleaks.toml`
- `harness.lock` pins gitleaks and osv-scanner per platform in `[[tool]]` entries, written by init; a pinned tool is fetched once into `~/.cache/enallagi/tools/` and checked against its sha256
- `claims` re-runs only the `ran:`/`exit:` pairs under the `verdict:` line, and a PR body's Verification section prints that line and its pairs
- `rejection-stale` and the implementer's retry prompt read the `verdict:` line; REJECTED prose in comments is no longer a verdict
- the verifier and implementer write one `verdict:` line in the ticket's `## Harness`; the `verdict` gate sends back to `review` a `done` not `verified by verifier` and a rejection with no reason, and `implementer-not-done` refuses a `review` over a standing rejection
- `enallagi eval` puts its own binary first on the agent's `PATH`, sets `ENALLAGI_TASK` to the fixture's ready ticket, and reads `EVAL_AGENT` with shell quoting
- the `immutable` hook lets a lane edit a hashed file its own ticket's `scope:` line names, as the scope gate does; `test-hashes.json` and sessions outside a lane stay refused
- a new `skip-added` gate forces a `done` back when its commits add a skip with no stated reason; init writes `layout.skip_patterns` for the detected runner
- a new `red-first` gate forces a `done` back when its test-only files pass with the rest of the change reverted; init writes `layout.test_only_glob` for the detected runner
- `queue-intact` halts on any ticket file deleted since the base, whatever DECISIONS.md holds; a kill is `wontfix`
- a `claims_allow` entry matches whole command words only, so `cargo test` no longer admits `cargo testx`
- skill clones are cached under `~/.cache/enallagi/` (or `$XDG_CACHE_HOME/enallagi/`); the old `~/.cache/harness/` is no longer read
- `commit-task`, `commit-identity` and `scope` fail with git's error when the commit range cannot be read, instead of passing an empty one
- the role prompts work in ticket files and the new state names, and the adjudicator kills a ticket by setting it `wontfix` instead of deleting it
- TASKS.md, the root layout and `HARNESS_*` support are removed: the queue is ticket files under `specs/`, `init` seeds `specs/<repo>/spec.md`, and `tasks migrate`, `tasks unblock` and `init --move` are gone
- once a queue is migrated, `run` and `watch` show ticket states by their new names, `init` seeds no TASKS.md or SPEC.md beside it, and the spec probes read the criteria table from its `spec.md`; a queue probe with no queue at all prints ERROR again
- the probes read ticket files once a queue is migrated, and a finding cites the ticket's own file
- once a queue is migrated, `pr`, `audit`, `issue` and `review` read and write ticket files, a done ticket stays in its file, an untriaged one expires to `wontfix`, and a lane may add a ticket only at `needs-triage`
- the gates read ticket files once a queue is migrated, and the scope gate lets a lane edit only its own ticket
- `tasks list`, `ready`, `set-status` and `unblock`, and the loop's own pick, read and write ticket files once a queue is migrated
- `tasks migrate` converts TASKS.md and SPEC.md into one file per ticket under `specs/`, in one commit
- an adopted rule is retested, and one that stops working is revised or removed
- `audit --harness` prints, as seed lines, the rules two installs earned
- a proposed learning is decided by the adjudicator, expires after `queue.proposed_rounds` rounds, and `learning-standing` reports it until then
- the claims gate re-runs the commands a verdict's `ran:` lines name
- text probes skip the state repository's files
- a release is cut by hand: `bump.yml` is removed, and `release.yml` runs only on a pushed `v*` tag

## v0.3.0-beta.10 (2026-10-10)

- commit-task refuses a commit naming no task

## v0.3.0-beta.9 (2026-10-10)

- write the version bump onto each pull request

## v0.3.0-beta.8 (2026-10-10)

- the implementer role invokes each skill as a tool call at the step that uses it, and invokes `debugging` when a task reports a failure; before, lanes followed the restated steps and loaded `tdd` in 2 of 62 stages and `debugging` in none

## v0.3.0-beta.7 (2026-10-09)

- a probe `review-merged` reports a block still at `status: review` whose id a product commit on the remote's default branch names; the loop's own state commits do not count
- a probe `context-behind` reports a Commands line the context template renders that the installed context file lacks
- `enallagi pr` builds a task whose commits conflict with the default branch on the newest `task/<id>` branch that changed a conflicting file and takes the commits, and names that task in the description; a task branch deleted on the remote is no longer a candidate
- `enallagi skills --cost` prints each skill's vendored bytes and the role stages that loaded and used it, and names a skill its loading stages never used
- `enallagi events --prefix` prints each role stage's repaid and cached prefix tokens, and `--json` prints them as objects

## v0.3.0-beta.6 (2026-10-09)

- the verifier reproduces each test, command and outside fact a task's notes claim before it returns a verdict, and rejects one it could not reproduce
- `enallagi audit` has an auditor role name each recurring class of finding; the binary checks every citation it gives and merges classes that overlap. The auditor's prompt drops the role file's frontmatter, and a failed auditor writes nothing
- `enallagi init` rewrites the config key in an existing `test-hashes.json`, so a config edit is re-hashed
- with a detected runner other than `cargo` and `check.tally` unset, `enallagi init` asks the agent for `check.tally` on a terminal or under `--propose-check`, and writes it only once the check counts some passed and none failed
- `docs/demo.sh` and `install.sh` create their temporary directories under `${TMPDIR:-/tmp}`, so both run inside a lane's sandbox

## v0.3.0-beta.5 (2026-09-29)

- under `[pr] per_task`, a lane resumes an earlier lane branch only while one of its commits names a task not yet `done`, and a resumed lane merges in what the checkout's HEAD gained since, so a round never runs on a tree older than its checkout

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
