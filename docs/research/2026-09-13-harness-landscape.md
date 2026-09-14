# RESEARCH

Read 2026-09-13. Ten sources from the link list (kept at the foot of this file). Every claim below
carries a `path:line`, a command, or a URL with the date it was fetched. Repos were cloned at the
commits named and read; the three web sources were fetched. Nothing here is a task: findings that
should become work are marked `-> QUEUE` and need a probe and an adjudicator before they enter
TASKS.md.

## 0. Summary verdict

The ten sources are not of comparable value. They fall into three tiers.

| Tier | Source | Verdict |
| --- | --- | --- |
| Read it twice | `dirge-code/dirge` (esp. `docs/harness-review-2026-08.md`, `docs/failure-ladder.md`, `docs/hang-paths.md`) | TAKE, heavily |
| Read it twice | `majiayu000/harness` (the workflow evidence contract only) | TAKE, narrowly |
| Read it twice | `omp.sh/docs/compaction` | TAKE, and it settles the compaction question |
| Worth one pass | `cordiverse/cordis` + arXiv:2608.25512 | TAKE one idea, leave the rest |
| Worth one pass | `yogthos.net` dirge post | Pointer only. The repo is the source; the post overstates it |
| Near zero for us | `UditAkhourii/adhd`, `ayghri/i-have-adhd`, `move38studios/thinkfu` | LEAVE as harness work. Candidates for `[[skill]]` entries at most, and two of the three cannot pass `skill-ungated` |

Disagreeing with the framing up front: there is not a lot of value in each. There is a lot of value
in three, one idea in a fourth, and nothing in three that our `[[skill]]` gate would admit. The
three prompt-library repos are the same shape as each other and the same shape as the
`brainstorming` skill we already declare (`crates/harness/harness.default.toml`, `[[skill]]`
id `brainstorming`). Adding them adds prompt surface with no mechanism behind it, which is the
exact thing `skill-ungated` exists to refuse.

The single most useful thing across all ten is not a mechanism. It is `dirge`'s method: they built
a per-decision trace, ran the harness once against a deliberately slow, deliberately weak model,
and found four P0-P2 bugs on the first traced run. We have no equivalent, and our five roles plus
nine gates are exactly the machinery that fails silently.

---

## 1. arXiv:2608.25512 + `cordiverse/cordis`

Fetched https://arxiv.org/pdf/2608.25512 on 2026-09-13. "A Programming Paradigm for Spatiotemporal
Composability", Shi (PKU / DeepSeek-AI), Zhang (PKU), Cui (DeepSeek-AI). Cordis is its reference
implementation, so these are one source. Repo read at `f8ea3cd`, 4,991 LOC src across 9 packages,
6,497 LOC tests, 247 `it()` cases.

The paper formalises two things: **revertible effects** (every context transformation carries an
explicit inverse held by the runtime, reversed LIFO) and **reactive coeffects** (components declare
dependency keys; every context change is classified activating / deactivating / neutral). It names
"self-evolving agent harnesses" as a motivating case, which is why it is in the list.

**What the code actually delivers, and what it does not.** The theory does not survive contact with
durable state, and the repo is honest about it:

- LIFO reversal is sequential only *within* one effect (`packages/core/src/fiber.ts:283`). Across a
  fiber's top-level effects, disposal is LIFO-*start* and concurrent-*finish*
  (`fiber.ts:439-450`), because each body opens with `await Promise.resolve()`.
- A throwing disposer aborts the rest of its own chain, and `disposables.splice(0)` has already
  emptied the list, so the remaining inverses are unrecoverable (`fiber.ts:281-294`, no try/catch).
  Untested.
- `hmr.watch()`'s inverse is *deliberately partial*: it drops the callback and leaves the chokidar
  watch installed, because a total inverse would over-revert
  (`packages/hmr/src/index.ts:206-219`, with the comment saying so).
- There is no notion of an irrevocable effect. No commit point, no way to mark a change as past the
  point of no return.
- The moment cordis touches a file, the inverse-closure model is abandoned and replaced by 947 LOC
  of three-way merge, compare-and-swap writes and conflict reporting (`packages/include/`, 35
  tests). **`include` is empirical evidence that the paper's model does not extend to durable
  state.**

**TAKE: the epoch token.** `Fiber._refresh` / `_setEpoch` (`packages/core/src/fiber.ts:385-415`) is
eight lines and is the best idea in either source. Verified by reading:

```ts
_refresh() {
  let epoch: string | boolean = false
  epoch = ''
  for (const name of Object.keys(this.inject)) {
    const impl = this._store[name]
    if (!impl) { epoch = INACTIVE; break }
    epoch += ':' + impl.fiber.uid
  }
  this._setEpoch(epoch)
}
```

The epoch concatenates the **identity** of each satisfied dependency into one comparable string.
Then: same epoch is neutral (return); was INACTIVE and now is not is activating (reload); anything
else is deactivating (unload, then reload if still satisfiable). One string compare answers "should
I run?", "should I invalidate?" and "did my input get replaced under me?" at once.

For us: a stage's epoch is the concatenation of content hashes of its declared inputs (the task
block, the `scope:` files at base, `.check-baseline`, the role prompt's locked hash, LEARNINGS.md).
That gives, for free, the thing we do not have: a defensible answer to "did anything this stage
depends on move while it was running?" Today `gate_scope` answers a narrower question (did the diff
stay inside `scope:`) and `queue-intact` answers another (did an id vanish). Neither notices that
LEARNINGS.md changed under a running implementer. `-> QUEUE`

**LEAVE: everything else.** Git already gives us a *derived* inverse, which beats a *declared* one.
Rust `Drop` already gives LIFO with compile-time ordering. Reactive re-activation is actively wrong
for a short-lived CLI: a flaky check probe toggling availability would cycle a stage indefinitely.
The Proxy layer (`packages/core/src/utils.ts:161-216`, four allocations per `ctx.foo.bar()`, no
memoisation) exists only because JS cannot tell you who called.

**Second thing worth stealing, from `include` rather than the theory:** `JournalRecord`
(`packages/include/src/journal.ts:4-32`) records a **per-key delta captured at report time**, not a
whole-object snapshot, and `reconcile` (`journal.ts:195-241`) has a hard stated policy: the file
wins every conflict, and every dropped change is reported. That is the right shape for any future
"two writers on TASKS.md" problem, which our `one-writer` hook currently solves by refusing rather
than merging.

---

## 2. DeepSeek Harness, `reference/capability-seams`

Fetched https://deepseek-harness.github.io/deepseek-harness/en/reference/capability-seams on
2026-09-13. Docs only; no source was read, so everything here is what the page asserts, not what was
verified in code. It is the missing link between the two sources above and the rest of the list: a
production agent SDK whose services are, in its own words, "discovered from **Cordis** declarations"
with "interface/implementation/consumer roles classified in `scripts/gen-doc-graphs.ts` with a
completeness guard". So the paper in section 1 is not academic furniture; someone built an agent
harness on it.

**The organising idea is worth the read on its own.** Services are classified into exactly three
kinds:

- **core services**: foundational, one implementation
- **seams**: extension points where multiple providers register
- **bundles**: compositions of dependent services

Every seam is a `ctx.<name>`, and the page is essentially a list of which parts of an agent are
allowed to have more than one implementation. The ones that matter to us:

| Seam | What is swappable |
| --- | --- |
| `ctx.llm` | provider implementations; "the loop and compaction call the provider-neutral stream service" |
| `ctx.subprocess`, `ctx.shell`, `ctx.terminals`, `ctx.sandbox` | process spawning, shell, PTY, confinement, with `ctx.sandboxPolicy` as the deployment default |
| `ctx.fs` | local / sandboxed / remote, with `fs-sandbox` fencing mutations by the shared sandbox mode |
| `ctx.subagents` | in-process spawn/fork, ACP, Codex, **Claude Code** as transports |
| `ctx.compaction`, `ctx.tokenMeter`, `ctx.toolResultPruner` | three separate seams, see below |
| `ctx.sessionQuery`, `ctx.sessionProjections`, `ctx.sessionPersistence` | JSONL storage, state-driven folds with watermark tracking, SQLite full-text |
| `ctx.spillStore` | oversized tool results, replaced by "model-facing locators" |
| `ctx.approval`, `ctx.userQuestions`, `ctx.commands` | one-shot permission decisions, human-answer promises, human commands with no model invocation |

**The one design decision to take.** Compaction is split across three seams, not one:
`ctx.tokenMeter` owns per-session replay folds, `ctx.toolResultPruner` "rewrites oversized results
through replayable surface replacements **before** summary compaction via the `ctx.compaction`
seam", and `ctx.spillStore` handles oversized tool results through model-facing locators.

That is the same ordering omp implements and the same one the compaction advice in section 7
omits: **prune what is replayable before you summarize what is not, and replace bulk with a
locator rather than a paraphrase.** Two independent systems arriving at the same order is the
strongest evidence in this whole document for anything.

**What we already have, in our own shape.** Our `[[stage]] role xor command`, the thirteen
`agent.preset` files, `[[skill]]` and `[[role]]` with `source`/`path`/`rev`/lock are the same idea:
a small set of declared seams, everything else fixed. Where they have twenty-five-plus seams we have
five (agent command, check command, driver command, role, skill), which is the right number for a
single binary that installs into one repo. The page is a useful check on that: every seam they name
that we do not have is one we deliberately do not need, except `ctx.approval` and
`ctx.userQuestions`, which are the shape our `attended: true` flag gestures at without implementing.

**Not taken.** The architecture is a TypeScript plugin SDK with a web server, a client module graph
and a zod-based RPC gateway (`ctx.typert`, `ctx.typertGateway`). None of that is portable to a
static binary, and the Cordis runtime underneath it carries the costs named in section 1.

---

## 3. `majiayu000/harness`

Read at `46ab176` (2026-09-12). 872 `.rs` files, 316,721 LOC, 4,139 test attributes, 376 `.md`.
Rust control plane for fleets of Claude Code / Codex agents. Postgres-backed, HTTP server, workflow
state machine. It is ~18x our size and a different product: we install into one repo and spawn one
process at a time; they run a durable multi-tenant orchestrator.

**The repo has two halves and only one of them is real.**

The workflow runtime (`harness-workflow` + `harness-server/workflow_runtime_worker`, ~110k LOC,
~2,700 tests) genuinely enforces. The quality / self-improvement half (`harness-rules`,
`harness-skills`, `harness-gc`, `harness-context`, the interceptor chain, ~11k LOC) is elaborate
declaration attached to loops that were never closed. Verified:

- `HookEnforcer::detect_modified_files` returns `Vec::new()` unconditionally with the comment
  "host-side git inspection disabled by project policy"
  (`crates/harness-server/src/hook_enforcer.rs:70-76`, read 2026-09-13).
- `post_tool_use` has no production call site. `grep -rn "post_tool_use" crates/` outside the trait
  and the enforcer's own tests returns `handlers/observe.rs:174` (a string literal) and nothing
  else.
- `AppState.interceptors` is built with exactly three interceptors and never read.
- `skill_governance` scores an EMA over a `skill_used` event table that no production code writes.
- Their own March 2026 audit named the pattern ("comprehensive declaration, near-zero runtime
  enforcement") and the September tree still exhibits it.

Root cause, and this is the transferable lesson: the project banned host-side `git`/`gh` subprocess
calls in harness crates, which severed the file-change telemetry that `HookEnforcer`, `GcAgent` and
the `HotFiles` signal all depend on, and nothing replaced it. **A policy that removes an input
source must ship the replacement, or every loop downstream of that input becomes decoration.** We
have the same exposure in the other direction: `learning-unenforced` and `rail-unenforced` are
grep-over-text probes, and `PROBE driver OFF` is in `roles/scout.md:` step 2 precisely because we
already know this.

### TAKE 1: `ClaimTrustLevel` (the highest-value item in all ten sources)

`crates/harness-core/src/claim_trust.rs:5-13`, verified by reading:

```rust
pub enum ClaimTrustLevel {
    SelfDeclared, RepositoryObserved, RuntimeObserved,
    RunnerObserved, Reexecuted, CryptographicallyAttested, HumanApproved,
}
```

Derive order is the trust order; `satisfies(required) = self >= required` (`claim_trust.rs:42-44`).
Each level above `SelfDeclared` must carry a matching `ClaimProof` variant with the metadata that
makes it checkable (`claim_trust.rs:75-108`), and `ClaimProvenance::validate`
(`claim_trust.rs:276-302`) refuses a declared level whose proof does not match. ~400 LOC with tests,
zero coupling to the rest of their system.

**Why this matters to us specifically.** Our `blocked-is-allowed` and `green` rails already draw
the distinction informally: "the exact command and its pasted output" is `Reexecuted`; a probe
FINDING line is `SelfDeclared`; the tree itself is `RepositoryObserved`. But we have no vocabulary,
so every gate treats every piece of evidence as equal. `gate_verdict` re-runs the check, which is
`Reexecuted`, and that is the strongest thing we do. The verifier's other fourteen checks are
`SelfDeclared` prose.

### TAKE 2: evidence bound to transitions, not to tasks

`TransitionRule` (`crates/harness-workflow/src/runtime/validator.rs:32-40`) carries
`required_evidence: BTreeSet<String>` and `required_evidence_trust: BTreeMap<String,
ClaimTrustLevel>` **on the edge**, not on the work item. Requiring evidence on a transition that is
not allowed **panics at registry construction** (`validator_evidence.rs:31-35`) rather than leaving
it silently unguarded.

Three details make it usable rather than hostile:

1. **The retry exemption** (`validator_progress.rs:58-122`): a decision that is same-state, single
   `EnqueueActivity`, and named `retry_failed_runtime_activity` skips the evidence check. "Evidence
   requirements bind fact-minting transitions, not retries."
2. **Server-reserved artifact stripping** (`completion_evidence.rs:217-222`): 14 artifact types only
   the harness may author are stripped from agent output *before* the harness attaches its own,
   with a test that forges seven of them and asserts one survives. Without this, every trust level
   above `SelfDeclared` is a suggestion.
3. **A waiver is a recorded fact, not an absence.** The deployment kill switch writes
   `WorkflowEvidence::new(EVIDENCE_VERIFIED_PR_BINDING, "enforcement_lifted_by_deployment_config")`
   (`builtin_github_issue.rs:392-398`), and verification keeps running regardless of the switch so
   the audit trail stays complete during a kill-switch release
   (`completion_evidence_integration.rs:32-40`).

Mapped onto us: `review -> done` requires a `Reexecuted` check digest; `ready -> review` requires a
`RepositoryObserved` committed diff; `proposed -> ready` requires a `SelfDeclared` FINDING line plus
the adjudicator's own re-run, which is `Reexecuted`. That last one is already what
`roles/adjudicator.md:` step 2 demands in prose. Making it a typed requirement on the edge is the
change. `-> QUEUE`

### TAKE 3: the progress-mode contract

Every state declares how it makes progress: `CommandDriven | ExternalWait | OperatorGate |
ParentHandoff` (`state_registry.rs:45-50`). A transition into a `CommandDriven` state that does not
carry a job-producing command in the same decision is rejected as `ProgressDriverMissing`
(`validator_progress.rs:158-196`).

This is the structural cure for the failure our architecture is most exposed to: a status moves and
nothing is scheduled to run next. We hit exactly this at T-020 (`git show 9c0b4af`), where a
verifier deleted a heading and the block became unreachable. `queue-intact` now catches the deleted
case. It does not catch "status advanced to a state no pipeline's `when` selects".

### TAKE 4: succeeded-with-blockers, and zero-output detection

`activity_status_contract.rs:21-94`: a `succeeded` result that also reports blockers is downgraded
to a first-class `SucceededWithBlockers`, against a closed table of blocking signal types, count
fields and merge states. Directly applicable to a verifier that writes `done` while its own notes
name an unresolved point.

`activity_result.rs:142-240`: zero assistant messages + zero tool invocations + no structured
result is `ZeroOutputSpawnFailure`, not success. We have `turns-exhausted` but nothing for "the CLI
launched and did nothing", which currently reads as a clean stage.

### LEAVE

- The 4,611-line `workflow-first` RFC (commit `46ab176`). `Status: Proposed, owner approval
  pending`, unimplemented, and its own Phase 0 map concludes the existing kernel is already
  adequate. Read section 4 (the seven design principles, ~20 lines). Skip the rest.
- `harness-rules` as an enforcement layer. Its severity is derived by substring-searching the rule's
  own prose for the words "critical" / "high" / "medium" (`engine/mod.rs:246-254`), its guard IDs do
  not match its rule IDs (guard `RS-03` is unwrap, rule `RS-03` is `Box<dyn Error>`), and every
  ast-grep guard degrades to `exit 0` when `sg` is absent (`.harness/guards/rs-03-unwrap.sh:23-26`).
  **Fails open.** That is our `ZERO IS NOT PASS` learning, violated.
- Their `ExecPolicy` (918 LOC of hardened Starlark), reachable only from a CLI subcommand, gating
  nothing.

### One thing to copy that costs almost nothing

`crates/harness-workflow/tests/docs_examples.rs` parses `docs/workflow-declarative-definitions.md`,
extracts fenced blocks tagged by an HTML comment marker, and runs them through the **real**
validation path. Documentation examples as executable tests. Our README is 280 lines of tables
describing behaviour, capped by a test that counts lines but does not execute anything in them.

---

## 4. `dirge-code/dirge` and the yogthos post

Post fetched https://yogthos.net/posts/2026-06-08-dirge-code.html on 2026-09-13. Repo cloned from
https://github.com/dirge-code/dirge at `83504aa`, v0.25.5, single crate, GPL-3.0, 530 `.rs` files,
329,416 LOC, 6,189 test attributes, 79 direct dependencies, 637 packages in `Cargo.lock`.

**Architecturally it is our opposite.** dirge *is* the agent: it owns the model calls, the tool
loop, and the context. We are a launcher that shells out to somebody else's agent CLI. That split
decides what is portable, and most of the post's headline features are not.

**On the post's numbers.** The "~8 MB RAM idle vs ~300 MB for OpenCode" claim has nothing behind it
in the repo: no measurement harness, no recorded run, no CI job. `README.md:11` at least hedges with
"approximate"; the post does not, and extends it to "twenty copies for the cost of one". The 300 MB
comparison figure has no source at all. The "budget models punch above their weight" thesis is
stated, never measured: the nearest evidence is one hand-built 22-test task passed by three models,
presented in the repo as a cross-model generalisation check, not a capability claim. The binary-size
number is checkable from release artifacts. **The repo is markedly more honest than the post.** Read
the repo, cite the repo.

### TAKE 1: `docs/harness-review-2026-08.md`

This is the document in the list closest to what we are building, and its finding is the one that
should change how we work:

> Every guard's reasoning was right. Three of them described themselves wrongly, and in each
> measured case the model did not comply, it worked around the guard.

Method, in order, all of it portable:

1. **The premise check was the baseline run.** Before building anything they ran the harness against
   a deliberately slow (~9 tok/s) and deliberately weak (27B local) model, precisely so a wasted
   turn is expensive enough to notice and the steering features are exercised as designed.
   `RUST_LOG=debug` produced 384 lines for one small task, six of them from the loop, and those
   logged `ratio=1.0173125` without either operand.
2. **`--trace <path>`**, one JSON record per loop decision, tapped at the single point every event
   passes through. **Four bugs on the first live traced run**, including `dirge-2js0` (P0): a
   force-ended turn ended the entire *run*, and it "only appeared to work because a critic or
   verifier gate happened to fire and restart the outer loop."
3. **Record the decisions that produce no output.** "A trace cannot otherwise distinguish 'the stall
   never came up' from 'the stall came up three times and was declined', and those call for
   opposite fixes."
4. **The reporter is unverified code, every time.** Their `loop-trace.py` read 0 turns for a 15-turn
   run. Two claim-gate tests went green *against the very bug they were written for*. "Both were
   caught by pairing an assertion with its negation, which is the only thing that has ever caught
   this class here."
5. **A stale binary reads exactly like a broken feature.**

We have `.harness/events.jsonl` with eleven kinds (`crates/harness/src/events.rs`) and `harness
watch` / `harness events` over it. What we do not have is a record of a gate that *did not* fire, or
a probe that stood down. `PROBE <name> 0` today means both "clean" and "the heading I parse drifted"
for six of our probes, and `roles/scout.md:` step 3 pushes that onto the scout's judgment rather
than onto the record. `-> QUEUE`

### TAKE 2: `masks_failure`

`src/agent/agent_loop/verifier.rs:773-800`, read directly:

```rust
/// would have believed it too, measured, `cargo test || true` latched
/// VerifiedGreen. A gate that cannot fail for the reason that matters is worse
/// than no gate, because it is trusted.
fn masks_failure(command: &str) -> bool {
```

Any `|`, `||`, or `;`-with-something-after masks the exit status, because the status belongs to the
last stage. A bare trailing `;` or newline does not. A backslash-newline is a continuation and is
honest.

**Phase 6 of the harness review is the result worth quoting: all three models tested piped their
test output through `tail`.** Masking is not a small-model quirk, it is what models do.

This applies to us directly and today. `harness.toml`'s `check.command` is

```
export PATH="$HOME/.cargo/bin:$PATH"; cargo test --workspace -q 2>&1 && cargo clippy --all-targets -q -- -D warnings 2>&1 && cargo fmt --all --check
```

The `2>&1` is a redirect, not a pipe, so the `&&` chain's status is honest here. But
`gate_verdict` accepts whatever `check.command` says, and `layout.driver_command` is arbitrary
operator-supplied shell. A `check` or `driver` command containing a pipe or a trailing `;` is a
green nobody ran, which is our seed learning stated in a form our probes cannot see.
`-> QUEUE: a probe that refuses a `check.command`, `check.force` or `driver_command` whose exit
status is masked.`

### TAKE 3: `docs/failure-ladder.md`, the progress monitor

Derived from Nayak et al., *Validating the DS1 Remote Agent Experiment* (ISAIRAS'99), a symbolic
planner flying a spacecraft, and the doc is explicit that "what transfers is the control structure,
not the techniques."

Every other guard keys on **errors**. A model making successful, varied, useless calls trips none of
them. A **progress event** is exactly three things:

- a todo item closed (**count went down**; writing more todos is planning, not progress)
- a file mutated **that was never mutated before**
- verification going green

Three subtleties, each load-bearing and each learned the hard way:

- **The stall counter arms only after the first progress event.** A run opening with twenty reads is
  exploring, not stalling.
- **The endgame is barren by definition** (`dirge-hwk9.7`). By the time a run is finishing, its
  todos are closed so they cannot decrease, its files are touched so they cannot increase, and its
  green is latched so there is no fresh edge. All three signals are structurally unable to move, so
  *any* run with a multi-turn endgame was guaranteed to be told it had stalled. Two different models
  produced the symptom to within 0.1s of the end. Fix: only judge a boundary with a **successful
  tool call**.
- The discrimination control mattered more than the fix: a task that writes a file and then searches
  for a symbol that does not exist still fires the stall twice, mid-run. Narrowing did not mute it.

Our `discover` pipeline's `end_after_dry_rounds = 2` is the same idea at a much coarser grain. What
we lack is any notion of a *productive* iteration: an implement stage that commits, passes the
check, and changes nothing a criterion names is indistinguishable from one that did the work.

### TAKE 4: `docs/hang-paths.md`, section 1

The pattern, not the bug: **make the terminal event a property of the run's lifetime, not of the
events the run chose to emit.** `RunEpitaph` holds a sender for as long as the spawned task exists
and sends `Error` on drop if nothing terminal went out, so it fires on the panic path too.

For us: a stage is a child process. A stage that exits without writing its PROGRESS.md entry or its
status should produce a synthetic terminal record **from the wait status**, not from a missing
artifact. T-020 already made the launcher write the missing PROGRESS.md entry when no role did
(`git show 1294b78`), which is half of this. The other half is that a stage killed by a signal, or
one that produced zero output, currently reads the same as one that wrote nothing on purpose.

Two more rules from section 3, both verbatim-portable to `[[stage]].timeout`:

- **An override may only raise a ceiling, never lower it.** An override says "this needs longer"; a
  thing that wants to be cut sooner should bound itself, where it can say why.
- **The budget bounds work, not a person.** Their watchdog re-arms rather than firing while a human
  is being waited on. Our `attended: true` tasks are the analogue.

### TAKE 5: the 14-section checkpoint template, as a stage handoff schema

`src/agent/compression.rs:948-1002`. Not for compaction (we do not own a context), but as the schema
for what one stage hands the next. The best part is the bracket text on `## Active Task`
(`compression.rs:950-963`): the active task is **not necessarily the user's original wording**,
because "the current work is often an emergent follow-up that arose mid-session and was never an
explicit user request, capture THAT"; and if the original request is already complete, say so
plainly "so the next context does not redo it."

Paired with two mechanisms:

- **Verbatim user turns** (`compression.rs:1160-1210`): the folded window's user messages are
  appended **unedited**, newest-first under a shared budget, because "the summarizer paraphrases,
  and paraphrase is exactly where a user's stated constraints go soft: 'use ESM not CJS' becomes
  'discussed module format'." Our analogue: the task block's own text, and the finding's own words,
  must reach the implementer unparaphrased. `roles/scout.md` already demands "in the finding's own
  words". Nothing enforces it.
- **Write-once `intent`** (`session_db.rs:1891-1911`): the `ON CONFLICT` clause deliberately omits
  `intent`, "so the original goal can't drift as the body is re-summarized fold after fold." Three
  lines of SQL that solve goal drift. Our analogue is that a verifier rejection must never rewrite
  the task's own one-line heading.

### TAKE 6: two cheap ones

- **Tree-sitter pre-write syntax validation** as a *post-stage gate over `git diff --name-only`*. We
  lose same-turn correction, we keep "broken writes are caught, not silently trusted." Steal the two
  exclusion decisions verbatim: `.sql` is not registered because `tree-sitter-sequel` reports valid
  `CREATE PROCEDURE` and all of T-SQL as ERROR, and a hard block would make valid SQL unsaveable
  (`src/semantic/syntax_validator.rs:139-148`, read 2026-09-13); `.mojo` is not registered because
  the grammar false-errors on ~10% of 800 real files (`syntax_validator.rs:130-137`). **A hard block
  needs a grammar you have measured.**
- **Issue ids in code comments.** Every `dirge-XXXX` token in their source is a real issue in
  `.beads/issues.jsonl` (1,063 issues: 993 closed, 64 open, 6 in progress). Our task ids already
  exist; putting `T-###` in the comment next to the code it produced costs nothing and turns
  `git blame` into a design record. `.beads/` itself is Steve Yegge's tracker, not theirs, backed by
  an embedded Dolt database with git hooks; not something we need.

### LEAVE

Everything that requires owning the model loop, which is most of the post's headline list:

| Feature | Why not |
| --- | --- |
| Tool-call repair (all six kinds) | Needs the JSON Schema and raw tool-call args before dispatch. We see neither |
| StormBreaker / inert-command collapse | Operates on individual tool calls inside one turn |
| Reflect-then-pivot | Fabricates a tool result mid-turn and grants one more turn |
| The whole compaction ladder, MiMo checkpointing, fixed-overhead accounting | Every threshold is a fraction of `prompt_tokens` from a call *we* made |
| `CapabilityTier` | 1,081 lines whose entire behavioural effect is scaling one base of 3 down to 2, at two call sites, only under `Struggling`. Their own module docs say `Strong` scales identically to `Nominal` and therefore drives nothing |
| Janet plugins | Pulls `janetrs`/`evil_janet`, compiles Janet from C, runs `bindgen`, needs libclang. It broke enough source builds that a whole `no-plugin` feature set exists to avoid it |

Also note `2605.18747v1.md` (357 KB, "Code as Agent Harness", Ning et al., UIUC/Meta/Stanford) sits
in their repo root with **zero references anywhere in the tree**; verified with
`grep -rl "2605.18747\|Code as Agent Harness" dirge/ --include=*.rs --include=*.md --include=*.toml`
returning only the file itself. A reading dump committed to the repo root. Our `litter` probe would
have caught it, which is a small point in our favour.

### The bug of theirs most likely to be ours

`dirge-hwk9.3`: `claim_gate` and the verifier disagreed about the same command, because one took the
segment's **first** token (`python3`, not a test runner) and the other matched **any** token
(`pytest`, found). The model, which had *just* been corrected by the verifier and complied exactly,
was then told its truthful report was unsupported. Fixed by a test asserting the two recognisers
agree where they overlap.

We have nine gates (`crates/harness/src/gates.rs:59-67`) and twenty-one probes (`crates/harness/src/probes/mod.rs:55-76`) reading the same tree with independently written
matchers. `queue-uncovered` and `spec-untested` both parse `layout.rows_heading`.
`friction-repeat` and `ponytail-ceiling` share `kill_lines` (which is already the right answer:
`git show a77df04`). `rail-unenforced` and `hash-uncovered` both resolve names in
`.harness/RAILS.md`. **Any two of ours that parse the same heading and disagree produce exactly this
failure: one gate demanding what another forbids.** We have already hit it twice: T-024 and T-025
both BLOCKED because `friction-repeat` asked for a LEARNINGS.md line that `eval --gate` refuses to
admit (`PROGRESS.md`, 2026-09-10 entries). That was the same class of bug and we fixed the instance,
not the class. `-> QUEUE: a test asserting that any two probes parsing the same heading agree on
what they parsed.`

---

## 5. `omp.sh`

Fetched https://omp.sh/docs/compaction, /docs/memory and /docs/editing on 2026-09-13 (the site is
client-rendered; read through a browser, not `curl`). We already ship an `omp` preset
(`crates/harness/adapters/presets/omp.toml`), so this is a source we are already downstream of.

### TAKE: the compaction ladder. This is the answer to the screenshot question.

`compaction.methodOrder` default is `[remote, snapcompact, handoff, shake, soft]`, tried in order,
an unavailable or failed method advancing to the next. Before any of them run, omp "first removes
stale file reads, empty searches, and other bulky results that are safe to elide."

The load-bearing distinction, which the "compact every 200k" advice does not make at all:

- **`/shake`** replaces command output, search results, file reads and large fenced blocks with short
  placeholders, **with no summarization request**, and in a persisted session saves the removed
  regions as an artifact whose placeholder carries an `artifact://…` recovery reference. Lossless,
  no model call, recoverable.
- `compaction.supersedeReads: true` elides older copies of a file's contents once the same file is
  read again. Deterministic, free.
- `compaction.dropUseless: true` elides consumed output carrying no context: empty searches,
  timed-out waits.
- Only `soft` summarizes, and it is **last** in the order.

Plus two sizing decisions better than a magic number:

- `thresholdPercent: -1` by default means **reserve-based sizing**: reserve the larger of 16,384
  tokens or 15% of the model window, rather than a fixed trigger.
- `compaction.asyncEnabled: true` prepares the summary shortly *before* the threshold so the commit
  pause is smaller.
- `keepRecentTokens: 20000` verbatim tail.

**The principle: before you summarize, elide.** Most of a long coding session's tokens are
re-derivable tool output, not conversation. Paying a model to lossily describe a file read you could
replace with a pointer is the expensive way to do the cheap thing.

### TAKE: staleness is an error, not a transaction outcome

From /docs/editing, on structural-edit previews: acceptance re-runs the rewrite against current
files rather than replaying old bytes, and if the match set moved, "**Treat a stale result as an
error, not as a successful all-or-nothing transaction.**"

That is our verifier's diff-base problem stated precisely. `roles/verifier.md` step 3 establishes
`$BASE` once and reuses it, which is right, but nothing detects that the tree moved between the
implementer's commit and the verifier's read. The epoch token from section 1 is the mechanism.

### Noted, not taken: memory backends

omp's memory is off by default and offers four backends (`off`, `local`, `mnemopi`, `hindsight`)
with explicit scoping (`global` / `per-project` / `per-project-tagged`) and an unusually blunt
privacy table naming, per backend, exactly what leaves the machine. The framing worth keeping:
"**Recalled or summarized memory is background context, not an instruction.** Current prompts and
current repository state take precedence when they conflict with it." That is the same rule
majiayu000 applies by fencing repo memory as untrusted, and the same rule our `citable` rail
implies. We do not need a memory store: LEARNINGS.md capped at twelve entries, gated by
`harness eval --gate`, is a better version of the same thing, because entry requires a measurement
and ours is the only one of the six that does.

---

## 6. `UditAkhourii/adhd`, `ayghri/i-have-adhd`, `move38studios/thinkfu`

Three prompt libraries. Different authors, same shape. None is a harness.

**`UditAkhourii/adhd`** (read at `16dc239`, 11 `.ts` files, 430 LOC engine). A real implementation:
`src/engine.ts` runs reframe, then N isolated parallel LLM calls each under one of 15 cognitive
frames (`src/frames.ts`), then a separate critic pass that scores novelty/viability/fit, flags
traps, clusters, and deepens top-K. The generator/critic split is mechanical (separate calls,
opposite system prompts), which is a genuine structural claim.

Its eval will not survive our `measure-first` rail. `documentation/evals.md` reports breadth 9.00 vs
4.83, trap detection 9.50 vs 1.83, over **six problems, one run each, no repeats, no variance**, with
an LLM judge from the same model family as the generator. The repo says so itself under "Known
limitations", and `bench/run-evals.ts:main` runs each problem exactly once. The `evals.md` page also
states plainly: "The eval suite is local only. There is no CI workflow for it."

**`ayghri/i-have-adhd`** (read at `6f1f982`). Ten output-formatting rules (lead with the next action,
number steps, cap lists to five, no preamble, no closers) plus install shims for eight agent tools.
`evals/cases.jsonl` and a `scripts/judge.py` exist. This is an output-style skill. It has no
mechanism that a probe could gate.

**`move38studios/thinkfu`** (read at `6a0c830`). 208 moves across five categories (verified:
`find catalog/moves -name "*.md" | wc -l` = 208), ten pools, a Cloudflare Worker serving REST + MCP
+ website, a smart router doing embeddings then an 8B LLM selection
(`api/src/router.ts:6-10`: `embeddinggemma-300m`, `llama-3.1-8b-instruct`, 3 similar + 2 random
candidates). Licensed PolyForm Small Business 1.0.0, which is **not** OSI-approved and is a
commercial-use restriction: it is incompatible with our MIT `LICENSE` if we were ever to vendor it,
and `[[skill]]` vendors sources into the tree. The "seed" mechanism (a random concrete noun appended
unlabelled to every response, as deliberate cognitive perturbation) is the one genuinely novel idea
in the three.

**Verdict: LEAVE all three as harness work.** The honest comparison is against what we already
declare. `harness.default.toml` already carries `[[skill]] id = "brainstorming"` from
`obra/superpowers` at `v6.3.0`, gated on `queue-uncovered`, with the reason "criteria are written
before code, which here happens in the queue and not in a chat". Every one of these three would
enter as a `[[skill]]`, and `[[skill]]` requires a `gate` naming a real gate, probe or rail, with
`skill-ungated` reporting a `gate = "none"` as a finding. We killed exactly this three times already
(T-007, T-008, T-009, all `done`, all "declared with gate: none, nothing fails without it, so
relying on it is a hope").

So the question for each is: **which probe emits a FINDING that stops when this skill is present?**

- `thinkfu`: none. Also a licence conflict.
- `i-have-adhd`: none. Output style for a human reader; our roles report to a launcher.
- `adhd`: arguably `rejection-repeat`, if the adjudicator's kill list widened were the problem.
  It is not. Our scout is forbidden from having ideas by the `anchored` rail
  (`roles/scout.md`: "You transcribe findings; you do not have any"), so a divergent-ideation skill
  attached to the scout would be a rail violation, not an improvement.

The place divergence could legitimately live is the **researcher**, whose job is grounding a
decision someone else made and who already has `WebSearch`/`WebFetch`. But `roles/researcher.md`
opens with "You have no findings and you may not acquire any", and zero rows is explicitly a
success. Divergent ideation is the opposite instruction. Adding it would need a SPEC change, which
per `roles/adjudicator.md` step 4 is a halt for a human, not a skill install.

**If one is worth anything to us it is the shape, not the code:** `UditAkhourii/adhd`'s critic pass
requires a `strength` for **every** idea and a `trap` only where one exists (`src/engine.ts`,
`SCORE_SYSTEM`), i.e. two symmetric signals rather than one. Our verifier produces only the negative
signal. A `VERIFIED` verdict that had to name what the diff got right that an alternative would not
is a cheap change and would make rejections more legible. That is a role-prompt edit, not a
dependency.

---

## 7. Compaction: the claim, checked

The claim under review (screenshots, 2026-09-13): compaction is fixed; session depth is a huge cost
driver because every request re-pays for the whole window; cache writes cost 80x a cache hit;
rehydrating 900k costs about $18 and 32% of a five-hour limit; therefore run `/autocompact 400k` and
manually compact past about 200k.

Pricing verified at https://platform.claude.com/docs/en/about-claude/pricing and
https://platform.claude.com/docs/en/build-with-claude/prompt-caching, both fetched 2026-09-13.

### What is right

**The direction is right and the mechanism is real.** Cache writes are 1.25x base input at the
5-minute TTL and 2x at the 1-hour TTL; cache hits are 0.1x (0.025x on Fable 5.1 / Mythos 5.1). A
cold cache genuinely costs 12.5x to 80x a warm read for the same tokens.

**The best point in the post is the one made almost in passing**, and it is correct: after an idle
period you can `/compact` "without paying extra". You are going to pay one full pass over the
context either way, because the cache is gone. Spending that forced pass on a compaction converts
it into a permanently smaller prefix. Compacting on a **cold** cache is close to free. Compacting on
a **warm** cache is the expensive case, because you discard a warm prefix to write a new one.

**The 1-hour idle figure is right for Claude Code on a subscription**, where the default TTL is one
hour. On an API key or usage credits it is five minutes, and the docs add a trap the post does not
mention: "the lifetime is measured from the start of the request that writes or reads the cache
entry, not from the end of its response", so a four-minute response leaves about one minute.

### What is wrong or unstated

**The 80x and the $18 are both exactly right for one model and wrong by 2x to 6x for everything
else.** Worked:

| Model | 1h cache write | Cache hit | write:hit | 900k rehydrated at 1h |
| --- | ---: | ---: | ---: | ---: |
| Fable 5.1 / Mythos 5.1 | $20/MTok | $0.25/MTok | **80x** | **$18.00** |
| Fable 5 / Mythos 5 | $20/MTok | $1.00/MTok | 20x | $18.00 |
| Opus 5 / Opus 4.8 | $10/MTok | $0.50/MTok | 20x | $9.00 |
| Sonnet 5 | $4/MTok | $0.20/MTok | 20x | $3.60 |
| Haiku 4.5 | $2/MTok | $0.10/MTok | 20x | $1.80 |

At the 5-minute TTL the ratio is 12.5x everywhere except Fable/Mythos 5.1, where it is 50x. **80x
and $18 co-occur only on Fable 5.1 or Mythos 5.1 with a 1-hour cache.** The numbers are internally
consistent; the configuration they belong to is not stated. On Opus 5 the same session rehydrates
for $9 at 20x. The "32% of a five-hour limit" figure cannot be checked: subscription limits are not
published as dollar amounts.

**"EVERY REQUEST pays the price of sending back those 900k tokens" overstates the steady state by
10x.** That is what the cache is for. On Opus 5, 900k warm costs 0.9 x $0.50 = **$0.45 per request**,
against $0.10 for a 200k context. The marginal cost of carrying 700k extra tokens is $0.35 a
request. Over a hundred requests that is $35, which is real but is not the headline. **The cost is
concentrated in the cold-start events, not spread across the requests.** That reframes the rule:
compact before you go idle, not at a token count.

**A fixed 200k trigger is the wrong shape.** Break-even, Opus 5, folding 900k down to 100k on a warm
cache: one full read ($0.45) + ~10k summary output ($0.25) + the new 100k prefix written next
request ($0.63) = about $1.33, against $0.35 a request saved. Roughly four requests. So compaction
pays back fast when many turns remain and never when few do. The trigger should be a function of
turns remaining and the next idle gap, neither of which a token count knows. Claude Code's own
default autocompact window is already model-derived (about 967k on 1M-context models, 200k on
200k-context models), so `/autocompact 400k` is a deliberate move to roughly 40% of the window, not
a magic constant. **Reserve-based sizing, as omp does it (reserve the larger of 16,384 or 15% of the
window), is the defensible version of the same instinct.**

**The unmentioned cost is quality, and for us it outranks the money.** Compaction is lossy and has
no gate. A summary is un-citable by construction: it is a model's paraphrase of evidence, which is
precisely the artifact class our `citable` and `measure-first` rails reject everywhere else. The
dirge checkpoint template exists because they measured paraphrase eroding stated constraints
("'use ESM not CJS' becomes 'discussed module format'", `compression.rs:1160-1210`), and they answer
it by carrying user text **verbatim** alongside the summary rather than trusting the summary.

**And the elide-before-summarize point is missing entirely.** See section 5. Most of a long session
is re-derivable tool output. `supersedeReads`, `dropUseless` and `shake` are free, lossless and
recoverable; `soft` is the last resort, not the first move.

### Verdict

Agree with the mechanism and the practice, reject the rule as stated.

1. **Compact before you go idle, not at a token count.** The cache is about to die; the pass is free.
2. **Compact on a cold cache, never on a warm one mid-task.** This is the post's own best point,
   and it is the inverse of a fixed threshold.
3. **Elide before you summarize.** Stale file reads, superseded reads of the same file, empty
   searches and timed-out waits are removable with no model call and no loss.
4. **Set autocompact as a fraction of the window, not an absolute.** 400k on a 1M window is about
   40%; the same instruction on a 200k-context model is a no-op.
5. **Prefer a structured handoff to a free summary** when the next phase differs from this one.
   dirge's 14 sections and write-once `intent` exist because a generic summary loses the goal.

### What this means for our harness, which is the part that matters

**We mostly do not have this problem, by construction, and we should say why rather than adopt the
advice.**

Every stage is a fresh process with a turn cap (`harness.default.toml`: implement 120, verify 100,
scout 30, adjudicate 40). We never accumulate a 900k session. Our documents **are** our compaction,
and they are strictly better than a summary because every line is re-derivable: TASKS.md is the
queue, PROGRESS.md the per-iteration record with a pasted check, LEARNINGS.md the capped and gated
rule set, DECISIONS.md the archive addressable by sha (`git show <sha>:<path>`). A summary cannot be
re-derived. A `git show` can.

Two real exposures remain:

1. **The implement stage at 120 turns is the one place a single process can go deep.** We cap turns,
   not tokens. `BUDGET_TOKENS` exists but see the finding below.
2. **`stage-outlier` reports a stage over twice its role's median seconds or cost.** Cost is
   captured correctly. Tokens are not.

### FINDING against our own tree

`crates/harness/src/agent.rs:23-28`:

```rust
pub struct UsagePaths {
    pub cost: Option<String>,
    pub input_tokens: Option<String>,
    pub output_tokens: Option<String>,
    pub turns: Option<String>,
}
```

and the `claude` preset (`crates/harness/adapters/presets/claude.toml`) maps only
`usage.input_tokens` and `usage.output_tokens`.

Claude Code's final `result` event reports four token lanes, not two: `input_tokens`,
`output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens` (plus a `cache_creation`
breakdown into `ephemeral_5m_input_tokens` / `ephemeral_1h_input_tokens` under the 1-hour TTL).
Total input is `cache_read + cache_creation + input`
(https://code.claude.com/docs/en/costs.md, fetched 2026-09-13, whose own worked example is
"1.2k input, 5.3k output, 940.0k cache read, 50.0k cache write").

Consequences, both real:

- **`BUDGET_TOKENS` undercounts, by up to three orders of magnitude in a cached stage.** On that
  example it would count 1.2k of 991.2k input tokens. A token budget that cannot be exceeded is not
  a budget.
- **`stage-outlier` cannot see a cache-write spike**, which is exactly the signal that a stage went
  deep or restarted cold. It sees cost, which is correct (`total_cost_usd` is inclusive of cache
  writes and reads), so the dollar budget and the cost half of the probe are sound. The token half
  is not.

This is an `anchored` finding with a command behind it, so it belongs in the queue through the
scout, not here. The fix is four lines in `UsagePaths` plus two in the preset plus the `events.rs`
field, and `codex`/`amp`/`qwen` presets need the same audit.
`-> QUEUE`

---

## 8. Ranked take list

Nothing below is a task until a probe emits it and the adjudicator promotes it.

| # | Take | From | Cost | Why |
| --- | --- | --- | --- | --- |
| 1 | Cache token lanes in `[agent.usage]` | our own tree, via section 7 | hours | `BUDGET_TOKENS` is currently unenforceable |
| 2 | A per-decision trace that records gates which did **not** fire | dirge `--trace` | small | A gate that never fires and a gate that always passes are indistinguishable today |
| 3 | `masks_failure` over `check.command` / `check.force` / `driver_command` | dirge `verifier.rs:773` | small | Our seed learning, in a form a probe can see. All three models tested piped through `tail` |
| 4 | `ClaimTrustLevel` on gate output | majiayu `claim_trust.rs` | medium | Gives `green` and `blocked-is-allowed` a vocabulary instead of prose |
| 5 | Required evidence bound to the status transition, with a retry exemption | majiayu `validator.rs:32` | medium | Turns `roles/adjudicator.md` step 2 from an instruction into a rule |
| 6 | A test asserting any two probes parsing the same heading agree | dirge `dirge-hwk9.3` | small | We have hit this class twice (T-024, T-025) and fixed the instances |
| 7 | Synthetic terminal record from a stage's wait status | dirge `RunEpitaph` | small | Completes what T-020 started |
| 8 | Epoch token over a stage's declared inputs | cordis `fiber.ts:385` | medium | Detects "an input moved under a running stage", which nothing detects now |
| 9 | Zero-output stage detection | majiayu `activity_result.rs:142` | small | "The CLI launched and did nothing" currently reads as a clean stage |
| 10 | Verdict names a strength, not only rejections | UditAkhourii/adhd `SCORE_SYSTEM` | trivial | Role-prompt edit; makes rejections legible |
| 11 | Doc examples as executable tests | majiayu `tests/docs_examples.rs` | small | Our README describes nine gates and executes none of them |
| 12 | `T-###` in the code comment next to what it produced | dirge `.beads` ids | trivial | `git blame` becomes a design record |

Not taken, listed so the decision is on the record: tool-call repair, storm breaking, reflect-and-
pivot, the compaction ladder, capability tiering, Janet plugins, the Starlark exec policy, the
`workflow-first` RFC, `harness-rules` as an enforcement layer, all three prompt libraries as
`[[skill]]` entries, and cordis's effect/inverse machinery.

## 9. Where we actually stand

Against these five harnesses, the three things we do that none of them do:

1. **Roles are separated by construction, not by prompt.** The verifier runs in a fresh process that
   never sees the implementation conversation, and `verifier-not-implementer` names the launcher as
   its enforcement. dirge's critic is a gate inside the same loop. majiayu's reviewer is a role in
   the same session.
2. **Every rail names what enforces it, and `rail-unenforced` fails when the named thing is not
   real.** majiayu has 61 rules across seven files whose enforcement is 17 shell guards with
   mismatched IDs that fail open. dirge has "roughly fifty behavioural guards, all tuned by hand
   against reasoning and anecdote" (`benchmarks/README.md`, their words).
3. **A rule enters LEARNINGS.md only through `harness eval --gate`**, which requires the eval to
   fail without the rule and pass with it. We have refused two rules that way this month (T-024,
   T-025) and recorded the refusals. None of the other five has an admission gate on its own rules.
   dirge's benchmark README says outright: "Until now nothing measured whether any of them help, and
   nothing would notice when one regresses."

And the three things every one of them has that we do not:

1. **A per-decision trace**, including the decisions that produce no output.
2. **A measured run of the harness itself** against a deliberately weak model. dirge's took one
   session and found four bugs.
3. **Correct token accounting.** Ours is broken and we did not know.

The first is the one to do next. The gap is not mechanisms; we have more enforcement per line than
any of them. The gap is that we cannot see our own loop deciding.

---

## Sources

- https://arxiv.org/pdf/2608.25512 (fetched 2026-09-13)
- https://github.com/cordiverse/cordis (read at `f8ea3cd`, 2026-09-13)
- https://deepseek-harness.github.io/deepseek-harness/en/reference/capability-seams (fetched 2026-09-13)
- https://github.com/majiayu000/harness (read at `46ab176`, 2026-09-13)
- https://yogthos.net/posts/2026-06-08-dirge-code.html (fetched 2026-09-13) and https://github.com/dirge-code/dirge (read at `83504aa`, v0.25.5, 2026-09-13)
- https://omp.sh/ and /docs/{compaction,memory,editing} (fetched 2026-09-13)
- https://github.com/ayghri/i-have-adhd (read at `6f1f982`, 2026-09-13)
- https://github.com/UditAkhourii/adhd (read at `16dc239`, 2026-09-13)
- https://github.com/move38studios/thinkfu (read at `6a0c830`, 2026-09-13)
- https://platform.claude.com/docs/en/about-claude/pricing and /build-with-claude/prompt-caching (fetched 2026-09-13)
- https://code.claude.com/docs/en/costs.md, /prompt-caching.md, /context-window.md, /model-config.md (fetched 2026-09-13)
