# Adapter: bun + turborepo

Install with `./install.sh /path/to/repo --adapter bun-turbo`. It adds:

| File | What it is |
| --- | --- |
| `check.ts` | the five-stage floor: `precheck` (hash-verify immutables, grep the forbidden list) → `typecheck` → `lint` (with a complexity ceiling and a file-length cap) → `test` (JUnit out, fixed seed) → `trace` (every exit-criteria row maps to a testcase that ran and asserted). Also `bun check.ts hash` to re-cut `test-hashes.json`. |
| `.claude/hooks/check-covered.sh` | the build-system-aware gate. Reads turbo's dry-run plan, discards `<NONEXISTENT>` tasks, and **fails any changed file no executed task covers** — the zero-as-pass mitigation the portable `check-gate.sh` cannot do. `verify-done.sh` prefers it when present. |
| `.claude/hooks/typecheck-changed.sh` | PostToolUse fast typecheck after an edit. Wire it yourself, it is not in the base settings snippet. |

After installing, in the target repo:

```jsonc
// package.json
"scripts": { "check": "bun check.ts" }

// turbo.json — every document a test reads goes here or the cache serves a green nobody ran
"globalDependencies": [".claude/hooks/**", "SPEC.md"]

// .claude/settings.json — add if you want the PostToolUse typecheck
{ "matcher": "Edit|Write",
  "hooks": [{ "type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/typecheck-changed.sh" }] }
```

`check.ts` carries this repo's own constants — `SPEC_ROW_COUNT`, `MAX_LINES`, `MAX_COMPLEXITY`,
the section heading it slices, the seed. Read it once and set them; they are the four numbers that
make it yours. It is the one file in the package that is not tokenised, because a floor you have
not read is a floor you do not have.
