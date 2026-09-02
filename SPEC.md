# harness — specification

This package is a starter autonomous goal loop: `install.sh` writes a launcher, four role prompts,
twelve probes and a document set into any git repository. It is dogfooding itself here — the
artifact under test is the installed harness, and the floor is `./selftest.sh`.

## 0. How to use this file — the loop contract

This section is the contract between this file and the agent that builds from it. It binds before
anything below it.

### 0.1 Verify-before-build

Whatever this project rests on that could have moved since it was written — a protocol revision, an
SDK constant, an API shape — is checked here **before any code is written**, and the loop stops if
the check fails. A spec that assumes a version it has not confirmed produces software nobody can
call. Record each result with the date you checked it.

### 0.2 Rails the loop runs in

| Rail | Rule | Why |
| --- | --- | --- |
| `tests-immutable` | Every file named in §11 is immutable. The check compares each against a committed SHA-256 in `test-hashes.json` and a `PreToolUse` hook refuses the edit in-session — those **detect and make visible**; they are not authority. The check runs as the same principal as the lane, over data the lane can write, with no secret, so any predicate it evaluates the lane can satisfy. **The authority is the verifier**, in a fresh session, reading `git diff $BASE -- '<test glob>'` and `test-hashes.json` together: a re-cut key that does not correspond to a file on the task's `scope:` line is a rejection. The hash still earns its place — it forces the tamper to touch a second file and land as a loud line in the diff the verifier reads. | Read-only test files are the measured mitigation with the least performance cost (ImpossibleBench, [arXiv:2510.20270](https://arxiv.org/html/2510.20270v1)). Prose does not work: METR measured "please do not reward hack" failing in **70–95%** of attempts ([METR, 5 Jun 2025](https://metr.org/blog/2025-06-05-recent-reward-hacking/)). |
| `harness-immutable` | The build config, every preload script, the package scripts, the check script **and the loop script itself** are covered by the same hashes. | The three hacks Anthropic found in its own production RL environments were `AlwaysEqual`, `sys.exit(0)` before assertions, and a `conftest.py` monkey-patch ([arXiv:2511.18397](https://arxiv.org/html/2511.18397v1)). Every runtime has analogues. |
| `one-row` | One §11 row per iteration. Commit, then write what happened to `PROGRESS.md`. Re-read its tail, this file and `git log --oneline -20` at the start of every iteration. | Per-bug accuracy falls **58.9% → 36.5%** when an agent inherits its own prior state rather than a clean one (ChainSWE, via [arXiv:2607.27283](https://arxiv.org/html/2607.27283v1)); multi-turn degradation averages **39%** and "when LLMs take a wrong turn… they get lost and do not recover" ([Laban et al., arXiv:2505.06120](https://arxiv.org/abs/2505.06120)). |
| `minutes-not-hours` | No row in §11 may be more than ~30 minutes of human-equivalent work. Split it if it is. | Agent success decays exponentially with task length at a constant hazard rate: **T₉₀ ≈ ⅐ T₅₀**, **T₉₉ ≈ 1/70 T₅₀** ([Ord, arXiv:2505.05115](https://arxiv.org/pdf/2505.05115)). At a 320-minute 50%-horizon ([METR TH1.1, 29 Jan 2026](https://metr.org/blog/2026-1-29-time-horizon-1-1/)), 90% reliability means ~45-minute units. |
| `blocked-is-allowed` | A row may be marked `BLOCKED` in `PROGRESS.md` with a written reason and the loop stops. This is a success, not a failure. A row may **never** be marked done without the exact command and its pasted output. | An abort affordance cut GPT-5's cheating **54% → 9%** ([ImpossibleBench](https://arxiv.org/html/2510.20270v1)). An agent with no exit but "pass" will manufacture a pass. |
| `no-clarification-left` | If any `[NEEDS CLARIFICATION]` marker exists anywhere in this file, the loop does not start. The launcher ignores backticked mentions like this one, so writing about the marker is safe; a bare one halts the run. | Spec Kit's mechanism ([spec-driven.md](https://github.com/github/spec-kit/blob/main/spec-driven.md)) — vendor practice, motivated by the measured early-assumption-lock finding above. |
| `verifier-not-implementer` | Final acceptance runs in a fresh session that sees only the diff and §11. It never sees the implementation conversation. | An agent-written suite the same agent implements against measures self-consistency, not correctness: SpecBench measured **43–48pp** visible-vs-held-out gaps for Claude Code, growing **~27pp per 10× LOC** ([arXiv:2605.21384](https://arxiv.org/pdf/2605.21384)). |

### 0.3 Forbidden by name

The check greps for each of these and fails on a hit. They are not hypothetical; each was measured
in a production or benchmark setting. Adapt the list to your runtime — the taxonomy is what ports,
not the syntax.

- Editing any file listed in §11 or in §0.2 `harness-immutable`.
- Process exit calls anywhere under the source or test tree.
- Module mocking of a source module, or of the test framework itself.
- Custom equality, serialisation or coercion overrides on domain types used to satisfy an assertion.
- Branching on a fixture value, or on a test-environment variable, inside source.
- Focused, skipped or todo tests in any file named in §11.

Taxonomy from ImpossibleBench's four categories ([arXiv:2510.20270](https://arxiv.org/html/2510.20270v1)),
Anthropic's production findings ([arXiv:2511.18397](https://arxiv.org/html/2511.18397v1)), METR's
timer-patching and stack-walking ([METR](https://metr.org/blog/2025-06-05-recent-reward-hacking/)),
and NIST CAISI's "removing pre-existing checks in the code" ([NIST, 28 Nov 2025](https://www.nist.gov/caisi/cheating-ai-agent-evaluations/2-examples-cheating-caisis-agent-evaluations)).

### 0.4 What the check is

`./selftest.sh` runs these stages in order and fails closed on the first:

```
precheck   → hash-verify every immutable file
             grep the §0.3 list
typecheck  → the language's own type gate
lint       → plus a complexity ceiling and a file-length cap
test       → machine-readable output, fixed seed
trace      → parse §11 and the test output; assert every row maps to a test that
             ran, carries at least one assertion, and is not skipped or todo
```

`trace` is a **step after the tests, not a test inside them**, because a suite cannot testify that
it ran. Test names in §11 are constrained to `[A-Za-z0-9 _:-]` so that name-filtering, which usually
takes a regex, cannot misfire.

The `lint` structural gate is not decoration. Agent code shows structural erosion rising in **77%**
of trajectories and verbosity in **75.5%**, at **2.3× the verbosity and 2.0× the erosion** of human
repos, accumulating **5–6.6× faster per checkpoint** — all while passing the tests
([SlopCodeBench, arXiv:2603.24755](https://arxiv.org/pdf/2603.24755)).

---

## 11. Exit criteria

Every row is one behaviour, named by the test that proves it. The loop turns rows green one at a
time and `trace` refuses a row whose test did not run. The heading above and its terminator are
what `harness.json` points the probes at — rename it there if you rename it here.

| Behaviour | Test |
| --- | --- |
| A driver reaches the built artifact and reports shortfalls as findings, never a pass or a fail | `selftest.sh::the package driver reports shortfalls as FINDING lines` |
| Every install carries a runnable example driver, so `driverCommand` has somewhere to point | `selftest.sh::an example driver installs into the harness directory` |
| A lane runs in its own worktree and the parent checkout is untouched while it works | `selftest.sh::a worktree lane leaves the parent checkout untouched` |
| A lane whose branch cannot fast-forward is left for a human, never force-merged | `selftest.sh::a lane that cannot fast forward is left for a human` |
| A role prompt that obeys its own hard rule passes its eval | `selftest.sh::the eval runner passes a role that obeys its rule` |
| A role prompt that breaks its own hard rule fails its eval | `selftest.sh::the eval runner fails a role that breaks its rule` |

## 12. Out of scope
