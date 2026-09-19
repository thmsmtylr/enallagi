# Configuration

`.enallagi/enallagi.toml` merges over the embedded defaults in `crates/harness/harness.default.toml`.
Tables merge key by key. Arrays, and arrays of tables, replace the default whole.
A key left out takes its default. The fields are declared in `crates/harness/src/config.rs`.

Keys under `[[pipeline]]`, `[[stage]]`, `[[skill]]` and `[[role]]` are written `stage.turns` here.
Re-run `enallagi init` after every edit, as [setup.md](setup.md) says.

## `[agent]`

- `agent.preset`: the agent CLI each lane drives, or `custom`. Default `"claude"`.
- `agent.command`: the argv for `custom`, with `{prompt}` and `{turns}` substituted. No default.
- `agent.model`, `agent.effort`: passed to the preset. No default. A task's `model:` and `effort:` win.
- `agent.usage`: JSON paths to `cost`, `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens` and `turns` in the agent's output. The preset's own by default.
- `agent.rate_limit_pattern`: output that means the agent hit a rate limit, matched without case. Default `"hit your session limit"`.
- `agent.dangerously_skip_permissions`: adds the preset's bypass flag to every lane. Default `false`.
- `[agent.<role>]`: `preset`, `command`, `model`, `effort` and `usage` for one role. The role is one of `scout`, `adjudicator`, `implementer`, `verifier` or `researcher`.

## `[check]`

- `check.command`: the one command that decides green. Empty by default, and init sets it when it detects a runner. A run refuses to start while it is empty.
- `check.force`: the same check with its cache defeated. Empty by default. Left out of `enallagi.toml`, it follows `check.command`.
- `check.fail_name`: a regex over the check's output whose group 1 is a failing test's name. Empty by default, and init writes the detected runner's pattern below.
- `check.timeout`: `<n>s`, `<n>m` or `<n>h`. Default `"30m"`. A check that runs past it halts the run.

### `check.fail_name`

Each runner init detects writes the pattern below.
Each pattern is shown beside a failing line from a captured run, and the name it captures.
`floor.rs::each_fail_name_example_captures_its_line` re-checks every one against `crates/harness/runners/`.
It also checks each failing line is a whole line of `crates/harness/tests/fixtures/runners/<runner>/fail.txt`.

cargo test, from `cargo test` under cargo 1.98.1:

- fail_name: `^(?:test )?(\S+) (?:\.\.\. |--- )FAILED$`
- failing line: `test tests::a_sum_is_wrong ... FAILED`
- captures: `tests::a_sum_is_wrong`

node --test, from node v26.7.0:

- fail_name: `^✖ (.+?) \([0-9.]+m?s\)$`
- failing line: `✖ a sum is wrong (0.572958ms)`
- captures: `a sum is wrong`

bun test, from bun 1.3.14:

- fail_name: `\(fail\) (.+?)(?: \[[0-9.]+m?s\])?$`
- failing line: `(fail) a sum is wrong [0.14ms]`
- captures: `a sum is wrong`

vitest, from `vitest run` under vitest 3.2.7:

- fail_name: `^\s*×\s(.+?)\s+[0-9.]+m?s$`
- failing line: `   × a sum is wrong 3ms`
- captures: `a sum is wrong`

jest, from jest 30.5.0:

- fail_name: `^ {2}● (C|Co|Con|Cons|Conso|Consol|(?:[^C]|C[^o]|Co[^n]|Con[^s]|Cons[^o]|Conso[^l]|Consol[^e]|Console.).*)$`
- failing line: `  ● a sum is wrong`
- captures: `a sum is wrong`

pytest, from pytest 8.4.2:

- fail_name: `^FAILED \S+::(\S+)`
- failing line: `FAILED tests/test_sum.py::test_a_sum_is_wrong - assert (1 + 1) == 3`
- captures: `test_a_sum_is_wrong`

go test, from go1.26.2:

- fail_name: `^\s*--- FAIL: (\S+) \([0-9.]+m?s\)$`
- failing line: `--- FAIL: TestSumIsWrong (0.00s)`
- captures: `TestSumIsWrong`

## `[queue]`

- `queue.drain`: how many standing `proposed` blocks one adjudicate stage takes, oldest first. Default `3`.
- `queue.turns_per_block`: turns added to that stage for each block. Default `25`.
- `queue.proposed_rounds`: how many state commits a `proposed` block stands before it expires. Default `6`.

## `[pr]`

- `pr.per_task`: `enallagi run` opens one pull request per landed task, as `--pr-per-task` does. Default `false`.

## `[layout]`

Where the documents live:

- `layout.harness_dir`: the roles, hooks, queue and scratch state. Default `".enallagi"`.
- `layout.skills_dir`: where skills are vendored. Unset means the preset's own directory.
- `layout.context_file`: the file every role reads first. Unset means `AGENTS.md` beside the queue.
- `layout.pointer_files`: one-line pointers to the context file. Default `["CLAUDE.md", "GEMINI.md", "QWEN.md", ".github/copilot-instructions.md"]`.
- `layout.skill_invocation`: how a role prompt tells the agent to load a skill. Default `"invoke it via the Skill tool"`.
- `layout.driver_command`: runs the built artifact and prints one `FINDING ` line per shortfall. Default `""`, which is off. `ENALLAGI_DRIVER=1` is also required.
- `layout.learnings_cap`: the most entries `LEARNINGS.md` may hold. Default `12`.

The spec and its exit criteria:

- `layout.spec`: the spec file. Default `"SPEC.md"`.
- `layout.rows_heading`: the heading that opens the exit-criteria rows. Default `"## 11. Exit criteria"`.
- `layout.rows_end_heading`: the heading that closes them. Default `"## 12."`.
- `layout.contract_file`: the file the roles name as the one home of every shared shape. Default `"src/schema.ts"`.

The source and its tests, which init sets when it detects a runner:

- `layout.source_root`: the product source. Empty by default, and init sets it when it detects a runner.
- `layout.source_ext`: extensions the probes read as source. Default `[".ts", ".tsx", ".js", ".mjs", ".cjs", ".sh", ".py"]`.
- `layout.test_file_suffix_re`: a regex naming a test file. Empty by default, and init writes the detected runner's.
- `layout.test_decl_patterns`: how a test is declared, with `{name}` for its name. Empty by default, and init writes the detected runner's.
- `layout.test_glob`: git pathspecs holding test code, which the verifier diffs as `__TEST_GLOB__`. Empty by default.

The allowlists the `litter` and `rail-unenforced` probes read:

- `layout.harness_files`: build files that can run a rail's enforcement. Default `package.json`, `turbo.json`, `check.ts`, `Makefile` and `pyproject.toml`.
- `layout.harness_globs`: globs for more of the same. Default `["packages/*/package.json"]`.
- `layout.allowed_prefixes`: path prefixes a tracked file may sit under. Default `src/`, `packages/`, `evals/`, `.enallagi/` and one directory per agent CLI.
- `layout.docs`: file names that are documents, not litter. Default is the 25 names in `harness.default.toml`, from `.check-baseline` to `test-hashes.json`.
- `layout.harness_allow`: build files that are not litter. Default is the 9 names in `harness.default.toml`, from `package.json` to `pyproject.toml`.
- `layout.machinery`: path parts that are build output or scratch. Default `.DS_Store`, `.check`, `.turbo`, `.venv`, `STOP`, `__pycache__`, `build`, `dist`, `node_modules`, `out` and `target`.

## `[[pipeline]]`

Declaring one `[[pipeline]]` replaces all three defaults. [pipeline.md](pipeline.md) shows an override.

- `pipeline.name`: what `enallagi run --pipeline` names. The defaults are `review`, `task` and `discover`.
- `pipeline.when`: the predicate that picks it, such as `queue.takeable`. The list is in [reference.md](reference.md#configuration).
- `pipeline.stages`: the `[[stage]]` names it runs, in order.
- `pipeline.end_after_dry_rounds`: dry rounds in a row that end the run. Default `0`, which never ends it. `discover` sets `2`.

## `[[stage]]`

Declaring one `[[stage]]` replaces all four defaults.

- `stage.name`: what a pipeline's `stages` names.
- `stage.role`: the role prompt `<harness_dir>/roles/<role>.md` the agent runs. Set this or `stage.command`.
- `stage.command`: a shell command run with `sh -c`. Set this or `stage.role`.
- `stage.turns`: the agent's turn cap. Default `40`. The shipped stages set 120, 100, 30 and 40.
- `stage.timeout`: `<n>s`, `<n>m` or `<n>h`. No default. Required for a preset with no turn cap.
- `stage.env`: variables added to the stage's environment. Default `{}`. `scout` sets `ENALLAGI_DRIVER = "1"`.
- `stage.post`: the gates run after the stage. Default `[]`. The gates are in [reference.md](reference.md#gates).

## `[[skill]]`

Declaring one `[[skill]]` replaces all eight defaults.

- `skill.id`: the name a role writes as `{{skill:<id>}}`.
- `skill.source`: `github:owner/repo`, `git+<url>` or `path:<dir>`.
- `skill.path`: the directory inside the source that holds `SKILL.md`.
- `skill.rev`: the tag or commit to fetch. No default.
- `skill.gate`: the probe, rail or hook that fails without the skill, or `none`.
- `skill.why`: one line on what the skill is for.

## `[[role]]`

No role is declared by default.

- `role.name`, `role.source`, `role.path` and `role.rev` fetch `<path>/<name>.md` as a skill is fetched.
- The file lands at `<harness_dir>/roles/<name>.md` and is pinned in `harness.lock`.
