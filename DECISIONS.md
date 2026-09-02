# DECISIONS

Two things, in this order, because the top of this file is read far more often than the rest.

`## Rejected findings` is every proposal the adjudicator killed, one line each, with the command
that refutes it. Read it with `sed -n '/^## Rejected findings/,/^## \[T-/p' DECISIONS.md` and
never read past that range.

Below it, completed task blocks, verbatim, moved out of TASKS.md once `done` by
`.harness/archive-done.sh`. The queue stays small; the audit trail stays whole. Each block is the
implementer's and the verifier's own words, never summarised on the way in.

## Rejected findings

<!-- - [YYYY-MM-DD] <the claim in one sentence> — refuted by `<command>`: <the output that refutes it> -->
