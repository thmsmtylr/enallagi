# Register

How this repository writes: comments, commit subjects, `notes:`, printed lines, and the
names of the documents the loop reads. The operator authors the rules here; AGENTS.md
§ Style carries only the one-line form a lane reads.

Three blocks were moved out of TASKS.md on 2026-09-16 so no lane works them as code tasks
before the rules exist. Nothing here is in the queue. Migrate a block back by pasting it
into `.enallagi/TASKS.md` with `status: ready`.

## Settled

- A `ponytail:` marker is an exception to the comment rule, recorded in AGENTS.md § Style on
  2026-09-16. It names a ceiling and its upgrade path, `harness probe ponytail-ceiling` reports
  it until a kill line settles it, and it stays in the source. The six markers stay where they
  are: init.rs:366, agent.rs:413, probes/friction_repeat.rs:38 and :65,
  probes/plain_record.rs:148, probes/check_unnamed.rs:3.

## Parked blocks

## [T-067] three live documents quote a test name T-059 renamed
scope: .enallagi/_TASKS.md, .github/workflows/ci.yml
blockedBy:
status: proposed
probe: verifier
command: `grep -rn 'the_context_file_is_resynced_to_the_configured_check\|the_git_add_line_init_prints_on_a_fresh_install_exits_0\|the_package_driver_reports_shortfalls_as_finding_lines' .enallagi/_TASKS.md .github/workflows/ci.yml`
output: `.enallagi/_TASKS.md:317` (T-008, ready) names `the_context_file_is_resynced_to_the_configured_check`; `.enallagi/_TASKS.md:638` (proposed) names `the_git_add_line_init_prints_on_a_fresh_install_exits_0`; `.github/workflows/ci.yml:46` names `the_package_driver_reports_shortfalls_as_finding_lines`. None of the three declares a test under `crates/` any more: `grep -rc '<name>' crates --include='*.rs'` prints 0 for each (verifier, 2026-09-16).
rows: none — harness
criteria:
  - each of the three sites names the test that exists today: `the_context_file_resyncs_the_check` (crates/harness/tests/init.rs), `the_printed_git_add_line_exits_0` (crates/harness/tests/init.rs), `the_driver_reports_shortfalls_as_findings` (crates/harness/tests/floor.rs), and the command above prints nothing
notes: T-059 renamed 166 test names and left these three references behind; `.enallagi/_TASKS.md` and `.github/workflows/ci.yml` were not on its scope line. The `driver` job itself is unaffected — it selects on the substring `driver`, which the new name still carries — so this is a stale reference, not a broken job.

## [T-075] a deferred shortcut is recorded twice, as a comment in the source and as a probe finding, and the comment is the copy that reads as narrative
scope: crates/harness/src/probes/ponytail_ceiling.rs, crates/harness/src/init.rs, crates/harness/src/agent.rs, crates/harness/src/probes/friction_repeat.rs, crates/harness/src/probes/plain_record.rs, crates/harness/src/probes/check_unnamed.rs, .enallagi/AGENTS.md, crates/harness/tests/**
blockedBy:
status: ready
rows: none — harness
criteria:
  - a ceiling is recorded in the queue, not in the source: `harness probe ponytail-ceiling` reads task blocks and their `notes:` lines, and a marker left in a `.rs` file is itself reported as a finding naming the block it belongs in
  - the six markers counted below are migrated, each to the block whose work would raise the ceiling, and `grep -rn 'ponytail:' crates --include='*.rs'` prints nothing
  - AGENTS.md § Style states the single rule for a comment in one line, and says where a ceiling goes instead
  - a kill line in `DECISIONS.md § Rejected findings` still settles a ceiling, by its text, exactly as it does today
  - the check exits 0
notes: Operator, 2026-09-16, quoting `crates/harness/src/probes/plain_record.rs:148`: "I'm also still seeing this shit ... as comments I thought we addressed the prose rubbish already?" Nothing failed: the ponytail skill's own rule requires a marker naming the ceiling AND the upgrade path, which is the second clause that reads as narrative, and T-056's style criterion only asked for one line. The conflict is between that rule and AGENTS.md § Style, which admits a comment only to stop a repeated mistake or explain hard code; a ceiling does neither, it records deferred work. The queue is already the second home: `probes/ponytail_ceiling.rs` reports every marker until a kill line quotes its text, so the debt is tracked there whether or not the comment exists. Counted 2026-09-16 on main: six markers, at init.rs:366, agent.rs:413, probes/friction_repeat.rs:38 and :65, probes/plain_record.rs:148, probes/check_unnamed.rs:3. Deleting them by hand is the wrong fix — the probe would stop seeing the debt.

## [T-076] `.enallagi/_TASKS.md` is a tracked 90KB copy of the queue that no code reads and two tasks still cite
scope: .enallagi/_TASKS.md, .enallagi/AGENTS.md, crates/harness/src/probes/**, crates/harness/tests/**
blockedBy:
status: ready
rows: none — harness
criteria:
  - either the file is the queue's archive under a name that says so and one document states what writes it, or it is deleted; `grep -rn '_TASKS' crates/harness/src` printing nothing stays true either way
  - T-067's scope and command no longer cite it as a live document, or cite it by its new name
  - a probe or the check refuses a second tracked file under the harness directory whose name differs from a document the loop reads by a leading underscore
  - the check exits 0
notes: Operator, 2026-09-16: "So there's two task.md files one in .enallgi dir and one in the worktree wont these collide at some point?" — that question was about the worktree, and the answer is no, the two are one repository reconciled by `merge --ff-only`. The real duplicate is this one. Counted 2026-09-16: `.enallagi/_TASKS.md` is tracked, 90601 bytes, 57 blocks, last written 14:48, against a live TASKS.md of 9 blocks; `grep -rn "_TASKS" crates/harness/src` prints nothing. T-067, written by a verifier this round, calls it one of "three live documents" and puts it on its scope line, so the queue now contains a task to edit a file nothing consumes. The operator was also told by this session to write a new block into it, which is how it misleads.
