# evals

One directory per eval. Each is a fixture repo, a request, and an assertion:

| File | |
| --- | --- |
| `setup.sh` | builds the fixture state, cwd is a throwaway repo with the harness installed |
| `prompt.txt` | the request, in the shape `harness run` sends it |
| `assert.sh` | exits 0 when the role obeyed its rule |
| `ablate.sh` | removes the rule from the fixture, so the gate can tell a load-bearing rule from a decorative one |

```bash
harness eval                 every eval
harness eval verifier        one
harness eval --gate <name>   decide a candidate rule
```

`--gate` is the write-path check on a new `LEARNINGS.md` rule. It accepts only when the eval passes
with the rule, fails with the rule ablated, and every other eval still passes.

The agent comes from `EVAL_AGENT`, else `harness.toml`'s `[agent]`. With neither, `harness eval`
refuses rather than reporting a result it did not measure.

## A REJECT

Run against `claude -p` on 2026-09-02, `--gate verifier` rejected the rule it was holding:

```
GATE verifier REJECT the case passes with the rule ablated, so the rule changed no outcome
```

The rule was `verifier.md` step 0, which says uncommitted source is a rejection. With it removed,
the model still rejected the uncommitted task. On that model and that fixture the line is not
load-bearing; one sample is not a reason to delete it.
