# Reference

The tables the README links. Each names the source file that decides it.

## Pipelines

`enallagi run` takes the first pipeline whose `when` holds, in the order `enallagi.toml` lists them.
The defaults are in `crates/harness/harness.default.toml`.

| Pipeline | `when` | Stages |
| --- | --- | --- |
| `review` | `queue.reviewing` | **verify** |
| `decompose` | `spec.undecomposed` | **decompose** → **adjudicate** |
| `task` | `queue.takeable` | **implement** → **verify** → **adjudicate** |
| `triage` | `queue.proposed` | **adjudicate** |
| `discover` | `!queue.takeable` | **scout** → **adjudicate** |

A queue holding a `needs-triage` ticket and nothing takeable takes `triage`, so a ticket the operator
queued is decided before the scout files more.
An accepted spec with no tickets takes `decompose`: the decomposer writes `to-tickets` drafts beside
it, the `tickets` gate numbers them into `needs-triage` tickets and commits them, and the adjudicator
triages them in the same round. With several such specs, the lowest slug goes first.
`discover` ends the run after two dry rounds.
`triage` has one: a round that leaves its `needs-triage` ticket undecided hands the next round to
`discover`, and a round that leaves takeable work reopens it.

When the launcher opens a ticket's pull request, it sets the ticket `open` and writes `pr:` (the URL) and `branch:` (its head) under `## Harness`.
`done` then means merged: no lane takes an `open` ticket, and a blocker at `open` already lets its dependent go, since `enallagi pr` stacks on the blocker's branch.
Before selecting a task, an iteration polls each `open` ticket's pull request with one light read (state, head sha, newest comment and review, check rollup).
A merged pull request sets `done`, and a closed one sets `wontfix` with gate `pull request closed unmerged`.
The full review read runs only when that key differs from the ticket's `polled:`, which then takes the new key.
A finding inside the ticket's `scope:`, or with no anchor, goes back to that ticket as a `finding:` entry with its body quoted; the ticket returns to `ready-for-agent`, `rounds:` goes up by one, and its criteria are never touched.
A finding outside the scope becomes a `needs-triage` ticket, and the launcher replies on its thread naming that ticket and resolves it.
A returned ticket, a `ready-for-agent` one carrying `pr:`, is taken before any other.
Its round runs in a worktree started from `origin/<branch>` after a fetch, with that head as the gate base, and the checkout's branch never carries it.
After verify passes, the launcher fast-forward pushes the round's commits to `<branch>`, writes `pushed:` (sha and time) and sets the ticket `open`.
A range holding a state commit, or a remote head that moved, refuses the push: the ticket goes back to `open` with the reason under `gate:`.
The implementer answers each `finding:` with one `answer:` line, `accept <test>` or `reject <concept>: <command>`, and the `findings` gate forces back a `done` that leaves one unanswered.
After the push, each reject is appended to `.out-of-scope/<concept>.md` in Pocock's format; a concept file that exists gains only a `## Prior requests` line.
The launcher then replies once per finding answered since the last push, marked `<!-- enallagi:F-<n> -->`: a thread reply on a thread (an accept names the fixing sha and the test, a reject the concept and the command), and one comment listing the other findings by URL. Each reply's URL is written as `settled: F-<n> <url>`.
Each push also asks every `[respond] rereview` reviewer again with `gh pr edit <pr> --add-reviewer <r>`; a failure is a warning.
Replies, comments, resolves and re-review requests run with `GH_TOKEN` set to the value of the variable `[respond] token_env` names, when it is set.
The poll judges nothing on a ticket whose `pushed:` time is less than `[respond] quiet` ago. When no pipeline's `when` holds and a pushed ticket is inside its window, the launcher sleeps until the earliest window closes and iterates.
Once judged, the poll re-reads each answered thread and resolves only the ones still unresolved, so a thread a bot resolved itself is never resolved again.
A new finding with the same author, path and words as one the ticket rejected is not routed: the launcher replies with the rejection's URL, resolves its thread, writes `settled: F-<n> <url> for <new url>` and starts no round.
The loop stops on the first rule that holds: new in-scope findings on a ticket that has had `[respond] max_rounds` rounds set it `ready-for-human` with gate `respond: max_rounds (<n>) reached, <k> findings open` and `respond: max_rounds`; no new finding, a `SUCCESS` check rollup and every thread resolved write `respond: clean at <sha>` and leave it `open` for a person, or `merge = "auto"`, to merge.
Under `[pr] merge = "auto"`, after the poll, each iteration judges every `open` ticket's pull request with `gh pr view --json state,baseRefName,headRefName,headRefOid,statusCheckRollup` and the review read, and leaves it open with one `pull requests:` digest line, `T-NNN: left open, <reason>`, on the first that holds:
- `no holdout set`: the ticket's spec has no holdout set;
- `<head> is not a task/ branch`;
- `stacked on <base>`: the base is not the remote's default branch;
- `head moved`: `headRefOid` is not the tip of the head on `origin`;
- `no checks reported`: the rollup is empty;
- `checks: <name> <state>`: a check is not completed, or completed other than `SUCCESS`, `NEUTRAL` or `SKIPPED`;
- `<n> unresolved thread(s)`: any thread is unresolved, outdated ones too;
- `respond: not clean at <sha>`: the ticket's `respond:` is not `clean at` the head, so a clean written before the last push counts for nothing;
- `holdouts: <k> of <n> red`: the spec's holdouts run on the fetched head; the ticket goes back to `ready-for-agent`, and a second red in one run halts it;
- `merge refused: <gh's first line>`: `gh pr merge <url> --merge --match-head-commit <sha> --delete-branch` failed, also a warning.
Otherwise the pull merges, the ticket goes `done` with gate `merged <url>`, and the digest line reads `T-NNN: merged <url>`.
`merge = "human"`, the default, never calls `gh pr merge`.
A finding is already carried, and not routed again, when any ticket (`wontfix` included) holds its URL or its key, or an `.out-of-scope/` file lists its URL under `## Prior requests`.
`enallagi review <pr>` reads a pull request no ticket owns, and every finding there lands as one `needs-triage` ticket: an unresolved thread (carrying its thread id), a review body, a plain comment, or a failed check on the head.
The same finding raised twice on the same lines is one ticket.
A bot's summary comment is keyed by its marker, so one it edits in place is one finding; it is no finding while that bot has an unresolved thread, or when it counts zero findings.
A comment carrying `<!-- enallagi:` is the loop's own and never a finding.
The bot table in `crates/harness/src/review.rs` is the only vendor-specific code: it maps a `Bot` account's login, `[bot]` dropped, to its vendor, and never drops a finding for its author.
When a ticket filed from a thread lands, the launcher replies on the thread with the sha and resolves it.
No host tool or a failed host call is a warning, never a halt.

`enallagi audit` reads review findings, frictions and rejections, and writes proposed learnings to DECISIONS.md: a model names each class, and code checks each citation.
`enallagi run` runs it once when a round ends, after its last iteration.
It never accepts what it proposes.
A learning that overlaps a standing entry adds its instances to that entry.

The adjudicator decides each proposed learning.
It promotes one under `## Earned rules` only with three or more instances and a passing `enallagi eval --gate`.
Otherwise it writes a dated `killed: YYYY-MM-DD` line under the entry, which keeps its class from being proposed again.
A learning undecided for `queue.proposed_rounds` rounds moves to `## Expired findings` as a dated line.
The run digest counts and names the learnings proposed, promoted, killed and expired, and the rules removed.

The audit stamps each earned rule with a `promoted:` line naming the commit that added it.
An instance merged into the rule whose line that commit did not hold makes the rule ineffective.
A line appended again word for word counts as a new one.
The next audit hands an ineffective rule back to the auditor, which proposes a revision like any learning.
The rule gains a dated `revised:` line, and a promoted revision replaces it with an `adopted:` line for it and for each adoption before.
A rule that recurs after two adopted revisions moves to `## Expired findings` as a dated line.
The audit moves each citation into `DECISIONS.md` with the line it names when it edits the file.

`enallagi audit --harness <dir>...` reads `DECISIONS.md` in each install directory given.
A directory named twice, by any path, counts as one install.
It reports each rule that two or more installs earned under `## Earned rules`.
A rule killed in any install is left out.
Each report names every install's dated line and prints the rule as a `[seed]` line.
It only reports: it writes to no install and to no file in this repository.
A person adds the line to `templates/LEARNINGS.md` in a pull request.

A role file's frontmatter may declare two keys, which the sandbox enforces on that role's spawns (`crates/harness/src/roles.rs`).
`writes:` lists paths relative to the lane's checkout, comma-separated, a directory covering its subpath; an absolute entry, a `..`, or one that resolves outside the checkout is dropped.
Present, it replaces the checkout and both repositories' git dirs as the writable set; the preset's state paths and the spawn's `TMPDIR` stay writable.
Present and empty, only those two are writable. Absent, the writable set is unchanged.
`hosts:` lists proxy allowlist entries added for that role's spawns; `*` matches any name, and the proxy still refuses IP literals and names resolving to loopback, link-local or unspecified addresses.
Checks and gates keep the run's policy.
A role file carrying `tools:` is refused at load: it reached the model as prose and enforced nothing.
The shipped `auditor` declares `writes:` empty; the `researcher` declares `writes: REFERENCES.md` and `hosts: "*"`.

`enallagi events --prefix` prints one row per role stage in `events.jsonl`.
Each row carries the stage's three input token lanes.
`handed_bytes` prints `-`: no event records the size of the prompt a stage was given.
`repaid` is input plus cache creation, which every stage pays in full.
`cached` is the cache read, which a warm cache re-reads at a discount.
A stage that logged no usage prints `-`.
The first line names the window the log covers, and no figure is scaled past it.
`--json` prints one object per row, then one `total` object per role.

`enallagi skills --cost` prints one row per declared skill.
Each row carries its vendored bytes and two stage counts.
`loaded` counts the role stages whose role file names the skill.
`used` counts the stages whose transcript shows a `Skill` tool call for it.
A skill that no stage loading it ever used is named as unproven in this log.
`--since <ts>` and `--until <ts>` count only stages that started at or after `since` and before `until`.
`--cost` takes no subcommand and no `--frozen`.

### Watching a run

`enallagi watch` attaches read-only to a live loop, and `enallagi run` draws the same view.
The output pane opens on the event feed.
It shows one line per stage and gate event, as `enallagi events` prints it, and one line per agent tool call.
Each run in the log is a lane: the stages and output panes show the focused lane, newest by default, and the queue pane names the lane branch of each claimed ticket.

| Key | What |
| --- | --- |
| `Tab` | cycle focus between the queue, stages and output panes |
| `↑` `↓`, mouse wheel | scroll the focused pane by one line, or three for the wheel |
| `PgUp` `PgDn` | scroll the focused pane by its height |
| `Home` `End` | the top of the focused pane, and back to following its tail |
| `r` | switch the output pane between the event feed and the agent's raw output |
| `l` | focus the next lane |
| `?` | toggle the key list |
| `q` | write `STOP` under `run`, and leave under `watch` or once a run has finished |

### Parallel lanes

Under `[parallel] writers = N` above 1, `enallagi worktree` runs up to N writer lanes, each pinned to one ticket with `run --task`.
A lane takes its ticket through a claim in `.enallagi/run/claims.json`, under a file lock: the lowest-id takeable ticket no live claim holds and whose `scope:` overlaps no live writer's.
A ticket with no `scope:`, or a glob with no literal prefix, overlaps every other.
A claim is live while its lane's worktree exists.
A reader lane (`run --task <id> --pipeline review`) takes a reader claim on a `review` ticket no claim holds; it blocks no writer and does not count toward N.
When nothing can be claimed and no lane is live, one unpinned lane runs as under one writer.
Each lane's `BUDGET_USD` and `BUDGET_TOKENS` are what is left divided by N.
Lanes land one at a time under `.enallagi/run/land.lock`, in the order they finish.
A lane whose parent branch moved is rebased onto it and its check run again.
A ticket the lane filed whose id another lane landed first is renamed to the next free id, in its file and in the lane's own ticket.
Every landing emits an `integrate` gate event.
A writer lane's implementer prompt lists the commits other lanes landed since its base, each once.

## Gates

`[[stage]].post` names the gates run after a stage. A failed gate sends the ticket back to `ready-for-agent`
or halts the run. The names are matched in `crates/harness/src/gates.rs`.

| Gate | Refuses | Runs after |
| --- | --- | --- |
| `implementer-not-done` | a `done` from anyone but the verifier, a `review` over a standing `rejected` verdict, or an implementer that stopped short of `review` | implement |
| `commit-verdict` | a verdict whose `deferred:` line or new prose defers a finding and adds no `needs-triage` ticket, or a failed commit of the queue | verify |
| `verdict` | a `done` without `verdict: verified by verifier` or without its `spec:` and `standards:` lines, a rejection without `verdict: rejected by verifier: <reason>`, or a `done` whose work is uncommitted or whose check is red on delta | verify |
| `claims` | a `done` whose `ran:`/`exit:` pairs under the `verdict:` line claim an exit that two re-runs both contradict, for commands `[check] claims_allow` names | verify |
| `findings` | a `done` on a ticket carrying `finding:` entries where a finding has no `answer:` or more than one, an `accept <test>` names a test no test file the round changed holds, or a `reject <concept>: <command>` names no command or a command with no `ran:` line under the verdict | verify |
| `skip-added` | a `done` whose commits add a `[layout] skip_patterns` match to a test file, named by `file:line` | verify |
| `security` | a `done` whose commits carry a secret gitleaks finds (run with `--redact`) that `security/gitleaks-baseline.json` does not hold; the ticket goes to `ready-for-human` naming `RuleID file:line commit` per finding, and the run halts. When the commits change a lockfile or manifest, osv-scanner scans it at HEAD and at the base (with `osv-scanner.toml` from the harness directory when present); an advisory at HEAD that the base lacks for the same package sends the ticket back to `ready-for-agent`, naming package, version and id. A missing pin or baseline, a failed fetch or an exit other than 0 or 1 also halts | verify |
| `red-first` | a `done` whose `[layout] test_only_glob` files pass the check with every other changed file at the iteration base, run in a temporary worktree | verify |
| `scope` | a file outside the task's `scope:` globs, or a product task editing the harness | verify |
| `queue-intact` | a ticket id at the iteration's base commit whose file is gone; a kill is `wontfix`, never a deletion | implement, verify, adjudicate |
| `commit-identity` | a commit in the iteration's range whose author is not the repository's configured `user.email` | implement, verify |
| `commit-task` | a commit in the iteration's range whose subject names no task id, unless it touches only the loop's own state | implement |
| `check-delta` | a check failure that is not already in `.check-baseline` | wherever a stage's `post` names it |
| `commit-round` | a failed commit of the queue and DECISIONS.md | adjudicate |
| `tickets` | a spec's drafts that fail `enallagi tasks check <spec>`: halts, drafts left in place; on a pass numbers them `T-NNN` from the next id, maps their blockers, sets `needs-triage` with `probe: decompose`, and commits them (`enallagi gate tickets <spec>` by hand) | decompose |
| `adjudicator-halt` | an adjudicator output line opening with `halt` and naming a task id, and halts the run | adjudicate |
| `dry-round` | a round that leaves no `ready-for-agent` ticket, counted toward `end_after_dry_rounds` | adjudicate |
| `install-stale` | an installed file that drifted from what `enallagi init` writes now, read by spawning the binary | verify |
| `integrate` | under `[parallel] writers` above 1, a lane whose rebase onto the parent branch conflicts, whose check is red after the rebase, or whose state merge conflicts; the ticket goes back to `ready-for-agent` with a reason naming the paths or tests, what landed first and `git show <tip>`, and the lane branch is kept | landing |
| `holdout` | a `done` whose spec's holdouts are red at HEAD; the ticket goes back to `ready-for-agent` with `holdouts: <k> of <n> red`, and a second red for one ticket in a run halts | verify |

Holdouts are tests the operator writes and no lane may read (ADR 0006).
A spec's set is the executable regular files in `.enallagi/holdouts/<spec>/`, where `<spec>` is the ticket's directory under `specs/`; other files there are data the holdouts read.
Each holdout runs in name order, in a scratch worktree of the commit under test under `.enallagi/worktrees/`, with `ENALLAGI_HOLDOUTS` set to the set's absolute path and `[check] timeout` as its bound.
Exit 0 passes it and anything else is red.
Its output goes to `~/.cache/enallagi/holdouts/<repo>/<ticket>/<holdout>.log` and nowhere else, so the queue never names a holdout.
After every lane of every stage, whatever its `post` list, the launcher looks for `holdouts/<spec>` of any spec with a set in the stage's transcript and in the product and state diffs since the iteration's bases.
A match sets the ticket to `ready-for-human` and halts the run with `<stage>: a lane named a holdout path; the set may be burned`, naming no path.

## Probes

`enallagi probe` prints one `PROBE <name> <count>` line per probe and a `FINDING` line per shortfall.
A probe that cannot run prints `PROBE <name> ERROR`, never a count of zero.
The queue probes print it when there is no `specs/` directory to read (`no queue: no specs/`).
The list is `registry()` in `crates/harness/src/probes/mod.rs`.

| Probe | Reports |
| --- | --- |
| `spec-untested` | an exit-criteria row whose test does not exist |
| `queue-uncovered` | a spec row no task claims, or a claimed row the spec lacks |
| `rail-unenforced` | a rail whose enforcement does not exist, or exists and nothing runs |
| `hash-uncovered` | a file a rail names with no key in `test-hashes.json` |
| `check-unnamed` | a context file whose Commands section omits the configured check |
| `learning-unenforced` | a LEARNINGS.md entry naming no file, command or hook |
| `learning-ungated` | a dated rule naming no eval, or a library past its cap |
| `learning-standing` | a proposed learning still undecided, with its age and the rounds left before it expires |
| `skill-ungated` | a `[[skill]]` whose `gate` is `none` |
| `ponytail-ceiling` | a `ponytail:` marker no kill line names |
| `rejection-stale` | a ticket whose `verdict:` line reads `rejected` and whose status is not `ready-for-agent` or `ready-for-human`, or a ticket at `needs-info` |
| `title-length` | a ticket title past 72 characters, from `queue.title_cap_from` on, except an imported issue still `needs-triage`; the id and the count |
| `queue-hygiene` | a repeated id, a missing status, an undefined blocker, or a scope entry matching nothing |
| `friction-repeat` | a friction recorded twice that no LEARNINGS.md rule, `## Earned rules` line or kill line covers, and an earned rule whose class recurred after its `promoted:` commit |
| `check-red` | a check that exits non-zero, with its first failing test |
| `litter` | a tracked path the repository treats as disposable, or an untracked path on no allowlist |
| `plain-record` | a commit subject, note or printed line that comments instead of recording |
| `install-stale` | an installed file that differs from what `enallagi init` writes now |
| `contribution-policy` | a guide sentence that refuses or conditions generated changes |
| `upstream-drift` | the checkout's branch and its upstream each carrying commits the other lacks |
| `branch-protection` | what this checkout and a host tool can establish about the default branch |
| `review-merged` | a block at `status: review` whose id a product commit on the remote's default branch names |
| `context-behind` | a Commands line the context template renders that the installed context file lacks |
| `verdict-flip` | a task flipped from `done` to `ready` more than once in one run |
| `rejection-repeat` | one rejection reason repeated across tasks |
| `stage-outlier` | a stage over twice its role's median time or cost |
| `turns-exhausted` | a stage that used its whole turn cap |
| `limit-repeat` | a rate limit hit in consecutive stages |
| `driver` | a shortfall the built artifact reports when `layout.driver_command` runs it |

`litter` reads a tracked path as disposable when `layout.machinery` names a part of it.
The repository's own ignore rules covering a path git still tracks reads the same way.
`layout.strict_prefixes = true` widens the tracked arm to every path outside `layout.allowed_prefixes`.

`branch-protection` is advisory, and silent on a GitHub repository whose `permissions.admin` reads `false`.
Its host-free leg counts the non-merge first-parent commits on the remote's default branch.
Its host leg quotes what a host tool answered, or `unknown` when that tool exited non-zero.

## Configuration

`.enallagi/enallagi.toml` deep-merges over `crates/harness/harness.default.toml`.
Tables merge key by key and arrays merge whole.

Unknown keys are refused per table.
The fields are declared in `crates/harness/src/config.rs`.

Every table and key, with its default, is in [configuration.md](configuration.md).

`[pr] per_task = true` hands the operator one pushed branch per landed task.
The checkout's own branch then follows its own tracking ref and carries nothing else.
With `per_task = false` a lane fast-forwards the checkout's branch and pushes nothing.

That split needs the harness documents in a repository of their own.
When they share the product repository, `by_branch` is false even under `per_task = true`.
Lane commits then fast-forward into the checkout, exactly as they do under `per_task = false`.

A task's own `model:` and `effort:` lines win over `[agent]` and `[agent.<role>]`.

`when` takes any of these, each negated by a leading `!`:

- `queue.takeable`
- `queue.reviewing`
- `queue.proposed` (a `needs-triage` ticket stands)
- `queue.empty`
- `task.attended` (the first ready ticket is `ready-for-human`)
- `spec.undecomposed` (a `specs/<slug>/spec.md` at `**Status:** ready-for-agent` has no `T-NNN-*.md` in `issues/`)
- `check.red`
- `probe.<name>`

A skill or role `source` is `github:owner/repo`, `git+file://` or `path:`.

## Files

`enallagi init` writes these, relative to the repo root, with `.enallagi` as `layout.harness_dir`.
The plan is built in `crates/harness/src/init.rs`.

`enallagi eject` removes them and moves the harness directory, the run's record, to
`$XDG_DATA_HOME/enallagi/ejected/<repository>-<stamp>` (`~/.local/share/...` when unset);
`--keep-record <path>` names the place instead, and `--delete` is the one way to remove it.

Init commits one line to the product's `.gitignore`, and eject commits its removal.
Those two commits are what an install leaves in the product's history.

| Path | What |
| --- | --- |
| `.enallagi/roles/{decomposer,scout,adjudicator,implementer,verifier,researcher}.md` | the six role prompts, always resubstituted |
| `.enallagi/RAILS.md`, `.enallagi/.gitignore` | the rails, each naming its enforcement, and the ignore file for scratch state |
| `<skills_dir>/running-the-loop/` | the loop's own usage skill, in the preset's skill directory |
| `.enallagi/{enallagi.toml,PROGRESS.md,LEARNINGS.md,DECISIONS.md,.check-baseline,evals/README.md}` | seeded once and never overwritten |
| `.enallagi/specs/<repo>/spec.md` | the spec, seeded once; its tickets go in `issues/` beside it, one file each |
| `.enallagi/AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `QWEN.md`, `.github/copilot-instructions.md` | seeded once: the context file, and one-line pointers to it |
| `.gitignore` | `/.enallagi/` appended and committed, so the product's linters skip the harness directory |

## Twins

The GitHub twin stands in for `gh` in tests and the demo (ADR 0010).
`enallagi twin gh <args>` is a hidden subcommand that serves one call from the JSON state file `ENALLAGI_TWIN` names.
`fixture::twin::install(dir, bin, state)` writes the state as `dir/twin.json` and a two-line `gh` shim that execs the binary.
It returns the `PATH` to use and the state's path; `twin::load` reads the state back.

The state is one object, declared in `crates/harness/src/fixture/twin.rs`:

- `repo` (`owner/name`), `origin` (the bare remote's path) and `default_branch`.
- `pulls`: `number`, `url`, `title`, `body`, `head`, `base`, `head_sha`, `state` (`OPEN`, `MERGED` or `CLOSED`), `round`, `checks` (in `statusCheckRollup`'s shape), `reviews` and `comments` (`author`, `typename`, `body`, `url`), and `threads` (`id`, `resolved`, `outdated`, and `comments` with `author`, `typename`, `path`, `line`, `start_line`, `body` and `url`).
- `script`: `{head, round, checks, bots, resolve}` entries; `head` is a branch or `*`. When a pull reaches the round, its checks are replaced, each of `bots` (`{bot, path, line, text}`) is posted, and each thread id in `resolve` is resolved, as a bot resolving its own thread.
- `responses`: `{args, stdout, stderr, exit, sleep}`, matched on exact argv before any served call. `sleep` hangs the call that many seconds in a child `sleep`, whose pid goes in `sleepers`.
- `calls`: every argv the twin was handed, in order, and `tokens`: each call's `GH_TOKEN`, empty when unset.

On every call the twin reads each open pull's head from `origin`.
A moved head is a push: `head_sha` follows it, `round` goes up by one, and that round's script applies.
`pr create` applies round 0.

Served calls:

- `pr create` (`--head`, `--base`, `--title`, `--body` or `--body-file`), `pr list --json` (`--state`), `pr view <url|number|branch> --json`, and `pr comment <pull> --body`.
- `pr merge <pull> --merge --match-head-commit <sha> [--delete-branch]`: refuses a merged or closed pull, a sha other than `head_sha`, or a conflict, in gh's words; otherwise writes a two-parent merge commit onto `origin`'s base, sets `MERGED`, and under `--delete-branch` deletes the head ref and retargets open pulls based on it to that base.
- `--json` takes `number`, `url`, `title`, `body`, `state`, `headRefName`, `baseRefName`, `headRefOid`, `statusCheckRollup`, `reviews` and `comments`.
- `api graphql` dispatched on the document's operation name: `ReviewThreads` (the review read: head sha, comments, reviews and threads with their authors, and the head's check rollup), `PullState` (the poll's light read, its check rollup derived from `checks`), `ReplyThread` (a thread reply, answering its URL) and `ResolveThread`.

Any other call, or an operation no handler names, exits 1 with `twin: no response for gh <args>`.

### Review-bot twins

A scripted finding is rendered through its bot's recording in `crates/harness/src/fixture/bots.json`.
Each finding becomes a review thread in the bot's shape; one with an empty `path` is a row of the bot's summary only.
A bot whose marker sits in an issue comment also rewrites that one comment in place, listing the round's findings.

| Bot | Login | Marker | Where the marker sits | Source | Date |
| --- | --- | --- | --- | --- | --- |
| Kilo | `kilo-code-bot` | `<!-- kilo-review -->` | issue comment | thmsmtylr/enallagi#94, recorded | 2026-11-07 |
| Greptile | `greptile-apps` | `<!-- greptile_summary -->` | issue comment | thmsmtylr/enallagi#92, recorded | 2026-11-07 |
| Bugbot | `cursor` | `<!-- BUGBOT_BUG_ID -->` | review thread | marker table, unrecorded | 2026-11-06 |
| CodeRabbit | `coderabbitai` | `<!-- cr-comment:v1 -->` | review thread | marker table, unrecorded | 2026-11-06 |
| Claude | `claude` | `<!-- bhfn:1:... -->` | review thread | marker table, unrecorded | 2026-11-06 |

An unrecorded shape carries `"recorded": false`.
Its login is confirmed, 2026-11-07, as a `Bot` from public pull requests: `cursor[bot]` (CleanExpo/RestoreAssist#2510), `coderabbitai[bot]` (red-hat-data-services/RHOAI-Build-Config#38425, whose summary carries `<!-- This is an auto-generated comment: summarize by coderabbit.ai -->`) and `claude[bot]` (BrendenJones1/Bayesian-Study-Planner#145, a review body).

To refresh a recording, read the bot's newest comments on a real pull request:

```sh
gh api repos/<owner>/<repo>/pulls/<n>/comments --jq '.[] | select(.user.type == "Bot")'
gh api repos/<owner>/<repo>/issues/<n>/comments --jq '.[] | select(.user.type == "Bot")'
```

Then replace the bot's `thread`, `summary` and `row` with the new text, put `{path}`, `{line}`, `{text}`, `{url}`, `{count}` and `{rows}` where the finding's values were, and set `source`, `date` and `recorded`.
