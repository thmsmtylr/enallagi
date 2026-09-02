# Adapter: Claude Code

`./install.sh /path/to/repo --adapter claude` adds two things the base install leaves out, because
neither mechanism is portable:

| File | Why it is an adapter, not core |
| --- | --- |
| `.claude/agents/*.md` | Subagents exist for Claude Code, Copilot (`.github/agents/`) and Cursor (`.cursor/agents/`) and **do not exist for Codex or Gemini** (arXiv:2602.14690, Table 1). The launcher does not need them: it gets role isolation from one fresh agent *process* per stage and points that process at `.harness/roles/<role>.md`. These are copies, for when you want to drive a role by hand with the Task tool. |
| `.claude/settings.json` | Hooks are configured per tool — Claude and Gemini in `settings.json`, Copilot in `.github/hooks/*.json`, Cursor in `hooks.json`, and **Codex has none** (same table). The `Stop` hook is belt-and-braces here: `loop.sh`'s own `gate_verdict` re-runs the gate behind every verdict, so an agent with no hook support loses nothing that decides a task. |

The `PreToolUse` immutable hook is worth having where you can get it: it turns an edit to a frozen
file into a refusal in-session rather than a diff someone has to notice.

If you use another tool, point its hook mechanism at the same two scripts:
`.harness/hooks/verify-done.sh` (Stop) and `.harness/hooks/immutable.sh` (before an edit).
