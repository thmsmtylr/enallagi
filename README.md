# harness

An autonomous goal loop for coding agents. Shell scripts, agent prompts and document templates that
install into a git repository. No dependencies beyond `bash` and `python3`.

One task per fresh agent process, every claim gated on a re-runnable command, all state in files.

## Install

```bash
git clone <this> ~/harness
~/harness/install.sh /path/to/repo --dry-run
~/harness/install.sh /path/to/repo
```

Then, in your repo:

```bash
$EDITOR harness.json          # check, spec, agentCommand
~/harness/install.sh .        # re-run after any edit; idempotent, and how you upgrade
git add .harness evals harness.json AGENTS.md
.harness/hooks/probes.sh      # what the tree says about itself
.harness/loop.sh 1            # one iteration, attended
```

Documents are seeded only if absent, so re-running never overwrites your own. `--adapter claude`
additionally writes `.claude/agents/` and `.claude/settings.json`.

Installed layout:

```
AGENTS.md                 context file, ~50 lines. CLAUDE.md/GEMINI.md/copilot-instructions.md point at it
SPEC.md                   objective and exit criteria, one row per behaviour named by its test
TASKS.md                  the queue
PROGRESS.md               one entry per iteration, append-only, read by its tail
LEARNINGS.md              one line per paid-for mistake, each naming a file, command or hook
DECISIONS.md              killed findings, then archived task blocks
.check-baseline           inherited failures; only ever shrinks
.harness/loop.sh          the launcher: stage sequence, halts, digest
.harness/lib/             its modules — queue.sh, agent.sh, gates.sh
.harness/tasks.py         the one parser for TASKS.md
.harness/worktree.sh      one lane in its own git worktree
.harness/archive-done.sh  trims TASKS.md and PROGRESS.md
.harness/hooks/           probes.sh, check-gate.sh, immutable.sh, verify-done.sh
.harness/roles/           the five role prompts
.harness/RAILS.md         the rails, each naming what enforces it
evals/run.sh              the write-path gate for a new LEARNINGS.md rule
```

## Commands

```bash
.harness/loop.sh [n]              n iterations, default 3
.harness/worktree.sh [n]          the same, in an isolated worktree, --ff-only back
DRY_RUN=1 .harness/loop.sh 1      print the stage plan, spawn nothing
BUDGET_USD=5 .harness/loop.sh 8   halt at a spend
touch STOP                        halt before the next stage; delete to resume
.harness/hooks/probes.sh          the findings
.harness/watch.sh                 follow a run from another terminal
evals/run.sh [--gate <name>]      test a role prompt, or decide a candidate rule
./selftest.sh                     the package's own floor
```

## Layout of the launcher

`loop.sh` is the stage sequence, the halts and the digest. Everything else is a module it sources:

| Module | Holds |
| --- | --- |
| `lib/queue.sh` | the shell surface over `tasks.py`; no parser of its own |
| `lib/agent.sh` | one process per stage, the spinner, the rate-limit wait, the run log |
| `lib/gates.sh` | `gate_verdict`, `gate_scope`, `in_scope` |
| `tasks.py` | TASKS.md parsed once. `--selftest` asserts it against fixture queues |

`TASKS.md` used to be read by seven separate awk and sed programs inside the launcher. Both parser
defects this repository has had came from that layer: BSD `sed` reads `[ \t]` as
space-backslash-t, and awk cannot see a code fence, so the block-format example in the template was
a task a lane could take. asdf hit the same wall at a larger scale and rewrote to Go — *"Bash is
very limiting when it comes to data structures. By default everything in Bash is just a string"*
([asdf v0.16](http://stratus3d.com/blog/2025/02/03/asdf-has-been-rewritten-in-go/)) — and kept
shell for plugins. Here bash keeps process spawning, the tty and the traps; Python takes the data.

## Roles

Five prompts with separated authority. One fresh process per stage; role isolation does not use
vendor subagents, which exist for Claude Code, Copilot and Cursor and not for Codex or Gemini.

| Role | Does | May never |
| --- | --- | --- |
| `scout` | turns `FINDING` lines into `status: proposed` blocks | have a finding of its own, promote, or fix |
| `adjudicator` | promotes a proposal to `ready`, or kills it with the command that refutes it | write a proposal, or edit a file a block names |
| `implementer` | one task, inside its `scope:` globs, test first | mark anything `done` |
| `verifier` | fresh session, promotes to `done` or rejects with reproducible reasons | fix code |
| `researcher` | attaches a source to a decision already made | find anything, or amend a governing document |

A single agent that selects its own work and grades it measures self-consistency; the measured
visible-versus-held-out gap is 43-48pp (SpecBench).

## Gates

The launcher re-runs every verdict. A task at `done` is forced back to `ready` when:

| Gate | Condition |
| --- | --- |
| `gate_verdict` | the tree is dirty — the implementation is not on the branch |
| `gate_verdict` | the check is red on delta against `.check-baseline` |
| `gate_scope` | the diff touches a file the task's `scope:` globs do not name |
| `gate_scope` | the diff touches the harness under a task whose `rows:` is not `none — harness` |

The check command is yours. The harness verifies on delta against `.check-baseline`, not on zero:
a tree whose exit criteria are not all covered cannot reach zero failures. `.check-baseline` only
ever shrinks.

## Probes

`.harness/hooks/probes.sh` runs thirteen analyses and reports; it never gates. `FINDING` lines are
the only legal queue input: the scout transcribes, the adjudicator re-runs the command and kills
what does not reproduce.

| Probe | Reports |
| --- | --- |
| `spec-untested` | an exit-criteria row with no test |
| `queue-uncovered` | an untested row no open task names, or a task naming a row that does not exist |
| `rail-unenforced` | a rail naming enforcement that does not exist or does not run |
| `hash-uncovered` | a file a `test-hashes.json` rail names with no key |
| `learning-unenforced` | a LEARNINGS.md entry naming no file, command or hook |
| `learning-ungated` | a dated rule with no eval, or a rule library over `learningsCap` |
| `ponytail-ceiling` | a `ponytail:` shortcut marked in the source |
| `rejection-stale` | a REJECTED note under a non-`ready` status, or a `needs-spec` block |
| `queue-hygiene` | duplicate ids, missing fields, dangling `blockedBy` |
| `friction-repeat` | the same `friction:` twice with no LEARNINGS.md rule |
| `check-red` | the check is failing |
| `litter` | a tracked or untracked file on no allowlist |
| `driver` | whatever your driver reports (off by default) |

A fresh install reports three findings and all three are correct: the seeded exit-criteria row has
no test file, `test-hashes.json` does not exist yet, and `immutable.sh` carries one marked shortcut.

## The driver

Twelve probes read text. `driver` runs your artifact through the surface a user touches, which is
the only source of capability findings. It is off until `driverCommand` is set and
`HARNESS_DRIVER=1` is in the environment.

Contract: exit 0 whenever you reached the artifact, whatever you found; print one line per shortfall
beginning `FINDING `. A non-zero exit is `PROBE driver ERROR` and nothing is proposed from it. Off
prints `PROBE driver OFF`, never a count of zero. The command runs in a throwaway directory under
`env -i`, so an agent you drive inherits none of the loop's context.

`.harness/driver.example.sh` is the skeleton. `driver.sh` in this package is a worked example that
drives the harness itself.

## Rules and the write-path gate

A repeated `friction:` line in `PROGRESS.md` becomes a rule in `LEARNINGS.md`, which every task
reads. `evals/run.sh --gate <name>` decides whether the rule earns its place:

| Condition | Establishes |
| --- | --- |
| the eval passes with the rule | it fixes the case it came from |
| the eval fails with the rule ablated | the case would not have passed anyway |
| every other eval still passes | it regresses nothing that worked |

Ablation is a per-eval `ablate.sh`. `learning-ungated` reports rules that skipped the gate and a
library over `learningsCap` (default 12).

Conditions 1 and 3 are GRASP's admission rule under a hard regression budget, and GSE's local plus
replay validation. Condition 2 is not in either: both ask whether the case passes now, neither asks
whether it would have passed without the rule. The cap follows GRASP's capacity-bounded library.
Unvalidated self-written rules are measurably worse than none — see References.

## Evals

One directory per eval: `setup.sh` builds a fixture repo with the harness installed, `prompt.txt` is
the request in the shape `loop.sh` sends it, `assert.sh` exits 0 when the role obeyed its rule,
`ablate.sh` removes the rule for the gate. Each runs in a throwaway repo with a fresh agent process.

Three ship, one per queue-gating role. They need a real agent, so `selftest.sh` asserts the runner
rather than spawning one. With no agent configured, `run.sh` refuses rather than reporting a result.

## Configuration

`harness.json` at the repo root. Every key becomes a `__SCREAMING_SNAKE__` token that `install.sh`
substitutes into the scripts and prompts; the installed harness reads no config at runtime.

| Key | Default | |
| --- | --- | --- |
| `agentCommand` | `["claude","-p",…]` | headless invocation of your agent. Also accepts an object keyed by role |
| `check` | `bun run check` | the floor. Anything that exits non-zero on failure |
| `checkForce` | `bun run check -- --force` | the same, cache-defeating |
| `failNameSed` | `s/.*(fail) //p` | extracts one failure name per line; this is what makes `.check-baseline` work |
| `costSed` | matches `total_cost_usd` | extracts run cost from the agent's output |
| `spec` | `SPEC.md` | the contract |
| `rowsHeading` | `## 11. Exit criteria` | heading the probes slice for rows; `rowsEndHeading` terminates it |
| `driverCommand` | `""` | the driver, off by default |
| `learningsCap` | `12` | bound on the rule library |

The rest tune `litter` and `spec-untested` to your layout: `sourceRoot`, `contractFile`,
`testFileSuffixRe`, `testDeclPatterns`, `sourceExt`, `harnessFiles`, `harnessGlobs`,
`allowedPrefixes`, `docs`, `harnessAllow`, `machinery`. Defaults in `harness.default.json`.

`install.sh` greps the installed files for a surviving `__TOKEN__`, fails if it finds one, then
`bash -n`s every script.

`AGENTS.md` is the context file because it is an open format read by 20+ agents;
`CLAUDE.md`, `GEMINI.md` and `.github/copilot-instructions.md` are generated one-line pointers at it.

## Agents

| Agent | `agentCommand` |
| --- | --- |
| Claude Code | `["claude","-p","{prompt}","--dangerously-skip-permissions","--max-turns","{turns}"]` |
| Codex CLI | `["codex","exec","{prompt}","--sandbox","workspace-write"]` |
| Gemini CLI | `["gemini","-p","{prompt}","--yolo"]` |
| opencode | `["opencode","run","{prompt}"]` |
| Copilot CLI | `["copilot","-p","{prompt}"]` |
| Goose | `["goose","run","-t","{prompt}"]` |
| Aider | `["aider","--message","{prompt}","--yes"]` |

Per-role commands, so a role can run on a different model or vendor:

```json
"agentCommand": {
  "default":  ["claude", "-p", "{prompt}", "--dangerously-skip-permissions", "--max-turns", "{turns}"],
  "verifier": ["codex", "exec", "{prompt}", "--sandbox", "workspace-write"]
}
```

A role the object does not name uses `default`. `DRY_RUN=1 .harness/loop.sh 1` prints what each
stage would spawn.

## Cost

Every spawned stage appends a tab-separated record to `.harness/run.log`: UTC timestamp, iteration,
role, task, seconds, exit code, cost. The log is git-ignored. Cost comes from `costSed` over the
agent's own output; an agent that reports nothing leaves the column empty.

`BUDGET_SECONDS` and `BUDGET_USD` halt before the next stage once the total reaches either. They are
checked at stage boundaries, so a budget stops the next agent rather than killing a running one.

## Skills

The role prompts name skills (TDD, systematic debugging, code review, minimalism) and continue when
one is missing. Skills install per tool, not per repository, so this package vendors none. It ships
its own procedure as a `SKILL.md` in `skillsDir` in the [agentskills.io](https://agentskills.io/)
format, which ~48 clients read.

## Adapters

The floor is language-specific. `adapters/bun-turbo/` ships a five-stage `check.ts` and a turbo-aware
`check-covered.sh` that fails any changed file no executed build task covered. Install with
`--adapter bun-turbo`.

Without an adapter you get `check-gate.sh`: run the check, forgive only exact `.check-baseline`
matches, fail closed when it cannot name what failed. You lose the coverage assertion. An adapter is
about forty lines.

## Not included

| | |
| --- | --- |
| A driver for your artifact | the probe, the skeleton and a worked example ship; the surface is yours |
| Parallel lanes | `worktree.sh` isolates one. Several at once is not shipped |
| A held-out suite | the verifier sees the same tests the implementer did |
| `test-hashes.json` | `install.sh` does not write it; `hash-uncovered` reports that until you do |
| Evals for `implementer` and `researcher` | three of five roles covered |

Fresh sessions isolate context; worktrees isolate the checkout. Every stage is already a new process
that cannot see the last one's conversation, and all of them write to the same working tree, which
is why `one checkout is one writer` is a rail rather than a mechanism.

## When to use it

When the capability sequence is not known up front, when the failures that matter sit outside the
tests you have, and when you intend to leave the thing running. If complete tests already describe
the work, give an agent the tests instead.

Start with one exit-criteria row, one task, and `.harness/loop.sh 1`.

## Testing the harness

```bash
./selftest.sh                    # installs into a throwaway repo and asserts against it
HARNESS_DRIVER=1 ./selftest.sh   # plus the driver, ~90s
KEEP=1 ./selftest.sh             # leave the scratch repo
```

Covers install and re-install, queue resolution, the clarification halt, every probe running without
erroring, all five delta cases of the gate, both scope gates, worktree isolation and merge-back, the
run log and budget halts, the eval runner, and all four write-path gate outcomes.

## References

| Claim | Source |
| --- | --- |
| Self-graded work: 43-48pp visible-versus-held-out gap | [arXiv:2605.21384](https://arxiv.org/pdf/2605.21384) |
| Fresh context per task: per-bug accuracy 58.9% → 36.5% when an agent inherits its own state | [arXiv:2607.27283](https://arxiv.org/html/2607.27283v1) |
| Multi-turn degradation averages 39% | [arXiv:2505.06120](https://arxiv.org/abs/2505.06120) |
| Context files cost >20% inference, ~3% success when auto-generated | [arXiv:2602.11988](https://arxiv.org/abs/2602.11988) |
| `AGENTS.md` adoption; subagents and hooks are per-vendor | [arXiv:2602.14690](https://arxiv.org/abs/2602.14690), [agents.md](https://agents.md/) |
| Read-only tests are the cheapest anti-reward-hacking mitigation; abort affordance cut cheating 54% → 9% | [arXiv:2510.20270](https://arxiv.org/html/2510.20270v1) |
| "Please do not reward hack" fails in 70-95% of attempts | [METR](https://metr.org/blog/2025-06-05-recent-reward-hacking/) |
| Rule admission under a hard regression budget; capacity-bounded library; ungated accumulation fell to 41.2% from a 40.6% baseline | [arXiv:2605.29668](https://arxiv.org/abs/2605.29668) |
| Local then replay-driven validation of a candidate skill | [arXiv:2608.06153](https://arxiv.org/html/2608.06153) |
| Unvalidated reflective memory worse than none; 0 of 121 reflections named the correct target | [arXiv:2605.29463](https://arxiv.org/html/2605.29463) |
| Context collapse: 18,282 tokens at 66.7% → 122 tokens at 57.1%, baseline 63.7% | [arXiv:2510.04618](https://arxiv.org/html/2510.04618v1) |
| Task length and reliability: T₉₀ ≈ ⅐ T₅₀ | [arXiv:2505.05115](https://arxiv.org/pdf/2505.05115) |
