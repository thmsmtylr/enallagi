# DECISIONS

Rejected findings first, then archived blocks.

`## Rejected findings` is every proposal the adjudicator killed, one line each, with the command
that refutes it. Read it with `sed -n '/^## Rejected findings/,/^## \[T-/p' DECISIONS.md` and
never read past that range.

Below it, completed task blocks, verbatim, moved out of TASKS.md once `done` by
`harness run`. Each block is the implementer's and the verifier's own words, never summarised on
the way in.

## Rejected findings

<!-- - [YYYY-MM-DD] <the claim in one sentence> — refuted by `<command>`: <the output that refutes it> -->
- [2026-09-08] `.claude/skills/ponytail/SKILL.md:64` is a ponytail marker with no kill line naming its text — refuted by `sed -n 64p .claude/skills/ponytail/SKILL.md`: the line is "- Mark deliberate simplifications with a `ponytail:` comment (`// ponytail: this exists`), simple reads as intent, not ignorance. ...", the vendored skill's own prose describing the marker in a file harness.lock pins, not a shortcut taken
- [2026-09-08] `README.md:127` is a ponytail marker with no kill line naming its text — refuted by `sed -n 127p README.md`: the line is the probe table's own row, "| `ponytail-ceiling` | a `ponytail:` marker in code with no dated kill line already naming its text |", which describes the probe rather than taking a shortcut
- [2026-09-08] `crates/harness/src/agent.rs:377` is a ponytail marker with no kill line naming its text — refuted by `sed -n '377,379p' crates/harness/src/agent.rs`: "// ponytail: a timed-out child can leave a grandchild holding the pipe, so the readers are joined only on a clean exit; upgrade to a process-group kill if a preset turns out to orphan writers on a clean exit too" names its ceiling and its upgrade condition, and the condition waits on a measurement nobody has taken (`measure-first`)
- [2026-09-08] `crates/harness/src/probes/check_unnamed.rs:3` is a ponytail marker with no kill line naming its text — refuted by `sed -n 3p crates/harness/src/probes/check_unnamed.rs`: "//! ponytail: the context file only; widen to the spec or LEARNINGS.md once a second document is measured to have drifted." names its ceiling and gates the upgrade on a measurement not yet taken (`measure-first`); `harness probe` → `PROBE check-unnamed 0`
- [2026-09-08] `crates/harness/src/probes/friction_repeat.rs:29` is a ponytail marker with no kill line naming its text — refuted by `sed -n 29p crates/harness/src/probes/friction_repeat.rs`: "// ponytail: each group compares only to its first member, so an A-B-C chain whose ends don't overlap stays two groups" names its ceiling, and no chain has split in practice: `harness probe` → `PROBE friction-repeat 0`
- [2026-09-08] `docs/intent.md:171` is a ponytail marker with no kill line naming its text — refuted by `sed -n '170,171p' docs/intent.md`: the lines are the paragraph "Stop `ponytail-ceiling` counting its own documentation", which records this very class of hit — the probe firing on the text that describes it — and take no shortcut
- [2026-09-13] `crates/harness/src/probes/friction_repeat.rs:66` is a ponytail marker with no kill line naming its text — refuted by `sed -n 66p crates/harness/src/probes/friction_repeat.rs`: "// ponytail: coverage is per-line word-set containment; a rule or kill line that paraphrases every word escapes it" names its ceiling, and no paraphrase has escaped it in practice: `cargo run -q -p harness -- probe` → `PROBE friction-repeat 0`
