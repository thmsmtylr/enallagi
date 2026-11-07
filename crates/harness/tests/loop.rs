//! The launcher: the dry plan, the halts, the run log and the budget, with stub agents in place of a coding agent.

use enallagi::events::{Event, Kind};
use enallagi::fixture::Repo;
use enallagi::pipeline::{self, Digest, RunOpts};
use std::sync::{Arc, Mutex};

const TASKS: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: ready-for-agent
criteria:
  - it happens
";

const DONE_TASK: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: done
criteria:
  - it happens
";

const REVIEW_TASK: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: review
criteria:
  - it happens
";

const REVIEW_AND_READY_TASKS: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: review
criteria:
  - it happens

## [T-002] do another thing

scope: src/other.ts
rows: none — harness
status: ready-for-agent
criteria:
  - it happens
";

const LEVELLED_TASK: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: ready-for-agent
model: m-task
effort: e-task
criteria:
  - it happens
";

const QUIET: &str = "echo '{\"total_cost_usd\":0.5}'\n";

// one claude result line carrying all four token lanes
const CACHED: &str = "echo '{\"total_cost_usd\":0.5,\"usage\":{\"input_tokens\":22,\"cache_creation_input_tokens\":54825,\"cache_read_input_tokens\":505740,\"output_tokens\":6233}}'\n";

fn script(repo: &Repo, rel: &str, body: &str) -> String {
    repo.write(rel, &format!("#!/usr/bin/env bash\nset -u\n{body}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(repo.root.join(rel), std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
    }
    format!("./{rel}")
}

// [[pipeline]] and [[stage]] replace the defaults whole, so every test spells out the full pair
fn base_toml(extra: &str) -> String {
    format!(
        r#"
[agent]
preset = "custom"
command = ["./src/fakeagent.sh", "{{prompt}}", "{{turns}}"]

[agent.usage]
cost = "total_cost_usd"
input_tokens = "usage.input_tokens"
output_tokens = "usage.output_tokens"
cache_creation_input_tokens = "usage.cache_creation_input_tokens"
cache_read_input_tokens = "usage.cache_read_input_tokens"
turns = "num_turns"

[check]
command = "./src/fakecheck.sh"

[[pipeline]]
name = "review"
when = "queue.reviewing"
stages = ["verify"]

[[pipeline]]
name = "task"
when = "queue.takeable"
stages = ["implement", "verify", "adjudicate"]

[[pipeline]]
name = "triage"
when = "queue.proposed"
stages = ["adjudicate"]

[[pipeline]]
name = "discover"
when = "!queue.takeable"
stages = ["scout", "adjudicate"]
end_after_dry_rounds = 2

[[stage]]
name = "implement"
role = "implementer"
turns = 5
post = ["implementer-not-done"]

[[stage]]
name = "verify"
role = "verifier"
turns = 5
post = ["commit-verdict", "verdict", "scope"]

[[stage]]
name = "scout"
role = "scout"
turns = 5

[[stage]]
name = "adjudicate"
role = "adjudicator"
turns = 5
post = ["commit-round", "queue-intact", "adjudicator-halt", "dry-round"]
{extra}
"#
    )
}

// Every fixture config must carry the `path:` skills `repo()` wrote: rewriting enallagi.toml with
// base_toml alone falls back to the shipped `[[skill]]` table, whose sources are github clones, and
// the test then reaches the network.
fn write_toml(r: &Repo, toml: &str) {
    write_toml_at(r, ".enallagi/enallagi.toml", toml);
}

fn write_toml_at(r: &Repo, rel: &str, toml: &str) {
    let full = format!("{toml}{}", r.local_skills(toml));
    // built, never written literally: the floor test greps this tree for the literal
    let remote = concat!("source = \"", "github:");
    assert!(
        !full.contains(remote) && !full.contains("source = \"git+"),
        "a fixture config may not name a remote skill source:\n{full}"
    );
    r.write(rel, &full);
}

fn instance(rel: &str) -> String {
    format!(".enallagi/{rel}")
}

fn queue_path() -> String {
    ".enallagi/specs".to_string()
}

fn queue(r: &Repo, text: &str) {
    r.queue(".enallagi", text);
}

fn issues(r: &Repo) -> String {
    format!(
        ".enallagi/specs/{}/issues",
        enallagi::tracker::repo_slug(&r.root)
    )
}

fn spec(r: &Repo) -> String {
    format!(
        ".enallagi/specs/{}/spec.md",
        enallagi::tracker::repo_slug(&r.root)
    )
}

// the file a stub agent appends a ticket's lines to; it may run before the queue is written
fn ticket(r: &Repo, id: &str, title: &str) -> String {
    format!("{}/{}", issues(r), enallagi::tracker::file_name(id, title))
}

// a stub's shell line that writes block text as its ticket file
fn queues(r: &Repo, block: &str) -> String {
    if block.is_empty() {
        return String::new();
    }
    let b = &enallagi::block::parse(block).expect("parse")[0];
    format!(
        "printf '%s' '{}' >{}\n",
        enallagi::tracker::ticket_text(b),
        ticket(r, &b.id, &b.title)
    )
}

// the queue rendered as blocks, for asserts that read it whole
fn tasks_text(r: &Repo) -> String {
    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    t.blocks()
        .expect("the queue")
        .iter()
        .map(enallagi::block::block_text)
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn state_of(r: &Repo, id: &str) -> Option<String> {
    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    enallagi::tracker::state(&t.get(id).expect("the queue")?)
}

fn repo(toml: &str, tasks: &str) -> Repo {
    let repo = Repo::new();
    // what `enallagi init` ignores under the harness dir; vendored skills and roles are committed
    repo.write(
        ".enallagi/.gitignore",
        "events.jsonl\n*.log\nlogs/\nworktrees/\nloop.pid\nrun/\n__pycache__/\nholdouts/\n",
    );
    script(&repo, "src/fakecheck.sh", "exit 0\n");
    script(&repo, "src/fakeagent.sh", QUIET);
    if !tasks.is_empty() {
        queue(&repo, tasks);
    }
    repo.write(&spec(&repo), "# spec\n");
    repo.write(&instance("PROGRESS.md"), "# progress\n");
    write_toml(&repo, toml);
    repo.commit_all("harness");
    repo
}

fn implementer(repo: &Repo, extra: &str) -> String {
    script(
        repo,
        "src/fakeimpl.sh",
        &format!(
            "sleep 1\n\
             echo work >src/thing.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>{progress}\n\
             {extra}\
             git add -A >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            progress = instance("PROGRESS.md"),
        ),
    )
}

fn verifier(repo: &Repo) -> String {
    script(
        repo,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status T-001 done 'stub verified'\n{verified}{QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            verified = verdict("verified by verifier"),
        ),
    )
}

// a stub's verdict, written as a role writes it: one line, replacing the last, in `## Harness`, the file's last section
// a verified verdict carries both review axes, as the verdict gate wants
fn verdict(line: &str) -> String {
    verdict_for("T-001", line)
}

fn verdict_for(id: &str, line: &str) -> String {
    let axes = if line.starts_with("verified") {
        "spec: none\\nstandards: none\\n"
    } else {
        ""
    };
    format!(
        "f=$(ls {}/*/issues/{id}-*.md)\nsed -i.bak -e '/^verdict:/d' -e '/^spec:/d' -e '/^standards:/d' \"$f\"\nrm -f \"$f.bak\"\nprintf 'verdict: {line}\\n{axes}' >>\"$f\"\n",
        queue_path()
    )
}

fn role_commands(implement: &str, verify: &str) -> String {
    format!(
        "\n[agent.implementer]\ncommand = [\"{implement}\", \"{{prompt}}\", \"{{turns}}\"]\n\
         \n[agent.verifier]\ncommand = [\"{verify}\", \"{{prompt}}\", \"{{turns}}\"]\n"
    )
}

fn opts(max_iter: u32) -> RunOpts {
    RunOpts {
        max_iter,
        ..RunOpts::default()
    }
}

fn go(repo: &Repo, opts: &RunOpts) -> (Digest, Vec<Event>) {
    let (digest, events) = try_go(repo, opts);
    (digest.expect("the pipeline ran"), events)
}

// installed on the shared events::Writer, so this collects the live stream in emit order, not a replay of the log
fn try_go(repo: &Repo, opts: &RunOpts) -> (anyhow::Result<Digest>, Vec<Event>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let digest = pipeline::run(
        &repo.root,
        opts,
        Box::new(move |e| {
            if let Ok(mut v) = sink.lock() {
                v.push(e.clone());
            }
        }),
    );
    let events = seen.lock().map(|v| v.clone()).unwrap_or_default();
    (digest, events)
}

fn ends(events: &[Event]) -> Vec<&Kind> {
    events
        .iter()
        .map(|e| &e.kind)
        .filter(|k| matches!(k, Kind::StageEnd { .. }))
        .collect()
}

fn plan_of(repo: &Repo) -> String {
    plan_named(repo, &[])
}

fn plan_named(repo: &Repo, pipelines: &[&str]) -> String {
    let cfg = enallagi::config::load(&repo.root).expect("load");
    let names: Vec<String> = pipelines.iter().map(|p| p.to_string()).collect();
    pipeline::plan(&repo.root, &cfg, &names).expect("plan")
}

fn headings(plan: &str) -> Vec<&str> {
    plan.lines()
        .filter(|l| l.starts_with("=== pipeline"))
        .collect()
}

#[test]
fn a_dry_plan_skips_a_pipeline_left_unnamed() {
    let r = repo(&base_toml(""), REVIEW_AND_READY_TASKS);
    let all = plan_of(&r);
    assert_eq!(headings(&all).len(), 2, "{all}");

    let filtered = plan_named(&r, &["task"]);
    assert_eq!(
        headings(&filtered),
        ["=== pipeline task (queue.takeable) ==="],
        "{filtered}"
    );
}

#[test]
fn a_dry_iteration_plans_implement_and_verify() {
    let r = repo(&base_toml(""), TASKS);
    let plan = plan_of(&r);
    assert!(plan.contains("DRY_RUN would spawn"), "{plan}");
    assert!(plan.contains("implement"), "{plan}");
    assert!(plan.contains("verify"), "{plan}");
    assert!(plan.contains("commit-verdict"), "{plan}");
}

// base_toml opens [agent] once, so a default level goes inside that table rather than after it
fn with_agent_levels(toml: &str) -> String {
    toml.replace(
        "preset = \"custom\"",
        "preset = \"custom\"\nmodel = \"m-agent\"\neffort = \"e-agent\"",
    )
}

#[test]
fn a_dry_stage_line_names_model_and_effort() {
    let with_task = repo(&with_agent_levels(&base_toml("")), LEVELLED_TASK);
    let plan = plan_of(&with_task);
    assert!(
        plan.contains("implement as role implementer via ./src/fakeagent.sh (turns: 5, model: m-task, effort: e-task)"),
        "{plan}"
    );
    assert!(
        plan.contains("verify as role verifier via ./src/fakeagent.sh (turns: 5, model: m-task, effort: e-task)"),
        "{plan}"
    );

    let empty = repo(&with_agent_levels(&base_toml("")), "");
    let plan = plan_of(&empty);
    assert!(
        plan.contains("scout as role scout via ./src/fakeagent.sh (turns: 5, model: m-agent, effort: e-agent)"),
        "{plan}"
    );
    assert!(
        plan.contains("adjudicate as role adjudicator via ./src/fakeagent.sh (turns: 5, model: m-agent, effort: e-agent)"),
        "{plan}"
    );
}

#[test]
fn a_dry_stage_line_says_default_with_no_level() {
    let plan = plan_of(&repo(&base_toml(""), TASKS));
    assert!(
        plan.contains("(turns: 5, model: default, effort: default)"),
        "{plan}"
    );
}

#[test]
fn the_launcher_spawns_the_configured_agent() {
    let r = repo(&base_toml(""), TASKS);
    assert!(plan_of(&r).contains("via ./src/fakeagent.sh"));
}

#[test]
fn the_implement_stage_names_its_role_file() {
    let r = repo(&base_toml(""), TASKS);
    assert!(plan_of(&r).contains(".enallagi/run/roles/implementer.md"));
}

#[test]
fn a_roles_own_agent_command_is_used() {
    let extra =
        "\n[agent.verifier]\ncommand = [\"./src/fakeverifier.sh\", \"{prompt}\", \"{turns}\"]\n";
    let r = repo(&base_toml(extra), TASKS);
    assert!(plan_of(&r).contains("as role verifier via ./src/fakeverifier.sh"));
}

#[test]
fn a_role_with_no_agent_falls_back() {
    let extra =
        "\n[agent.verifier]\ncommand = [\"./src/fakeverifier.sh\", \"{prompt}\", \"{turns}\"]\n";
    let r = repo(&base_toml(extra), TASKS);
    assert!(plan_of(&r).contains("as role implementer via ./src/fakeagent.sh"));
}

#[test]
fn a_preset_argv_carries_dir_and_context() {
    let extra = "\n[agent.scout]\ncommand = [\"./src/fakeargs.sh\", \"{prompt}\", \"{harness_dir}\", \"{context_file}\"]\n";
    let r = repo(&base_toml(extra), "");
    script(
        &r,
        "src/fakeargs.sh",
        &format!("printf '%s %s\\n' \"$2\" \"$3\" >>args.txt\n{QUIET}"),
    );
    r.commit_all("args");
    let _ = try_go(&r, &opts(1));
    let args = std::fs::read_to_string(r.root.join("args.txt")).expect("the scout ran");
    assert_eq!(
        args.lines().next(),
        Some(".enallagi .enallagi/AGENTS.md"),
        "{args}"
    );
}

#[test]
fn clarification_halts() {
    let r = repo(&base_toml(""), TASKS);
    r.write(&spec(&r), "# spec\n\n[NEEDS CLARIFICATION] which store?\n");
    let (digest, events) = go(&r, &opts(1));
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h.contains("[NEEDS CLARIFICATION]")),
        "{:?}",
        digest.halts
    );
    assert!(ends(&events).is_empty(), "nothing may spawn");
}

#[test]
fn backticked_marker_runs() {
    let r = repo(&base_toml(""), TASKS);
    r.write(
        &spec(&r),
        "# spec\n\nA `[NEEDS CLARIFICATION]` marker is prose about the rail.\n",
    );
    let (digest, _) = go(&r, &opts(1));
    assert!(
        !digest
            .halts
            .iter()
            .any(|h| h.contains("[NEEDS CLARIFICATION]")),
        "{:?}",
        digest.halts
    );
}

#[test]
fn every_stage_appends_a_run_log_record() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (_, events) = go(&r, &opts(1));
    assert_eq!(ends(&events).len(), 2, "{events:#?}");
}

#[test]
fn the_verdict_gate_records_the_check_tally() {
    let r = repo("", "");
    script(
        &r,
        "src/fakecheck.sh",
        "echo 'test result: ok. 237 passed; 0 failed; 0 ignored'\n\
         echo 'test result: ok. 239 passed; 0 failed; 5 ignored'\n",
    );
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    // a check is counted only when its output has a reader, here the cargo preset's
    let cargo = enallagi::runners::presets()
        .into_iter()
        .find(|p| p.name == "cargo")
        .expect("the cargo preset")
        .fail_name;
    let toml = base_toml(&role_commands(&implement, &verify)).replace(
        "[check]\ncommand = \"./src/fakecheck.sh\"",
        &format!("[check]\ncommand = \"./src/fakecheck.sh\"\nfail_name = '{cargo}'"),
    );
    write_toml(&r, &toml);
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    let tally = events.iter().find_map(|e| match &e.kind {
        Kind::Gate { gate, tally, .. } if gate == "verdict" => *tally,
        _ => None,
    });
    let tally = tally.unwrap_or_else(|| panic!("no verdict tally: {events:#?}"));
    assert_eq!((tally.passed, tally.ignored, tally.lines), (476, 5, 2));
    assert!(
        !digest.warnings.iter().any(|w| w.contains("count line")),
        "{:?}",
        digest.warnings
    );
}

#[test]
fn a_check_with_no_count_warns_in_the_digest() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    // the warning is owed only by a runner whose summary the tally parses
    let cargo = enallagi::runners::presets()
        .into_iter()
        .find(|p| p.name == "cargo")
        .expect("the cargo preset")
        .fail_name;
    let toml = base_toml(&role_commands(&implement, &verify)).replace(
        "[check]\ncommand = \"./src/fakecheck.sh\"",
        &format!("[check]\ncommand = \"./src/fakecheck.sh\"\nfail_name = '{cargo}'"),
    );
    write_toml(&r, &toml);
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert!(
        digest
            .warnings
            .iter()
            .any(|w| w == "the check printed no count line: `./src/fakecheck.sh`"),
        "{:?}",
        digest.warnings
    );
    let text = enallagi::pipeline::digest_text(&digest);
    assert!(text.contains("./src/fakecheck.sh"), "{text}");
}

#[test]
fn the_record_carries_role_seconds_cost() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (_, events) = go(&r, &opts(1));
    let role = events.iter().find_map(|e| match &e.kind {
        Kind::StageStart { stage, role, .. } if stage == "implement" => role.clone(),
        _ => None,
    });
    assert_eq!(role.as_deref(), Some("implementer"));
    let (cost, task, seconds) = events
        .iter()
        .find_map(|e| match &e.kind {
            Kind::StageEnd {
                stage,
                cost,
                task,
                seconds,
                ..
            } if stage == "implement" => Some((*cost, task.clone(), *seconds)),
            _ => None,
        })
        .expect("an implement stage.end");
    assert_eq!(cost, Some(0.5));
    assert_eq!(task.as_deref(), Some("T-001"));
    assert!((1..60).contains(&seconds), "seconds was {seconds}");
}

#[test]
fn the_loop_stops_before_the_budget() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.commit_all("stubs");

    let o = RunOpts {
        budget_usd: Some(0.4),
        ..opts(1)
    };
    let (digest, events) = go(&r, &o);
    assert!(
        digest.halts.iter().any(|h| h.contains("the run has spent")),
        "{:?}",
        digest.halts
    );
    assert_eq!(ends(&events).len(), 1, "{events:#?}");
}

#[test]
fn a_dollar_budget_over_a_cost_nothing_reports_halts() {
    let r = repo(
        &base_toml("").replace("cost = \"total_cost_usd\"\n", ""),
        TASKS,
    );
    let o = RunOpts {
        budget_usd: Some(10.0),
        ..opts(1)
    };
    let (digest, _) = go(&r, &o);
    assert!(
        digest.halts.iter().any(|h| h.contains("reported no cost")),
        "{:?}",
        digest.halts
    );
}

fn cached_usage_repo() -> Repo {
    let r = repo(&base_toml(""), TASKS);
    script(&r, "src/fakeagent.sh", CACHED);
    r.commit_all("cached usage");
    r
}

#[test]
fn a_stage_end_carries_the_cache_token_lanes() {
    let r = cached_usage_repo();
    let (_, events) = go(&r, &opts(1));
    let lanes = events
        .iter()
        .find_map(|e| match &e.kind {
            Kind::StageEnd {
                stage,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                ..
            } if stage == "implement" => {
                Some((*cache_creation_input_tokens, *cache_read_input_tokens))
            }
            _ => None,
        })
        .expect("an implement stage.end");
    assert_eq!(lanes, (Some(54_825), Some(505_740)));
}

#[test]
fn a_token_budget_counts_the_cache_lanes() {
    let r = cached_usage_repo();
    let o = RunOpts {
        budget_tokens: Some(100_000),
        ..opts(1)
    };
    let (digest, events) = go(&r, &o);
    assert!(
        digest.halts.iter().any(|h| h.contains("566820 tokens")),
        "{:?}",
        digest.halts
    );
    assert_eq!(ends(&events).len(), 1, "{events:#?}");
}

// the result line of CACHED with its two cache lanes gone
const UNCACHED: &str =
    "echo '{\"total_cost_usd\":0.5,\"usage\":{\"input_tokens\":22,\"output_tokens\":6233}}'\n";

#[test]
fn an_unreported_declared_lane_halts_the_budget() {
    let r = repo(&base_toml(""), TASKS);
    script(&r, "src/fakeagent.sh", UNCACHED);
    r.commit_all("uncached usage");
    let o = RunOpts {
        budget_tokens: Some(100_000),
        ..opts(1)
    };
    let (digest, events) = go(&r, &o);
    assert!(
        digest.halts.iter().any(|h| h
            .contains("[agent.usage].cache_creation_input_tokens and .cache_read_input_tokens")),
        "{:?}",
        digest.halts
    );
    assert_eq!(ends(&events).len(), 1, "{events:#?}");
}

#[test]
fn a_two_lane_usage_table_enforces_the_budget() {
    let toml = base_toml("")
        .replace(
            "cache_creation_input_tokens = \"usage.cache_creation_input_tokens\"\n",
            "",
        )
        .replace(
            "cache_read_input_tokens = \"usage.cache_read_input_tokens\"\n",
            "",
        );
    let r = repo(&toml, TASKS);
    script(&r, "src/fakeagent.sh", UNCACHED);
    r.commit_all("two-lane usage");
    let o = RunOpts {
        budget_tokens: Some(1_000),
        ..opts(1)
    };
    let (digest, events) = go(&r, &o);
    assert!(
        digest.halts.iter().any(|h| h.contains("6255 tokens")),
        "{:?}",
        digest.halts
    );
    assert!(
        !digest
            .halts
            .iter()
            .any(|h| h.contains("cannot be enforced")),
        "{:?}",
        digest.halts
    );
    assert_eq!(ends(&events).len(), 1, "{events:#?}");
}

#[test]
fn a_config_naming_an_unknown_gate_is_refused() {
    let r = repo(
        &base_toml("").replace("\"commit-round\"", "\"nope\""),
        TASKS,
    );
    let err = try_go(&r, &opts(1)).0.expect_err("refused");
    assert!(err.downcast_ref::<pipeline::Refused>().is_some(), "{err}");
    assert!(err.to_string().contains("nope"), "{err}");
}

#[test]
fn an_uncapped_untimed_stage_is_refused() {
    let toml = base_toml("").replace(
        "preset = \"custom\"\ncommand = [\"./src/fakeagent.sh\", \"{prompt}\", \"{turns}\"]",
        "preset = \"aider\"",
    );
    let r = repo(&toml, TASKS);
    let err = try_go(&r, &opts(1)).0.expect_err("refused");
    assert!(err.downcast_ref::<pipeline::Refused>().is_some(), "{err}");
    assert!(err.to_string().contains("timeout"), "{err}");
}

#[test]
fn an_unresolved_skill_refuses_a_stage() {
    let r = repo(&base_toml(""), TASKS);
    r.write(
        ".enallagi/roles/implementer.md",
        "Walk the ladder with {{skill:ponytail}} before you write anything.\n",
    );
    let o = RunOpts {
        frozen: true,
        ..opts(1)
    };
    let (digest, events) = go(&r, &o);
    assert!(
        digest.halts.iter().any(|h| h.contains("ponytail")),
        "{:?}",
        digest.halts
    );
    assert!(ends(&events).is_empty(), "the stage may not spawn");
}

#[test]
fn a_command_stage_runs_with_the_harness_environment() {
    let toml = base_toml("").replace(
        "stages = [\"implement\", \"verify\", \"adjudicate\"]",
        "stages = [\"note\"]",
    ) + "\n[[stage]]\nname = \"note\"\ncommand = \"printf '%s %s %s' \\\"$ENALLAGI_TASK\\\" \\\"$ENALLAGI_STAGE\\\" \\\"$ENALLAGI_ITERATION\\\" >env.txt\"\nturns = 1\n";
    let r = repo(&toml, TASKS);
    go(&r, &opts(1));
    let seen = std::fs::read_to_string(r.root.join("env.txt")).expect("the command stage ran");
    assert_eq!(seen, "T-001 note 1");
}

#[test]
fn queue_empty_bad_queue() {
    let toml = r#"
[agent]
preset = "custom"
command = ["./src/fakeagent.sh", "{prompt}", "{turns}"]

[check]
command = "./src/fakecheck.sh"

[[pipeline]]
name = "empty"
when = "queue.empty"
stages = ["note"]

[[stage]]
name = "note"
command = "touch ran-empty-pipeline"
turns = 1
"#;
    // a duplicate id is what makes the queue unreadable
    let r = repo(toml, "## [T-001] first\nstatus: ready-for-agent\n");
    r.write(
        ".enallagi/specs/x/issues/T-001-again.md",
        "# T-001: again\n\n**Status:** ready-for-agent\n",
    );
    r.commit_all("a second T-001");

    let (digest, _) = go(&r, &opts(1));
    assert!(
        !r.root.join("ran-empty-pipeline").exists(),
        "an unparseable queue is not evidence the queue is empty"
    );
    assert!(
        digest.warnings.iter().any(|w| w.contains("queue.empty")),
        "{:?}",
        digest.warnings
    );
}

#[test]
fn stop_file_halts_at_the_next_boundary() {
    let r = repo("", "");
    let implement = implementer(&r, &format!("touch {}\n", instance("STOP")));
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    assert!(
        digest.halts.iter().any(|h| h.contains("STOP")),
        "{:?}",
        digest.halts
    );
    assert_eq!(ends(&events).len(), 1, "verify may not spawn");
}

#[test]
fn new_needs_info_halts() {
    let r = repo("", "");
    let implement = implementer(
        &r,
        &format!(
            "{bin} tasks set-status T-001 needs-info 'the contract does not answer it'\n",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let toml = base_toml(&role_commands(&implement, "./src/fakeagent.sh")).replace(
        "stages = [\"implement\", \"verify\", \"adjudicate\"]",
        "stages = [\"implement\"]",
    );
    write_toml(&r, &toml);
    queue(&r, TASKS);
    r.commit_all("stubs");

    // one iteration: the halt must come from this round's end, not the next round's boundary
    let (digest, _) = go(&r, &opts(1));
    assert!(
        digest.halts.iter().any(|h| h.contains("needs-info")),
        "{:?}",
        digest.halts
    );
}

#[test]
fn two_dry_rounds_end() {
    let r = repo(&base_toml(""), "");
    queue(&r, "# queue\n");
    // an empty queue is an empty directory, which git does not track
    let (digest, _) = go(&r, &opts(5));
    assert_eq!(digest.iterations, 2, "{digest:#?}");
}

#[test]
fn run_without_a_tty_prints_one_line_per_event() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    installed(&r);
    r.commit_all("stubs");

    // this asserts what a tty-less run prints; CI would make the run --frozen and refuse the
    // fixture's unvendored skills, which is `an_unresolved_skill_refuses_a_stage`
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--no-tui", "--iterations", "1"])
        .current_dir(&r.root)
        .env_remove("CI")
        .output()
        .expect("run enallagi run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert_eq!(
        stdout.lines().filter(|l| l.contains(" stage.end ")).count(),
        2,
        "{stdout}"
    );
}

#[test]
fn run_names_its_binary_and_each_event_its_head() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    installed(&r);
    r.commit_all("stubs");
    let head = enallagi::git::git(&r.root, &["rev-parse", "HEAD"]).unwrap();

    let (code, out) = harness(&r.root, &["run", "--no-tui", "--iterations", "1"]);
    assert_eq!(code, 0, "{out}");
    let named = format!(
        "enallagi {} {}",
        env!("CARGO_PKG_VERSION"),
        enallagi::events::COMMIT
    );
    assert_eq!(out.lines().next(), Some(named.as_str()), "{out}");
    assert!(!out.contains("binary predates HEAD"), "{out}");

    let events = enallagi::events::Log::open(&r.root.join(".enallagi"))
        .read()
        .unwrap();
    assert!(events.iter().all(|e| e.sha.is_some()), "{events:#?}");
    let start = &events[0];
    assert_eq!(start.sha.as_deref(), Some(head.as_str()));
    assert!(
        matches!(&start.kind, Kind::RunStart { binary: Some(b), .. } if *b == enallagi::events::binary()),
        "{start:#?}"
    );

    let (_, rendered) = harness(&r.root, &["events"]);
    let first = rendered.lines().next().unwrap_or_default();
    assert!(first.contains(&format!("sha={head}")), "{rendered}");
    assert!(
        first.contains(&format!(
            r#"binary={{"version":"{}","#,
            env!("CARGO_PKG_VERSION")
        )),
        "{rendered}"
    );
    let (_, json) = harness(&r.root, &["events", "--json"]);
    let first = json.lines().next().unwrap_or_default();
    assert!(first.contains(&format!(r#""sha":"{head}""#)), "{json}");
    assert!(first.contains(r#""binary":{"version":"#), "{json}");
}

#[test]
fn harness_run_exits_2_on_a_refused_config() {
    let r = repo(
        &base_toml("").replace("\"commit-round\"", "\"nope\""),
        TASKS,
    );
    installed(&r);
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--no-tui", "--iterations", "1"])
        .current_dir(&r.root)
        .output()
        .expect("run enallagi run");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
}

// the binary refuses a tree whose install does not match enallagi.toml, so a fixture it drives is installed
fn installed(r: &Repo) {
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
}

fn harness(root: &std::path::Path, args: &[&str]) -> (i32, String) {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(args)
        .current_dir(root)
        // CI freezes the run, which refuses the fixture's skills before the stage
        .env_remove("CI")
        .output()
        .expect("run enallagi");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

// the first external run's shape: enallagi.toml's check was edited and no one re-ran init
#[test]
fn stale_install_refuses_run() {
    let r = Repo::new();
    script(&r, "src/fakecheck.sh", "exit 0\n");
    script(&r, "src/fakeagent.sh", QUIET);
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");

    write_toml_at(&r, ".enallagi/enallagi.toml", &base_toml(""));
    queue(&r, TASKS);
    r.commit_all("a check the install has not seen");

    let (code, out) = harness(&r.root, &["run", "--no-tui", "--iterations", "1"]);
    assert_ne!(code, 0, "{out}");
    assert!(
        out.contains("check-unnamed") || out.contains("install-stale"),
        "{out}"
    );
    assert!(out.contains("enallagi init"), "{out}");
    let log = std::fs::read_to_string(r.root.join(".enallagi/events.jsonl")).unwrap_or_default();
    assert!(!log.contains("stage.start"), "{log}");

    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    let (_, out) = harness(&r.root, &["run", "--no-tui", "--iterations", "1"]);
    assert!(out.contains("stage.start"), "{out}");
}

// the gate spawns current_exe(), which in-process would be the test binary, so this drives enallagi
#[test]
fn stale_install_fails_verify() {
    let r = Repo::new();
    script(&r, "src/fakecheck.sh", "exit 0\n");
    script(&r, "src/fakeagent.sh", QUIET);
    let verify = script(
        &r,
        "src/fakeverify.sh",
        &format!(
            "printf 'drift\\n' >>.enallagi/RAILS.md\n\
             {bin} tasks set-status T-001 done 'stub verified'\n{verified}{QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            verified = verdict("verified by verifier"),
        ),
    );
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");

    let toml = base_toml(&format!(
        "\n[agent.verifier]\ncommand = [\"{verify}\", \"{{prompt}}\", \"{{turns}}\"]\n"
    ))
    .replace(
        r#"post = ["commit-verdict", "verdict", "scope"]"#,
        r#"post = ["install-stale"]"#,
    );
    write_toml_at(&r, ".enallagi/enallagi.toml", &toml);
    queue(&r, REVIEW_TASK);
    // the toml the install was rendered from just changed, and preflight refuses a stale install
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    r.commit_all("a verifier that drifts the install");

    let (_, out) = harness(&r.root, &["run", "--no-tui", "--iterations", "1"]);
    let listed = out
        .split_once("\nwarnings:\n")
        .and_then(|(_, rest)| rest.split_once("Loop finished"))
        .map(|(warnings, _)| warnings.to_string())
        .unwrap_or_default();
    assert!(listed.contains(".enallagi/RAILS.md"), "{out}");
    assert!(listed.contains("re-run `enallagi init`"), "{out}");

    let (_, events) = harness(&r.root, &["events", "--task", "T-001"]);
    let gate = events
        .lines()
        .find(|l| l.contains("gate=install-stale"))
        .unwrap_or_else(|| panic!("{events}"));
    assert!(gate.contains("pass=false"), "{gate}");
}

const SKILL: &str = r#"
[[skill]]
id = "tdd"
source = "path:vendor"
path = "tdd"
gate = "none"
why = "the failing test is written first"
"#;

#[test]
fn role_render_keeps_source() {
    let toml = base_toml(SKILL).replace(
        "stages = [\"implement\", \"verify\", \"adjudicate\"]",
        "stages = [\"implement\"]",
    );
    let r = repo(&toml, TASKS);
    r.write("vendor/tdd/SKILL.md", "# tdd\n\nWrite the test first.\n");
    r.write(
        ".enallagi/roles/implementer.md",
        "Do the work with {{skill:tdd}} in hand.\n",
    );

    for run in 1..=2 {
        let (_, events) = go(&r, &opts(1));
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.kind, Kind::SkillResolved { id, .. } if id == "tdd")),
            "run {run} resolved no skill"
        );
    }
    let source = std::fs::read_to_string(r.root.join(".enallagi/roles/implementer.md")).unwrap();
    assert!(source.contains("{{skill:tdd}}"), "the source was rewritten");
    let rendered =
        std::fs::read_to_string(r.root.join(".enallagi/run/roles/implementer.md")).unwrap();
    assert!(!rendered.contains("{{skill:"), "the token was not rendered");
}

const SKILL_TASKS: &str = "\
## [T-001] do the thing

scope: src/a.ts
rows: none — harness
status: ready-for-agent
criteria:
  - it happens
";

const SKILL_DECL: &str = "\
[[skill]]
id = \"tdd\"
source = \"path:vendor/tdd\"
path = \"\"
gate = \"none\"
why = \"because\"

[[role]]
name = \"implementer\"
source = \"path:vendor\"
path = \"roles\"
";

// reproduces the bug: skills::resolve vendors a skill and writes harness.lock but nothing commits
// them, so the verdict gate sees the untracked vendor dir as work off the branch and forces the
// task back to ready even though the implementer and verifier committed everything in their scope.
// a declared role rides the same commit, and the scope gate exempts it like a fresh skill.
#[test]
fn a_fetched_skill_commits_before_its_stage() {
    let toml = base_toml(SKILL_DECL);
    let r = repo(&toml, SKILL_TASKS);
    r.write("vendor/tdd/SKILL.md", "# tdd\n\nWrite the test first.\n");
    r.write(
        "vendor/roles/implementer.md",
        "Do the work with {{skill:tdd}} in hand.\n",
    );

    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "echo work >src/a.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>{progress}\n\
             git add -- src/a.ts {progress} {queue} >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            progress = instance("PROGRESS.md"),
            queue = queue_path(),
        ),
    );
    let verify = script(
        &r,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status T-001 done 'stub verified'\n\
             {verified}\
             git add -- {queue} >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'verify: T-001 verdict' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            queue = queue_path(),
            verified = verdict("verified by verifier"),
        ),
    );
    let skills = r.local_skills(&toml);
    write_toml(&r, &(toml + &role_commands(&implement, &verify) + &skills));
    r.commit_all("stubs");

    let (_, events) = go(&r, &opts(1));

    assert_eq!(state_of(&r, "T-001").as_deref(), Some("done"));
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            Kind::Gate { gate, pass, .. } if gate == "verdict" && *pass)),
        "{events:#?}"
    );
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            Kind::Gate { gate, pass, .. } if gate == "scope" && *pass)),
        "{events:#?}"
    );

    let log = std::process::Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(&r.root)
        .output()
        .expect("git log");
    assert!(
        String::from_utf8_lossy(&log.stdout).contains("chore(vendor): tdd implementer"),
        "{}",
        String::from_utf8_lossy(&log.stdout)
    );

    let ls = std::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(&r.root)
        .output()
        .expect("git ls-files");
    let ls = String::from_utf8_lossy(&ls.stdout);
    assert!(ls.contains("harness.lock"), "{ls}");
    assert!(ls.contains(".enallagi/skills/tdd/SKILL.md"), "{ls}");
    assert!(ls.contains(".enallagi/roles/implementer.md"), "{ls}");
    let lock = enallagi::skills::read_lock(&r.root, ".enallagi").expect("lock");
    assert_eq!(lock.role.len(), 1, "{lock:?}");
    assert_eq!(lock.role[0].id, "implementer");
}

#[test]
fn an_unnamed_red_check_is_a_finding() {
    let r = repo(&base_toml(""), "");
    script(&r, "src/fakecheck.sh", "echo boom\nexit 1\n");
    queue(&r, "# queue\n");
    r.commit_all("a red check");

    let plan = plan_of(&r);
    assert!(plan.contains("FINDING check-red"), "{plan}");
    assert!(!plan.contains("PROBE check-red ERROR"), "{plan}");
}

#[test]
fn a_check_past_its_timeout_halts_the_run() {
    let toml = base_toml(&role_commands("./src/fakeimpl.sh", "./src/fakeverify.sh")).replace(
        "command = \"./src/fakecheck.sh\"",
        "command = \"./src/fakecheck.sh\"\ntimeout = \"2s\"",
    );
    let r = repo(&toml, REVIEW_TASK);
    implementer(&r, "");
    verifier(&r);
    script(&r, "src/fakecheck.sh", "sleep 30\n");
    r.commit_all("a hanging check");

    let (digest, events) = go(&r, &opts(1));
    assert!(
        digest.halts.iter().any(|h| h.contains("2s")),
        "{:?}",
        digest.halts
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.kind, Kind::Halt { reason, .. } if reason.contains("2s"))),
        "no halt event"
    );
    assert_eq!(
        state_of(&r, "T-001").as_deref(),
        Some("done"),
        "a hang overturned the verdict"
    );
}

#[test]
fn proposed_contract_halt() {
    let r = repo("", "");
    let scout = script(
        &r,
        "src/fakescout.sh",
        &queues(
            &r,
            &format!(
                "\n## [T-002] the schema is wrong\nstatus: needs-triage\nprobe: spec-untested\noutput: {} does not say which store\n",
                spec(&r)
            ),
        ),
    );
    write_toml(&r, &base_toml(&role_command("scout", &scout)));
    queue(&r, "# queue\n");
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h.contains("T-002") && h.contains("still proposed")),
        "{:?}",
        digest.halts
    );
}

#[test]
fn stage_output_streams_to_the_sink() {
    let toml = base_toml("").replace(
        "stages = [\"implement\", \"verify\", \"adjudicate\"]",
        "stages = [\"implement\"]",
    );
    let r = repo(&toml, TASKS);
    script(&r, "src/fakeagent.sh", "echo hello\nsleep 2\n");
    r.commit_all("a slow agent");

    let seen: Arc<Mutex<Vec<(std::time::Instant, &'static str)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    pipeline::run(
        &r.root,
        &opts(1),
        Box::new(move |e| {
            let name = match &e.kind {
                Kind::StageOutput { .. } => "output",
                Kind::StageEnd { .. } => "end",
                _ => return,
            };
            if let Ok(mut v) = sink.lock() {
                v.push((std::time::Instant::now(), name));
            }
        }),
    )
    .expect("the pipeline ran");

    let seen = seen.lock().unwrap().clone();
    let output = seen
        .iter()
        .find(|(_, k)| *k == "output")
        .expect("a stage.output");
    let end = seen.iter().find(|(_, k)| *k == "end").expect("a stage.end");
    // a live sink sees output a whole sleep before its stage finishes; post-hoc forwarding would deliver both at once
    assert!(
        end.0.duration_since(output.0) > std::time::Duration::from_millis(500),
        "output arrived only {:?} before the end",
        end.0.duration_since(output.0)
    );
}

#[test]
fn an_implementer_done_skips_verify() {
    let r = repo("", "");
    let implement = implementer(
        &r,
        &format!(
            "{bin} tasks set-status T-001 done 'I verified myself'\n",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let verify = script(&r, "src/fakeverify.sh", "touch verify-ran\n");
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(ends(&events).len(), 1, "verify may not spawn");
    assert!(!r.root.join("verify-ran").exists());
    assert!(
        digest
            .warnings
            .iter()
            .any(|w| w.contains("forced back to ready")),
        "{:?}",
        digest.warnings
    );
    assert!(digest.landed.is_empty(), "{:?}", digest.landed);
}

#[test]
fn short_of_review_skips_verify() {
    let r = repo("", "");
    let implement = implementer(
        &r,
        &format!(
            "{bin} tasks set-status T-001 ready-for-agent 'the fixture is missing'\n",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let verify = script(&r, "src/fakeverify.sh", "touch verify-ran\n");
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(ends(&events).len(), 1, "verify may not spawn");
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.kind, Kind::StageStart { stage, .. } if stage == "verify")),
        "{events:#?}"
    );
    assert!(!r.root.join("verify-ran").exists());
    let warning = "T-001 ended the iteration at ready-for-agent, not done.".to_string();
    assert!(digest.warnings.contains(&warning), "{:?}", digest.warnings);
}

fn with_verifier(r: &Repo, verify: &str) {
    write_toml(
        r,
        &base_toml(&format!(
            "\n[agent.verifier]\ncommand = [\"{verify}\", \"{{prompt}}\", \"{{turns}}\"]\n"
        )),
    );
}

#[test]
fn a_stranded_review_verifies_next_run() {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = verifier(&r);
    with_verifier(&r, &verify);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    let starts: Vec<&Kind> = events
        .iter()
        .map(|e| &e.kind)
        .filter(|k| matches!(k, Kind::StageStart { .. }))
        .collect();
    assert_eq!(starts.len(), 1, "only verify may spawn: {events:#?}");
    match starts[0] {
        Kind::StageStart { stage, task, .. } => {
            assert_eq!(stage, "verify");
            assert_eq!(task.as_deref(), Some("T-001"));
        }
        other => panic!("expected a StageStart, got {other:?}"),
    }

    assert_eq!(state_of(&r, "T-001").as_deref(), Some("done"));
    assert_eq!(digest.landed, vec!["T-001".to_string()]);
}

fn entries(r: &Repo) -> usize {
    std::fs::read_to_string(r.root.join(instance("PROGRESS.md")))
        .expect("PROGRESS.md")
        .lines()
        .filter(|l| l.starts_with("## "))
        .count()
}

#[test]
fn verify_only_one_entry() {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = verifier(&r);
    with_verifier(&r, &verify);
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(
        entries(&r),
        1,
        "{}",
        std::fs::read_to_string(r.root.join(instance("PROGRESS.md"))).unwrap()
    );
    assert!(
        !digest
            .warnings
            .iter()
            .any(|w| w.contains("no PROGRESS.md entry")),
        "{:?}",
        digest.warnings
    );
    let text = std::fs::read_to_string(r.root.join(instance("PROGRESS.md"))).expect("PROGRESS.md");
    assert!(text.contains("review pipeline"), "{text}");
    assert!(text.contains("\nfriction: none\n"), "{text}");
    assert!(
        enallagi::git::porcelain(&r.root).is_empty(),
        "the entry is committed"
    );
}

#[test]
fn implementer_entry_no_second() {
    let r = repo("", "");
    let implement = implementer(
        &r,
        &format!("echo '## stub entry' >>{}\n", instance("PROGRESS.md")),
    );
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(
        entries(&r),
        1,
        "{}",
        std::fs::read_to_string(r.root.join(instance("PROGRESS.md"))).unwrap()
    );
    assert!(
        !digest
            .warnings
            .iter()
            .any(|w| w.contains("no PROGRESS.md entry")),
        "{:?}",
        digest.warnings
    );
}

#[test]
fn review_wins_over_ready() {
    let r = repo(&base_toml(""), REVIEW_AND_READY_TASKS);
    let verify = verifier(&r);
    with_verifier(&r, &verify);
    r.commit_all("stubs");

    let (_, events) = go(&r, &opts(1));
    let verify_task = events.iter().find_map(|e| match &e.kind {
        Kind::StageStart { stage, task, .. } if stage == "verify" => task.clone(),
        _ => None,
    });
    assert_eq!(
        verify_task.as_deref(),
        Some("T-001"),
        "the review pipeline, not task, must win"
    );

    assert_eq!(state_of(&r, "T-001").as_deref(), Some("done"));
    assert_eq!(
        state_of(&r, "T-002").as_deref(),
        Some("ready-for-agent"),
        "T-002 is untouched: the task pipeline never ran"
    );
}

#[test]
fn landed_task_stays_whole() {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = verifier(&r);
    with_verifier(&r, &verify);
    r.write(&instance("DECISIONS.md"), "# DECISIONS\n");
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(digest.landed, vec!["T-001".to_string()]);

    let tasks = std::fs::read_to_string(r.root.join(ticket(&r, "T-001", "do the thing")))
        .expect("the queue");
    let decisions =
        std::fs::read_to_string(r.root.join(instance("DECISIONS.md"))).expect("DECISIONS.md");
    // a done ticket stays whole where it is
    assert!(tasks.contains("**Status:** done"), "{tasks}");
    assert!(tasks.contains("- [ ] it happens"), "{tasks}");
    assert!(!decisions.contains("T-001"), "{decisions}");
    // loop.pid still names this process on the way out, so the live-loop refusal must not fire
    assert!(
        !digest
            .warnings
            .iter()
            .any(|w| w.contains("an agent is running")),
        "{:?}",
        digest.warnings
    );
}

fn deferring_verifier(r: &Repo, proposed: &str) -> String {
    verifier_noting(
        r,
        "VERIFIED. Minor, not a reason for rejection: the test never removes the entry it registers.",
        proposed,
    )
}

fn verifier_noting(r: &Repo, note: &str, proposed: &str) -> String {
    script(
        r,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status T-001 done ''\n\
             {verified}\
             printf 'notes: {note}\\n' >>{ticket}\n\
             {queued}{QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            verified = verdict("verified by verifier"),
            ticket = ticket(r, "T-001", "do the thing"),
            queued = queues(r, proposed),
        ),
    )
}

#[test]
fn a_deferred_finding_with_no_block_is_refused() {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = deferring_verifier(&r, "");
    with_verifier(&r, &verify);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    let refusal = events.iter().find_map(|e| match &e.kind {
        Kind::Gate {
            gate, pass, reason, ..
        } if gate == "commit-verdict" && !*pass => Some(reason.clone()),
        _ => None,
    });
    let reason = refusal.unwrap_or_else(|| panic!("commit-verdict passed: {events:#?}"));
    assert!(reason.contains("minor"), "{reason}");
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("review"));
    assert!(digest.landed.is_empty(), "{:?}", digest.landed);
    assert!(
        enallagi::git::porcelain(&r.root).is_empty(),
        "the refused verdict is committed, not left in the tree"
    );
}

#[test]
fn same_verdict_filed() {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = deferring_verifier(
        &r,
        "\n## [T-002] the test never removes the entry it registers\nscope: src/thing.test.ts\nstatus: needs-triage\n",
    );
    with_verifier(&r, &verify);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            Kind::Gate { gate, pass, .. } if gate == "commit-verdict" && *pass)),
        "{events:#?}"
    );
    assert_eq!(digest.landed, vec!["T-001".to_string()]);
}

fn a_verdict_noting(note: &str) {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = verifier_noting(&r, note, "");
    with_verifier(&r, &verify);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            Kind::Gate { gate, pass, .. } if gate == "commit-verdict" && *pass)),
        "{note}: {events:#?}"
    );
    assert_eq!(digest.landed, vec!["T-001".to_string()], "{note}");
}

#[test]
fn a_verdict_saying_no_minor_issues_is_committed() {
    a_verdict_noting("VERIFIED. No minor issues.");
}

#[test]
fn a_verdict_saying_nothing_minor_is_committed() {
    a_verdict_noting("VERIFIED. Nothing minor to raise.");
}

#[test]
fn a_verdict_of_no_findings_is_committed() {
    a_verdict_noting("VERIFIED. No findings, minor or otherwise.");
}

#[test]
fn a_quoted_minor_is_not_the_verdict() {
    a_verdict_noting("VERIFIED.\\n> implementer: one minor edge left, handled");
}

#[test]
fn a_file_named_minor_is_not_the_verdict() {
    a_verdict_noting("VERIFIED. src/minor.test.ts and lib/for-later.ts pass.");
}

#[test]
fn a_test_named_minor_is_not_the_verdict() {
    a_verdict_noting("VERIFIED. `a_minor_case` and the_minor_path_is_covered pass.");
}

fn a_verdict_refused_for(note: &str) -> String {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = verifier_noting(&r, note, "");
    with_verifier(&r, &verify);
    r.commit_all("stubs");

    let (_, events) = go(&r, &opts(1));
    let refusal = events.iter().find_map(|e| match &e.kind {
        Kind::Gate {
            gate, pass, reason, ..
        } if gate == "commit-verdict" && !*pass => Some(reason.clone()),
        _ => None,
    });
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("review"), "{note}");
    refusal.unwrap_or_else(|| panic!("commit-verdict passed: {events:#?}"))
}

#[test]
fn a_deferred_line_with_no_block_names_the_field() {
    let reason = a_verdict_refused_for(
        "VERIFIED.\\ndeferred: the test never removes the entry it registers",
    );
    assert!(
        reason.starts_with("the verdict's `deferred:` line"),
        "{reason}"
    );
    assert!(
        reason.contains("the test never removes the entry it registers"),
        "{reason}"
    );
}

#[test]
fn a_prose_deferral_is_told_the_field() {
    let reason = a_verdict_refused_for("VERIFIED. Minor: the fixture leaks a file.");
    assert!(reason.contains("\"minor\""), "{reason}");
    assert!(reason.contains("`deferred:`"), "{reason}");
}

fn implementer_note(r: &Repo) -> String {
    format!(
        "printf '  implementer: one minor edge left\\n' >>{}\n",
        ticket(r, "T-001", "do the thing")
    )
}

fn a_clean_verdict_after_the_implementer(r: &Repo, implement: &str) {
    let verify = verifier(r);
    write_toml(r, &base_toml(&role_commands(implement, &verify)));
    queue(r, TASKS);
    r.commit_all("stubs");

    let (digest, events) = go(r, &opts(1));
    let refusal = events.iter().find_map(|e| match &e.kind {
        Kind::Gate {
            gate, pass, reason, ..
        } if gate == "commit-verdict" && !*pass => Some(reason.clone()),
        _ => None,
    });
    assert_eq!(refusal, None);
    assert_eq!(digest.landed, vec!["T-001".to_string()]);
}

#[test]
fn an_implementer_minor_is_not_the_verdict() {
    let r = repo("", "");
    let implement = implementer(&r, &implementer_note(&r));
    a_clean_verdict_after_the_implementer(&r, &implement);
}

#[test]
fn uncommitted_minor() {
    let r = repo("", "");
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "echo work >src/thing.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             git add -A >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {note}{QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            note = implementer_note(&r),
        ),
    );
    a_clean_verdict_after_the_implementer(&r, &implement);
}

#[test]
fn nested_lands_stub() {
    let r = Repo::new();
    r.write(
        ".enallagi/.gitignore",
        "events.jsonl\n*.log\nlogs/\nworktrees/\nloop.pid\nrun/\n__pycache__/\n",
    );
    script(&r, "src/fakecheck.sh", "exit 0\n");
    script(&r, "src/fakeagent.sh", QUIET);
    queue(&r, TASKS);
    r.write(&spec(&r), "# spec\n");
    r.write(".enallagi/PROGRESS.md", "# progress\n");
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "echo work >src/thing.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>.enallagi/PROGRESS.md\n\
             git add -A >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let verify = verifier(&r);
    let toml = base_toml(&role_commands(&implement, &verify));
    r.write(
        ".enallagi/enallagi.toml",
        &format!("{toml}{}", r.local_skills(&toml)),
    );
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(digest.landed, vec!["T-001".to_string()], "{events:#?}");
    let tasks = tasks_text(&r);
    assert!(tasks.contains("status: done"), "{tasks}");
    for name in [
        "specs",
        "PROGRESS.md",
        "DECISIONS.md",
        "SPEC.md",
        "enallagi.toml",
        "harness.lock",
    ] {
        assert!(
            !r.root.join(name).exists(),
            "{name} was written at the root"
        );
    }
    assert!(r.root.join(".enallagi/harness.lock").is_file());
    assert!(enallagi::git::porcelain(&r.root).is_empty());
}

#[test]
fn install_lands_task() {
    let r = Repo::new();
    enallagi::init::install(
        &r.root,
        &enallagi::init::InitOpts {
            adapter: Some("claude".to_string()),
            ..Default::default()
        },
    )
    .expect("install");
    script(&r, "src/fakecheck.sh", "exit 0\n");
    script(&r, "src/fakeagent.sh", QUIET);
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "echo work >src/thing.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>.enallagi/PROGRESS.md\n\
             git add -A >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let verify = verifier(&r);
    let toml = base_toml(&role_commands(&implement, &verify));
    r.write(
        ".enallagi/enallagi.toml",
        &format!("{toml}{}", r.local_skills(&toml)),
    );
    queue(&r, TASKS);
    r.commit_all("installed");
    let base = enallagi::git::git(&r.root, &["rev-parse", "HEAD"]).expect("HEAD");

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(digest.landed, vec!["T-001".to_string()], "{events:#?}");
    let changed =
        enallagi::git::git(&r.root, &["diff", "--name-only", &base, "HEAD"]).expect("diff");
    assert!(changed.lines().any(|p| p == "src/thing.ts"), "{changed}");
    for path in changed.lines() {
        assert!(
            path.starts_with(".enallagi/") || path == "src/thing.ts",
            "{path} changed outside the harness directory"
        );
    }
    for name in [
        "specs",
        "PROGRESS.md",
        "SPEC.md",
        "enallagi.toml",
        "harness.lock",
    ] {
        assert!(!r.root.join(name).exists(), "{name} is at the root");
    }
    assert!(enallagi::git::porcelain(&r.root).is_empty());
}

#[test]
fn claude_install_no_dot_claude() {
    let r = Repo::new();
    enallagi::init::install(
        &r.root,
        &enallagi::init::InitOpts {
            adapter: Some("claude".to_string()),
            ..Default::default()
        },
    )
    .expect("install");
    script(&r, "src/fakecheck.sh", "exit 0\n");
    script(&r, "src/fakeagent.sh", QUIET);
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "echo work >src/thing.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>.enallagi/PROGRESS.md\n\
             git add src/thing.ts >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let verify = verifier(&r);
    let toml = base_toml(&role_commands(&implement, &verify)).replace(
        "preset = \"custom\"\ncommand = [\"./src/fakeagent.sh\", \"{prompt}\", \"{turns}\"]",
        "preset = \"claude\"",
    );
    assert!(toml.contains("preset = \"claude\""));
    r.write(
        ".enallagi/enallagi.toml",
        &format!("{toml}{}", r.local_skills(&toml)),
    );
    queue(&r, TASKS);
    r.commit_all("installed");

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(digest.landed, vec!["T-001".to_string()], "{events:#?}");
    assert!(r
        .root
        .join(".enallagi/adapters/claude/skills/tdd/SKILL.md")
        .is_file());
    assert!(!r.root.join(".claude").exists());
    assert!(enallagi::git::porcelain(&r.root).is_empty());
}

fn in_dir(dir: &std::path::Path, args: &[&str]) -> String {
    enallagi::git::git(dir, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

// the harness directory is its own repository, ignored by the product's
fn nested(check: &str, implement_extra: &str) -> Repo {
    let r = Repo::new();
    r.write(".git/info/exclude", ".enallagi/\n");
    r.write(
        ".enallagi/.gitignore",
        "events.jsonl\n*.log\nlogs/\nworktrees/\nloop.pid\nrun/\n__pycache__/\n",
    );
    script(&r, "src/fakecheck.sh", check);
    script(&r, "src/fakeagent.sh", QUIET);
    queue(&r, TASKS);
    r.write(&spec(&r), "# spec\n");
    r.write(".enallagi/PROGRESS.md", "# progress\n");
    r.write(".enallagi/.check-baseline", "# inherited red\n");
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "echo work >src/thing.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>.enallagi/PROGRESS.md\n\
             {implement_extra}\
             git add src/thing.ts >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let verify = verifier(&r);
    let toml = base_toml(&role_commands(&implement, &verify));
    r.write(
        ".enallagi/enallagi.toml",
        &format!("{toml}{}", r.local_skills(&toml)),
    );
    r.commit_all("stubs");
    let state = r.root.join(".enallagi");
    in_dir(&state, &["init", "-q"]);
    in_dir(&state, &["config", "user.name", "someone else"]);
    in_dir(&state, &["config", "user.email", "else@else"]);
    in_dir(&state, &["add", "-A"]);
    in_dir(
        &state,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "queue"],
    );
    r
}

fn nested_status(r: &Repo) -> Option<String> {
    state_of(r, "T-001")
}

#[test]
fn unsupported_done_forced_back() {
    let r = nested("echo '(fail) alpha'\nexit 1\n", "");

    let (digest, events) = go(&r, &opts(1));
    assert!(digest.landed.is_empty(), "{events:#?}");
    assert_eq!(nested_status(&r).as_deref(), Some("ready-for-agent"));
    let state_log = in_dir(&r.root.join(".enallagi"), &["log", "--format=%s"]);
    assert!(
        state_log
            .lines()
            .any(|s| s.starts_with("chore(T-001): enallagi gate rejected a false VERIFIED at ")),
        "{state_log}"
    );
    assert!(in_dir(&r.root.join(".enallagi"), &["status", "--porcelain"]).is_empty());
}

#[test]
fn nested_grown_baseline() {
    let r = nested("exit 0\n", "echo alpha >>.enallagi/.check-baseline\n");

    let (digest, events) = go(&r, &opts(1));
    assert!(digest.landed.is_empty(), "{events:#?}");
    // back to the implementer, the one role that can remove the line; the commits stay on the branch
    assert_eq!(nested_status(&r).as_deref(), Some("ready-for-agent"));
    let refusal = events.iter().find_map(|e| match &e.kind {
        Kind::Gate {
            gate, pass, reason, ..
        } if gate == "scope" && !*pass => Some(reason.clone()),
        _ => None,
    });
    let reason = refusal.unwrap_or_else(|| panic!("scope passed: {events:#?}"));
    assert!(
        reason.contains("added a line to .check-baseline"),
        "{reason}"
    );
}

// the implement round rewrites the ticket, widening scope: with no `widened:` line
const WIDENED: &str = "cat > .enallagi/specs/*/issues/T-001-*.md <<'EOF'\n\
# T-001: do the thing\n\
\n\
**Status:** review\n\
**Blocked by:** None\n\
\n\
## Acceptance criteria\n\
\n\
- [ ] it happens\n\
\n\
## Harness\n\
\n\
scope: src/thing.ts, src/other.ts\n\
rows: none \u{2014} harness\n\
EOF\n";

// the implement round that follows a scope refusal finds its commits on the branch and answers the
// gate: it adds the `widened:` line, commits no product change, and the task lands on the widening
#[test]
fn scope_refusal_answered() {
    let confirm = format!(
        "t=$(ls .enallagi/specs/*/issues/T-001-*.md)\n\
         if grep -q 'scope gate rejected it' \"$t\"; then\n\
         awk '{{print}} /^scope: src\\/thing.ts, src\\/other.ts$/{{print \"widened: src/other.ts holds the flag\"}}' \"$t\" >\"$t.new\"\n\
         mv \"$t.new\" \"$t\"\n\
         else\n{WIDENED}fi\n"
    );
    let r = nested("exit 0\n", &confirm);

    let (digest, events) = go(&r, &opts(3));
    assert_eq!(digest.landed, vec!["T-001".to_string()], "{events:#?}");
    let refusals = events
        .iter()
        .filter(|e| matches!(&e.kind, Kind::Gate { gate, pass, .. } if gate == "scope" && !*pass))
        .count();
    assert_eq!(refusals, 1, "{events:#?}");
    assert!(events.iter().any(|e| matches!(&e.kind,
        Kind::TaskStatus { to, by, .. } if to == "ready-for-agent" && by == "scope")));
    // one product commit: the confirm round committed nothing
    let subjects = in_dir(&r.root, &["log", "--format=%s"]);
    assert_eq!(
        subjects.lines().filter(|l| *l == "T-001: stub").count(),
        1,
        "{subjects}"
    );
    let passed = events.iter().find_map(|e| match &e.kind {
        Kind::Gate {
            gate, pass, reason, ..
        } if gate == "scope" && *pass && reason.contains("widened scope") => Some(reason.clone()),
        _ => None,
    });
    let reason = passed.unwrap_or_else(|| panic!("no pass on the widening: {events:#?}"));
    assert!(reason.contains("holds the flag"), "{reason}");
    assert!(digest.halts.is_empty(), "{:?}", digest.halts);
}

// the widening lands in the first implement round, so the second round's own bases already carry
// it and only the queued commit still shows it; a lane that widens silently twice halts the run
#[test]
fn review_reads_queued_scope() {
    let r = nested("exit 0\n", WIDENED);

    let (digest, events) = go(&r, &opts(3));
    assert!(digest.landed.is_empty(), "{events:#?}");
    assert_eq!(nested_status(&r).as_deref(), Some("ready-for-agent"));
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h.contains("T-001 was refused by the scope gate twice this run")),
        "{:?}",
        digest.halts
    );
    assert_eq!(digest.iterations, 2, "{events:#?}");
    let refusals: Vec<String> = events
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Gate {
                gate, pass, reason, ..
            } if gate == "scope" && !*pass => Some(reason.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(refusals.len(), 2, "{events:#?}");
    assert!(
        refusals
            .iter()
            .all(|r| r.contains("widened scope: with src/other.ts")),
        "{refusals:?}"
    );
    let tasks = tasks_text(&r);
    assert!(
        tasks.contains("gate: ") && tasks.contains("widened scope: with src/other.ts"),
        "{tasks}"
    );
}

#[test]
fn landed_leaves_no_instance_path() {
    let r = nested("exit 0\n", "");
    let before = in_dir(&r.root, &["rev-parse", "HEAD"]);

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(digest.landed, vec!["T-001".to_string()], "{events:#?}");

    let paths = in_dir(&r.root, &["log", "--name-only", "--format="]);
    assert!(paths.lines().any(|p| p == "src/thing.ts"), "{paths}");
    for path in paths.lines().filter(|p| !p.is_empty()) {
        assert!(
            !path.starts_with(".enallagi")
                && ![
                    "specs",
                    "PROGRESS.md",
                    "DECISIONS.md",
                    ".check-baseline",
                    "harness.lock"
                ]
                .contains(&path),
            "{path} is an instance path in the product history"
        );
    }

    let state = r.root.join(".enallagi");
    let after = in_dir(&r.root, &["rev-parse", "HEAD"]);
    assert_ne!(before, after);
    let subjects = in_dir(&state, &["log", "--format=%s|%an|%ae|%b"]);
    assert!(
        subjects
            .lines()
            .any(|l| l == format!("implement T-001 at {after}|t|t@t|")),
        "{subjects}"
    );
    assert!(in_dir(&state, &["status", "--porcelain"]).is_empty());
    assert!(enallagi::git::porcelain(&r.root).is_empty());
}

// A drain round needs a block filed this iteration; the verifier files one when its rejection turns
// something up, which is where every finding this session came from.
const FILED: &str = "\n## [T-009] the fence scan reads one language only\n\nscope: src/thing.ts\nrows: none — harness\nstatus: needs-triage\ncriteria:\n  - the scan reads every fence\n";

const RESTATED: &str = "\n## [T-009] the check was red at HEAD\n\nscope: src/thing.ts\nrows: none — harness\nstatus: needs-triage\ncriteria:\n  - the check was red at HEAD\n";

const REJECTION: &str = "the check was red at HEAD";

const NOTED_REJECTION: &str =
    "the fence scan reads one language only and nothing in the queue says so";

const NOTED_RESTATED: &str = "\n## [T-009] the fence scan reads one language only\n\nscope: src/thing.ts\nrows: none — harness\nstatus: needs-triage\ncriteria:\n  - the fence scan reads one language only and nothing in the queue says so\n";

const ATTENDED_TASKS: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: ready-for-agent
criteria:
  - it happens

## [T-002] the one a human runs

scope: src/other.ts
rows: none — harness
status: ready-for-human
criteria:
  - it happens
";

fn role_command(role: &str, path: &str) -> String {
    format!("\n[agent.{role}]\ncommand = [\"{path}\", \"{{prompt}}\", \"{{turns}}\"]\n")
}

fn rejecting_verifier(r: &Repo) -> String {
    script(
        r,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status T-001 ready-for-agent '{REJECTION}'\n\
             {rejected}\
             {filing}\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            rejected = verdict(&format!("rejected by verifier: {REJECTION}")),
            filing = filing(r),
        ),
    )
}

// the role prompt has the verifier edit the block and append `REJECTED` to notes; only set-status writes a `gate:` line
fn verifier_rejecting_in_notes(r: &Repo) -> String {
    script(
        r,
        "src/fakeverify.sh",
        &format!(
            "sed -i.bak 's/^\\*\\*Status:\\*\\* review$/**Status:** ready-for-agent/' {t}\n\
             rm -f {t}.bak\n\
             {rejected}\
             printf '\\n## Comments\\n\\nVerifier: REJECTED on criterion 1. {NOTED_REJECTION}. Re-run it.\\n' >>{t}\n\
             {filing}\
             {QUIET}",
            t = ticket(r, "T-001", "do the thing"),
            filing = filing(r),
            rejected = verdict(&format!("rejected by verifier: {NOTED_REJECTION}")),
        ),
    )
}

// files what `draining_with` wrote to src/filed.md as its ticket file; empty files nothing
fn filing(r: &Repo) -> String {
    let to = ticket(r, "T-009", "filed");
    format!("if [ -s src/filed.md ]; then cat src/filed.md >{to}; fi\n")
}

fn write_filed(r: &Repo, filed: &str) {
    let filed = match enallagi::block::parse(filed).expect("parse").first() {
        Some(b) => enallagi::tracker::ticket_text(b),
        None => String::new(),
    };
    r.write("src/filed.md", &filed);
}

fn adjudicator(r: &Repo, body: &str) -> String {
    script(r, "src/fakeadj.sh", &format!("{body}{QUIET}"))
}

fn promotes(id: &str) -> String {
    format!(
        "{bin} tasks set-status {id} ready-for-agent 'promoted'\n",
        bin = env!("CARGO_BIN_EXE_enallagi"),
    )
}

// one repo whose verifier rejects, files `filed`, and whose adjudicator runs `body`
fn draining(r: &Repo, filed: &str, body: &str) {
    let verify = rejecting_verifier(r);
    draining_with(r, &verify, filed, body);
}

fn draining_with(r: &Repo, verify: &str, filed: &str, body: &str) {
    let implement = implementer(r, "");
    let adj = adjudicator(r, body);
    let roles = format!(
        "{}{}",
        role_commands(&implement, verify),
        role_command("adjudicator", &adj)
    );
    write_toml(r, &base_toml(&roles));
    queue(r, TASKS);
    write_filed(r, filed);
    r.commit_all("stubs");
}

fn stages_started(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::StageStart { stage, .. } => Some(stage.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn adjudicate_follows_a_rejection_in_one_round() {
    let r = repo("", "");
    draining(&r, FILED, &promotes("T-009"));

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(
        stages_started(&events),
        vec!["implement", "verify", "adjudicate"],
        "{events:#?}"
    );
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("ready-for-agent"));
    assert!(
        digest.promoted.contains(&"T-009".to_string()),
        "{:?}",
        digest.promoted
    );
}

#[test]
fn a_verdict_filing_nothing_skips_adjudicate() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    let adj = adjudicator(&r, "touch adjudicate-ran\n");
    let roles = format!(
        "{}{}",
        role_commands(&implement, &verify),
        role_command("adjudicator", &adj)
    );
    write_toml(&r, &base_toml(&roles));
    queue(&r, TASKS);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(
        stages_started(&events),
        vec!["implement", "verify"],
        "{events:#?}"
    );
    assert!(!r.root.join("adjudicate-ran").exists());
    assert_eq!(digest.landed, vec!["T-001".to_string()]);
}

// one fixture, every arm: what the verifier files decides whether the stage spawns at all, and the
// adjudicator's body decides how it ended
fn adjudicating_with(filed: &str, body: &str) -> Digest {
    let r = repo("", "");
    draining(&r, filed, body);
    go(&r, &opts(1)).0
}

fn adjudicating(filed: &str) -> Digest {
    adjudicating_with(filed, "")
}

#[test]
fn the_digest_tells_no_adjudicator_from_no_decision() {
    let ran = pipeline::digest_text(&adjudicating(FILED));
    assert!(
        ran.contains("findings: the adjudicator ran and decided nothing"),
        "{ran}"
    );

    let skipped = pipeline::digest_text(&adjudicating(""));
    assert!(
        skipped.contains("findings: no adjudicate stage ran"),
        "{skipped}"
    );
}

#[test]
fn a_halted_adjudicator_is_not_a_decision() {
    let halted = pipeline::digest_text(&adjudicating_with(
        FILED,
        "echo 'HALT T-009: the fix needs SPEC.md'\n",
    ));
    assert!(
        halted.contains("findings: the adjudicator could not decide"),
        "{halted}"
    );
}

#[test]
fn a_nonzero_adjudicator_is_not_a_decision() {
    let failed = pipeline::digest_text(&adjudicating_with(FILED, "exit 1\n"));
    assert!(
        failed.contains("findings: the adjudicator could not decide"),
        "{failed}"
    );
}

#[test]
fn a_stage_at_its_turn_cap_is_named_in_the_digest() {
    let r = repo(&base_toml(""), "");
    queue(&r, "# queue\n");
    script(
        &r,
        "src/fakeagent.sh",
        "echo '{\"total_cost_usd\":0.5,\"num_turns\":5}'\n",
    );
    r.commit_all("a scout that spends every turn");

    let (digest, _) = go(&r, &opts(1));
    let text = pipeline::digest_text(&digest);
    assert!(text.contains("turn caps hit:\n  scout: turns 5"), "{text}");
}

#[test]
fn digest_and_probe_agree_past_the_turn_cap() {
    use enallagi::probes::telemetry::{turns_exhausted, ProbeResult};
    let r = repo(&base_toml(""), "");
    queue(&r, "# queue\n");
    script(
        &r,
        "src/fakeagent.sh",
        "echo '{\"total_cost_usd\":0.5,\"num_turns\":6}'\n",
    );
    r.commit_all("a scout reporting one turn past its cap");

    let (digest, _) = go(&r, &opts(1));
    let in_digest = pipeline::digest_text(&digest).contains("scout: turns 5");
    let cfg = enallagi::config::load(&r.root).expect("config");
    let log = enallagi::events::Log::open(&r.root.join(".enallagi"));
    let ProbeResult::Count(findings) = turns_exhausted(&log, &cfg) else {
        panic!("turns_exhausted could not read the log");
    };
    let in_probe = findings.iter().any(|f| f.message.contains("stage scout"));
    assert_eq!(in_digest, in_probe, "digest {in_digest}, probe {in_probe}");
}

#[test]
fn digest_and_probe_agree_on_an_uncapped_stage() {
    use enallagi::probes::telemetry::{turns_exhausted, ProbeResult};
    let toml = base_toml("").replacen(
        "command = [\"./src/fakeagent.sh\", \"{prompt}\", \"{turns}\"]",
        "command = [\"./src/fakeagent.sh\", \"{prompt}\"]",
        1,
    );
    let r = repo(&toml, "");
    queue(&r, "# queue\n");
    script(
        &r,
        "src/fakeagent.sh",
        "echo '{\"total_cost_usd\":0.5,\"num_turns\":6}'\n",
    );
    r.commit_all("a scout reporting one turn past a cap it was never handed");

    let (digest, _) = go(&r, &opts(1));
    let in_digest = pipeline::digest_text(&digest).contains("scout: turns 5");
    let cfg = enallagi::config::load(&r.root).expect("config");
    let log = enallagi::events::Log::open(&r.root.join(".enallagi"));
    let ProbeResult::Count(findings) = turns_exhausted(&log, &cfg) else {
        panic!("turns_exhausted could not read the log");
    };
    let in_probe = findings.iter().any(|f| f.message.contains("stage scout"));
    assert_eq!(in_digest, in_probe, "digest {in_digest}, probe {in_probe}");
    assert!(!in_probe, "the probe reported an uncapped stage");
}

// turns = 20 and one block handed at turns_per_block = 25 is a cap of 45, not 20
fn adjudicating_at_turns(reported: u64) -> Repo {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = rejecting_verifier(&r);
    let adj = script(
        &r,
        "src/fakeadj.sh",
        &format!("echo '{{\"total_cost_usd\":0.5,\"num_turns\":{reported}}}'\n"),
    );
    let roles = format!(
        "{}{}",
        role_commands(&implement, &verify),
        role_command("adjudicator", &adj)
    );
    let toml = base_toml(&roles)
        .replace("[check]", "[queue]\nturns_per_block = 25\n\n[check]")
        .replace(
            "name = \"adjudicate\"\nrole = \"adjudicator\"\nturns = 5\n",
            "name = \"adjudicate\"\nrole = \"adjudicator\"\nturns = 20\n",
        );
    write_toml(&r, &toml);
    queue(&r, TASKS);
    write_filed(&r, FILED);
    r.commit_all("stubs");
    r
}

#[test]
fn a_handed_adjudicator_reads_one_turn_cap() {
    use enallagi::probes::telemetry::{turns_exhausted, ProbeResult};
    for (reported, named) in [(30, false), (45, true)] {
        let r = adjudicating_at_turns(reported);
        let (digest, _) = go(&r, &opts(1));
        let in_digest = pipeline::digest_text(&digest).contains("adjudicate: turns");
        let cfg = enallagi::config::load(&r.root).expect("config");
        let log = enallagi::events::Log::open(&r.root.join(".enallagi"));
        let ProbeResult::Count(findings) = turns_exhausted(&log, &cfg) else {
            panic!("turns_exhausted could not read the log");
        };
        let in_probe = findings
            .iter()
            .any(|f| f.message.contains("stage adjudicate"));
        assert_eq!(in_digest, named, "digest at {reported} turns");
        assert_eq!(in_probe, named, "probe at {reported} turns");
    }
}

#[test]
fn an_uncapped_stage_is_not_named_as_a_turn_cap() {
    let toml = base_toml("").replace(
        r#"command = ["./src/fakeagent.sh", "{prompt}", "{turns}"]"#,
        r#"command = ["./src/fakeagent.sh", "{prompt}"]"#,
    );
    let r = repo(&toml, "");
    queue(&r, "# queue\n");
    script(
        &r,
        "src/fakeagent.sh",
        "echo '{\"total_cost_usd\":0.5,\"num_turns\":5}'\n",
    );
    r.commit_all("a scout whose command carries no turn flag");

    let (digest, _) = go(&r, &opts(1));
    let text = pipeline::digest_text(&digest);
    assert!(!text.contains("turn caps hit:"), "{text}");
}

#[test]
fn a_timed_out_stage_is_named_as_a_timeout() {
    let toml = base_toml("")
        .replace(
            r#"command = ["./src/fakeagent.sh", "{prompt}", "{turns}"]"#,
            r#"command = ["./src/fakeagent.sh", "{prompt}", "{timeout}"]"#,
        )
        .replace(
            "name = \"scout\"\nrole = \"scout\"\nturns = 5\n",
            "name = \"scout\"\nrole = \"scout\"\nturns = 5\ntimeout = \"1s\"\n",
        );
    let r = repo(&toml, "");
    queue(&r, "# queue\n");
    script(
        &r,
        "src/fakeagent.sh",
        "echo '{\"total_cost_usd\":0.5,\"num_turns\":5}'\nsleep 5\n",
    );
    r.commit_all("a scout that outlives its stage timeout");

    let (digest, _) = go(&r, &opts(1));
    let text = pipeline::digest_text(&digest);
    assert!(
        text.contains("timeouts hit:\n  scout: timeout 1s"),
        "{text}"
    );
    assert!(!text.contains("turn caps hit:"), "{text}");
}

#[test]
fn the_adjudicate_gates_run_in_a_task_round() {
    let r = repo("", "");
    draining(&r, FILED, &promotes("T-009"));

    let (_, events) = go(&r, &opts(1));
    let at = events
        .iter()
        .position(|e| matches!(&e.kind, Kind::StageStart { stage, .. } if stage == "adjudicate"))
        .expect("the adjudicate stage started");
    let gates: Vec<&str> = events[at..]
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Gate { gate, .. } => Some(gate.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        gates,
        vec![
            "commit-round",
            "queue-intact",
            "adjudicator-halt",
            "dry-round"
        ],
        "{events:#?}"
    );
}

#[test]
fn a_promotion_repeating_a_rejection_is_refused() {
    let r = repo("", "");
    draining(&r, RESTATED, &promotes("T-009"));

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(state_of(&r, "T-009").as_deref(), Some("needs-triage"));
    assert!(
        !digest.promoted.contains(&"T-009".to_string()),
        "{:?}",
        digest.promoted
    );
    let named = digest
        .warnings
        .iter()
        .find(|w| w.contains("T-009") && w.contains("T-001") && w.contains(REJECTION));
    assert!(named.is_some(), "{:?}", digest.warnings);
}

#[test]
fn no_scout_adjudicates() {
    let r = repo("", "");
    draining(&r, FILED, &promotes("T-009"));
    let toml =
        std::fs::read_to_string(r.root.join(".enallagi/enallagi.toml")).expect("enallagi.toml");
    let toml = toml
        .replace(
            "[[pipeline]]\nname = \"discover\"\nwhen = \"!queue.takeable\"\nstages = [\"scout\", \"adjudicate\"]\nend_after_dry_rounds = 2\n",
            "",
        )
        .replace("[[stage]]\nname = \"scout\"\nrole = \"scout\"\nturns = 5\n", "");
    assert!(!toml.contains("scout"), "{toml}");
    r.write(".enallagi/enallagi.toml", &toml);
    r.commit_all("no scout");

    let (digest, events) = go(&r, &opts(1));
    assert!(
        stages_started(&events).contains(&"adjudicate".to_string()),
        "{events:#?}"
    );
    assert!(
        digest.promoted.contains(&"T-009".to_string()),
        "{:?}",
        digest.promoted
    );
}

#[test]
fn a_stage_that_exits_records_what_it_promoted() {
    let r = repo("", "");
    draining(&r, FILED, &format!("{}exit 1\n", promotes("T-009")));

    let (digest, _) = go(&r, &opts(1));
    assert!(
        digest.promoted.contains(&"T-009".to_string()),
        "{:?}",
        digest.promoted
    );
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h.contains("adjudicate exited 1")),
        "{:?}",
        digest.halts
    );
}

#[test]
fn the_dry_plan_names_adjudicate_after_verify() {
    let r = repo(&base_toml(""), TASKS);
    let plan = plan_of(&r);
    let at = plan
        .find("=== pipeline task (queue.takeable) ===")
        .unwrap_or_else(|| panic!("{plan}"));
    let section = &plan[at..];
    let section = &section[..section[1..]
        .find("=== pipeline ")
        .map(|i| i + 1)
        .unwrap_or(section.len())];
    let verify = section.find("would spawn: verify").expect(section);
    let adjudicate = section.find("would spawn: adjudicate").expect(section);
    assert!(verify < adjudicate, "{section}");
}

#[test]
fn task_pipeline_adjudicates() {
    let r = Repo::new();
    r.write(".enallagi/enallagi.toml", "[check]\ncommand = \"true\"\n");
    let cfg = enallagi::config::load(&r.root).expect("load");
    let task = cfg
        .pipeline
        .iter()
        .find(|p| p.name == "task")
        .expect("the task pipeline");
    assert_eq!(task.stages, vec!["implement", "verify", "adjudicate"]);
}

#[test]
fn triage_before_discover() {
    let r = Repo::new();
    r.write(".enallagi/enallagi.toml", "[check]\ncommand = \"true\"\n");
    let cfg = enallagi::config::load(&r.root).expect("load");
    let triage = cfg
        .pipeline
        .iter()
        .find(|p| p.name == "triage")
        .expect("the triage pipeline");
    assert_eq!(triage.when, "queue.proposed");
    assert_eq!(triage.stages, vec!["adjudicate"]);
    let names: Vec<&str> = cfg.pipeline.iter().map(|p| p.name.as_str()).collect();
    assert!(
        names.iter().position(|n| *n == "triage") < names.iter().position(|n| *n == "discover"),
        "{names:?}"
    );
}

#[test]
fn an_attended_block_waits_for_a_scout_round() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    let scout = script(&r, "src/fakescout.sh", QUIET);
    let roles = format!(
        "{}{}",
        role_commands(&implement, &verify),
        role_command("scout", &scout)
    );
    write_toml(&r, &base_toml(&roles));
    queue(&r, ATTENDED_TASKS);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(2));
    assert!(
        stages_started(&events).contains(&"scout".to_string()),
        "{events:#?}"
    );
    assert!(
        !digest
            .halts
            .iter()
            .any(|h| h.contains("A human has to run it")),
        "{:?}",
        digest.halts
    );
}

#[test]
fn adjudicator_names_filed_ids() {
    let r = repo("", "");
    draining(&r, FILED, "printf '%s' \"$1\" >adjudicate-prompt\n");

    go(&r, &opts(1));
    let prompt = std::fs::read_to_string(r.root.join("adjudicate-prompt")).expect("the prompt");
    assert!(prompt.contains("T-009"), "{prompt}");
}

const REJECTED_TASK: &str = "\
## [T-001] do the thing

scope: src/thing.ts, implement-prompt
rows: none — harness
status: ready-for-agent
criteria:
  - it happens
verdict: rejected by verifier: the red deleted the whole call site, so it covered no branch.
notes: REJECTED: an older verdict quoted in prose.
";

fn implementing(r: &Repo, tasks: &str) {
    let implement = implementer(r, "printf '%s' \"$1\" >implement-prompt\n");
    let verify = verifier(r);
    write_toml(r, &base_toml(&role_commands(&implement, &verify)));
    queue(r, tasks);
    r.commit_all("stubs");
}

fn implement_prompt(r: &Repo) -> String {
    std::fs::read_to_string(r.root.join("implement-prompt")).expect("the prompt")
}

#[test]
fn the_implement_prompt_names_a_rejected_attempt() {
    let r = repo("", "");
    implementing(&r, REJECTED_TASK);
    r.write("src/thing.ts", "first attempt\n");
    r.commit_all("T-001: first attempt");
    let attempt = enallagi::git::head(&r.root).expect("the attempt sha");

    go(&r, &opts(1));
    let prompt = implement_prompt(&r);
    assert!(prompt.contains(&format!("git show {attempt}")), "{prompt}");
    assert!(prompt.contains("it covered no branch"), "{prompt}");
    assert!(!prompt.contains("older verdict"), "{prompt}");
}

#[test]
fn the_implement_prompt_refuses_a_resent_diff() {
    let r = repo("", "");
    implementing(&r, REJECTED_TASK);
    r.write("src/thing.ts", "first attempt\n");
    r.commit_all("T-001: first attempt");

    go(&r, &opts(1));
    let prompt = implement_prompt(&r);
    assert!(prompt.contains("Do not re-send a diff"), "{prompt}");
    assert!(prompt.contains("rejected for the same reason"), "{prompt}");
}

#[test]
fn a_task_with_no_attempt_leaves_the_prompt_alone() {
    let r = repo("", "");
    implementing(&r, REJECTED_TASK);

    go(&r, &opts(1));
    let prompt = implement_prompt(&r);
    assert!(!prompt.contains("prior implementation attempt"), "{prompt}");
    assert!(!prompt.contains("git show"), "{prompt}");
}

#[test]
fn a_longer_id_is_not_read_as_a_prior_attempt() {
    let r = repo("", "");
    implementing(&r, REJECTED_TASK);
    r.write("src/thing.ts", "another task\n");
    r.commit_all("T-0012: a different task");

    go(&r, &opts(1));
    let prompt = implement_prompt(&r);
    assert!(!prompt.contains("prior implementation attempt"), "{prompt}");
}

#[test]
fn pinned_run_own_ticket() {
    let r = repo(
        &base_toml(""),
        "## [T-001] one\nscope: src/one.ts\nstatus: ready-for-agent\n\n## [T-002] two\nscope: src/thing.ts\nstatus: ready-for-agent\n\n## [T-003] three\nstatus: needs-triage\n",
    );
    let bin = env!("CARGO_BIN_EXE_enallagi");
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "printf '%s' \"$1\" >>implement-prompt\n\
             echo work >src/thing.ts\n\
             {bin} tasks set-status T-002 review 'stub implemented'\n\
             echo 'iteration' >>{progress}\n\
             git add -- src/thing.ts >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-002: stub' >/dev/null 2>&1\n\
             {QUIET}",
            progress = instance("PROGRESS.md"),
        ),
    );
    let verify = script(
        &r,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status T-002 done 'stub verified'\n{verified}{QUIET}",
            verified = verdict("verified by verifier").replace("T-001", "T-002"),
        ),
    );
    write_toml(&r, &(base_toml("") + &role_commands(&implement, &verify)));
    r.write(".gitignore", "implement-prompt\n");
    r.commit_all("stubs");

    let (_, events) = go(
        &r,
        &RunOpts {
            task: Some("T-002".into()),
            ..opts(3)
        },
    );

    assert_eq!(state_of(&r, "T-002").as_deref(), Some("done"));
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("ready-for-agent"));
    let stages: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::StageStart { stage, .. } => Some(stage.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(stages, ["implement", "verify"], "{events:#?}");
    let prompt = std::fs::read_to_string(r.root.join("implement-prompt")).expect("prompt");
    assert!(
        prompt.contains("Complete exactly ONE ticket: T-002."),
        "{prompt}"
    );
}

#[test]
fn a_task_round_does_not_spend_the_discovery_budget() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    let scout = script(&r, "src/fakescout.sh", QUIET);
    let roles = format!(
        "{}{}",
        role_commands(&implement, &verify),
        role_command("scout", &scout)
    );
    write_toml(&r, &base_toml(&roles));
    queue(&r, TASKS);
    r.commit_all("stubs");

    // one task round, then the discover pipeline's two dry rounds: the task round is not one of them
    let (digest, _) = go(&r, &opts(5));
    assert_eq!(digest.iterations, 3, "{digest:#?}");
}

#[test]
fn a_promotion_repeating_a_noted_verdict_is_refused() {
    let r = repo("", "");
    let verify = verifier_rejecting_in_notes(&r);
    draining_with(&r, &verify, NOTED_RESTATED, &promotes("T-009"));

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(state_of(&r, "T-009").as_deref(), Some("needs-triage"));
    assert!(
        !digest.promoted.contains(&"T-009".to_string()),
        "{:?}",
        digest.promoted
    );
    let named = digest
        .warnings
        .iter()
        .find(|w| w.contains("T-009") && w.contains("T-001") && w.contains(NOTED_REJECTION));
    assert!(named.is_some(), "{:?}", digest.warnings);
}

fn standing(n: u32) -> String {
    format!("\n## [T-{n}] a standing proposal\n\nscope: src/thing.ts\nrows: none — harness\nstatus: needs-triage\ncriteria:\n  - it happens\n")
}

#[test]
fn a_standing_pile_drains_oldest_first() {
    let r = repo("", "");
    let verify = rejecting_verifier(&r);
    let implement = implementer(&r, "");
    let adj = adjudicator(
        &r,
        "printf '%s' \"$1\" >adjudicate-prompt\nprintf '%s' \"$2\" >adjudicate-turns\n",
    );
    let roles = format!(
        "{}{}",
        role_commands(&implement, &verify),
        role_command("adjudicator", &adj)
    );
    write_toml(
        &r,
        &base_toml(&format!("{roles}\n[queue]\nproposed_rounds = 100\n")),
    );
    queue(&r, TASKS);
    write_filed(&r, FILED);
    r.commit_all("stubs");

    // one commit each, oldest first: the state commit that added a heading is what orders the pile
    let mut tasks = TASKS.to_string();
    for n in 101..=110 {
        tasks.push_str(&standing(n));
        queue(&r, &tasks);
        r.commit_all(&format!("file T-{n}"));
    }

    go(&r, &opts(1));
    let prompt = std::fs::read_to_string(r.root.join("adjudicate-prompt")).expect("the prompt");
    assert!(prompt.contains("T-009"), "{prompt}");
    for n in 101..=103 {
        assert!(
            prompt.contains(&format!("T-{n}")),
            "T-{n} missing: {prompt}"
        );
    }
    for n in 104..=110 {
        assert!(
            !prompt.contains(&format!("T-{n}")),
            "T-{n} handed: {prompt}"
        );
    }

    let turns = std::fs::read_to_string(r.root.join("adjudicate-turns")).expect("the turns");
    assert_eq!(turns, "105");
}

#[test]
fn the_dry_plan_prints_the_adjudicate_cap() {
    let tasks = format!("{TASKS}{}{}", standing(101), standing(102));
    let r = repo(&base_toml(""), &tasks);
    let plan = plan_of(&r);
    assert!(
        plan.contains("adjudicate as role adjudicator via ./src/fakeagent.sh (turns: 55,"),
        "{plan}"
    );
}

#[test]
fn the_digest_counts_standing_and_expired() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    let roles = role_commands(&implement, &verify);
    write_toml(
        &r,
        &base_toml(&format!(
            "{roles}\n[queue]\ndrain = 0\nproposed_rounds = 20\n"
        )),
    );
    let tasks = format!("{TASKS}{}", standing(100));
    queue(&r, &tasks);
    r.commit_all("stubs");

    // far more commits than proposed_rounds, so only T-100 is past the bound whatever the round commits
    for n in 1..=30 {
        r.write("filler.txt", &n.to_string());
        r.commit_all(&format!("filler {n}"));
    }
    queue(&r, &format!("{tasks}{}{}", standing(101), standing(102)));
    r.commit_all("two fresh proposals");

    let (digest, _) = go(&r, &opts(1));
    let text = pipeline::digest_text(&digest);
    let line = text
        .lines()
        .find(|l| l.starts_with("proposed: "))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(line.starts_with("proposed: 2 standing, oldest "), "{line}");
    assert!(line.ends_with(" rounds, expired 1"), "{line}");
}

fn named(pipelines: &[&str], max_iter: u32) -> RunOpts {
    RunOpts {
        max_iter,
        pipelines: pipelines.iter().map(|p| p.to_string()).collect(),
        ..RunOpts::default()
    }
}

// no queue and a scout stub: `!queue.takeable` holds and discover is the round's pipeline
fn empty_queue_with_scout() -> Repo {
    let r = repo("", "");
    let scout = script(&r, "src/fakescout.sh", QUIET);
    write_toml(&r, &base_toml(&role_command("scout", &scout)));
    r.commit_all("a scout");
    r
}

#[test]
fn only_a_named_pipeline_runs_its_stages() {
    let (_, events) = go(&empty_queue_with_scout(), &named(&["discover"], 1));
    assert!(
        stages_started(&events).contains(&"scout".to_string()),
        "{events:#?}"
    );

    let (_, events) = go(&empty_queue_with_scout(), &named(&["task"], 1));
    assert!(stages_started(&events).is_empty(), "{events:#?}");
}

#[test]
fn a_filtered_round_with_nothing_to_run_ends() {
    let (digest, events) = go(&empty_queue_with_scout(), &named(&["task"], 3));
    assert_eq!(digest.iterations, 1, "{digest:#?}");
    let warnings = events
        .iter()
        .find_map(|e| match &e.kind {
            Kind::RunEnd { warnings, .. } => Some(warnings.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{events:#?}"));
    assert!(
        warnings.iter().any(|w| w.contains("--pipeline task")),
        "{warnings:?}"
    );
}

#[test]
fn an_unnamed_run_still_reaches_discover() {
    let (_, events) = go(&empty_queue_with_scout(), &opts(1));
    assert!(
        stages_started(&events).contains(&"scout".to_string()),
        "{events:#?}"
    );
}

// the block under test is the one `enallagi issue` wrote, so the twin answers with the issue
fn queued_issue(r: &Repo) {
    let issue = Response::to(
        &enallagi::issue::gh_args("owner/repo#12").expect("args"),
        include_str!("fixtures/issues/help-wanted.json"),
    );
    let state = State {
        responses: vec![issue],
        ..State::default()
    };
    let (path, _) = twin::install(&r.root.join("tools"), BIN, &state);
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["issue", "owner/repo#12"])
        .current_dir(&r.root)
        .env("PATH", path)
        .output()
        .expect("run enallagi");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    r.commit_all("the operator's issue");
}

#[test]
fn a_proposed_block_is_adjudicated_before_scouting() {
    let r = repo("", "");
    let scout = script(&r, "src/fakescout.sh", QUIET);
    let adj = adjudicator(&r, &promotes("T-002"));
    let roles = format!(
        "{}{}",
        role_command("scout", &scout),
        role_command("adjudicator", &adj)
    );
    write_toml(&r, &base_toml(&roles));
    queue(&r, DONE_TASK);
    r.commit_all("stubs");
    queued_issue(&r);
    assert_eq!(state_of(&r, "T-002").as_deref(), Some("needs-triage"));

    let (_, events) = go(&r, &opts(1));
    let stages = stages_started(&events);
    assert!(!stages.contains(&"scout".to_string()), "{stages:?}");
    assert!(stages.contains(&"adjudicate".to_string()), "{stages:?}");
    assert_ne!(state_of(&r, "T-002").as_deref(), Some("needs-triage"));
}

// the shipped pipelines, not base_toml's copy: the ceiling under test is the default's own
fn default_pipelines_toml(extra: &str) -> String {
    let full = base_toml(extra);
    let cut = full
        .find("[[pipeline]]")
        .expect("base_toml declares pipelines");
    let stages = full.find("[[stage]]").expect("base_toml declares stages");
    let rest = &full[stages..];
    let after_stages = rest.find("\n[agent.").map(|i| &rest[i..]).unwrap_or("");
    format!("{}{}", &full[..cut], after_stages)
}

#[test]
fn an_undecided_block_hands_the_round_to_discover() {
    let r = repo("", "");
    let scout = script(&r, "src/fakescout.sh", QUIET);
    let adj = adjudicator(&r, "");
    let roles = format!(
        "{}{}",
        role_command("scout", &scout),
        role_command("adjudicator", &adj)
    );
    write_toml(&r, &default_pipelines_toml(&roles));
    queue(&r, DONE_TASK);
    r.commit_all("stubs");
    queued_issue(&r);
    installed(&r);
    assert_eq!(state_of(&r, "T-002").as_deref(), Some("needs-triage"));

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--no-tui", "--iterations", "3"])
        .current_dir(&r.root)
        .env_remove("CI")
        .output()
        .expect("run enallagi run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let started: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains(" stage.start "))
        .map(|l| {
            if l.contains("stage=scout") {
                "scout"
            } else {
                "other"
            }
        })
        .collect();
    // the queued block is decided first, then a round the adjudicator left dry goes to the scout
    assert_eq!(started.first(), Some(&"other"), "{stdout}");
    assert!(stdout.contains("stage=scout"), "{stdout}");
    assert!(stdout.contains("triage spent its 1 dry round"), "{stdout}");
    assert_eq!(state_of(&r, "T-002").as_deref(), Some("needs-triage"));
}

#[test]
fn an_unknown_pipeline_name_is_refused() {
    let r = repo(&base_toml(""), TASKS);
    let (outcome, events) = try_go(&r, &named(&["nope"], 1));
    let err = outcome.expect_err("refused");
    assert!(err.downcast_ref::<pipeline::Refused>().is_some(), "{err}");
    let text = err.to_string();
    for name in ["nope", "review", "task", "discover"] {
        assert!(text.contains(name), "{text}");
    }
    assert!(stages_started(&events).is_empty(), "{events:#?}");
}

// a stop arrives as a signal to a process, so these drive the binary rather than pipeline::run
fn sleeping_agent() -> Repo {
    let r = repo(&base_toml(""), TASKS);
    script(
        &r,
        "src/fakeagent.sh",
        "echo $$ >agent.pid\nsleep 120 &\necho $! >child.pid\nsleep 120\n",
    );
    r.commit_all("a sleeping agent");
    installed(&r);
    r
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn pid_from(path: &std::path::Path) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(path) {
            let pid = text.trim().to_string();
            if !pid.is_empty() {
                return pid;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("{} was never written", path.display());
}

fn stop_leaves_nothing_running(signal: &str) -> Vec<Event> {
    let r = sleeping_agent();
    let mut launcher = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--iterations", "1", "--no-tui"])
        .current_dir(&r.root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("enallagi run");
    let child = pid_from(&r.root.join("child.pid"));
    let agent = pid_from(&r.root.join("agent.pid"));

    let sent = std::process::Command::new("kill")
        .args([&format!("-{signal}"), &launcher.id().to_string()])
        .status()
        .expect("signal the launcher");
    assert!(sent.success(), "{signal} was not delivered");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while (alive(&agent) || alive(&child)) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(!alive(&agent), "the agent {agent} outlived the launcher");
    assert!(!alive(&child), "the agent's child {child} outlived it");
    launcher.wait().expect("the launcher exited");

    let log = enallagi::events::Log::open(&r.root.join(".enallagi"))
        .read()
        .expect("events.jsonl");
    assert!(
        log.iter()
            .any(|e| matches!(&e.kind, Kind::Halt { halt, .. } if halt == "signal")),
        "{log:#?}"
    );
    assert!(
        matches!(log.last().map(|e| &e.kind), Some(Kind::RunEnd { .. })),
        "{log:#?}"
    );
    log
}

#[test]
fn a_sigterm_stops_the_lane_and_its_child() {
    stop_leaves_nothing_running("TERM");
}

#[test]
fn a_sigint_stops_the_lane_and_its_child() {
    stop_leaves_nothing_running("INT");
}

#[test]
fn a_stopped_stage_still_records_its_end() {
    let log = stop_leaves_nothing_running("TERM");
    let at = |p: fn(&Kind) -> bool| log.iter().position(|e| p(&e.kind));
    let start = at(|k| matches!(k, Kind::StageStart { stage, .. } if stage == "implement"))
        .unwrap_or_else(|| panic!("no stage.start for implement: {log:#?}"));
    let end = at(
        |k| matches!(k, Kind::StageEnd { stage, exit, .. } if stage == "implement" && *exit != 0),
    )
    .unwrap_or_else(|| panic!("the stopped stage logged no end and no exit: {log:#?}"));
    let run_end =
        at(|k| matches!(k, Kind::RunEnd { .. })).unwrap_or_else(|| panic!("no run.end: {log:#?}"));
    assert!(start < end && end < run_end, "{log:#?}");
}

#[test]
fn a_sigterm_stops_a_running_check() {
    let r = repo(
        &base_toml(&role_commands("./src/fakeimpl.sh", "./src/fakeverify.sh")),
        REVIEW_TASK,
    );
    implementer(&r, "");
    verifier(&r);
    script(&r, "src/fakecheck.sh", "echo $$ >check.pid\nsleep 120\n");
    installed(&r);
    r.commit_all("a sleeping check");
    let mut launcher = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--iterations", "1", "--no-tui"])
        .current_dir(&r.root)
        .env_remove("CI")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("enallagi run");
    let check = pid_from(&r.root.join("check.pid"));

    let sent = std::process::Command::new("kill")
        .args(["-TERM", &launcher.id().to_string()])
        .status()
        .expect("signal the launcher");
    assert!(sent.success(), "TERM was not delivered");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while alive(&check) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        !alive(&check),
        "the check {check} outlived the signal by 5s"
    );
    launcher.wait().expect("the launcher exited");

    let log = enallagi::events::Log::open(&r.root.join(".enallagi"))
        .read()
        .expect("events.jsonl");
    assert!(
        log.iter()
            .any(|e| matches!(&e.kind, Kind::Halt { halt, .. } if halt == "signal")),
        "{log:#?}"
    );
}

// the shape both limit-wait tests share: a limit three minutes out, then the operator ends the wait
fn a_limit_wait_ended_by(end: impl FnOnce(&Repo, &std::process::Child)) -> Vec<Event> {
    let r = repo(&base_toml(""), TASKS);
    let reset = jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::UTC)
        .checked_add(jiff::Span::new().minutes(3))
        .expect("three minutes ahead")
        .strftime("%I:%M%p")
        .to_string();
    script(
        &r,
        "src/fakeagent.sh",
        &format!("echo 'hit your session limit resets {reset} (UTC)'\n"),
    );
    r.commit_all("a limited agent");
    installed(&r);
    let mut launcher = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--iterations", "1", "--no-tui"])
        .current_dir(&r.root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("enallagi run");
    let read = || {
        enallagi::events::Log::open(&r.root.join(".enallagi"))
            .read()
            .unwrap_or_default()
    };
    let limits = |log: &[Event]| {
        log.iter()
            .filter(|e| matches!(e.kind, Kind::Limit { .. }))
            .count()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while limits(&read()) == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(limits(&read()), 1, "no limit wait began: {:#?}", read());

    end(&r, &launcher);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while launcher.try_wait().expect("poll the launcher").is_none()
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let exited = launcher.try_wait().expect("poll the launcher").is_some();
    if !exited {
        let _ = launcher.kill();
        let _ = launcher.wait();
    }
    assert!(exited, "the launcher outlived the stop by 10s");

    let log = read();
    assert_eq!(limits(&log), 1, "{log:#?}");
    assert!(
        matches!(log.last().map(|e| &e.kind), Some(Kind::RunEnd { .. })),
        "{log:#?}"
    );
    log
}

// a wait the operator ended is that operator's halt, never a stage that could not start
fn assert_operator_halt(log: &[Event], halt: &str) {
    assert!(
        log.iter()
            .any(|e| matches!(&e.kind, Kind::Halt { halt: h, .. } if h == halt)),
        "no {halt} halt: {log:#?}"
    );
    assert!(
        !log.iter().any(
            |e| matches!(&e.kind, Kind::Halt { reason, .. } if reason.contains("could not start"))
        ),
        "{log:#?}"
    );
}

#[test]
fn a_sigterm_ends_the_limit_wait() {
    let log = a_limit_wait_ended_by(|_, launcher| {
        let sent = std::process::Command::new("kill")
            .args(["-TERM", &launcher.id().to_string()])
            .status()
            .expect("signal the launcher");
        assert!(sent.success(), "TERM was not delivered");
    });
    assert_operator_halt(&log, "signal");
}

#[test]
fn a_stop_file_ends_the_limit_wait() {
    let log = a_limit_wait_ended_by(|r, _| {
        let dir = enallagi::config::harness_dir(&r.root);
        let stop = enallagi::config::instance_path(&r.root, &dir, "STOP");
        std::fs::write(&stop, b"").unwrap_or_else(|e| panic!("write {}: {e}", stop.display()));
    });
    assert_operator_halt(&log, "stop");
}

#[test]
fn a_weekly_limit_halt_still_books_the_stage() {
    let r = repo(&base_toml(""), TASKS);
    // two days out is past the wait ceiling, so the stage halts where a nearer reset would sleep
    let reset = jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::UTC)
        .checked_add(jiff::Span::new().days(2))
        .expect("two days ahead")
        .strftime("%b %d at %I:%M%p")
        .to_string();
    script(
        &r,
        "src/fakeagent.sh",
        &format!("echo 'hit your session limit resets {reset} (UTC)'\n{QUIET}"),
    );
    r.commit_all("a weekly-limited agent");

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(digest.stages_run, 1, "{digest:#?}");
    assert_eq!(digest.cost, 0.5, "{digest:#?}");
    assert_eq!(ends(&events).len(), 1, "{events:#?}");
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h.contains(&reset) && h.contains("(UTC)")),
        "{:?}",
        digest.halts
    );
    // the stage ran and printed the notice, so it is not a stage that could not start
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h.contains("further out than the wait ceiling")
                && !h.contains("could not start")),
        "{:?}",
        digest.halts
    );
}

fn claude_args_repo(key: &str) -> Repo {
    let toml = base_toml("").replacen(
        "preset = \"custom\"\ncommand = [\"./src/fakeagent.sh\", \"{prompt}\", \"{turns}\"]",
        &format!("preset = \"claude\"\n{key}command = [\"./src/fakeargs.sh\", \"{{prompt}}\"]"),
        1,
    );
    let r = repo(&toml, "");
    script(
        &r,
        "src/fakeargs.sh",
        &format!("printf '%s\\n' \"$*\" >>args.txt\n{QUIET}"),
    );
    r.commit_all("args");
    r
}

#[test]
fn lane_carries_bypass_flag() {
    let r = claude_args_repo("");
    let _ = try_go(&r, &opts(1));
    let args = std::fs::read_to_string(r.root.join("args.txt")).expect("the scout ran");
    assert!(args.contains("--dangerously-skip-permissions"), "{args}");
}

#[test]
fn run_start_records_sandbox() {
    let r = claude_args_repo("");
    let (_, events) = try_go(&r, &opts(1));
    let sandbox = events.iter().find_map(|e| match &e.kind {
        Kind::RunStart { sandbox, .. } => Some(sandbox.as_str()),
        _ => None,
    });
    let want = if std::env::var_os("ENALLAGI_SANDBOX").is_some() {
        "nested"
    } else if cfg!(target_os = "macos") {
        "seatbelt"
    } else {
        "bwrap"
    };
    assert_eq!(sandbox, Some(want), "{events:#?}");
}

const EXTRA_SKILL: &str = "
[[skill]]
id = \"security\"
source = \"path:vendor/security\"
path = \"\"
rev = \"v1\"
gate = \"none\"
why = \"fixture\"
";

fn synced_extra_skill() -> Repo {
    let r = repo(&base_toml(EXTRA_SKILL), TASKS);
    r.write("vendor/security/SKILL.md", "# security v1\n");
    let (code, out) = harness(&r.root, &["skills", "sync"]);
    assert_eq!(code, 0, "{out}");
    r
}

fn locked_rev(r: &Repo, id: &str) -> Option<String> {
    let lock = enallagi::skills::read_lock(&r.root, ".enallagi").expect("lock");
    lock.skill
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.rev.clone().unwrap_or_default())
}

#[test]
fn sync_prunes_a_removed_skill() {
    let r = synced_extra_skill();
    let dir = r.root.join(".enallagi/skills/security");
    assert!(dir.join("SKILL.md").is_file());
    r.write(".enallagi/skills/mine/SKILL.md", "# mine\n");

    write_toml(&r, &base_toml(""));
    for cmd in [&["skills", "check"][..], &["skills", "sync", "--frozen"]] {
        let (code, out) = harness(&r.root, cmd);
        assert_eq!(code, 0, "{out}");
        assert!(dir.is_dir(), "{cmd:?} pruned");
    }
    let (code, out) = harness(&r.root, &["skills", "sync"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.lines().any(|l| l == "security  removed"), "{out}");
    assert!(!dir.exists(), "the vendored copy was kept");
    assert_eq!(locked_rev(&r, "security"), None);
    assert!(
        r.root.join(".enallagi/skills/mine/SKILL.md").is_file(),
        "an unlocked skill was pruned"
    );
}

#[test]
fn sync_prunes_nothing_on_a_bad_lock_id() {
    let r = synced_extra_skill();
    let lock = enallagi::skills::lock_path(&r.root, ".enallagi");
    let text = std::fs::read_to_string(&lock).expect("lock");
    std::fs::write(
        &lock,
        format!(
            "{text}\n[[skill]]\nid = \"zz/../victim\"\nsource = \"path:vendor/victim\"\nsha256 = \"0\"\n"
        ),
    )
    .expect("append a bad lock id");

    write_toml(&r, &base_toml(""));
    let (code, out) = harness(&r.root, &["skills", "sync"]);
    assert_ne!(code, 0, "{out}");
    assert!(
        r.root.join(".enallagi/skills/security/SKILL.md").is_file(),
        "a pruned directory went before the bad id was read: {out}"
    );
    assert_eq!(locked_rev(&r, "security").as_deref(), Some("v1"), "{out}");
}

#[test]
fn a_changed_rev_revendors_the_skill() {
    let r = synced_extra_skill();
    let vendored = r.root.join(".enallagi/skills/security/SKILL.md");
    r.write("vendor/security/SKILL.md", "# security v2\n");

    let (_, out) = harness(&r.root, &["skills", "sync"]);
    assert!(out.lines().any(|l| l == "security  cached"), "{out}");
    assert_eq!(
        std::fs::read_to_string(&vendored).unwrap(),
        "# security v1\n"
    );

    write_toml(&r, &base_toml(&EXTRA_SKILL.replace("v1", "v2")));
    let (code, out) = harness(&r.root, &["skills", "sync"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.lines().any(|l| l == "security  fetched"), "{out}");
    assert_eq!(
        std::fs::read_to_string(&vendored).unwrap(),
        "# security v2\n"
    );
    assert_eq!(locked_rev(&r, "security").as_deref(), Some("v2"));
}

#[test]
fn a_task_pipeline_without_adjudicate_skips_it() {
    let toml = base_toml("").replace(
        "stages = [\"implement\", \"verify\", \"adjudicate\"]",
        "stages = [\"implement\", \"verify\"]",
    );
    let plan = plan_of(&repo(&toml, TASKS));
    let at = plan
        .find("=== pipeline task (queue.takeable) ===")
        .unwrap_or_else(|| panic!("{plan}"));
    let section = &plan[at..];
    let section = &section[..section[1..]
        .find("=== pipeline ")
        .map_or(section.len(), |i| i + 1)];
    assert!(section.contains("would spawn: verify"), "{section}");
    assert!(!section.contains("adjudicate"), "{section}");
}

#[test]
fn a_stage_naming_a_new_role_runs_it() {
    let toml = base_toml("").replace(
        "stages = [\"implement\", \"verify\", \"adjudicate\"]",
        "stages = [\"security\"]",
    ) + "\n[[stage]]\nname = \"security\"\nrole = \"security\"\nturns = 5\n";
    let r = repo(&toml, TASKS);
    r.write(
        ".enallagi/roles/security.md",
        "Read the diff for secrets.\n",
    );
    r.commit_all("a security role");

    let (_, events) = go(&r, &opts(1));
    assert_eq!(stages_started(&events), ["security"], "{events:#?}");
    let rendered = std::fs::read_to_string(r.root.join(".enallagi/run/roles/security.md"))
        .expect("the role was rendered");
    assert!(
        rendered.contains("Read the diff for secrets."),
        "{rendered}"
    );
}

const TAUTOLOGY_TASK: &str = "\
## [T-001] do the thing

scope: src/thing.rs
rows: none — harness
status: ready-for-agent
criteria:
  - it happens
";

// runs only the diff the rendered prompt names, so a pathspec that misses lets the tautology through
fn diff_reading_verifier(r: &Repo) -> String {
    script(
        r,
        "src/fakeverify.sh",
        &format!(
            "role=.enallagi/run/roles/verifier.md\n\
             specs=$(grep 'Tests weakened' \"$role\" | grep -o 'git diff \\$BASE -- [^`]*' | sed 's/^git diff \\$BASE -- //')\n\
             base=$(git rev-list --max-parents=0 HEAD)\n\
             if [ -n \"$specs\" ] && eval \"git diff $base -- $specs\" | grep -q '^+.*assert!(true)'; then\n\
             {bin} tasks set-status T-001 ready-for-agent 'REJECTED: assert!(true) replaced an assertion'\n\
             {rejected}\
             else\n\
             {bin} tasks set-status T-001 done 'stub verified'\n\
             {verified}\
             fi\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            rejected = verdict("rejected by verifier: assert!(true) replaced an assertion"),
            verified = verdict("verified by verifier"),
        ),
    )
}

#[test]
fn tautologised_test_not_done() {
    let r = repo("", "");
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "sed -i.bak 's/assert_eq!(1 + 1, 2);/assert!(true);/' src/thing.rs\n\
             rm -f src/thing.rs.bak\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>{progress}\n\
             git add src/thing.rs >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
            progress = instance("PROGRESS.md"),
        ),
    );
    let verify = diff_reading_verifier(&r);
    let toml = base_toml(&role_commands(&implement, &verify));
    write_toml(
        &r,
        &toml.replace("[check]", "[layout]\ntest_glob = [\"src/*.rs\"]\n\n[check]"),
    );
    r.write(
        "src/thing.rs",
        "#[test]\nfn adds() {\n    assert_eq!(1 + 1, 2);\n}\n",
    );
    queue(&r, TAUTOLOGY_TASK);
    r.commit_all("stubs");

    go(&r, &opts(1));
    let thing = std::fs::read_to_string(r.root.join("src/thing.rs")).expect("thing.rs");
    assert!(thing.contains("assert!(true);"), "{thing}");
    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    let block = t.get("T-001").expect("the queue").expect("T-001");
    let tasks = enallagi::block::block_text(&block);
    assert!(!tasks.contains("status: done"), "{tasks}");
    assert!(tasks.contains("REJECTED: assert!(true)"), "{tasks}");
    let role = std::fs::read_to_string(r.root.join(".enallagi/run/roles/verifier.md"))
        .expect("the rendered verifier role");
    assert!(role.contains("git diff $BASE -- 'src/*.rs'"), "{role}");
}

use enallagi::fixture::twin::{self, Note, Pull, Remark, Response, Round, State, Thread};

const BIN: &str = env!("CARGO_BIN_EXE_enallagi");

struct Pulls {
    repo: Repo,
    origin: tempfile::TempDir,
    tools: tempfile::TempDir,
    twin: std::path::PathBuf,
}

// a bare origin at the fixture's HEAD, and the twin's gh on PATH
fn pulls(extra: &str, check: &str) -> Pulls {
    let r = repo("", "");
    script(&r, "src/fakecheck.sh", check);
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(
        &r,
        &base_toml(&format!("{}{extra}", role_commands(&implement, &verify))),
    );
    queue(&r, TASKS);
    r.commit_all("stubs");
    installed(&r);
    r.commit_all("installed");
    let origin = tempfile::TempDir::new().expect("tempdir");
    let bare = origin.path().join("origin.git");
    let git = |root: &std::path::Path, args: &[&str]| {
        enallagi::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    };
    git(
        origin.path(),
        &[
            "clone",
            "-q",
            "--bare",
            &r.root.display().to_string(),
            "origin.git",
        ],
    );
    git(
        &r.root,
        &["remote", "add", "origin", &bare.display().to_string()],
    );
    git(&r.root, &["fetch", "-q", "origin"]);
    let tools = tempfile::TempDir::new().expect("tempdir");
    let state = State {
        repo: "owner/repo".to_string(),
        origin: bare,
        default_branch: git(&r.root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        ..State::default()
    };
    let (_, twin) = twin::install(tools.path(), BIN, &state);
    Pulls {
        repo: r,
        origin,
        tools,
        twin,
    }
}

impl Pulls {
    fn run(&self, args: &[&str]) -> (u32, String) {
        self.run_env(args, &[])
    }

    fn run_env(&self, args: &[&str], env: &[(&str, &str)]) -> (u32, String) {
        let path = format!(
            "{}:{}",
            self.tools.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let launcher = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(["run", "--iterations", "1", "--no-tui"])
            .args(args)
            .current_dir(&self.repo.root)
            .env_remove("CI")
            .env("PATH", path)
            .envs(env.iter().copied())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("enallagi run");
        let pid = launcher.id();
        let out = launcher.wait_with_output().expect("the launcher exited");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (pid, text)
    }

    fn calls(&self) -> Vec<Vec<String>> {
        twin::load(&self.twin).calls
    }

    fn remote_branches(&self) -> String {
        enallagi::git::git(
            &self.origin.path().join("origin.git"),
            &["branch", "--list"],
        )
        .expect("branches")
    }
}

const THREADED_TASK: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: ready-for-agent
criteria:
  - it happens
notes: https://github.com/owner/repo/pull/13#discussion_r1000000007
  thread: PRRT_kwDOA1
";

// the block a review thread raised, its pull request in the twin holding `threads`
fn threaded(threads: Vec<Thread>) -> Pulls {
    let p = pulls("", "exit 0\n");
    queue(&p.repo, THREADED_TASK);
    p.repo.commit_all("a block from a review thread");
    let mut state = twin::load(&p.twin);
    state.pulls.push(Pull {
        number: 13,
        url: "https://github.com/owner/repo/pull/13".to_string(),
        head: "feature/thread".to_string(),
        state: "OPEN".to_string(),
        threads,
        ..Pull::default()
    });
    twin::save(&p.twin, &state);
    p
}

#[test]
fn a_landed_block_answers_its_thread() {
    let p = threaded(vec![Thread {
        id: "PRRT_kwDOA1".to_string(),
        comments: vec![Remark {
            path: Some("src/thing.ts".to_string()),
            body: "do the thing".to_string(),
            url: "https://github.com/owner/repo/pull/13#discussion_r1000000007".to_string(),
            ..Remark::default()
        }],
        ..Thread::default()
    }]);
    let (_, out) = p.run(&[]);
    assert!(out.contains("tasks landed: T-001"), "{out}");
    let calls = p.calls();
    assert_eq!(resolves(&calls), ["PRRT_kwDOA1"], "{out}");
    let args = calls
        .iter()
        .find(|c| {
            c.iter()
                .any(|a| a.starts_with("query=mutation ReplyThread"))
        })
        .unwrap_or_else(|| panic!("no reply:\n{out}"));
    assert!(args.contains(&"thread=PRRT_kwDOA1".to_string()), "{args:?}");
    assert!(twin::load(&p.twin).pulls[0].threads[0].resolved);
    let sha = args
        .iter()
        .find_map(|l| l.strip_prefix("body=Answered in "))
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or_else(|| panic!("no sha in the reply: {args:?}"));
    let subject = enallagi::git::git(&p.repo.root, &["log", "-1", "--format=%s", sha])
        .unwrap_or_else(|e| panic!("{sha} is not a commit here: {e}"));
    assert!(subject.contains("T-001"), "{subject}");
}

#[test]
fn an_unanswered_thread_is_a_warning_not_a_halt() {
    let p = threaded(Vec::new());
    let (_, out) = p.run(&[]);
    assert!(out.contains("tasks landed: T-001"), "{out}");
    assert!(
        out.contains("T-001: thread PRRT_kwDOA1 was left open: "),
        "{out}"
    );
    assert!(out.contains("Could not resolve"), "{out}");
    assert!(out.contains(" halts= landed=T-001 "), "{out}");
}

#[test]
fn pr_per_task_opens_one_from_the_launcher() {
    let p = pulls("", "exit 0\n");
    let (_, out) = p.run(&["--pr-per-task"]);
    assert!(out.contains("tasks landed: T-001"), "{out}");
    let calls = p.calls();
    let args = calls
        .first()
        .unwrap_or_else(|| panic!("gh never ran:\n{out}"));
    assert_eq!(args[..2], ["pr", "create"], "{args:?}");
    assert!(
        args.windows(2).any(|w| w == ["--head", "task/T-001"]),
        "{args:?}"
    );
    assert!(p.remote_branches().contains("task/T-001"), "{out}");
    let pulls = out.split("pull requests:").nth(1).expect(&out);
    assert!(
        pulls.contains("T-001: https://github.com/owner/repo/pull/1"),
        "{out}"
    );

    let claude = &enallagi::agent::presets()["claude"];
    let settings = claude
        .argv
        .iter()
        .position(|w| w == "--settings")
        .expect("the lane carries deny rules");
    assert!(claude.argv[settings + 1].contains("Bash(git push:*)"));
}

#[test]
fn auto_opens_pull() {
    let p = pulls("\n[pr]\nmerge = \"auto\"\n", "exit 0\n");
    script(
        &p.repo,
        &format!("{}/a.sh", holdout_dir(&p.repo)),
        "exit 0\n",
    );
    let (_, out) = p.run(&["--budget-usd", "5"]);
    assert!(out.contains("tasks landed: T-001"), "{out}");
    let calls = p.calls();
    assert!(
        calls.iter().any(|c| c[..2] == ["pr", "create"]),
        "{calls:?}\n{out}"
    );
}

#[test]
fn a_run_without_pr_per_task_opens_nothing() {
    let p = pulls("", "exit 0\n");
    let (_, out) = p.run(&[]);
    assert!(out.contains("tasks landed: T-001"), "{out}");
    assert!(p.calls().is_empty(), "gh ran:\n{out}");
    assert!(!p.remote_branches().contains("task/"), "{out}");
    assert!(!out.contains("pull requests:"), "{out}");
}

#[test]
fn bot_finding_read() {
    let p = pulls("", "exit 0\n");
    let mut state = twin::load(&p.twin);
    state.script = vec![twin::Round {
        head: "*".to_string(),
        round: 0,
        bots: vec![twin::Finding {
            bot: "kilo".to_string(),
            path: "src/thing.ts".to_string(),
            line: 1,
            text: "the thing drops its offset".to_string(),
        }],
        ..twin::Round::default()
    }];
    twin::save(&p.twin, &state);
    let (_, out) = p.run(&["--pr-per-task"]);
    assert!(out.contains("pull/1"), "{out}");
    let (_, out) = p.run(&[]);
    let tasks = tasks_text(&p.repo);
    assert!(
        tasks.contains("the thing drops its offset")
            && tasks.contains("Reply with `@kilocode-bot fix it`"),
        "{tasks}\n{out}"
    );
    let pull = &twin::load(&p.twin).pulls[0];
    assert_eq!(pull.threads[0].comments[0].author, "kilo-code-bot");
    assert!(
        pull.comments
            .iter()
            .any(|c| c.author == "kilo-code-bot" && c.body.starts_with("<!-- kilo-review -->")),
        "{:?}",
        pull.comments
    );
}

#[test]
fn pr_per_task_config_opens_one() {
    let p = pulls("\n[pr]\nper_task = true\n", "exit 0\n");
    let (_, out) = p.run(&[]);
    assert!(!p.calls().is_empty(), "gh never ran:\n{out}");
    assert!(
        out.contains("T-001: https://github.com/owner/repo/pull/1"),
        "{out}"
    );
}

#[test]
fn a_red_task_branch_opens_no_pr() {
    // green in the checkout, red in the replayed worktree: the marker is excluded, never committed
    let p = pulls("", "test -f green.marker\n");
    p.repo.write("green.marker", "");
    let exclude = enallagi::git::git(&p.repo.root, &["rev-parse", "--git-path", "info/exclude"])
        .expect("exclude");
    let exclude = p.repo.root.join(exclude);
    let text = std::fs::read_to_string(&exclude).unwrap_or_default();
    std::fs::write(&exclude, format!("{text}green.marker\n")).expect("exclude");

    let (_, out) = p.run(&["--pr-per-task"]);
    assert!(out.contains("tasks landed: T-001"), "{out}");
    assert!(p.calls().is_empty(), "gh ran:\n{out}");
    assert!(!p.remote_branches().contains("task/"), "{out}");
    let pulls = out.split("pull requests:").nth(1).expect(&out);
    assert!(pulls.contains("T-001: none opened"), "{out}");
    let warnings = out.split("warnings:").nth(1).expect(&out);
    assert!(
        warnings.contains("T-001 opened no pull request")
            && warnings.contains("the check failed in the task worktree"),
        "{out}"
    );
}

// a bare clone on disk under a URL that names a host: the probe reads real refs and calls nothing
fn hosted_origin(r: &Repo) -> (tempfile::TempDir, String) {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let git = |root: &std::path::Path, args: &[&str]| {
        enallagi::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    };
    let bare = dir.path().join("origin.git");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--bare",
            &r.root.to_string_lossy(),
            "origin.git",
        ],
    );
    git(
        &r.root,
        &["remote", "add", "origin", &bare.to_string_lossy()],
    );
    git(&r.root, &["fetch", "-q", "origin"]);
    let branch = git(&r.root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    git(&r.root, &["remote", "set-head", "origin", &branch]);
    git(
        &r.root,
        &[
            "remote",
            "set-url",
            "origin",
            "https://git.example.invalid/o/r.git",
        ],
    );
    (dir, branch)
}

fn protection(r: &Repo) -> String {
    let cfg = enallagi::config::load(&r.root).expect("load");
    let check = enallagi::probes::CheckOutcome {
        ran: true,
        red: false,
        output: String::new(),
    };
    let ctx = enallagi::probes::ProbeCtx {
        root: &r.root,
        cfg: &cfg,
        check: Some(&check),
        driver: false,
    };
    enallagi::probes::render(&enallagi::probes::run_all(
        &ctx,
        &["branch-protection".to_string()],
    ))
}

#[test]
fn an_iteration_lands_under_a_protection_finding() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.commit_all("stubs");
    let (_origin, _branch) = hosted_origin(&r);
    let reported = protection(&r);
    assert!(
        reported.starts_with("PROBE branch-protection 1"),
        "{reported}"
    );

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(ends(&events).len(), 2, "{events:#?}");
    assert!(digest.halts.is_empty(), "{:?}", digest.halts);
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("done"));
}

// pull 14 is T-002's in OPEN_TASKS and no ticket's elsewhere
fn open_pulls() -> Vec<Pull> {
    [(13, "task/T-001"), (14, "feature/elsewhere")]
        .map(|(number, head)| Pull {
            number,
            url: format!("https://github.com/owner/repo/pull/{number}"),
            head: head.to_string(),
            state: "OPEN".to_string(),
            ..Pull::default()
        })
        .to_vec()
}

const PLAIN_REVIEW: &str = include_str!("fixtures/reviews/plain.json");

// the twin under `tools/`, answering the review read of pull 13 with `review`; returns the PATH
fn recording(r: &Repo, pulls: Vec<Pull>, review: &str) -> String {
    let read = enallagi::review::gh_args("https://github.com/owner/repo/pull/13").expect("args");
    let state = State {
        repo: "owner/repo".to_string(),
        pulls,
        responses: vec![Response::to(&read, review)],
        ..State::default()
    };
    let (path, _) = twin::install(&r.root.join("tools"), BIN, &state);
    path.to_string_lossy().into_owned()
}

fn recorded(r: &Repo) -> Vec<Vec<String>> {
    twin::load(&r.root.join("tools/twin.json")).calls
}

// every directory holding gh is dropped; git, bash and sh are linked back in case one held them too
fn without_gh(r: &Repo) -> String {
    let dir = r.root.join("nogh");
    std::fs::create_dir_all(&dir).expect("nogh");
    let path = std::env::var("PATH").unwrap_or_default();
    for name in ["git", "bash", "sh"] {
        let found = path
            .split(':')
            .map(|d| std::path::Path::new(d).join(name))
            .find(|p| p.exists())
            .unwrap_or_else(|| panic!("{name} is not on PATH"));
        #[cfg(unix)]
        std::os::unix::fs::symlink(&found, dir.join(name)).expect("link");
    }
    let kept: Vec<&str> = path
        .split(':')
        .filter(|d| !std::path::Path::new(&format!("{d}/gh")).exists())
        .collect();
    format!("{}:{}", dir.display(), kept.join(":"))
}

// gh is found on PATH, which an in-process run cannot set per test, so these drive the binary
fn run_with_path(r: &Repo, path: &str) -> String {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--no-tui", "--iterations", "1"])
        .current_dir(&r.root)
        .env_remove("CI")
        .env("PATH", path)
        .output()
        .expect("run enallagi");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    text
}

fn driven_repo(tasks: &str) -> Repo {
    let r = Repo::new();
    script(&r, "src/fakecheck.sh", "exit 0\n");
    script(&r, "src/fakeagent.sh", QUIET);
    installed(&r);
    write_toml_at(&r, ".enallagi/enallagi.toml", &base_toml(""));
    queue(&r, tasks);
    installed(&r);
    r.commit_all("a fixture the binary drives");
    r
}

const OPEN_TASKS: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: open
pr: https://github.com/owner/repo/pull/13
branch: task/T-001
criteria:
  - it happens

## [T-002] do the other thing

scope: src/other.ts
rows: none — harness
status: open
pr: https://github.com/owner/repo/pull/14
branch: task/T-002
criteria:
  - it happens
";

fn field_of(r: &Repo, id: &str, key: &str) -> Option<String> {
    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    enallagi::block::field(&t.get(id).expect("the queue")?, key)
}

fn graphql_ops(calls: &[Vec<String>]) -> Vec<String> {
    calls
        .iter()
        .filter(|c| c[..2] == ["api", "graphql"])
        .filter_map(|c| c.iter().find_map(|a| a.strip_prefix("query=")))
        .map(|q| {
            q.split(['(', '{'])
                .next()
                .unwrap_or_default()
                .trim()
                .to_string()
        })
        .collect()
}

#[test]
fn opened_pull_sets_open() {
    let p = pulls("", "exit 0\n");
    let (_, out) = p.run(&["--pr-per-task"]);
    assert_eq!(state_of(&p.repo, "T-001").as_deref(), Some("open"), "{out}");
    assert_eq!(
        field_of(&p.repo, "T-001", "pr").as_deref(),
        Some("https://github.com/owner/repo/pull/1")
    );
    assert_eq!(
        field_of(&p.repo, "T-001", "branch").as_deref(),
        Some("task/T-001")
    );
    let state = p.repo.root.join(".enallagi");
    let dirty = enallagi::git::porcelain(&state);
    assert!(dirty.is_empty(), "{dirty:?}");
}

#[test]
fn merged_pull_sets_done() {
    let r = driven_repo(OPEN_TASKS);
    let mut pulls = open_pulls();
    pulls[0].state = "MERGED".to_string();
    pulls[1].state = "CLOSED".to_string();
    let path = recording(&r, pulls, PLAIN_REVIEW);
    let out = run_with_path(&r, &path);
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("done"), "{out}");
    assert_eq!(state_of(&r, "T-002").as_deref(), Some("wontfix"), "{out}");
    assert_eq!(
        field_of(&r, "T-002", "gate").as_deref(),
        Some("pull request closed unmerged")
    );
    assert_eq!(graphql_ops(&recorded(&r)), ["query PullState"; 2]);
}

#[test]
fn unchanged_pull_skipped() {
    let r = driven_repo(OPEN_TASKS);
    let mut pulls = open_pulls();
    pulls[1].state = "MERGED".to_string();
    let path = recording(&r, pulls, PLAIN_REVIEW);
    run_with_path(&r, &path);
    let first = graphql_ops(&recorded(&r));
    assert_eq!(
        first,
        ["query PullState", "query ReviewThreads", "query PullState"]
    );
    let out = run_with_path(&r, &path);
    let second = graphql_ops(&recorded(&r));
    assert_eq!(second[first.len()..], ["query PullState"], "{out}");
}

#[test]
fn iteration_queues_review() {
    let r = driven_repo(OPEN_TASKS);
    let path = recording(&r, open_pulls(), PLAIN_REVIEW);
    let out = run_with_path(&r, &path);

    let tasks = tasks_text(&r);
    assert!(
        tasks.contains("parse drops the timezone here"),
        "{tasks}\n{out}"
    );
    assert!(tasks.contains("#discussion_r1000000003"), "{tasks}");
    assert!(tasks.contains("status: needs-triage"), "{tasks}");
    assert!(field_of(&r, "T-001", "polled").is_some(), "{tasks}");
}

// the twin under `tools/` serving `pulls` from its own state, with no canned read
fn serving(r: &Repo, pulls: Vec<Pull>) -> String {
    let state = State {
        repo: "owner/repo".to_string(),
        pulls,
        ..State::default()
    };
    let (path, _) = twin::install(&r.root.join("tools"), BIN, &state);
    path.to_string_lossy().into_owned()
}

fn thread_on(path: &str, n: u32) -> Thread {
    Thread {
        id: format!("PRRT_{n}"),
        comments: vec![Remark {
            author: "pat".to_string(),
            path: Some(path.to_string()),
            line: Some(3),
            body: format!("finding {n} on {path}"),
            url: format!("https://github.com/owner/repo/pull/13#discussion_r{n}"),
            ..Remark::default()
        }],
        ..Thread::default()
    }
}

fn pull_with(threads: Vec<Thread>) -> Vec<Pull> {
    let mut pulls = open_pulls();
    pulls[0].threads = threads;
    pulls[1].state = "MERGED".to_string();
    pulls
}

fn ticket_file(r: &Repo, id: &str) -> String {
    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    std::fs::read_to_string(t.path_of(id).expect("ticket")).expect("read ticket")
}

fn criteria_of(text: &str) -> String {
    let at = text.find("## Acceptance criteria").expect("criteria");
    let rest = &text[at..];
    rest[..rest[2..].find("\n## ").map_or(rest.len(), |e| e + 2)].to_string()
}

#[test]
fn in_scope_finding_returns() {
    let r = driven_repo(OPEN_TASKS);
    let criteria = criteria_of(&ticket_file(&r, "T-001"));
    let path = serving(&r, pull_with(vec![thread_on("src/thing.ts", 7)]));
    let out = run_with_path(&r, &path);
    let text = ticket_file(&r, "T-001");
    assert_eq!(criteria_of(&text), criteria);
    assert!(
        text.contains("finding: F-1 thread round 1\n  by: pat\n  key: PRRT_7\n  url: https://github.com/owner/repo/pull/13#discussion_r7\n  anchor: src/thing.ts:3\n  thread-id: PRRT_7\n  > finding 7 on src/thing.ts\n"),
        "{text}\n{out}"
    );
    assert_eq!(field_of(&r, "T-001", "rounds").as_deref(), Some("1"));
    assert!(
        !tasks_text(&r).contains("needs-triage"),
        "{}",
        tasks_text(&r)
    );
    assert_ne!(state_of(&r, "T-001").as_deref(), Some("open"), "{out}");

    // nothing new: a second poll appends nothing
    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    t.set_status("T-001", "open", "").expect("open");
    t.set_field("T-001", "polled", "-").expect("polled");
    let before = ticket_file(&r, "T-001");
    run_with_path(&r, &path);
    let after = ticket_file(&r, "T-001");
    assert_eq!(after.matches("finding:").count(), 1, "{after}");
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("open"), "{after}");
    let unpolled = |t: &str| -> Vec<String> {
        t.lines()
            .filter(|l| !l.starts_with("polled:"))
            .map(String::from)
            .collect()
    };
    assert_eq!(unpolled(&after), unpolled(&before));
}

#[test]
fn foreign_finding_triaged() {
    let r = driven_repo(OPEN_TASKS);
    let path = serving(&r, pull_with(vec![thread_on("src/elsewhere.ts", 8)]));
    let out = run_with_path(&r, &path);
    assert_eq!(
        state_of(&r, "T-003").as_deref(),
        Some("needs-triage"),
        "{out}"
    );
    assert_eq!(
        field_of(&r, "T-003", "scope").as_deref(),
        Some("src/elsewhere.ts")
    );
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("open"), "{out}");
    let pull = &twin::load(&r.root.join("tools/twin.json")).pulls[0];
    let thread = &pull.threads[0];
    assert!(thread.resolved, "{thread:?}");
    assert!(thread.comments[1].body.contains("T-003"), "{thread:?}");
}

#[test]
fn rejected_url_skipped() {
    let killed = "\n## [T-003] an old finding\n\nscope: src/thing.ts\nstatus: wontfix\nnotes: https://github.com/owner/repo/pull/13#discussion_r7\n";
    let r = driven_repo(&format!("{OPEN_TASKS}{killed}"));
    r.write(
        ".enallagi/.out-of-scope/elsewhere.md",
        "# Elsewhere\n\nNot this loop's.\n\n## Why this is out of scope\n\nNo.\n\n## Prior requests\n\n- https://github.com/owner/repo/pull/13#discussion_r8: \"finding 8\"\n",
    );
    let path = serving(
        &r,
        pull_with(vec![
            thread_on("src/thing.ts", 7),
            thread_on("src/elsewhere.ts", 8),
        ]),
    );
    let out = run_with_path(&r, &path);
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("open"), "{out}");
    assert!(!ticket_file(&r, "T-001").contains("finding:"), "{out}");
    assert_eq!(state_of(&r, "T-004"), None, "{}", tasks_text(&r));
}

#[test]
fn no_open_pull_no_host() {
    let r = driven_repo(DONE_TASK);
    let path = recording(&r, open_pulls(), PLAIN_REVIEW);
    let out = run_with_path(&r, &path);
    assert!(recorded(&r).is_empty(), "{out}");
}

#[test]
fn missing_gh_warns() {
    let r = driven_repo(OPEN_TASKS);
    let out = run_with_path(&r, &without_gh(&r));
    assert!(out.contains("gh api graphql"), "{out}");
    assert!(!out.contains("HALT"), "{out}");
    let log = std::fs::read_to_string(r.root.join(".enallagi/events.jsonl")).unwrap_or_default();
    assert!(log.contains("stage.start"), "{out}");
}

const RETURNED: &str = "
## [T-900] answer the review

scope: src/thing.ts
rows: none — harness
status: ready-for-agent
criteria:
  - it happens
pr: https://github.com/owner/repo/pull/9
branch: task/T-900
finding: F-1 thread round 1
  url: https://github.com/owner/repo/pull/9#discussion_r1
  thread-id: PRRT_9
  > rename parse to read
finding: F-2 comment round 1
  url: https://github.com/owner/repo/pull/9#issuecomment-2
  > call it read instead
";

// a returned ticket's lane, its stubs working whichever ticket the launcher hands them
fn returned(commit: &str) -> Pulls {
    returned_with(commit, "", "", "")
}

// `answers` are appended to the ticket by the implementer, `runs` under the verdict by the verifier
fn returned_with(commit: &str, answers: &str, runs: &str, extra: &str) -> Pulls {
    lane_on(RETURNED, commit, answers, runs, extra)
}

// `ticket` is T-900, its pull request's branch `task/T-900` on the origin
fn lane_on(ticket: &str, commit: &str, answers: &str, runs: &str, extra: &str) -> Pulls {
    let p = pulls("", "exit 0\n");
    let r = &p.repo;
    let bin = env!("CARGO_BIN_EXE_enallagi");
    let implement = script(
        r,
        "src/fakeimpl.sh",
        &format!(
            "echo fix >src/thing.ts\n{bin} tasks set-status \"$ENALLAGI_TASK\" review stub\n\
             printf '{answers}' >>\"$(ls {}/*/issues/$ENALLAGI_TASK-*.md)\"\n\
             git add -A && git -c commit.gpgsign=false commit -qm {commit}\n{QUIET}",
            queue_path()
        ),
    );
    let verify = script(
        r,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status \"$ENALLAGI_TASK\" done stub\n{}\
             printf '{runs}' >>\"$(ls {}/*/issues/$ENALLAGI_TASK-*.md)\"\n{QUIET}",
            verdict_for("$ENALLAGI_TASK", "verified by verifier"),
            queue_path()
        ),
    );
    write_toml(
        r,
        &base_toml(&format!("{}{extra}", role_commands(&implement, &verify))),
    );
    installed(r);
    queue(r, &format!("{TASKS}{ticket}"));
    // as an install leaves it: the harness directory is its own repository, the product never tracks it
    let git = |root: &std::path::Path, args: &[&str]| {
        enallagi::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    };
    git(&r.root, &["rm", "-r", "-q", "--cached", ".enallagi"]);
    r.write(".git/info/exclude", "/.enallagi/\n");
    r.commit_all("a returned ticket");
    let state = r.root.join(".enallagi");
    git(&state, &["init", "-q"]);
    git(&state, &["add", "-A"]);
    git(
        &state,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "state",
        ],
    );
    git(
        &r.root,
        &["push", "-q", "origin", "HEAD:refs/heads/task/T-900"],
    );
    p
}

fn checkout(r: &Repo) -> String {
    enallagi::git::git(&r.root, &["rev-parse", "--abbrev-ref", "HEAD", "HEAD"]).expect("head")
}

#[test]
fn respond_pushes_branch() {
    let p = returned("'T-900: fix'");
    let before = checkout(&p.repo);
    let (_, out) = p.run(&[]);
    let origin = p.origin.path().join("origin.git");
    let log = enallagi::git::git(&origin, &["log", "--format=%s", "task/T-900"]).expect("log");
    assert_eq!(log.lines().next(), Some("T-900: fix"), "{out}");
    assert_eq!(checkout(&p.repo), before, "{out}");
    assert_eq!(state_of(&p.repo, "T-900").as_deref(), Some("open"), "{out}");
    let head = enallagi::git::git(&origin, &["rev-parse", "task/T-900"]).expect("head");
    let pushed = field_of(&p.repo, "T-900", "pushed").unwrap_or_default();
    assert!(pushed.starts_with(&head), "{pushed}\n{out}");
}

#[test]
fn push_refuses_state_commit() {
    let p = returned("'queue: T-900 at 1234567'");
    let origin = p.origin.path().join("origin.git");
    let head = enallagi::git::git(&origin, &["rev-parse", "task/T-900"]).expect("head");
    let (_, out) = p.run(&[]);
    assert_eq!(
        enallagi::git::git(&origin, &["rev-parse", "task/T-900"]).ok(),
        Some(head),
        "{out}"
    );
    assert_eq!(state_of(&p.repo, "T-900").as_deref(), Some("open"), "{out}");
    let gate = field_of(&p.repo, "T-900", "gate").unwrap_or_default();
    assert!(gate.contains("state commit"), "{gate}\n{out}");
}

#[test]
fn rejection_out_of_scope() {
    let answers = "answer: F-1 reject naming: true\\nanswer: F-2 reject naming: true\\n";
    let p = returned_with("'T-900: fix'", answers, "ran: true\\nexit: 0\\n", "");
    let (_, out) = p.run(&[]);
    let path = p.repo.root.join(".enallagi/.out-of-scope/naming.md");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{out}"));
    assert!(text.starts_with("# naming\n"), "{text}");
    assert_eq!(
        text.matches("## Why this is out of scope").count(),
        1,
        "{text}"
    );
    assert_eq!(text.matches("## Prior requests").count(), 1, "{text}");
    assert!(
        text.contains("- https://github.com/owner/repo/pull/9#discussion_r1: \"rename parse to read\"\n- https://github.com/owner/repo/pull/9#issuecomment-2: \"call it read instead\""),
        "{text}"
    );
    let state = p.repo.root.join(".enallagi");
    assert!(enallagi::git::porcelain(&state).is_empty(), "{out}");
}

// T-900's pull in the twin, F-1 its thread and F-2 its comment, answered one each way;
// the twin's round after the push resolves `resolved`
fn answered(resolved: &[&str]) -> Pulls {
    answered_with(resolved, "")
}

// `extra` follows a `[respond]` table holding `quiet = "0s"`
fn answered_with(resolved: &[&str], extra: &str) -> Pulls {
    let answers = "answer: F-1 accept fix\\nanswer: F-2 reject naming: true\\n";
    let respond = format!("\n[respond]\nquiet = \"0s\"\n{extra}");
    let p = returned_with("'T-900: fix'", answers, "ran: true\\nexit: 0\\n", &respond);
    let origin = p.origin.path().join("origin.git");
    let head = enallagi::git::git(&origin, &["rev-parse", "task/T-900"]).expect("head");
    let url = "https://github.com/owner/repo/pull/9";
    let mut state = twin::load(&p.twin);
    state.pulls.push(Pull {
        number: 9,
        url: url.to_string(),
        head: "task/T-900".to_string(),
        head_sha: head,
        state: "OPEN".to_string(),
        threads: vec![Thread {
            id: "PRRT_9".to_string(),
            comments: vec![Remark {
                author: "pat".to_string(),
                path: Some("src/thing.ts".to_string()),
                line: Some(1),
                body: "rename parse to read".to_string(),
                url: format!("{url}#discussion_r1"),
                ..Remark::default()
            }],
            ..Thread::default()
        }],
        comments: vec![Note {
            author: "pat".to_string(),
            body: "call it read instead".to_string(),
            url: format!("{url}#issuecomment-2"),
            ..Note::default()
        }],
        ..Pull::default()
    });
    state.script.push(Round {
        head: "task/T-900".to_string(),
        round: 1,
        resolve: resolved.iter().map(|s| s.to_string()).collect(),
        ..Round::default()
    });
    twin::save(&p.twin, &state);
    p
}

fn resolves(calls: &[Vec<String>]) -> Vec<String> {
    calls
        .iter()
        .filter(|c| {
            c.iter()
                .any(|a| a.starts_with("query=mutation ResolveThread"))
        })
        .filter_map(|c| c.iter().find_map(|a| a.strip_prefix("thread=")))
        .map(String::from)
        .collect()
}

#[test]
fn push_replies_and_resolves() {
    let p = answered(&[]);
    let (_, out) = p.run(&[]);
    p.run(&[]);
    let pull = twin::load(&p.twin).pulls.pop().expect("pull 9");
    let replies: Vec<&Remark> = pull.threads[0]
        .comments
        .iter()
        .filter(|c| c.body.contains("<!-- enallagi:F-1 -->"))
        .collect();
    assert_eq!(replies.len(), 1, "{pull:?}\n{out}");
    let head = enallagi::git::git(
        &p.origin.path().join("origin.git"),
        &["rev-parse", "task/T-900"],
    )
    .expect("head");
    assert!(replies[0].body.contains(head.trim()), "{}", replies[0].body);
    assert!(replies[0].body.contains("`fix`"), "{}", replies[0].body);
    let listed: Vec<&Note> = pull
        .comments
        .iter()
        .filter(|c| c.body.contains("<!-- enallagi:F-2 -->"))
        .collect();
    assert_eq!(listed.len(), 1, "{pull:?}");
    assert!(
        listed[0].body.contains("#issuecomment-2"),
        "{}",
        listed[0].body
    );
    assert!(listed[0].body.contains("`true`"), "{}", listed[0].body);
    assert_eq!(resolves(&p.calls()), ["PRRT_9"]);
    assert!(pull.threads[0].resolved);
    let text = ticket_file(&p.repo, "T-900");
    assert!(
        text.contains(&format!("settled: F-1 {}", replies[0].url)),
        "{text}"
    );
    assert!(
        text.contains(&format!("settled: F-2 {}", listed[0].url)),
        "{text}"
    );
}

#[test]
fn self_resolved_skipped() {
    let p = answered(&["PRRT_9"]);
    let (_, out) = p.run(&[]);
    p.run(&[]);
    assert!(twin::load(&p.twin).pulls[0].threads[0].resolved, "{out}");
    assert_eq!(resolves(&p.calls()), Vec::<String>::new(), "{out}");
    assert!(
        ticket_file(&p.repo, "T-900").contains("settled: F-1 "),
        "{out}"
    );
}

#[test]
fn token_env_reaches_gh() {
    let p = answered(&[]);
    let (_, out) = p.run_env(&[], &[("ENALLAGI_GH_TOKEN", "tok")]);
    let state = twin::load(&p.twin);
    let tokens: Vec<&String> = state
        .calls
        .iter()
        .zip(&state.tokens)
        .filter(|(c, _)| {
            c.iter()
                .any(|a| a.starts_with("query=mutation ReplyThread"))
        })
        .map(|(_, t)| t)
        .collect();
    assert_eq!(tokens, ["tok"], "{out}");
}

#[test]
fn max_rounds_human() {
    let r = driven_repo(&OPEN_TASKS.replacen(
        "branch: task/T-001\n",
        "branch: task/T-001\nrounds: 3\n",
        1,
    ));
    let path = serving(&r, pull_with(vec![thread_on("src/thing.ts", 7)]));
    let out = run_with_path(&r, &path);
    assert_eq!(
        state_of(&r, "T-001").as_deref(),
        Some("ready-for-human"),
        "{out}"
    );
    assert_eq!(
        field_of(&r, "T-001", "gate").as_deref(),
        Some("respond: max_rounds (3) reached, 1 findings open")
    );
    assert_eq!(
        field_of(&r, "T-001", "respond").as_deref(),
        Some("max_rounds")
    );
    assert_eq!(field_of(&r, "T-001", "rounds").as_deref(), Some("3"));
}

#[test]
fn clean_round_marks_clean() {
    let r = driven_repo(OPEN_TASKS);
    let mut pulls = open_pulls();
    let check =
        |c: &str| serde_json::json!({"name": "test", "status": "COMPLETED", "conclusion": c});
    pulls[0].head_sha = "abc123".to_string();
    pulls[0].checks = vec![check("SUCCESS")];
    pulls[0].threads = vec![Thread {
        resolved: true,
        ..thread_on("src/thing.ts", 7)
    }];
    pulls[1].checks = vec![serde_json::json!({"name": "test", "status": "IN_PROGRESS"})];
    let path = serving(&r, pulls);
    let out = run_with_path(&r, &path);
    assert_eq!(
        field_of(&r, "T-001", "respond").as_deref(),
        Some("clean at abc123"),
        "{out}"
    );
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("open"), "{out}");
    assert_eq!(field_of(&r, "T-002", "respond"), None, "{out}");
}

#[test]
fn quiet_window_defers_judging() {
    let now = jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ");
    let pushed = format!("branch: task/T-001\npushed: abc123 {now}\n");
    let r = driven_repo(&OPEN_TASKS.replacen("branch: task/T-001\n", &pushed, 1));
    let path = serving(&r, pull_with(vec![thread_on("src/thing.ts", 7)]));
    let out = run_with_path(&r, &path);
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("open"), "{out}");
    assert!(!ticket_file(&r, "T-001").contains("finding:"), "{out}");
    assert_eq!(graphql_ops(&recorded(&r)), ["query PullState"; 2], "{out}");
}

#[test]
fn rereview_requested_on_push() {
    let edit = |who: &str| -> Vec<String> {
        [
            "pr",
            "edit",
            "https://github.com/owner/repo/pull/9",
            "--add-reviewer",
            who,
        ]
        .map(String::from)
        .to_vec()
    };
    let p = answered_with(&[], "rereview = [\"@copilot\", \"@greptileai\"]\n");
    let mut state = twin::load(&p.twin);
    state.responses.push(Response::to(&edit("@copilot"), ""));
    twin::save(&p.twin, &state);
    let (_, out) = p.run(&[]);
    let calls = p.calls();
    assert!(calls.contains(&edit("@copilot")), "{calls:?}\n{out}");
    assert!(calls.contains(&edit("@greptileai")), "{calls:?}");
    assert!(out.contains("@greptileai"), "{out}");
    assert_eq!(state_of(&p.repo, "T-900").as_deref(), Some("open"), "{out}");
}

#[test]
fn recurring_rejection_gets_pointer() {
    let first = "https://github.com/owner/repo/pull/13#discussion_PRRT_7_2";
    let rejected = format!(
        "branch: task/T-001\nrounds: 1\nfinding: F-1 thread round 1\n  by: pat\n  key: PRRT_7\n  \
         url: https://github.com/owner/repo/pull/13#discussion_r7\n  anchor: src/thing.ts:3\n  \
         thread-id: PRRT_7\n  > finding 7 on src/thing.ts\nanswer: F-1 reject naming: true\n\
         settled: F-1 {first}\n"
    );
    let r = driven_repo(&OPEN_TASKS.replacen("branch: task/T-001\n", &rejected, 1));
    let mut again = thread_on("src/thing.ts", 8);
    again.comments[0].body = "Finding 7 on `src/thing.ts`".to_string();
    let old = Thread {
        resolved: true,
        ..thread_on("src/thing.ts", 7)
    };
    let path = serving(&r, pull_with(vec![old, again]));
    let out = run_with_path(&r, &path);
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("open"), "{out}");
    assert_eq!(field_of(&r, "T-001", "rounds").as_deref(), Some("1"));
    let text = ticket_file(&r, "T-001");
    assert_eq!(text.matches("finding:").count(), 1, "{text}");
    assert!(text.contains("#discussion_r8"), "{text}");
    let pull = &twin::load(&r.root.join("tools/twin.json")).pulls[0];
    let thread = &pull.threads[1];
    assert!(thread.resolved, "{thread:?}");
    assert!(thread.comments[1].body.contains(first), "{thread:?}");
}

const CONVERGING: &str = "
## [T-900] answer the review

scope: src/thing.ts
rows: none — harness
status: open
criteria:
  - it happens
pr: https://github.com/owner/repo/pull/9
branch: task/T-900
";

#[test]
fn respond_converges_on_twin() {
    let answers =
        "answer: F-1 accept fix\\nanswer: F-2 reject naming: true\\nanswer: F-3 accept fix\\n";
    let respond = "\n[respond]\nquiet = \"0s\"\n";
    let p = lane_on(
        CONVERGING,
        "'T-900: fix'",
        answers,
        "ran: true\\nexit: 0\\n",
        respond,
    );
    let url = "https://github.com/owner/repo/pull/9";
    let check = |c: &str| serde_json::json!({"name": "test", "status": "COMPLETED", "conclusion": c, "detailsUrl": format!("{url}/checks")});
    let found = |bot: &str, path: &str, text: &str| twin::Finding {
        bot: bot.to_string(),
        path: path.to_string(),
        line: 1,
        text: text.to_string(),
    };
    let mut state = twin::load(&p.twin);
    state.pulls.push(Pull {
        number: 9,
        url: url.to_string(),
        head: "task/T-900".to_string(),
        state: "OPEN".to_string(),
        threads: vec![Thread {
            id: "PRRT_copilot".to_string(),
            comments: vec![Remark {
                author: "copilot-pull-request-reviewer".to_string(),
                typename: "Bot".to_string(),
                path: Some("src/elsewhere.ts".to_string()),
                line: Some(2),
                body: "This helper is unused.".to_string(),
                url: format!("{url}#discussion_copilot"),
                ..Remark::default()
            }],
            ..Thread::default()
        }],
        ..Pull::default()
    });
    state.script = vec![
        Round {
            head: "task/T-900".to_string(),
            round: 1,
            checks: vec![check("FAILURE")],
            bots: vec![
                found("kilo", "src/thing.ts", "parse drops the offset"),
                found("greptile", "", "the error path is untested"),
            ],
            ..Round::default()
        },
        Round {
            head: "task/T-900".to_string(),
            round: 2,
            checks: vec![check("SUCCESS")],
            ..Round::default()
        },
    ];
    twin::save(&p.twin, &state);
    let origin = p.origin.path().join("origin.git");
    let base = enallagi::git::git(&origin, &["rev-parse", "task/T-900"]).expect("base");

    // the first run routes round one and pushes the answer; the second judges the re-review
    let (_, first) = p.run(&[]);
    let (_, second) = p.run(&[]);
    let out = format!("{first}{second}");
    let pull = twin::load(&p.twin).pulls.pop().expect("pull 9");
    let head = enallagi::git::git(&origin, &["rev-parse", "task/T-900"]).expect("head");
    let text = ticket_file(&p.repo, "T-900");
    assert_eq!(
        state_of(&p.repo, "T-900").as_deref(),
        Some("open"),
        "{text}\n{out}"
    );
    assert_eq!(
        field_of(&p.repo, "T-900", "respond"),
        Some(format!("clean at {}", head.trim())),
        "{text}\n{out}"
    );
    assert_eq!(field_of(&p.repo, "T-900", "rounds").as_deref(), Some("1"));
    assert_eq!(text.matches("finding:").count(), 3, "{text}");
    // filed at needs-triage; the second run's state commits may already have expired it
    let filed: Vec<String> = enallagi::tracker::Tracker::open(&p.repo.root, ".enallagi")
        .blocks()
        .expect("queue")
        .into_iter()
        .filter(|b| {
            b.id != "T-900"
                && b.body
                    .iter()
                    .any(|(_, l)| l.contains("#discussion_copilot"))
        })
        .map(|b| b.id)
        .collect();
    assert_eq!(filed, ["T-901"], "{}", tasks_text(&p.repo));
    assert!(out.contains("proposed: T-901 from"), "{out}");
    assert_eq!(pull.threads.len(), 2, "{pull:?}");
    assert!(pull.threads.iter().all(|t| t.resolved), "{pull:?}");
    let mut resolved = resolves(&p.calls());
    resolved.sort();
    assert_eq!(resolved, ["PRRT_copilot", "PRRT_twin_9_2"], "{out}");
    let range = format!("{}..task/T-900", base.trim());
    let subjects = enallagi::git::git(&origin, &["log", "--format=%s", &range]).expect("log");
    assert_eq!(subjects.trim(), "T-900: fix", "{out}");
}

const LEARNING: &str = "- [proposed] `vacuous-test`: a test passed with its logic deleted → watch it fail first\n  instances: `SPEC.md:1`, `PROGRESS.md:1`, `TASKS.md:1`\n";

fn decisions(earned: &str, proposed: &str) -> String {
    format!(
        "# DECISIONS\n\n## Earned rules\n\n{earned}\n## Rejected findings\n\n## Proposed learnings\n\n{proposed}"
    )
}

// the adjudicator's stub replaces DECISIONS.md with `after`, which is how it decides the learning
fn deciding_learning(after: &str, extra: &str) -> Repo {
    let r = repo("", "");
    let adj = adjudicator(
        &r,
        &format!("cp src/decided.md {}\n", instance("DECISIONS.md")),
    );
    write_toml(
        &r,
        &base_toml(&format!("{}{extra}", role_command("adjudicator", &adj))),
    );
    queue(&r, DONE_TASK);
    r.write(&instance("DECISIONS.md"), &decisions("", LEARNING));
    r.write("src/decided.md", after);
    r.commit_all("a proposed learning");
    r
}

#[test]
fn a_promoted_learning_is_named_in_the_digest() {
    let promoted = "- [2026-09-29] `vacuous-test`: a test passed with its logic deleted → watch it fail first (`enallagi eval --gate vacuous-test`: ACCEPT)\n";
    let r = deciding_learning(&decisions(promoted, ""), "");
    let (digest, events) = go(&r, &opts(1));
    assert!(
        stages_started(&events).contains(&"adjudicate".to_string()),
        "{events:#?}"
    );
    let text = pipeline::digest_text(&digest);
    assert!(
        text.contains("learnings: 0 proposed, 1 promoted, 0 killed, 0 expired"),
        "{text}"
    );
    assert!(text.contains("promoted `vacuous-test`"), "{text}");
}

#[test]
fn a_learning_killed_on_a_red_gate_is_named() {
    let killed = format!(
        "{LEARNING}  killed: 2026-09-29 `enallagi eval --gate vacuous-test`: REFUSE, the eval passes without the rule\n"
    );
    let r = deciding_learning(&decisions("", &killed), "");
    let (digest, _) = go(&r, &opts(1));
    let text = pipeline::digest_text(&digest);
    assert!(
        text.contains("learnings: 0 proposed, 0 promoted, 1 killed, 0 expired"),
        "{text}"
    );
    assert!(text.contains("killed `vacuous-test`"), "{text}");
}

#[test]
fn a_killed_revision_of_a_rule_is_not_promoted() {
    let rule = "- [2026-09-01] `vacuous-test`: a test passed with its logic deleted → delete it\n  revised: 2026-09-28 `vacuous-test`\n";
    let killed = format!("{LEARNING}  killed: 2026-09-29 two instances\n");
    let r = deciding_learning(&decisions(rule, &killed), "");
    r.write(&instance("DECISIONS.md"), &decisions(rule, LEARNING));
    r.commit_all("a revision proposed");
    let text = pipeline::digest_text(&go(&r, &opts(1)).0);
    assert!(
        text.contains("learnings: 0 proposed, 0 promoted, 1 killed, 0 expired"),
        "{text}"
    );
}

#[test]
fn learning_expires() {
    let r = deciding_learning(&decisions("", LEARNING), "\n[queue]\nproposed_rounds = 1\n");
    let archived = format!(
        "{}\n## Expired findings\n\n## [T-009] expired earlier\nstatus: needs-triage\n",
        decisions("", LEARNING)
    );
    r.write(&instance("DECISIONS.md"), &archived);
    r.write("src/decided.md", &archived);
    r.write("src/later.md", "later\n");
    r.commit_all("a later round");
    let text = pipeline::digest_text(&go(&r, &opts(1)).0);
    assert!(
        text.contains("learnings: 0 proposed, 0 promoted, 0 killed, 1 expired"),
        "{text}"
    );
    assert!(text.contains("expired `vacuous-test`"), "{text}");
    let after =
        std::fs::read_to_string(r.root.join(instance("DECISIONS.md"))).expect("DECISIONS.md");
    assert!(!after.contains("- [proposed] `vacuous-test`"), "{after}");
    let expired = after
        .split("## Expired findings")
        .nth(1)
        .unwrap_or_default();
    assert!(expired.contains("`vacuous-test` expired after"), "{after}");
    let task = expired
        .find("## [T-009]")
        .expect("the archived block stays");
    assert!(
        expired[..task].contains("`vacuous-test` expired after"),
        "{after}"
    );
}

// the auditor writes nothing in the checkout, so a stub recording its run needs a declared path
fn writable_src(r: &Repo) -> String {
    format!("\n[sandbox]\nwrite = [{:?}]\n", r.root.join("src"))
}

#[test]
fn the_audit_runs_once_after_the_last_iteration() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    // `[agent]` answers every role without its own command, and only the auditor's prompt carries its role
    script(
        &r,
        "src/fakeagent.sh",
        &format!(
            "case \"$1\" in *'BEGIN ENALLAGI LEARNING'*)\n\
             echo ran >>src/auditor-ran\n\
             printf 'BEGIN ENALLAGI LEARNING\\nclass: `slow-check`\\nthe check ran past its timeout → run it alone\\ninstances: `{p}:2`, `{p}:3`\\nEND ENALLAGI LEARNING\\n' ;;\n\
             esac\n{QUIET}",
            p = instance("PROGRESS.md"),
        ),
    );
    let toml = base_toml(&role_commands(&implement, &verify));
    write_toml(&r, &format!("{toml}{}", writable_src(&r)));
    queue(&r, TASKS);
    r.write(
        &instance("PROGRESS.md"),
        "# progress\nfriction: the check ran past its timeout\nfriction: the check timed out again\n",
    );
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(2));
    assert_eq!(digest.iterations, 2);
    let ran = std::fs::read_to_string(r.root.join("src/auditor-ran")).unwrap_or_default();
    assert_eq!(ran.lines().count(), 1, "{ran}");
    let text = pipeline::digest_text(&digest);
    assert!(
        text.contains("learnings: 1 proposed, 0 promoted, 0 killed, 0 expired"),
        "{text}"
    );
    assert!(text.contains("proposed `slow-check`"), "{text}");
    // every stub prints $0.5, the auditor's included
    assert_eq!(
        digest.cost,
        0.5 * (digest.stages_run + 1) as f64,
        "{digest:#?}"
    );
    assert!(digest.role_seconds.contains_key("auditor"), "{digest:#?}");
    let events = enallagi::events::Log::open(&r.root.join(".enallagi"))
        .read()
        .expect("events.jsonl");
    let report = enallagi::events::prefix_report(&events, true);
    assert!(report.contains(r#""role":"auditor""#), "{report}");
}

#[test]
fn a_failed_audit_still_counts_its_spend() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    script(
        &r,
        "src/fakeagent.sh",
        &format!("{QUIET}case \"$1\" in *'BEGIN ENALLAGI LEARNING'*) exit 1 ;; esac\n"),
    );
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    queue(&r, TASKS);
    r.write(
        &instance("PROGRESS.md"),
        "# progress\nfriction: the check ran past its timeout\nfriction: the check timed out again\n",
    );
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert!(
        digest.warnings.iter().any(|w| w.contains("exited 1")),
        "{digest:#?}"
    );
    assert_eq!(
        digest.cost,
        0.5 * (digest.stages_run + 1) as f64,
        "{digest:#?}"
    );
    assert!(digest.role_seconds.contains_key("auditor"), "{digest:#?}");
}

#[test]
fn a_stop_skips_the_audit() {
    let r = repo("", "");
    script(
        &r,
        "src/fakeagent.sh",
        &format!(
            "case \"$1\" in *'BEGIN ENALLAGI LEARNING'*) echo ran >>src/auditor-ran ;; esac\n{QUIET}"
        ),
    );
    write_toml(&r, &format!("{}{}", base_toml(""), writable_src(&r)));
    queue(&r, TASKS);
    r.write(
        &instance("PROGRESS.md"),
        "# progress\nfriction: the check ran past its timeout\nfriction: the check timed out again\n",
    );
    r.commit_all("stubs");
    r.write(&instance("STOP"), "");

    let (digest, _) = go(&r, &opts(1));
    assert!(
        digest.halts.iter().any(|h| h.contains("STOP")),
        "{digest:#?}"
    );
    assert!(!r.root.join("src/auditor-ran").exists(), "{digest:#?}");
}

// the fixture spec's holdout set; init's .gitignore keeps it out of every commit
fn holdout_dir(r: &Repo) -> String {
    format!(
        ".enallagi/holdouts/{}",
        enallagi::tracker::repo_slug(&r.root)
    )
}

// implement and verify land T-001 each round; the verify stage adds the holdout gate
fn holdout_repo() -> Repo {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    let toml = base_toml(&role_commands(&implement, &verify)).replace(
        r#"post = ["commit-verdict", "verdict", "scope"]"#,
        r#"post = ["commit-verdict", "verdict", "scope", "holdout"]"#,
    );
    write_toml(&r, &toml);
    queue(&r, TASKS);
    r.commit_all("stubs");
    r
}

#[test]
fn second_red_holdout_halts() {
    let r = holdout_repo();
    script(&r, &format!("{}/red.sh", holdout_dir(&r)), "exit 1\n");

    let (digest, events) = go(&r, &opts(3));
    assert_eq!(state_of(&r, "T-001").as_deref(), Some("ready-for-agent"));
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h.contains("T-001 was red on holdouts twice this run")),
        "{:?}",
        digest.halts
    );
    assert_eq!(digest.iterations, 2, "{events:#?}");
}

const BURNED: &str = "a lane named a holdout path; the set may be burned";

// a set for the fixture spec, and an implementer running `extra` with SET as the set's dir
fn leak_repo(extra: &str, post: &str) -> Repo {
    let r = repo("", "");
    let implement = implementer(&r, &extra.replace("SET", &holdout_dir(&r)));
    let verify = verifier(&r);
    let toml = base_toml(&role_commands(&implement, &verify))
        .replace(r#"post = ["implementer-not-done"]"#, post);
    write_toml(&r, &toml);
    queue(&r, TASKS);
    r.commit_all("stubs");
    script(&r, &format!("{}/a.sh", holdout_dir(&r)), "exit 0\n");
    r
}

fn assert_burned(r: &Repo, digest: &Digest) {
    assert_eq!(state_of(r, "T-001").as_deref(), Some("ready-for-human"));
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h == &format!("implement: {BURNED}")),
        "{:?}",
        digest.halts
    );
    let said = format!("{}{:?}{:?}", tasks_text(r), digest.halts, digest.warnings);
    assert!(!said.contains("holdouts/"), "{said}");
}

#[test]
fn leaked_holdout_path_halts() {
    let r = leak_repo("echo 'cat SET/a.sh'\n", "post = []");
    let (digest, events) = go(&r, &opts(3));
    assert_burned(&r, &digest);
    assert_eq!(digest.iterations, 1, "{events:#?}");
}

#[test]
fn holdout_leak_halts() {
    let r = leak_repo(
        "echo 'SET' >src/notes.txt\n",
        r#"post = ["implementer-not-done"]"#,
    );
    let (digest, _) = go(&r, &opts(3));
    assert_burned(&r, &digest);
}

#[test]
fn unset_slug_runs() {
    let r = leak_repo(
        "echo 'cat .enallagi/holdouts/other/a.sh'\n",
        r#"post = ["implementer-not-done"]"#,
    );
    let (digest, events) = go(&r, &opts(1));
    assert_eq!(digest.landed, vec!["T-001".to_string()], "{events:#?}");
    assert!(digest.halts.is_empty(), "{:?}", digest.halts);
}

// T-001 open in the fixture spec, a set for it, a USD budget: every refusal answered
fn auto_repo(toml_extra: &str) -> Repo {
    let r = holdout_repo();
    let toml = std::fs::read_to_string(r.root.join(".enallagi/enallagi.toml")).expect("toml");
    r.write(
        ".enallagi/enallagi.toml",
        &format!("{toml}\n[pr]\nmerge = \"auto\"\n{toml_extra}"),
    );
    r.commit_all("auto");
    script(&r, &format!("{}/a.sh", holdout_dir(&r)), "exit 0\n");
    r
}

fn usd() -> RunOpts {
    RunOpts {
        budget_usd: Some(5.0),
        ..opts(1)
    }
}

// the refusal's text, with no stage started
fn refused(r: &Repo, opts: &RunOpts) -> String {
    let (result, events) = try_go(r, opts);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.kind, Kind::StageStart { .. })),
        "{events:#?}"
    );
    match result {
        Err(e) => e.to_string(),
        Ok(d) => panic!("ran: {d:?}"),
    }
}

#[test]
fn auto_refuses_without_holdouts() {
    let r = auto_repo("");
    std::fs::remove_file(r.root.join(holdout_dir(&r)).join("a.sh")).expect("rm");
    let err = refused(&r, &usd());
    assert!(
        err.contains("no spec with open work has a holdout set"),
        "{err}"
    );
    // a set whose spec holds only done work does not count
    let r = auto_repo("");
    queue(&r, DONE_TASK);
    r.commit_all("done");
    assert!(refused(&r, &usd()).contains("holdout set"));
}

#[test]
fn auto_refuses_without_budget() {
    let r = auto_repo("");
    let err = refused(&r, &opts(1));
    assert!(err.contains("BUDGET_USD or BUDGET_TOKENS"), "{err}");
    let seconds = RunOpts {
        budget_seconds: Some(600),
        ..opts(1)
    };
    assert!(refused(&r, &seconds).contains("BUDGET_USD or BUDGET_TOKENS"));
    let tokens = RunOpts {
        budget_tokens: Some(1000),
        ..opts(1)
    };
    let (result, _) = try_go(&r, &tokens);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn auto_refuses_tracked_holdouts() {
    let r = auto_repo("");
    enallagi::git::git(&r.root, &["add", "-f", &holdout_dir(&r)]).expect("add");
    r.commit_all("tracked");
    let err = refused(&r, &usd());
    assert!(err.contains("tracks a holdout"), "{err}");
    // every reason is listed at once
    let both = refused(&r, &opts(1));
    assert!(
        both.contains("tracks a holdout") && both.contains("BUDGET_USD"),
        "{both}"
    );
}

#[test]
fn auto_refuses_sandbox_off() {
    let r = auto_repo("\n[sandbox]\nmode = \"off\"\n");
    let err = refused(&r, &usd());
    assert!(err.contains("[sandbox] mode = \"off\""), "{err}");
}

#[test]
fn auto_runs_answered() {
    let r = auto_repo("");
    let (digest, events) = go(&r, &usd());
    assert_eq!(digest.landed, vec!["T-001".to_string()], "{events:#?}");
}

#[test]
fn claims_after_verdict() {
    let repo = Repo::new();
    repo.write(".enallagi/enallagi.toml", "[check]\ncommand = \"true\"\n");
    let cfg = enallagi::config::load(&repo.root).expect("load");
    let verify = cfg
        .stage
        .iter()
        .find(|s| s.name == "verify")
        .expect("a verify stage");
    let at = verify
        .post
        .iter()
        .position(|g| g == "verdict")
        .expect("verdict on verify");
    assert_eq!(
        verify.post.get(at + 1).map(String::as_str),
        Some("claims"),
        "{:?}",
        verify.post
    );
}

const ACCEPTED: &str = "# spec\n\n**Status:** ready-for-agent\n";

// a stub decomposer's shell lines writing two drafts beside the accepted spec, the second blocked by the first
fn drafts(r: &Repo, second_blocked: &str) -> String {
    let draft = |num: &str, blocked: &str| {
        format!("# {num}: Step {num}\n\n**What to build:** step {num}.\n\n**Blocked by:** {blocked}\n\n**Status:** ready-for-agent\n\n- [ ] step {num} shows\n")
    };
    format!(
        "mkdir -p {issues}\nprintf '%s' '{}' >{issues}/01-one.md\nprintf '%s' '{}' >{issues}/02-two.md\n",
        draft("01", "None"),
        draft("02", second_blocked),
        issues = issues(r)
    )
}

fn decomposing(r: &Repo, second_blocked: &str, adjudicate: &str) {
    r.write(&spec(r), ACCEPTED);
    let decompose = script(
        r,
        "src/fakedecompose.sh",
        &format!(
            "printf '%s' \"$1\" >.git/decompose-prompt\n{}{QUIET}",
            drafts(r, second_blocked)
        ),
    );
    let adj = adjudicator(r, adjudicate);
    let roles = format!(
        "{}{}",
        role_command("decomposer", &decompose),
        role_command("adjudicator", &adj)
    );
    write_toml(r, &default_pipelines_toml(&roles));
    r.commit_all("stubs");
}

#[test]
fn decompose_queues_spec() {
    let r = repo("", "");
    decomposing(
        &r,
        "01",
        &format!("{}{}", promotes("T-001"), promotes("T-002")),
    );

    let (digest, events) = go(&r, &opts(1));
    assert!(digest.halts.is_empty(), "{digest:#?}");
    assert_eq!(stages_started(&events), ["decompose", "adjudicate"]);
    let prompt = std::fs::read_to_string(r.root.join(".git/decompose-prompt")).expect("the prompt");
    assert!(
        prompt.contains(&format!("The spec to decompose is {}.", spec(&r))),
        "{prompt}"
    );
    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    let two = t.get("T-002").unwrap().expect("T-002");
    assert_eq!(enallagi::block::blockers(&two), ["T-001"]);
    assert_eq!(
        enallagi::block::field(&two, "probe").as_deref(),
        Some("decompose")
    );
    for id in ["T-001", "T-002"] {
        assert_eq!(state_of(&r, id).as_deref(), Some("ready-for-agent"), "{id}");
    }
    let added = enallagi::git::git(
        &r.root,
        &[
            "log",
            "--diff-filter=A",
            "--format=%s",
            "--",
            &format!("{}/T-*", issues(&r)),
        ],
    )
    .unwrap();
    let subjects: Vec<&str> = added.lines().collect();
    assert_eq!(subjects.len(), 1, "{added}");
    assert!(subjects[0].starts_with("decompose"), "{added}");
}

#[test]
fn bad_drafts_halt() {
    let r = repo("", "");
    decomposing(&r, "02", &promotes("T-001"));

    let (digest, events) = go(&r, &opts(1));
    assert_eq!(stages_started(&events), ["decompose"]);
    assert!(
        digest
            .halts
            .iter()
            .any(|h| h.contains("blocking cycle through 02")),
        "{digest:#?}"
    );
    let issues = std::fs::read_dir(r.root.join(issues(&r))).unwrap();
    let names: Vec<String> = issues
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!names.iter().any(|n| n.starts_with("T-")), "{names:?}");
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn dry_run_plans_decompose() {
    let r = repo("", TASKS);
    r.write(".enallagi/specs/app/spec.md", ACCEPTED);
    write_toml(&r, &default_pipelines_toml(""));
    r.commit_all("accepted");
    let (code, out) = harness(&r.root, &["run", "--dry-run"]);
    assert_eq!(code, 0, "{out}");
    let at = |p: &str| {
        out.find(&format!("=== pipeline {p} "))
            .unwrap_or(usize::MAX)
    };
    assert!(
        at("decompose") < at("task") && at("task") != usize::MAX,
        "{out}"
    );
}

// T-460: a compound check reaches a host service only once its port is declared
#[test]
fn check_reaches_declared_port() {
    if std::env::var_os("ENALLAGI_SANDBOX").is_some() {
        println!("nested: skipped");
        return;
    }
    let server = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = server.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for mut conn in server.incoming().flatten() {
            use std::io::{Read, Write};
            let _ = conn.read(&mut [0; 1024]);
            let _ = conn.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok");
        }
    });
    let check = format!("cd sub && curl -sf http://127.0.0.1:{port}/");
    let toml = base_toml("").replace("./src/fakecheck.sh", &check);
    let r = repo(&toml, "");
    r.write("sub/.keep", "");
    queue(&r, "# queue\n");
    r.commit_all("a check on a host port");
    assert!(plan_of(&r).contains("FINDING check-red"), "undeclared");
    write_toml(&r, &format!("{toml}\n[sandbox]\nports = [{port}]\n"));
    r.commit_all("declared");
    let plan = plan_of(&r);
    assert!(!plan.contains("FINDING check-red"), "{plan}");
}

#[test]
fn check_cannot_read_holdouts() {
    if std::env::var_os("ENALLAGI_SANDBOX").is_some() {
        println!("nested: skipped");
        return;
    }
    let toml = base_toml("").replace("./src/fakecheck.sh", "cat .enallagi/holdouts/s/t");
    let r = repo(&toml, "");
    queue(&r, "# queue\n");
    r.write(".enallagi/holdouts/s/t", "scenario\n");
    assert!(plan_of(&r).contains("FINDING check-red"), "holdouts read");
}

// the proxy's refusals are recorded on threads with no writer; the loop logs them
#[test]
fn denial_emits_event() {
    if std::env::var_os("ENALLAGI_SANDBOX").is_some() {
        println!("nested: skipped");
        return;
    }
    let toml = format!("{}\n[sandbox]\nmode = \"native\"\n", base_toml(""));
    let r = repo(&toml, TASKS);
    let curl = "curl -s -o /dev/null -x \"$HTTPS_PROXY\" https://example.com/\n";
    script(&r, "src/fakeagent.sh", &format!("{curl}{QUIET}"));
    r.commit_all("agent");
    let (_, events) = try_go(&r, &opts(1));
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            Kind::SandboxDenied { host, port: 443, .. } if host == "example.com")),
        "{events:#?}"
    );
}

#[test]
fn red_check_names_denial() {
    if std::env::var_os("ENALLAGI_SANDBOX").is_some() {
        println!("nested: skipped");
        return;
    }
    let curl = "curl -sf -x $HTTPS_PROXY https://example.invalid/";
    let toml = base_toml("").replace("./src/fakecheck.sh", curl);
    let r = repo(&toml, "");
    let cfg = enallagi::config::load(&r.root).expect("load");
    let sandbox = enallagi::sandbox::Sandbox::start(&r.root, &cfg);
    let red = enallagi::gates::check_delta(&sandbox, &r.root, &cfg, false);
    assert!(red.red, "{}", red.output);
    assert!(
        red.tail(3)
            .join("\n")
            .contains("sandbox refused example.invalid:443"),
        "{}",
        red.output
    );
    write_toml(&r, &toml.replace(curl, "echo ok"));
    let cfg = enallagi::config::load(&r.root).expect("load");
    let green = enallagi::gates::check_delta(&sandbox, &r.root, &cfg, false);
    assert_eq!(green.output.trim(), "ok");
}

#[test]
fn auditor_cannot_write() {
    if std::env::var_os("ENALLAGI_SANDBOX").is_some() {
        println!("nested: skipped");
        return;
    }
    let toml = format!("{}\n[sandbox]\nmode = \"native\"\n", base_toml(""));
    let r = repo(&toml, "");
    script(
        &r,
        "src/fakeagent.sh",
        &format!(
            "case \"$1\" in *'BEGIN ENALLAGI LEARNING'*) echo ran >auditor-ran ;; esac\n{QUIET}"
        ),
    );
    queue(&r, "# queue\n");
    r.write(
        &instance("PROGRESS.md"),
        "# progress\nfriction: the check ran past its timeout\nfriction: the check timed out again\n",
    );
    r.commit_all("stubs");
    let (digest, _) = go(&r, &opts(1));
    assert!(digest.role_seconds.contains_key("auditor"), "{digest:#?}");
    assert!(!r.root.join("auditor-ran").exists());
}
