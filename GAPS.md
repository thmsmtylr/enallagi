# GAPS

What is still wrong with this harness, found by reviewing it against Jarred Kenny's
[Building Autonomous Goal Loops That Deliver](https://jx0.ca/building-autonomous-goal-loops-that-deliver/)
(20 Aug 2026). Ranked. Shipping these silently would be worse than shipping them written down.

Each section says what was fixed here and what is still open. Where something is fixed, the
assertion that proves it is named — a fix with nothing asserting it is a claim, not a fix.
Run `./selftest.sh` to see all of them execute.

---

## 1. The loop's direction signal was document hygiene

**The probe is fixed. What it drives is yours, and that is still the ceiling.**

`probes.sh` was eight analyses and every one read *text*: test declarations, a markdown table,
`git ls-files`, `TASKS.md` fields. `check-red` shelled out to the floor. Not one exercised the built
artifact through the surface a user touches.

So the floor and the direction signal were the same instrument. The article's line is
*"the floor cannot tell us what to build next"* — here it was the only thing that did.

**Fixed:** a twelfth probe, `driver`. It spawns whatever `harness.json`'s `driverCommand` names,
from a throwaway working directory with a stripped environment, and transcribes every line of its
output that begins `FINDING `. It slots into the existing pipeline with no other change: the scout
transcribes the `FINDING` line, the adjudicator re-runs the cited command and kills what does not
reproduce, and the scout template already asks for criteria phrased as *"what has to become true for
the probe to stop emitting that line"*.

The three things the article warns about are in the mechanism, not in a comment:

- **No score when the world is unhealthy.** A non-zero exit from the driver is
  `PROBE driver ERROR` and fails the whole probe run, which `scout.md` step 2 already treats as
  "not a clean tree". Configured-but-off prints `PROBE driver OFF` — never a count of zero, because
  a probe that did not run has found nothing, which is not the same as a clean tree. Asserted:
  *"an unreachable artifact is ERROR, never a count of zero"*, *"a driver that cannot reach the
  artifact fails the whole probe run"*.
- **The thing being driven is not a development agent.** The command runs in `mktemp -d` under
  `env -i`, so if what you drive is itself an agent it inherits none of this loop's context.
- **Watch the persistent effect, not the answer.** That is your script's job and the README says so.
  Software can describe the correct action without performing it.

Cost: it is **off** unless `driverCommand` is set *and* `HARNESS_DRIVER=1` is in the environment,
because `probes.sh` already runs the full check and a scout round is slow enough.

**Still open, and it is the real ceiling:** what the driver drives. Only you know your artifact's
surface. Until that script exists, every count the loop reads still came from a grep over the repo.

## 2. Rails naming enforcement that did not run

**All four fixed.**

In the source repo the rails were written when a multi-worktree launcher was the default. `loop.sh`
became the default and did not inherit its gates:

| Rail | Named enforcement | Was | Now |
| --- | --- | --- | --- |
| `blocked-is-allowed` | `gate_verdict` | lived only in the unused parallel launcher, so nothing re-ran the check behind a `done` | **fixed** — `gate_verdict()` is in `loop.sh`, runs the gate after the verify stage, and forces an unsupported `done` back to `ready` |
| `no-clarification-left` | `__HARNESS_DIR__/loop.sh` | not implemented anywhere | **fixed** — `loop.sh` greps the contract before the first iteration and exits 1 |
| `one-scope` | "the harness's scope check" | no such check; the M2 version was a *planner* that keeps lanes disjoint, not a check that a lane stayed in scope | **fixed** — `gate_scope()` diffs the iteration's own commits (`git diff --name-only $ITER_BASE HEAD`) against the task's `scope:` globs and forces a `done` that left them back to `ready`. The matcher `in_scope()` is asserted in `loop.sh --selftest`; the gate itself is asserted end to end in `selftest.sh` against a fixture lane that leaves its scope |
| `verifier-not-implementer` | a held-out test directory | did not exist | **fixed as written** — the rail names the fresh session and the separate process, which is what actually happens. A held-out suite is more and is still yours to write |

The general lesson is in the template's rails header: a rail enforced by an agent prompt must **say
so**, rather than name a mechanism. Six of the source repo's rails named "the verifier" and the
probe could not tell them apart from ones naming a file that had gone missing.

## 3. The harness was not under the harness

**Fixed, and now checked.**

`.loop/` was gitignored, so `loop.sh` — the scheduler, the halt conditions, every agent prompt the
launcher sends — was unversioned, undiffable, unrevertable and outside `harness-immutable`, which
covered the build config and the check script but not the thing that invokes them. The stated reason
("machinery, never read by a lane") was not achieved by gitignore: single-lane runs spawn agents in
the root checkout, where the directory is on disk and readable.

In this package the harness directory is tracked by default, `install.sh` never adds it to
`.gitignore`, the `litter` probe's `allowedPrefixes` includes it, and the template's
`harness-immutable` rail names `__HARNESS_DIR__/loop.sh`.

**Fixed:** the missing check is now the `hash-uncovered` probe — every file named in a rail whose
enforcement is `test-hashes.json` must have a key there. `rail-unenforced` went quiet as soon as the
file existed, whatever was in it; this one keeps emitting until the key exists. On a fresh install it
emits exactly one finding, which is the truth: **add `loop.sh` to `test-hashes.json` yourself.**
Asserted: *"harness-immutable names loop.sh and no test-hashes.json covers it"*.

## 4. PROGRESS.md was an archive wearing a state file's name

**Fixed.**

The implement prompt said "Read PROGRESS.md" of a 5,679-line append-only file whose newest entry is
last. The default file read takes ~2,000 lines from the top: the oldest third, and none of the
`next:` handoff the file exists to carry. Its own header cited the 58.9%→36.5% figure and said
*"a long handoff is the thing it mitigates"*.

Fixed: `loop.sh` asks for `tail -200 PROGRESS.md` and says why. And `archive-done.sh` — already the
precedent for `TASKS.md` → `DECISIONS.md` — now rolls `PROGRESS.md` over too: past `PROGRESS_MAX`
(2,000) lines, everything older than the last `PROGRESS_KEEP` (200) moves to `PROGRESS.archive.md`,
split on an entry heading so no entry is ever cut in half. The header with the entry format stays.

Not fixed, and a judgment call rather than a defect: the article separates `STATE.md` (current queue,
failures, next action, rewritten each round) from an archive nobody reads. Here TASKS.md is the queue
and the trimmed PROGRESS.md is the state; nothing is rewritten in place.

## 5. Nothing asked the round-end question

**Fixed.**

The article ends every round with *"What cost time that a rule or check could prevent?"* — first
occurrence is evidence, a **repeat** becomes a durable constraint. The source repo had the
destination (`LEARNINGS.md`) and the form police (the `learning-unenforced` probe), but nothing
asked, so every entry was a human post-mortem.

Fixed in three places, because one of them is a prompt and prompts drift:

- `PROGRESS.md`'s entry format carries a required `friction:` line with the two-occurrence rule
  written next to it — *"a rule per surprise is how a harness rewrites its operating system every
  week and gets worse"*.
- `verifier.md` rejects an iteration whose `PROGRESS.md` entry has no `friction:` line.
  `friction: none` is a valid answer; a missing line is not.
- The `friction-repeat` probe emits a `FINDING` for the same friction recorded twice with no rule in
  `LEARNINGS.md`, and goes quiet when the rule is written. It matches on normalised text, so a
  reworded repeat escapes it — the ceiling is marked in the source.

There is a `friction` rail naming all three.

## 6. The three loops were not separated at the task level

**Fixed at the gate.**

Product loop, harness loop, direction loop. `rows: none — harness` existed in the scout's template so
the distinction was *written down*, but nothing routed on it: a harness change and a product change
went through the same implementer, the same scope check and the same verifier. The article's rule is
that the loop *"cannot change a product lever and the measure of that lever in the same round"*.

Fixed: `gate_scope()` also rejects a `done` whose diff touches the launcher, the hooks, the check
script, `.check-baseline`, `test-hashes.json` or `harness.json` under a task whose `rows:` is not
`none — harness`, and `verifier.md` names the same rejection. The rail is `harness-lane`. Asserted:
*"a harness edit a harness task declared is allowed"* and *"the same edit under a product task is
forced back to ready"* — both directions, because a gate that only ever says no is not a gate.

Related and now written into the rail rather than only here: freeze your **exam** as well as your
tests. Whatever files hold your approved corpus, your fixtures and your scoring rules belong in
`test-hashes.json` next to the test files, or a lane can retune what it is graded on.
`hash-uncovered` will hold you to whatever the rail names.

## 7. Missing against the highest-traction projects in this space

Found by reviewing the field, not by using it. Each is a real capability with real adoption behind
it; two are now built, and the rest are listed so the absence is a decision rather than an oversight.

**From [Spec Kit](https://github.com/github/spec-kit) (132,938★):** it runs `/speckit.clarify`,
`/speckit.analyze` and `/speckit.converge` as distinct steps. `analyze` is
**cross-artifact consistency and coverage** — does the plan cover the spec, do the tasks cover the
plan. This harness had `spec-untested` (does every criterion have a test) and `queue-hygiene` (is the
queue internally consistent), but **nothing that checked the queue against the spec**.

**Fixed:** the `queue-uncovered` probe, both directions — a criterion that is untested and named by
no open task, and a task whose `rows:` names a row the exit criteria do not define. Asserted:
*"the seeded criterion is untested and no task in flight names it"*. Spec Kit also has a
*constitution* — governing principles fixed once, separate from the spec — which is what `RAILS.md`
already is. `converge` (validating the finished implementation back against spec, plan and tasks) is
what the verifier does per task and is not done corpus-wide.

**From [Archon](https://github.com/coleam00/Archon) (23,339★):** a git worktree per run, explicit
`depends_on` DAG edges, `until:` loop conditions, and human approval gates as first-class nodes.
This harness has `blockedBy:` (a DAG), fresh context per iteration, `attended: true` (an approval
gate of sorts) and `STOP`. It does not have worktree isolation — deliberately, see the README —
which is the one that would matter first if you ran more than one lane.

**From [Superpowers](https://github.com/obra/superpowers) (280,517★):** `evals/` beside `skills/`.
The skills are themselves tested. **Still not fixed:** nothing here tests a role prompt; a change to
`__HARNESS_DIR__/roles/verifier.md` is unverifiable except by watching a run. `selftest.sh` now
drives a fixture lane through both gates, which tests the launcher around the prompts, not the
prompts.

## 8. Minor: the batch boundary is a decision point, not a budget

**Fixed.** `MAX_ITER` defaulted to 8 against the article's three-round batch; it is 3. The five halt
conditions (`needs-spec`, attended-only queue, adjudicator escalation, unresolvable proposal, two dry
rounds) substantially cover the drift risk, and the digest prints what moved. Pass an argument —
`.harness/loop.sh 8` — when you want a longer run.

## 9. Found while fixing the above

- **The block-format example in `TASKS.md` was a real task.** `ready_unattended` scans for
  `^## \[T-[0-9]+\]` and knows nothing about code fences, so the illustration numbered `T-042` inside
  the template's fenced block was the first `ready` task a fresh install offered a lane. It is
  `T-###` now, with the reason written under it. Found by driving a fixture lane through `loop.sh`
  for the first time — which is itself the point of section 1.
- **Stale `.loop/` paths.** Four installed files told you to run `.loop/archive-done.sh` and
  `.loop/loop.sh`, from before the directory was renamed and made configurable. They are
  `__HARNESS_DIR__` tokens now, so they say whatever your `harness.json` says.
