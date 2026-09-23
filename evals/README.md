# evals

One directory per eval. Each is a fixture repo, a request, and an assertion:

| File | |
| --- | --- |
| `setup.sh` | builds the fixture state, cwd is a throwaway repo with enallagi installed |
| `prompt.txt` | the request, in the shape `enallagi run` sends it, absent when the eval asks no role anything |
| `assert.sh` | exits 0 when the role obeyed its rule |
| `ablate.sh` | removes the rule from the fixture, so the gate can tell a load-bearing rule from a decorative one |

```bash
enallagi eval                 every eval
enallagi eval verifier        one
enallagi eval --gate <name>   decide a candidate rule
```

`--gate` is the write-path check on a new `__ENALLAGI_DIR__/LEARNINGS.md` rule. It accepts only when the eval passes
with the rule, fails with the rule ablated, and every other eval still passes.

The agent comes from `EVAL_AGENT`, else `__ENALLAGI_DIR__/enallagi.toml`'s `[agent]`. With neither, `enallagi eval`
refuses rather than reporting a result it did not measure.

`verifier-findings` builds a JavaScript fixture and needs `bun` on `PATH`. Without it the
`setup.sh` exits non-zero and the eval reports `ERROR`, not a result.

## cold-start

`cold-start` is the only eval with no `prompt.txt`. It builds an ordinary repository the harness did
not grow up in. It then runs `enallagi init` against it with no hand editing, and asserts
`enallagi probe` reports zero findings.

It spawns no agent and reaches no network. `EVAL_AGENT` changes nothing about its result.

A failure prints one `cold-start <probe> <count>` line per probe that fired, then
`cold-start total <n>`. That total is the measure: every finding is the harness reporting a healthy
tree as unhealthy. The eval passes when the total is zero.

It counts nothing until it establishes that `enallagi probe` ran. A non-zero exit, no `PROBE` line,
or a `PROBE <name> ERROR` line each fail it before any total is printed.

A probe that reports `OFF` never looked, so it adds nothing to the total, and the eval prints
`cold-start off <count> <names>` before the total so the zero says which probes it covers. A run
in which every probe reports `OFF` fails, because nothing was measured.

## A REJECT

Run against `claude -p` on 2026-09-02, `--gate verifier` rejected the rule it was holding:

```
GATE verifier REJECT the case passes with the rule ablated, so the rule changed no outcome
```

The rule was `verifier.md` step 0, which says uncommitted source is a rejection. With it removed,
the model still rejected the uncommitted task. On that model and that fixture the line is not
load-bearing; one sample is not a reason to delete it.
