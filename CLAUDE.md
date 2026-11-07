# enallagi

## Commands

- Check: `cargo test --workspace --no-fail-fast -q && cargo clippy --all-targets -q -- -D warnings && cargo fmt --all --check`
- Loop rules, when the tree carries them: `.enallagi/AGENTS.md`

## Comments

A comment says what the code cannot. Write one only for:

- a trap that already caught someone, so it is not hit twice
- code that is hard to follow without the reason behind it

Each comment is one line, a lowercase fragment, under 100 characters.
It states the fact, never the story behind it.

Never in a comment:

- an ADR, spec, task id, pull request, issue or date
- a doc path or a "see X" reference
- a restatement of the name, the signature or the next line
- a paragraph, or a sentence about what the work means

A module gets one `//!` line or none. A `///` doc comment gets one line, and only on a public item whose name leaves a question open.

The one exception: a `ponytail:` marker naming a deliberate ceiling and its upgrade path.

```rust
// bad: the launcher, not a lane, pushes and calls GitHub (ADR 0004)
// bad: renamed into place, so that a reader of the state file never sees a half-written file
// good: renamed into place; a reader never sees half a file
```

## Names

- A function or test name is the behaviour in one to four words: `unfilled_placeholder`, not `a_ready_block_with_a_placeholder_is_unfilled`.
- No identifier runs past 50 characters (`floor.rs::no_identifier_runs_past_fifty_characters`).
- New code reads like the file around it.

## Commits

- The subject names what changed, in one line.
- No co-author or session trailer.
