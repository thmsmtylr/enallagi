# <project> — specification

## 0. How to use this file — the loop contract

This section is the contract between this file and the agent that builds from it. It binds before
anything below it.

### 0.1 Verify-before-build

Whatever this project rests on that could have moved since it was written — a protocol revision, an
SDK constant, an API shape — is checked here **before any code is written**, and the loop stops if
the check fails. Record each result with the date you checked it.

### 0.2 Rails the loop runs in

| Rail | Rule | Why |
| --- | --- | --- |
| `tests-immutable` | Every file named in §11 is immutable. The check compares each against a committed SHA-256 in `test-hashes.json` and a `PreToolUse` hook refuses the edit in-session — those **detect and make visible**; they are not authority. **The authority is the verifier**, in a fresh session, reading `git diff $BASE -- '<test glob>'` and `test-hashes.json` together: a re-cut key that does not correspond to a file on the task's `scope:` line is a rejection. | Read-only test files are the measured mitigation with the least performance cost (ImpossibleBench, [arXiv:2510.20270](https://arxiv.org/html/2510.20270v1)). Prose does not work: METR measured "please do not reward hack" failing in **70–95%** of attempts ([METR, 5 Jun 2025](https://metr.org/blog/2025-06-05-recent-reward-hacking/)). |
| `harness-immutable` | **`__ENALLAGI_DIR__/enallagi.toml`**, the file that names the check, is covered by the same hashes. | The three hacks Anthropic found in its own production RL environments were `AlwaysEqual`, `sys.exit(0)` before assertions, and a `conftest.py` monkey-patch ([arXiv:2511.18397](https://arxiv.org/html/2511.18397v1)). Every runtime has analogues. |
| `one-row` | One §11 row per iteration. Commit, then write what happened to `__ENALLAGI_DIR__/PROGRESS.md`. Re-read its tail, this file and `git log --oneline -20` at the start of every iteration. | Per-bug accuracy falls **58.9% → 36.5%** when an agent inherits its own prior state rather than a clean one (ChainSWE, via [arXiv:2607.27283](https://arxiv.org/html/2607.27283v1)); multi-turn degradation averages **39%** and "when LLMs take a wrong turn… they get lost and do not recover" ([Laban et al., arXiv:2505.06120](https://arxiv.org/abs/2505.06120)). |
| `minutes-not-hours` | No row in §11 may be more than ~30 minutes of human-equivalent work. Split it if it is. | Agent success decays exponentially with task length at a constant hazard rate: **T₉₀ ≈ ⅐ T₅₀**, **T₉₉ ≈ 1/70 T₅₀** ([Ord, arXiv:2505.05115](https://arxiv.org/pdf/2505.05115)). At a 320-minute 50%-horizon ([METR TH1.1, 29 Jan 2026](https://metr.org/blog/2026-1-29-time-horizon-1-1/)), 90% reliability means ~45-minute units. |
| `blocked-is-allowed` | A row may be marked `BLOCKED` in `__ENALLAGI_DIR__/PROGRESS.md` with a written reason and the loop stops. This is a success, not a failure. A row may **never** be marked done without the exact command and its pasted output. | An abort affordance cut GPT-5's cheating **54% → 9%** ([ImpossibleBench](https://arxiv.org/html/2510.20270v1)). An agent with no exit but "pass" will manufacture a pass. |
| `no-clarification-left` | If any `[NEEDS CLARIFICATION]` marker exists anywhere in this file, the loop does not start. The launcher ignores backticked mentions like this one, so writing about the marker is safe; a bare one halts the run. | Spec Kit's mechanism ([spec-driven.md](https://github.com/github/spec-kit/blob/main/spec-driven.md)) — vendor practice, motivated by the measured early-assumption-lock finding above. |
| `gated-rules` | `__ENALLAGI_DIR__/LEARNINGS.md` holds the seed rules the install shipped. A rule the loop earned is a dated line under `## Earned rules` in `__ENALLAGI_DIR__/DECISIONS.md`, written by the adjudicator, and `enallagi probe learning-ungated` reports a dated line left in the seed library. | `enallagi init` seeds both files and keeps either one that already has content (`crates/harness/src/init.rs:400`), so neither outlives the other. The store is the one a named role writes. The adjudicator writes `## Earned rules` and no role writes the seed library, which stays what the binary shipped. |
| `verifier-not-implementer` | Final acceptance runs in a fresh session that sees only the diff and §11. It never sees the implementation conversation. | An agent-written suite the same agent implements against measures self-consistency, not correctness: SpecBench measured **43–48pp** visible-vs-held-out gaps for Claude Code, growing **~27pp per 10× LOC** ([arXiv:2605.21384](https://arxiv.org/pdf/2605.21384)). |

### 0.3 Forbidden by name

Each of these is a rejection when the verifier finds it in a task's diff. Adapt the list to your
runtime. The taxonomy is what ports, not the syntax.

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

`__CHECK__` is the one command this project already runs. Done means it exits 0. The harness
supplies no stages of its own: whatever this command runs is what you wrote into it.

Read its result on delta against `__ENALLAGI_DIR__/.check-baseline`. A failure listed there is
inherited. A failure not listed there is a rejection, and that file only ever shrinks.

---

## 11. Exit criteria

Every row is one behaviour, named by the test that proves it. The loop turns rows green one at a
time and `enallagi probe spec-untested` reports every row whose test does not exist. A test name
here carries no backtick, and the file name before `::` is matched against the test-file suffix
`__ENALLAGI_DIR__/enallagi.toml` configures. The heading above and its terminator are what that
file points the probes at — rename it there if you rename it here.

The table starts empty, since a row nothing has earned is a finding and not a placeholder. A row
is two cells, the behaviour and the backticked test:
`<what must be true>` beside `src/thing.test.ts::a name copied from your suite`.

| Behaviour | Test |
| --- | --- |

## 12. Out of scope
