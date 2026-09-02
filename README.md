# harness

An autonomous goal loop for coding agents. Not a framework — a directory of shell scripts, agent
prompts and document templates you copy into a repo. Nothing to install, no dependency, no runtime.
`bash` and `python3`, both of which you have.

It runs one task per fresh session, gates every claim against a command someone else can run, and
keeps the whole decision trail in files so the next session can start cold and continue.

## What it is made of

Four parts, and the fourth is the one people skip.

**The floor.** The check command is yours — the harness never defines it. What the harness adds is a gate
that verifies on **delta** against `.check-baseline` rather than on absolute zero, because a tree
whose exit criteria are not all covered yet cannot reach zero failures, and a gate whose passing
state is unreachable is not strict, it is broken. `.check-baseline` only ever shrinks.

**The roles.** Five agent prompts with separated authority, and the separation is the whole point:

| Role | Does | May never |
| --- | --- | --- |
| `scout` | turns `FINDING` lines from `probes.sh` into `status: proposed` blocks | have a finding of its own, promote, or fix |
| `adjudicator` | promotes a proposal to `ready` with runnable criteria, or kills it with the command that refutes it | write a proposal, or edit a file a block names |
| `implementer` | one task, inside its `scope:` globs, test first | mark anything `done` |
| `verifier` | fresh session, adversarial, promotes to `done` or rejects with reproducible reasons | fix code |
| `researcher` | attaches a source to a decision already made | find anything, or amend a governing document |

An agent that finds its own work and then grades it is not a loop, it is one agent with extra steps.

**The probes.** `.harness/hooks/probes.sh` is twelve analyses over the tree. It reports and never
gates. Its `FINDING` lines are the only legal input to the queue (`anchored`): the scout transcribes
them, the adjudicator re-runs the command and kills what does not reproduce.

Eleven of them read text — test declarations, the exit-criteria table, `git ls-files`, `TASKS.md`
fields, `PROGRESS.md` entries. The twelfth is `driver`, and it is the only one that exercises the
built artifact through the surface a user touches. It is **off** until you set `driverCommand` in
`harness.json` and `HARNESS_DRIVER=1` in the environment, and while it is off it prints
`PROBE driver OFF` rather than a count of zero — because a probe that did not run has found nothing,
which is not the same as a clean tree. Your driver exits 0 whenever it reached the artifact, whatever
it found, and prints one line per shortfall beginning `FINDING `; a non-zero exit is
`PROBE driver ERROR`, and nothing is proposed from a probe that could not run. It gets a throwaway
working directory and a stripped environment, so if the thing you drive is itself an agent it does
not inherit this loop's context. Watch the persistent effect, not the answer — diff the store, the
file, the row your artifact was supposed to change.

**The evals.** The role prompts are the one part of this that nothing else can test: `selftest.sh`
tests the launcher *around* them, so a change to `roles/verifier.md` is otherwise unverifiable
except by watching a run. `evals/run.sh` fixes that — one eval per directory, each three files
(`setup.sh` builds a fixture repo with the harness freshly installed, `prompt.txt` is the request in
the shape `loop.sh` sends it, `assert.sh` exits 0 when the role obeyed its rule), each in a throwaway
repo with a fresh agent process. Three ship, one per queue-gating role: the scout must transcribe and
never promote, the adjudicator must kill an unanchored proposal unread, the verifier must reject a
task whose implementation was never committed. They need a real agent, so `./selftest.sh` asserts the
runner rather than spawning one — and with no agent configured `run.sh` refuses instead of reporting
a pass for something it never ran.

**The documents.** The repository is the control plane. Sessions end, context compresses, and the
next agent starts without the last one's reasoning:

| File | Holds |
| --- | --- |
| `AGENTS.md` | the context file every agent loads every session. Deliberately short |
| `SPEC.md` | the objective and the exit criteria, one row per behaviour, each named by its test |
| `.harness/RAILS.md` | the rails in full, each naming what enforces it. Loaded on demand |
| `TASKS.md` | the queue |
| `PROGRESS.md` | one entry per iteration, append-only, read by its tail |
| `LEARNINGS.md` | one line per mistake already paid for, each naming a file, command or hook |
| `DECISIONS.md` | killed findings at the top, archived task blocks below |
| `.check-baseline` | the inherited red, and it only shrinks |

## Install

```bash
git clone <this> ~/Documents/harness
~/Documents/harness/install.sh /path/to/your/repo --dry-run   # read the plan first
~/Documents/harness/install.sh /path/to/your/repo
```

It writes `.harness/`, `.claude/agents/`, `.claude/hooks/`, seeds the documents **only if absent**, and
wires the hooks into `.claude/settings.json` (printing the snippet instead if you already have one).
Re-run it any time — substitution is idempotent, and it is also how you upgrade.

Then:

```bash
cd /path/to/your/repo
$EDITOR harness.json               # check, spec, agentCommand
~/Documents/harness/install.sh .
git add .harness harness.json AGENTS.md
.harness/hooks/probes.sh           # what the tree says about itself
.harness/loop.sh 1                 # one iteration, attended, watch it work
```

`touch STOP` at the repo root stops the loop before its next stage. Delete it to resume.

## Which agent it drives

Any of them. `agentCommand` in `harness.json` is a word list with `{prompt}` and `{turns}` filled
in per stage:

| Agent | `agentCommand` |
| --- | --- |
| Claude Code | `["claude","-p","{prompt}","--dangerously-skip-permissions","--max-turns","{turns}"]` |
| Codex CLI | `["codex","exec","{prompt}","--sandbox","workspace-write"]` |
| Gemini CLI | `["gemini","-p","{prompt}","--yolo"]` |
| opencode | `["opencode","run","{prompt}"]` |
| Copilot CLI | `["copilot","-p","{prompt}"]` |
| Goose | `["goose","run","-t","{prompt}"]` |
| Aider | `["aider","--message","{prompt}","--yes"]` |

The launcher gets role isolation from **one fresh process per stage**, not from a vendor subagent
mechanism — subagents exist for Claude Code, Copilot and Cursor and do not exist for Codex or Gemini
([arXiv:2602.14690](https://arxiv.org/abs/2602.14690), Table 1). Each stage is pointed at
`.harness/roles/<role>.md` and reads it. `adapters/README.md` has the rest.

## Skills

The role prompts name skills — TDD, systematic debugging, code review, minimalism — and degrade
gracefully when one is missing. Skills are the **one extension mechanism that is genuinely
portable**: all five tools in the cross-tool study support them, and ~48 clients implement the
[agentskills.io](https://agentskills.io/) format.

They install per tool, not per repository. Superpowers, the largest skills framework, says it
plainly: *"Installation differs by harness. If you use more than one, install Superpowers separately
for each one."* So this package does not vendor anyone's skills. What it ships is the project's own
skill — `running-the-loop` — written to the same open format, installed into whichever
`skillsDir` you name (`.claude/skills`, `.codex/skills`, `.gemini/skills`, `.cursor/skills`,
`.github/skills`), so any skills-compatible agent discovers the task protocol on its own.

Set `skillInvocation` in `harness.json` to whatever your tool calls it.

### More than one model

`agentCommand` also accepts an object keyed by role. A role it does not name uses `default`.

```json
"agentCommand": {
  "default":  ["claude", "-p", "{prompt}", "--dangerously-skip-permissions", "--max-turns", "{turns}"],
  "verifier": ["codex", "exec", "{prompt}", "--sandbox", "workspace-write"]
}
```

Each stage is already a separate process, so a different command per role costs nothing extra. The
reason to use it is measurement: a verifier running the model that wrote the code reports agreement
between two samples of one model, not correctness. SpecBench measured 43-48pp visible-versus-held-out
gaps for a single agent grading its own work ([arXiv:2605.21384](https://arxiv.org/pdf/2605.21384)).
The scout and adjudicator are cheap and high-volume; a smaller model there and a larger one on
`verifier` is the other common split.

`DRY_RUN=1 .harness/loop.sh 1` prints the command each stage would spawn.

## Configuration

One file, `harness.json`, at your repo root. Every key becomes a `__SCREAMING_SNAKE__` token that
`install.sh` substitutes into the scripts and prompts, so **the installed harness is plain text with
no runtime config to read**. Lists render as literals, strings render raw.

Six keys matter on day one:

| Key | Default | |
| --- | --- | --- |
| `agentCommand` | `["claude","-p",…]` | the headless invocation of your coding agent |
| `check` | `bun run check` | the floor. Anything that exits non-zero on failure |
| `checkForce` | `bun run check -- --force` | the same, cache-defeating. If your check has no cache, repeat `check` |
| `failNameSed` | `s/.*(fail) //p` | how to extract one failure name per line from the output. This is what makes `.check-baseline` work |
| `spec` | `SPEC.md` | the contract |
| `driverCommand` | `""` (off) | the one probe that exercises the artifact instead of reading text. Also needs `HARNESS_DRIVER=1`. Name a script — `$HARNESS_ROOT/scripts/drive.sh` — because it runs from a throwaway directory |
| `rowsHeading` | `## 11. Exit criteria` | the heading the probes slice for exit-criteria rows, and `rowsEndHeading` terminates it |

The rest tune the `litter` and `spec-untested` probes to your layout: `sourceRoot`, `contractFile`,
`testFileSuffixRe`, `testDeclPatterns`, `sourceExt`, `harnessFiles`, `harnessGlobs`,
`allowedPrefixes`, `docs`, `harnessAllow`, `machinery`. Defaults in `harness.default.json`.

`install.sh` greps the installed files for a surviving `__TOKEN__` and fails if it finds one, then
`bash -n`s every script. A gate asserts what it executed.

## Why AGENTS.md and not CLAUDE.md

`AGENTS.md` is an open format read by 20+ agents and present in
[60,000+ repositories](https://agents.md/). In a study of 2,926 engineered repositories, CLAUDE.md
appeared in 45.4% and AGENTS.md in 40.6% — but AGENTS.md received by far the most **incoming
references** (368), and `CLAUDE.md → AGENTS.md` was the single most common pair, 311 times. The
authors' own recommendation is to keep AGENTS.md as the shared baseline and use "tool-specific files
as adapters that reference a shared core file"
([arXiv:2602.14690](https://arxiv.org/abs/2602.14690)). That is exactly what `install.sh` writes.

Claude Code does not read AGENTS.md natively — the feature request has 4,300+ upvotes and the answer
is "not planned" ([anthropics/claude-code#34235](https://github.com/anthropics/claude-code/issues/34235)) —
so the generated `CLAUDE.md` uses the documented `@AGENTS.md` import.

**The context file is short on purpose.** Across 138 real tasks and four agents, context files raised
inference cost by over 20%; LLM-generated ones cost about 3% of success rate and human-written ones
bought about 4% ([Gloaguen et al., arXiv:2602.11988](https://arxiv.org/abs/2602.11988)). Instructions
are demonstrably *followed* — a tool named in the file is used ~1.6 times per task versus under 0.01
when unnamed — so the risk is not that the file is ignored, it is that "unnecessary requirements from
context files make tasks harder." The full rails live in `.harness/RAILS.md` and the task protocol in
a skill, both loaded on demand. Only five rails and the commands are resident.

## Checking the harness itself

```bash
./selftest.sh          # installs into a throwaway repo and asserts 20 things about the result
KEEP=1 ./selftest.sh   # leave the scratch repo behind
```

It covers install and re-install, the launcher's queue resolution and `set_status`, the clarification
halt firing on a bare marker and *not* on the template's own backticked mention of it, every probe
running without erroring, and all five delta cases of the gate. Two of the bugs it was written to
catch were template edits that silently broke the probes' row parser.

**A fresh install reports three findings, and all three are correct.** `spec-untested` names the test
file the seeded exit-criteria row points at, which you have not written; `rail-unenforced` says
`test-hashes.json` does not exist, which it does not until you run your adapter's hash step; and
`ponytail-ceiling` reports the one deliberate shortcut marked in `immutable.sh`. Nothing is clean on
day one and the probes should not pretend otherwise.

## Adapters

The floor is language-specific and the harness does not pretend otherwise. `adapters/bun-turbo/`
ships the five-stage `check.ts` this harness grew up on, plus the turbo-aware `check-covered.sh`
that fails any changed file no executed build task covers. Install with `--adapter bun-turbo`.

Without an adapter you get the portable `check-gate.sh`: run the check, forgive only exact
`.check-baseline` matches, and **fail closed when it cannot name what failed**. What you lose is the
coverage assertion — that a changed file was actually reached by a task that ran. If your build tool
can report its plan, write an adapter; it is forty lines.

## What the research changed

Everything in this section is a change made because a cited source said so, not because it seemed
better. What the same research says is still missing is under **What is not here** above.

| Change | Grounded in |
| --- | --- |
| `AGENTS.md` is the core context file; `CLAUDE.md`, `GEMINI.md` and `copilot-instructions.md` are generated one-line pointers | 60,000+ repos ([agents.md](https://agents.md/)); 368 incoming references and the 311× `CLAUDE.md→AGENTS.md` pair across 2,926 repos, and the authors' own "adapters that reference a shared core file" recommendation ([arXiv:2602.14690](https://arxiv.org/abs/2602.14690)) |
| The context file is ~50 lines; rails moved to `.harness/RAILS.md`, the protocol to a skill | Context files cost >20% inference and ~3% success when auto-generated; "human-written context files should describe only minimal requirements" ([arXiv:2602.11988](https://arxiv.org/abs/2602.11988)) |
| Roles moved from `.claude/agents/` to `.harness/roles/`, fed to a fresh process | Subagents exist for Claude, Copilot and Cursor and **not** for Codex or Gemini ([arXiv:2602.14690](https://arxiv.org/abs/2602.14690), Table 1) |
| Hooks demoted to an adapter; every rail rests on the launcher's own gate | Hooks are per-tool and **Codex has none** (same table) |
| `agentCommand` is configurable | Seven agent CLIs expose different headless invocations; Archon ([23.3k★](https://github.com/coleam00/Archon)) parameterises the binary for the same reason |
| The project's own procedure ships as a `SKILL.md` in `skillsDir` | Skills are supported by all five studied tools and ~48 clients ([agentskills.io](https://agentskills.io/)); Superpowers ([280k★](https://github.com/obra/superpowers)): "Installation differs by harness" |
| `archive-done.sh` and `watch.sh` guard on the configured binary, not `claude -p` | A process guard that recognises one vendor never fires for anyone else — caught by this package's own selftest |

A caution worth keeping: of 601 skills studied, **83.3% bundled no resources at all** and only 5.7%
had a `scripts/` directory. "Configuration is currently used more as documentation than as
automation." An executable harness is the rare case, not the norm — which is a reason to keep it
small, not a reason to skip it.

## What is not here, deliberately

**A driver for *your* artifact.** The probe is here, `.harness/driver.example.sh` is the skeleton to
copy, and `driver.sh` in this package is a worked example — it installs the harness into four
throwaway repos, drives one request through each with a lane that is sloppy in exactly one way, and
reports which sloppiness the harness let through. What it cannot know is your artifact's surface.
Until you fill the skeleton in and set `driverCommand`, every count the loop reads came from a grep
over the repo, and capability shortfall is invisible to all of it.

**Parallel lanes.** `.harness/worktree.sh` gives one lane its own checkout — `git worktree add`, the
loop inside it, then `--ff-only` back, and if the parent moved it leaves the branch and says what a
human has to decide. That is the isolation; running several of them at once is not shipped. The
multi-worktree launcher in the repo this came from is 23KB, was unused for months, and its one
load-bearing function (`gate_verdict`) is in `loop.sh` here. Add lanes when you have file-disjoint
scopes to fight over, not before.

Worth being precise about, because the two get conflated: **fresh sessions isolate context, worktrees
isolate the checkout.** Every stage already runs as a new process that cannot see the last one's
conversation — that is what `verifier-not-implementer` buys — and all of them still write to the same
working tree, which is why `one checkout is one writer` is a rail rather than a mechanism.

**A held-out suite.** `verifier-not-implementer` gets you a fresh session and a separate process,
which is most of the value. A suite the implementer never sees is more, and it is yours to write.
Freeze your **exam** with your tests while you are there — the corpus, the fixtures and the scoring
rules belong in `test-hashes.json`, or a lane can retune what it is graded on.

**`test-hashes.json` itself.** `install.sh` does not write it, and a fresh install emits exactly one
`hash-uncovered` finding naming `loop.sh` until you do. The hashes are yours to cut: a rail claiming
coverage it does not have is worse than one saying so out loud.

**Evals for two of the five roles.** `evals/` covers the three that gate the queue. The implementer
and the researcher have none; each is a fixture and two files, and the pattern is in place.

## When not to use it

Most tickets do not need any of this. If complete tests describe the work, give an agent the tests
and let it finish. This earns its cost when the capability sequence is not known up front, when the
failures that matter sit outside the tests you already have, and when you intend to leave the thing
running.

Start with one exit-criteria row, one task, and `.harness/loop.sh 1`. Add a control after a failure
justifies it, not before.

## Provenance

Extracted from a working single-package repo where it ran for weeks. Every rail in the templates
carries the measured finding behind it, and every seed line in `LEARNINGS.md` is a mistake that was
actually paid for. It was then installed into its own repository and used to build its last three
capabilities through its own queue — the driver, worktree isolation and the role evals — which is
where the git history starts being the argument for it. That run found a task the loop should never
have been offered, a gate that let uncommitted work reach `done`, and two fresh instances of the
exact failure the whole harness is built around. None of the three came from reading the code.
