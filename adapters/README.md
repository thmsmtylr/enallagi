# Adapters

The core install is tool-agnostic. `harness init --adapter <preset>` names one of the thirteen
agent presets and adds the parts that are tool-specific: `.claude/agents/` and
`.claude/settings.json` hook wiring for `claude`; for a preset whose `hooks_file` is set
(`codex`, `gemini`, `copilot`, `cursor`, `qwen`), that file, merged with one that already exists;
for any other preset, nothing.

`bun-turbo` is not an agent and not a `harness init --adapter` value. Its files
(`adapters/bun-turbo/`: the five-stage `check.ts` floor and the turbo-aware coverage gate) are
copied in by hand — see `adapters/bun-turbo/README.md` — and wired up as `check.command`.

## Configuring another agent

`[agent]` in `harness.toml` names the preset: `claude`, `codex`, `gemini`, `opencode`, `copilot`,
`goose`, `aider`, `amp`, `cursor`, `kimi`, `qwen`, `omp`, `pi`, or `custom` with an explicit `argv`.
Each preset (`crates/harness/adapters/presets/*.toml`) carries its own `argv`, `turn_cap` and,
where the agent has one, `instruction_file` and `hooks_file`; `{prompt}` and `{turns}` are filled
in per stage. `[agent.<role>]` overrides the preset for one role.

Set `agent.rate_limit_pattern` to whatever your agent prints when it runs out of quota; the loop
waits for the reset rather than counting an exhausted agent as a finished iteration.

## Skills

Skills are installed **per tool, not per repository** — Superpowers states it plainly: "Installation
differs by harness. If you use more than one, install Superpowers separately for each one."
`layout.skills_dir` in `harness.toml` decides where this project's own skill lands; unset, it
falls back to the preset's own `skills_dir` (`claude`: `.claude/skills`, `kimi`: `.agents/skills`,
`opencode`: `.opencode/skills`, `pi`: `.pi/skills`), and then to `<harness_dir>/skills` for a
preset that names none.
