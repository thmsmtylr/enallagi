# SPEC — round seven: roles declared and fetched like skills

## 0. Scope

A stage's `role` resolves today to `<harness_dir>/roles/<name>.md` if present, else the copy
embedded in the binary. This round adds a `[[role]]` table to `harness.toml` with the same fields
as `[[skill]]` (`name`, `source`, `path`, `rev`), resolved through the same fetch, cache, vendor and
lock machinery, so a role prompt can be shared across repositories and pinned. An undeclared role
keeps today's lookup. The `[[role]]` entries are pinned in `harness.lock` under a `[[role]]` table
with the same fields as a skill entry.

## 11. Exit criteria

| Criterion | Test |
| --- | --- |
| A declared `[[role]]` is fetched from its source and vendored to `<harness_dir>/roles/<name>.md` before the stage that names it spawns, and the vendored file plus `harness.lock` are committed by the pipeline before that stage, as vendored skills are | `tests/roles.rs::a_declared_role_is_fetched_vendored_and_committed_before_its_stage` |
| `harness.lock` pins a vendored role by `source`, `rev`, `commit` and the SHA-256 of the file; a vendored role whose content no longer matches the lock refuses the stage under `--frozen` with a halt naming the role | `tests/roles.rs::a_role_whose_vendored_file_drifted_is_refused_under_frozen` |
| A stage naming a role with no `[[role]]` declaration reads `<harness_dir>/roles/<name>.md`, else the embedded copy, exactly as before this round | `tests/roles.rs::an_undeclared_role_falls_back_to_the_installed_or_embedded_file` |
| `harness hook immutable` refuses an edit to a vendored role that `harness.lock` pins | `tests/roles.rs::the_immutable_hook_refuses_an_edit_to_a_vendored_role` |

## 12. Notes

Rows name `<file>::<test>` under `crates/harness/`. `config::validate` refuses a `[[role]]` whose
`name` is not `[a-z0-9-]+` or whose `path` is not relative, as it does for skills.
