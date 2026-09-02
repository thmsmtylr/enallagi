# evals

One directory per eval. Each is a fixture repo, a request, and an assertion:

| File | |
| --- | --- |
| `setup.sh` | builds the fixture state, cwd is a throwaway repo with the harness installed |
| `prompt.txt` | the request, in the shape `loop.sh` sends it |
| `assert.sh` | exits 0 when the role obeyed its rule |
| `ablate.sh` | removes the rule from the fixture, so the gate can tell a load-bearing rule from a decorative one |

```bash
./evals/run.sh                 every eval
./evals/run.sh verifier        one
./evals/run.sh --gate <name>   decide a candidate rule
```

`--gate` is the write-path check on a new `LEARNINGS.md` rule. It accepts only when the eval passes
with the rule, fails with the rule ablated, and every other eval still passes. A rule that changes
no outcome is context that costs and buys nothing; a rule that breaks another eval costs more than
it buys.

The agent comes from `EVAL_AGENT`, else `harness.json`'s `agentCommand`. With neither, `run.sh`
refuses rather than reporting a result it did not measure.
