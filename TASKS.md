# TASKS

The queue. One block per task, in the order they were written. The loop takes the first `ready`
block whose blockers are all `done` and which is not `attended: true`.

Statuses: `proposed` (the scout's, inert to the loop) · `ready` · `blocked` · `review`
(the implementer's last act) · `done` (only the verifier's, and the launcher re-runs the gate
behind it) · `needs-spec` (the contract does not answer a question the task hit).

Finished blocks archive to DECISIONS.md — `.harness/archive-done.sh` moves them at the top of each
iteration, leaving a stub with the fields the loop still reads. Keep this file small: every
process an iteration spawns re-reads all of it.

## Block format

```
## [T-###] one line, in the finding's own words
scope: src/thing.ts, src/thing.test.ts
blockedBy:
status: ready
rows: <the exit-criteria rows this turns green, character for character, or `none — harness`>
criteria:
  - <objective, and naming the command whose output changes when it is done>
notes: <what a reviewer should scrutinise; the implementer's and the verifier's own words>
```

`T-###` above is written unnumbered on purpose: `## [T-<digits>]` is the exact shape
`loop.sh`'s `ready_unattended` scans for, and it does not know this one is inside a code fence —
a numbered example here is a task the loop will take. `blockedBy:` is empty, `none`, or a
comma-separated list of ids. `attended: true` marks a task
needing a human credential and no launcher auto-selects it.

---

## [T-001] a repeated friction becomes a rule with nothing checking the rule
scope: evals/**, selftest.sh, README.md
blockedBy: none
status: ready
rows: `selftest.sh::a candidate rule that does not fix its case is rejected`, `selftest.sh::a candidate rule whose case passes without it is rejected`, `selftest.sh::a candidate rule that regresses another eval is rejected`, `selftest.sh::a candidate rule that fixes its case and regresses nothing is accepted`
criteria:
  - `evals/run.sh --gate <eval>` decides one candidate rule and prints `GATE <eval> ACCEPT` or
    `GATE <eval> REJECT <reason>`, exiting non-zero on a reject.
  - It rejects when the eval fails with the rule in place (the rule does not fix its own case).
  - It rejects when the eval passes with the rule ablated (the case did not need the rule).
  - It rejects when any other eval that was passing now fails.
  - Ablation is per-eval: an `ablate.sh` in the eval directory removes the rule from the fixture.
  - `./selftest.sh` declares all four rows' assertions and passes.
notes: |
  Grounding, fetched 2026-09-02:
  - GRASP (arXiv:2605.29668) admits a candidate only on `(F(c)-F0)-(R(c)-R0)>0 and R(c)<=R0`, a
    hard regression budget rather than a soft tradeoff, measured on a balanced held-out probe of
    up to N/2 previously-failing and N/2 previously-passing samples (N=36, an 18-18 split).
  - GSE (arXiv:2608.06153) uses two stages: "Proposals that fail to resolve their originating
    failure are discarded immediately", then replay validation where "a candidate skill bank is
    accepted only if it maintains or improves performance across the replay cases".
  - Honest Lying (arXiv:2605.29463) is the negative result: reflective memory made two ALFWorld
    environments strictly worse (env_31 7 trials with memory vs 1 without, env_97 8 vs 1), and
    "0 of 121 reflections mention the correct target object". Their recommendation is that
    "write-path validation is as important as retrieval quality".
  The ablation step is ours, not theirs: it tests the converse of GSE's local validation. GRASP and
  GSE ask whether the case now passes; neither asks whether it would have passed anyway.

## [T-002] nothing bounds the rule library or checks a rule has an eval
scope: harness/hooks/probes.sh, harness.default.json, templates/LEARNINGS.md, templates/RAILS.md, selftest.sh, README.md
blockedBy: none
status: ready
rows: `selftest.sh::a dated learning with no eval is reported`, `selftest.sh::a learnings file over its cap is reported`
criteria:
  - A `learning-ungated` probe reports a dated LEARNINGS.md entry that names no `evals/<dir>`, or
    names one that does not exist. Entries marked `[seed]` predate the gate and are not reported.
  - The same probe reports a LEARNINGS.md carrying more entries than `learningsCap` in harness.json.
  - `./selftest.sh` declares both rows' assertions and passes.
notes: |
  Grounding, fetched 2026-09-02:
  - GRASP keeps a capacity-bounded library, 10 skills in its experiments, where "ADD operations are
    blocked at capacity unless a paired REMOVE frees a slot". Its ungated baseline regressed below
    the no-skills baseline as memory accumulated (41.2% vs 40.6% on MedAgentBench).
  - ACE (arXiv:2510.04618) measures the accumulation failure directly: at one step the context held
    18,282 tokens at 66.7% accuracy and "at the very next step it collapsed to just 122 tokens, with
    accuracy dropping to 57.1", against a 63.7% no-adaptation baseline.
  - Gloaguen et al. (arXiv:2602.11988), already cited in AGENTS.md, measured context files raising
    inference cost by over 20%, with LLM-generated ones costing about 3% of success rate.
  The cap is 12 by default rather than GRASP's 10: LEARNINGS.md entries are one line, not a skill
  document with four sections, and the harness ships four seeds.
