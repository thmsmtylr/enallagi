# Changing the pipeline

This guide removes and replaces a skill, overrides a pipeline and adds a stage.
Each section names the test in `crates/harness/tests/loop.rs` that runs it against a fixture.
Every edit goes in `.enallagi/enallagi.toml`, and `enallagi init` runs after it.

## Skills

A `[[skill]]` in `enallagi.toml` replaces the whole default list.
Keeping seven of the eight shipped skills means declaring all seven.

### Removing a skill

Leave its `[[skill]]` table out of the list and run `enallagi skills sync`.

Sync deletes the vendored copy under the skills directory and drops its entry in `harness.lock`.
It prints `<id>  removed` for each one.
A directory with no lock entry is never deleted, so a skill placed there by hand survives.
`enallagi skills check` and `sync --frozen` remove nothing.
`sync_prunes_a_removed_skill` holds this in place.

A shipped role names each shipped skill as `{{skill:<id>}}`.
Removing one of those refuses the config until the role that names it is replaced.
The refusal reads `role <role>: {{skill:<id>}} has no matching [[skill]] entry`.

### Replacing a skill

Keep the `id` and change its `source`, `path` or `rev`.
This points `tdd` at a copy kept in the repository:

```toml
[[skill]]
id = "tdd"
source = "path:skills/tdd"
path = ""
rev = "1"
gate = "spec-untested"
why = "the failing test that encodes the acceptance criteria is written before the code"
```

After an edit to the copy, set `rev = "2"` and run `enallagi skills sync`.
Sync prints `tdd  fetched`, re-vendors the skill and writes the new `rev` to `harness.lock`.
Without a change to `source` or `rev`, sync prints `tdd  cached` and keeps the old copy.
`a_changed_rev_revendors_the_skill` runs both.

## Pipelines

A `[[pipeline]]` in `enallagi.toml` replaces all three defaults.
This `task` pipeline runs without `adjudicate`:

```toml
[[pipeline]]
name = "review"
when = "queue.reviewing"
stages = ["verify"]

[[pipeline]]
name = "task"
when = "queue.takeable"
stages = ["implement", "verify"]

[[pipeline]]
name = "discover"
when = "!queue.takeable"
stages = ["scout", "adjudicate"]
end_after_dry_rounds = 2
```

A block the verifier files then waits for the next `discover` round's `adjudicate`.
`a_task_pipeline_without_adjudicate_skips_it` checks the dry-run plan.

## Stages

A `[[stage]]` in `enallagi.toml` replaces all four defaults.
A new stage is declared beside `implement`, `verify`, `scout` and `adjudicate`.

### A stage that runs a new role

Write the prompt at `.enallagi/roles/security.md`, then name it:

```toml
[[stage]]
name = "security"
role = "security"
turns = 40
```

Add `"security"` to a pipeline's `stages`.
The stage renders the prompt to `.enallagi/run/roles/security.md` and runs the configured agent.
`a_stage_naming_a_new_role_runs_it` runs it.

`[agent.<role>]` accepts only the five shipped role names.
A new role runs on `[agent]`, and `[agent.security]` is refused.

### A stage that runs a shell command

```toml
[[stage]]
name = "audit"
command = "npm audit --audit-level=high"
turns = 1
```

The command runs with `sh -c` at the repository root.
Its environment carries `ENALLAGI_TASK`, `ENALLAGI_STAGE`, `ENALLAGI_ITERATION` and `ENALLAGI_ROOT`.
`a_command_stage_runs_with_the_harness_environment` runs one.
