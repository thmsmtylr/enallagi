# Intent: what the harness is for

Author: Thomas Taylor. Status: accepted 2026-09-04. Format: Stage 1 of the AI-native SDLC playbook
([claude.com, 21 Aug 2026](https://claude.com/blog/the-ai-native-sdlc-playbook)).

## Problem

The playbook's thesis is that code stopped being the bottleneck and the human-speed stages around it
did not move. Its answer is a loop in which every stage commits an artifact the next stage reads,
and human judgment concentrates at gates rather than on every line.

The playbook describes that loop. It does not ship one. Every play is a set of instructions a
platform team executes by hand, and the two mechanisms that would make the loop close on its own —
"an independent confidence gate between stages, a deterministic check or an adversarial reviewing
agent" and a detection script that "invokes Claude without a person in the path" — are named in
Stage 6 and left as an exercise.

That exercise is this repository, narrowed to the half of the loop where the gates live.

## Proposed outcome

**A runnable implementation of Stages 3 through 6 of the AI-native SDLC, which is proven by building
itself.**

The harness takes an accepted specification and runs Build, Test, Deploy and Maintain without a
person in the invocation path, with the playbook's controls implemented as code rather than as
practice:

| Playbook control | How it is enforced here |
| --- | --- |
| "the agent that wrote the code has no way to approve it" | five role prompts with separated authority; the implementer may set `review` and never `done` (`.harness/roles/`) |
| "a fresh context window once the session believes the work is done" | one fresh process per stage, no shared conversation (`.harness/lib/agent.sh`) |
| "verification before a task is reported done" | `gate_verdict` re-runs the check behind the verifier's verdict and forces `done` back to `ready` (`harness/lib/gates.sh`) |
| "a hook that blocks edits to test files during a fix task" | `test-hashes.json` plus `.harness/hooks/immutable.sh`, with the verifier as the authority (SPEC.md §0.2) |
| "evals … run on any change to CLAUDE.md, skills or hooks" | `evals/run.sh`, plus an ablation condition the playbook does not have: a rule must **fail** without it |
| "a deterministic script watches … and invokes Claude when a control band is breached" | `.harness/hooks/probes.sh` — fourteen deterministic analyses, no model, emitting `FINDING` lines that are the queue's only legal input |

The last row is the differentiator. The playbook's detection script watches production metrics. This
one watches the repository's own invariants — an untested exit criterion, a rail naming enforcement
that does not exist, a rule with no eval behind it — which means the loop has a control band even
before there is a production to measure.

**Proof of the outcome is self-hosting.** The package was built by installing itself into its own
repository and letting the loop work. `docs/bootstrap.sh` derives that record from `git log` and
nothing else, and `selftest.sh` fails if a history contains no verifier rejection — a bootstrap
record with no refusals in it is a record of a loop that was not gating anything.

## What exists today (2026-09-04)

Measured, not asserted:

- `docs/bootstrap.sh` → six dogfood rounds across 90 commits; round 6 is 55 commits, 7 verifies,
  **6 rejections**, in flight.
- `./selftest.sh` → `EXIT=0`, 88 ok / 2 skip / 0 FAIL (PROGRESS.md, 2026-09-04).
- `.harness/hooks/probes.sh` → `EXIT=0`. `check-red 0`, `queue-hygiene 0`, `queue-uncovered 0`,
  `rail-unenforced 0`, `hash-uncovered 0`, `learning-unenforced 0`, `learning-ungated 0`,
  `rejection-stale 0`, `friction-repeat 0`, `litter 0`. Two report: `spec-untested 4` — the rows
  T-007, T-040, T-041 and T-042 are queued to cover — and `ponytail-ceiling 28`, of which seventeen
  are the probe matching the literal string `ponytail:` inside prose that describes a shortcut
  rather than a shortcut in code.
- `./driver.sh` → `EXIT=0`, **no `FINDING` lines**. The capability probe drives four adversarial
  lane modes — work left uncommitted, no PROGRESS.md entry, an edit outside `scope:`, a red floor —
  through a real installed loop and reads the persistent effect rather than what the loop said. All
  four are caught (2026-09-04).
- Queue: 7 `done`, 9 `ready` (T-004, T-007, T-040 through T-046), 1 `proposed`.
- CI: `ubuntu-latest` + `macos-latest`, GNU and BSD userlands — the regression test for the two
  parser defects this package has actually paid for.
- `harness/**` and `.harness/**` are in sync: a reinstall from source reproduces all eleven
  installed files byte-identically. Nothing asserts that, which is the gap — not drift.

Against the playbook's six stages:

| Stage | State |
| --- | --- |
| 1 Plan | **out of scope.** See Constraints. |
| 2 Design | **out of scope.** `SPEC.md` is written by a human and accepted by a human. |
| 3 Build | built — roles, `one-scope`, task-block criteria as the committed plan, hooks as guardrails |
| 4 Test | built, and past the playbook — delta against `.check-baseline`, the ablation gate |
| 5 Deploy | **partial.** The CI matrix exists; the PR review loop and the release gate do not |
| 6 Maintain | built — fourteen probes, deterministic, no model in the detection path |

## What is already proven, and is the reason to keep going

Three things in the history are the strongest evidence the thesis holds. They are easy to mistake
for churn, so they are named here.

**The loop found a contradiction between two of its own governing documents and refused to pick a
side.** `harness-lane` said any diff touching `test-hashes.json` belongs to a `rows: none — harness`
task. Every exit-criteria row is a `selftest.sh::` row, and `selftest.sh` is hashed — so any task
adding an assertion had to re-cut a hash, which forced `rows: none — harness`, which forbade it from
claiming the row it had just turned green. No product row could ever land. A lane hit this, halted at
`needs-spec`, and wrote the reproduction into the block rather than editing the rail it was standing
on. Resolved by a human in 30a8af7, and then again in f31adb4, because the first fix cleared one axis
and left the other — which the gate caught by re-rejecting T-002. `blocked-is-allowed` is the rail
that makes this possible, and it is the difference between a loop and a machine for manufacturing
green.

**The verifier rejected a passing test twice for passing vacuously.** T-004's CI workflow was
rejected at 93e567a — "the floor assertion still passes on a workflow that never runs the floor" —
and again at aff0b33 — "an `if:` on the floor job leaves the row green." A test that passes without
executing anything is the exact failure this package exists to catch, and it was caught by a fresh
session that never saw the implementation conversation. T-004 not having landed is a gap; T-004
having been rejected three times is the demo.

**The capability probe reports clean.** `./driver.sh` → `EXIT=0`, no findings: four adversarial lane
modes driven through a real installed loop, all four caught, verdict read from the tree rather than
from anything the loop said.

## The one thing intent has to resolve first

The loop's dominant input is currently its own paperwork.

Eleven of the fourteen probes read the harness's bookkeeping — the queue, the contract, the rails,
the rule library, the filesystem. Two read code or the check. One, `driver`, reads the surface a user
touches and is the README's own "only source of capability findings"; it was **off** until
a57416d.

The queue shows the effect. Three of the seven `done` tasks are tasks about tasks: T-013 ("T-002 is
parked at needs-spec"), T-014 ("notes carry REJECTED while status is done"), T-011. T-015 is T-013
again, still `proposed`. T-040 went `needs-spec` on 2026-09-04 and immediately produced a fresh
`rejection-stale` finding — the loop generating its next task out of its failure to finish the last
one. Rounds 2 through 5 were three to six commits each; round 6 is fifty-five.

The rail deadlock above is fixed, so the queue can finish work again. What remains is that it has
almost nothing but its own state to find work in.

There is a sharper version of the same blind spot, found on 2026-09-04 while writing this file.

**The harness's own governing documents are the unsubstituted template, and they name a check that
does not exist.** `harness.json` says `check: ./selftest.sh`. But `AGENTS.md:9` — the context file
every lane reads — says *"Verify, and this is what done means: `bun run check`"*, `AGENTS.md:10` and
`LEARNINGS.md:20` send agents to `bun run check -- --force`, and `AGENTS.md:31` defines the `green`
rail in the same terms. There is no `package.json` in this repository and `selftest.sh` has no
`--force` flag. `SPEC.md:47` is worse than stale: §0.4 describes a five-stage
`precheck → typecheck → lint → test → trace` pipeline that `selftest.sh` does not implement, so the
contract documents a different check from the one the gate runs.

Documents are seeded only if absent, so these were written on the first install from the default
`check` value and never re-seeded when `harness.json` changed. A lane already hit it and worked
around it locally rather than fixing the source — DECISIONS.md:709: *"The criteria say
`./selftest.sh`, not `bun run check`: there is no package.json in this repo."*

It survived because the probes read these documents for **shape** and none read them for **truth**.
`learning-unenforced` checks that a LEARNINGS.md entry names a file, command or hook; it never
checks that the thing named is the thing that runs. That is the whole diagnosis in one line, and it
is the strongest argument for the driver: an instrument pointed at the artifact would have said so
on the first run.

**Resolution, in order:**

0. ~~**Make the governing documents name the check that runs.**~~ **Done.** `AGENTS.md`,
   `LEARNINGS.md:20` and `SPEC.md` §0.3 and §0.4 now describe `./selftest.sh`. §0.4 documents what
   the check actually runs against the five stages it used to claim, and names `trace` as **absent**
   rather than implied; §0.3 states plainly that the forbidden list is not yet grepped. Converting a
   false claim into a documented gap is the only honest move available — the alternative is a
   contract that reads well and describes nothing.

   The probe that would have caught it now exists: `check-unnamed` reports when the context file's
   Commands section names a check other than `harness.json`'s. Scoped to that section on purpose —
   the check is usually named correctly further down where the `green` rail is explained, and a
   document that explains the rail correctly while telling a lane to run the wrong command is
   precisely the state being caught, so a match anywhere in the file is not a match. Ablated before
   it shipped: `0` on the fixed file, `1` with the exact original line restored, `0` again.

1. ~~**Turn `driver` on**~~ **Done** (a57416d). `driverCommand: "$HARNESS_ROOT/driver.sh"`, with
   `HARNESS_DRIVER=1` still required in the environment — the second key is deliberate, because the
   driver costs a full install and four loop iterations on every scout round.

   It did not produce a stream of findings on day one, because the harness passes its own driver on
   all four modes. Its value is twofold: the next capability regression is caught rather than
   discovered, and the driver becomes the thing worth *extending*. A fifth mode — a lane that
   re-cuts a hash for a file outside its scope, a lane that rewords a `friction:` line to dodge
   `friction-repeat`, a lane that weakens `.check-baseline` — is a capability question, and answering
   it is engineering rather than bookkeeping.

2. **Stop `ponytail-ceiling` counting its own documentation.** It reports 32, and only 14 of those
   are markers in code. The other 18 are the literal string `ponytail:` inside prose — TASKS.md 8,
   DECISIONS.md 6, two in this file, and one each in PROGRESS.md and README.md — that *describes* a
   shortcut rather than taking one. PROGRESS.md records this as the sixth sighting of a check firing
   on the text that documents it, the same class as `rejection-stale` reading `notes:`.

   The count moved 28 → 32 while this file was being written, which is the argument in miniature:
   two of the four are a genuine new marker in both copies of `probes.sh`, and two are this
   paragraph. It gates nothing, so nothing is red — but a probe whose number is not a count of the
   thing it names is a probe nobody will read, and a probe nobody reads is where a real shortcut
   goes to hide.

3. **Land T-004 and T-007.** Both are unblocked now and both are real work.

(0) and (1) are done. (2) and (3) are open, and the standing risk they leave is that a round spends
itself on its own bookkeeping rather than on the artifact.

## Where it goes

In the order a stranger can verify each one:

1. **Capability findings.** `driver` on. The measure of success is a `FINDING` line about the
   installed artifact rather than about the queue.
2. **Stage 5.** The playbook's PR review loop and hooks-as-approval-gates. A `REVIEW.md`, findings
   the launcher can read as a machine-readable tally, and a release gate that asks rather than
   blocks. This is the largest hole against the playbook and the one an employer reads first.
3. **A threat model.** The `FINDING`-to-queue path is an injection surface into an unattended loop.
   It is real, it is unstated, and stating it is worth more than closing it.
4. **Trajectory probes.** Thirteen of the fourteen probes read text at rest. None reads what the
   agent did.
5. **A thirty-second demo.** Nothing above matters if the first sixty seconds do not land.

## Affected users and systems

One repository and one author. There is no adoption goal, no niche and no monetisation. The audience
is a senior engineer reading the repository cold, deciding in minutes whether the person who wrote it
understands agentic software engineering well enough to be trusted with it.

## Constraints

- **Plan and Design stay human.** The harness starts where an accepted specification exists. Putting
  a model in the contract-writing path removes the only fixed point the gates measure against, and
  the playbook itself keeps the product owner accountable at both stages. This is a refusal, recorded
  so its absence is not read as an oversight.
- **Every claim carries the command that produces it** (`citable`). No number lands that was not run
  and stamped (`measure-first`).
- **Self-hosting is the validation.** Anything that cannot be demonstrated by the loop running on
  this repository is not evidence.
- **No parallel lanes, no daemon, no durable-execution engine, no hosting.** Recorded refusals:
  parallel lanes contradict `one-row`, files already survive a kill, the git tree is the journal,
  and hosting is a security business, not a portfolio artifact.
- Bash and `python3` only. Nothing else may become a dependency.

## Open questions

- Does the `driver` probe's finding stream stay inside the same queue as the paperwork probes, or
  does a capability finding deserve a different lane? Currently they are indistinguishable to the
  scout.
- Nothing compares `.harness/**` against `harness/**`. Measured 2026-09-04: a reinstall from source
  reproduces all eleven installed files byte-identically, so there is no drift **today** — the gap is
  that no assertion says so. Is the installed instance a build artifact that should not be tracked at
  all, or a second source that needs a diff assertion in `selftest.sh`?
- Does Stage 5's review loop run as `claude-code-action` in CI, or as a sixth role inside the loop?
  The former is the playbook's answer; the latter keeps the whole loop in one place and one process
  model.
