# Adapters

The core install is tool-agnostic. Adapters add the parts that are not.

| Adapter | Adds |
| --- | --- |
| `claude` | `.claude/agents/` role copies and `.claude/settings.json` hook wiring |
| `bun-turbo` | the five-stage `check.ts` floor and the turbo-aware coverage gate |

## Configuring another agent

Only `agentCommand` in `harness.json` has to change. It is a word list; `{prompt}` and `{turns}`
are filled in per stage.

| Agent | `agentCommand` |
| --- | --- |
| Claude Code | `["claude","-p","{prompt}","--dangerously-skip-permissions","--max-turns","{turns}"]` |
| Codex CLI | `["codex","exec","{prompt}","--sandbox","workspace-write"]` |
| Gemini CLI | `["gemini","-p","{prompt}","--yolo"]` |
| opencode | `["opencode","run","{prompt}"]` |
| Copilot CLI | `["copilot","-p","{prompt}"]` |
| Goose | `["goose","run","-t","{prompt}"]` |
| Aider | `["aider","--message","{prompt}","--yes"]` |

Set `rateLimitPattern` to whatever your agent prints when it runs out of quota; the launcher waits
for the reset rather than counting an exhausted agent as a finished iteration.

Not every agent takes a turn limit. If yours does not, drop `{turns}` from the list — the launcher
substitutes what is there and passes the rest through untouched.

## Skills

Skills are installed **per tool, not per repository** — Superpowers states it plainly: "Installation
differs by harness. If you use more than one, install Superpowers separately for each one."
`skillsDir` in `harness.json` is where this project's own skill lands: `.claude/skills`,
`.codex/skills`, `.gemini/skills`, `.cursor/skills` or `.github/skills`. All five tools studied
support Skills, which makes it the one extension mechanism that is genuinely portable
(arXiv:2602.14690, Table 1).
