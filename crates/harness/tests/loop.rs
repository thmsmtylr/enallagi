//! The launcher: the dry plan, the halts, the run log and the budget, with stub agents in place of a coding agent.

use enallagi::events::{Event, Kind};
use enallagi::fixture::Repo;
use enallagi::pipeline::{self, Digest, RunOpts};
use std::sync::{Arc, Mutex};

const TASKS: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: ready
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
status: ready
criteria:
  - it happens
";

const LEVELLED_TASK: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: ready
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
    write_toml_at(r, "enallagi.toml", toml);
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

fn repo(toml: &str, tasks: &str) -> Repo {
    let repo = Repo::new();
    // what `enallagi init` ignores under the harness dir; vendored skills and roles are committed
    repo.write(
        ".enallagi/.gitignore",
        "events.jsonl\n*.log\nlogs/\nworktrees/\nloop.pid\nrun/\n__pycache__/\n",
    );
    script(&repo, "src/fakecheck.sh", "exit 0\n");
    script(&repo, "src/fakeagent.sh", QUIET);
    if !tasks.is_empty() {
        repo.write("TASKS.md", tasks);
    }
    repo.write("SPEC.md", "# spec\n");
    repo.write("PROGRESS.md", "# progress\n");
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
             echo 'iteration' >>PROGRESS.md\n\
             {extra}\
             git add -A >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    )
}

fn verifier(repo: &Repo) -> String {
    script(
        repo,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status T-001 done 'stub verified'\n{QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
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
fn a_bare_clarification_marker_halts_the_loop() {
    let r = repo(&base_toml(""), TASKS);
    r.write("SPEC.md", "# spec\n\n[NEEDS CLARIFICATION] which store?\n");
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
fn a_backticked_marker_does_not_halt() {
    let r = repo(&base_toml(""), TASKS);
    r.write(
        "SPEC.md",
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
    r.write("TASKS.md", TASKS);
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
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", TASKS);
    r.commit_all("stubs");

    let (digest, events) = go(&r, &opts(1));
    let tally = events.iter().find_map(|e| match &e.kind {
        Kind::Gate { gate, tally, .. } if gate == "verdict" => *tally,
        _ => None,
    });
    let tally = tally.unwrap_or_else(|| panic!("no verdict tally: {events:#?}"));
    assert_eq!((tally.passed, tally.ignored, tally.lines), (476, 5, 2));
    assert!(
        !digest.warnings.iter().any(|w| w.contains("test result:")),
        "{:?}",
        digest.warnings
    );
}

#[test]
fn a_check_with_no_count_warns_in_the_digest() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", TASKS);
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert!(
        digest
            .warnings
            .iter()
            .any(|w| w == "the check printed no `test result:` line: `./src/fakecheck.sh`"),
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
    r.write("TASKS.md", TASKS);
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
    r.write("TASKS.md", TASKS);
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
fn queue_empty_fails_closed_on_a_bad_queue() {
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
    // duplicate ids are what makes `queue::parse` fail; the same fixture `takeable_is_none_on_an_unreadable_queue` uses
    let broken_tasks = "## [T-001] first\nstatus: ready\n\n## [T-001] again\nstatus: ready\n";
    let r = repo(toml, broken_tasks);

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
    let implement = implementer(&r, "touch STOP\n");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", TASKS);
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
fn a_new_needs_spec_halts() {
    let r = repo("", "");
    let implement = implementer(
        &r,
        &format!(
            "{bin} tasks set-status T-001 needs-spec 'the contract does not answer it'\n",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let toml = base_toml(&role_commands(&implement, "./src/fakeagent.sh")).replace(
        "stages = [\"implement\", \"verify\", \"adjudicate\"]",
        "stages = [\"implement\"]",
    );
    write_toml(&r, &toml);
    r.write("TASKS.md", TASKS);
    r.commit_all("stubs");

    // one iteration: the halt must come from this round's end, not the next round's boundary
    let (digest, _) = go(&r, &opts(1));
    assert!(
        digest.halts.iter().any(|h| h.contains("needs-spec")),
        "{:?}",
        digest.halts
    );
}

#[test]
fn two_dry_rounds_end_the_run() {
    let r = repo(&base_toml(""), "");
    r.write("TASKS.md", "# queue\n");
    r.commit_all("empty queue");
    let (digest, _) = go(&r, &opts(5));
    assert_eq!(digest.iterations, 2, "{digest:#?}");
}

#[test]
fn run_without_a_tty_prints_one_line_per_event() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", TASKS);
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
fn a_stale_install_refuses_the_run_until_init() {
    let r = Repo::new();
    script(&r, "src/fakecheck.sh", "exit 0\n");
    script(&r, "src/fakeagent.sh", QUIET);
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");

    write_toml_at(&r, ".enallagi/enallagi.toml", &base_toml(""));
    r.write(".enallagi/TASKS.md", TASKS);
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

const SKILL: &str = r#"
[[skill]]
id = "tdd"
source = "path:vendor"
path = "tdd"
gate = "none"
why = "the failing test is written first"
"#;

#[test]
fn rendering_a_role_keeps_its_source() {
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
status: ready
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
             echo 'iteration' >>PROGRESS.md\n\
             git add -- src/a.ts PROGRESS.md TASKS.md >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let verify = script(
        &r,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status T-001 done 'stub verified'\n\
             git add -- TASKS.md >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'verify: T-001 verdict' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let skills = r.local_skills(&toml);
    write_toml(&r, &(toml + &role_commands(&implement, &verify) + &skills));
    r.commit_all("stubs");

    let (_, events) = go(&r, &opts(1));

    let tasks = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    assert!(tasks.contains("status: done"), "{tasks}");
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
    r.write("TASKS.md", "# queue\n");
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
    let tasks = std::fs::read_to_string(r.root.join(".enallagi/TASKS.md"))
        .or_else(|_| std::fs::read_to_string(r.root.join("TASKS.md")))
        .expect("TASKS.md");
    assert!(
        tasks.contains("status: done"),
        "a hang overturned the verdict"
    );
}

#[test]
fn a_proposed_fix_naming_the_contract_halts() {
    let r = repo("", "");
    let scout = script(
        &r,
        "src/fakescout.sh",
        "printf '\\n## [T-002] the schema is wrong\\nstatus: proposed\\nprobe: spec-untested\\noutput: SPEC.md does not say which store\\n' >>TASKS.md\n",
    );
    write_toml(&r, &base_toml(&role_command("scout", &scout)));
    r.write("TASKS.md", "# queue\n");
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
    r.write("TASKS.md", TASKS);
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
fn stopping_short_of_review_skips_verify() {
    let r = repo("", "");
    let implement = implementer(
        &r,
        &format!(
            "{bin} tasks set-status T-001 blocked 'the fixture is missing'\n",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    let verify = script(&r, "src/fakeverify.sh", "touch verify-ran\n");
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", TASKS);
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
    assert!(
        digest
            .warnings
            .iter()
            .any(|w| w == "T-001 ended the iteration at blocked, not done."),
        "{:?}",
        digest.warnings
    );
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

    let text = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    let blocks = enallagi::queue::parse(&text).expect("parse");
    let t001 = blocks.iter().find(|b| b.id == "T-001").expect("T-001");
    assert_eq!(
        enallagi::queue::field(t001, "status").as_deref(),
        Some("done")
    );
    assert_eq!(digest.landed, vec!["T-001".to_string()]);
}

fn entries(r: &Repo) -> usize {
    std::fs::read_to_string(r.root.join("PROGRESS.md"))
        .expect("PROGRESS.md")
        .lines()
        .filter(|l| l.starts_with("## "))
        .count()
}

#[test]
fn a_verify_only_iteration_leaves_one_entry() {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = verifier(&r);
    with_verifier(&r, &verify);
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(
        entries(&r),
        1,
        "{}",
        std::fs::read_to_string(r.root.join("PROGRESS.md")).unwrap()
    );
    assert!(
        !digest
            .warnings
            .iter()
            .any(|w| w.contains("no PROGRESS.md entry")),
        "{:?}",
        digest.warnings
    );
    let text = std::fs::read_to_string(r.root.join("PROGRESS.md")).expect("PROGRESS.md");
    assert!(text.contains("review pipeline"), "{text}");
    assert!(text.contains("\nfriction: none\n"), "{text}");
    assert!(
        enallagi::git::porcelain(&r.root).is_empty(),
        "the entry is committed"
    );
}

#[test]
fn an_implementer_entry_gets_no_second() {
    let r = repo("", "");
    let implement = implementer(&r, "echo '## stub entry' >>PROGRESS.md\n");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", TASKS);
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(
        entries(&r),
        1,
        "{}",
        std::fs::read_to_string(r.root.join("PROGRESS.md")).unwrap()
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

    let text = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    let blocks = enallagi::queue::parse(&text).expect("parse");
    let t001 = blocks.iter().find(|b| b.id == "T-001").expect("T-001");
    let t002 = blocks.iter().find(|b| b.id == "T-002").expect("T-002");
    assert_eq!(
        enallagi::queue::field(t001, "status").as_deref(),
        Some("done")
    );
    assert_eq!(
        enallagi::queue::field(t002, "status").as_deref(),
        Some("ready"),
        "T-002 is untouched: the task pipeline never ran"
    );
}

#[test]
fn the_run_archives_the_task_it_landed() {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = verifier(&r);
    with_verifier(&r, &verify);
    r.write("DECISIONS.md", "# DECISIONS\n");
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(digest.landed, vec!["T-001".to_string()]);

    let tasks = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    assert!(tasks.contains("archived: DECISIONS.md"), "{tasks}");
    assert!(
        !tasks.contains("criteria:"),
        "the body went with it: {tasks}"
    );
    let decisions = std::fs::read_to_string(r.root.join("DECISIONS.md")).expect("DECISIONS.md");
    assert!(decisions.contains("## [T-001] do the thing"), "{decisions}");
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
             printf 'notes: {note}\\n' >>TASKS.md\n\
             printf '%s' '{proposed}' >>TASKS.md\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    )
}

fn t001_status(r: &Repo) -> Option<String> {
    let text = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    let blocks = enallagi::queue::parse(&text).expect("parse");
    let t001 = blocks.iter().find(|b| b.id == "T-001").expect("T-001");
    enallagi::queue::field(t001, "status")
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
    assert_eq!(t001_status(&r).as_deref(), Some("review"));
    assert!(digest.landed.is_empty(), "{:?}", digest.landed);
    assert!(
        enallagi::git::porcelain(&r.root).is_empty(),
        "the refused verdict is committed, not left in the tree"
    );
}

#[test]
fn the_same_verdict_proposed_is_committed() {
    let r = repo(&base_toml(""), REVIEW_TASK);
    let verify = deferring_verifier(
        &r,
        "\n## [T-002] the test never removes the entry it registers\nscope: src/thing.test.ts\nstatus: proposed\n",
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

const IMPLEMENTER_NOTE: &str = "printf '  implementer: one minor edge left\\n' >>TASKS.md\n";

fn a_clean_verdict_after_the_implementer(r: &Repo, implement: &str) {
    let verify = verifier(r);
    write_toml(r, &base_toml(&role_commands(implement, &verify)));
    r.write("TASKS.md", TASKS);
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
    let implement = implementer(&r, IMPLEMENTER_NOTE);
    a_clean_verdict_after_the_implementer(&r, &implement);
}

#[test]
fn an_uncommitted_minor_is_not_the_verdict() {
    let r = repo("", "");
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "echo work >src/thing.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             git add -A >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {IMPLEMENTER_NOTE}{QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    );
    a_clean_verdict_after_the_implementer(&r, &implement);
}

#[test]
fn a_root_layout_lands_a_stub_task() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", TASKS);
    r.commit_all("stubs");

    let (digest, _) = go(&r, &opts(1));
    assert_eq!(digest.landed, vec!["T-001".to_string()]);
    let tasks = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    assert!(tasks.contains("status: done"), "{tasks}");
    assert!(!r.root.join(".enallagi/TASKS.md").exists());
    assert!(enallagi::git::porcelain(&r.root).is_empty());
}

#[test]
fn a_nested_layout_lands_a_stub_task() {
    let r = Repo::new();
    r.write(
        ".enallagi/.gitignore",
        "events.jsonl\n*.log\nlogs/\nworktrees/\nloop.pid\nrun/\n__pycache__/\n",
    );
    script(&r, "src/fakecheck.sh", "exit 0\n");
    script(&r, "src/fakeagent.sh", QUIET);
    r.write(".enallagi/TASKS.md", TASKS);
    r.write(".enallagi/SPEC.md", "# spec\n");
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
    let tasks = std::fs::read_to_string(r.root.join(".enallagi/TASKS.md")).expect("TASKS.md");
    assert!(tasks.contains("status: done"), "{tasks}");
    for name in [
        "TASKS.md",
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
fn an_install_lands_a_task_and_touches_nothing_else() {
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
    r.write(".enallagi/TASKS.md", TASKS);
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
        "TASKS.md",
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
fn a_claude_install_writes_nothing_under_dot_claude() {
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
    r.write(".enallagi/TASKS.md", TASKS);
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
    r.write(".enallagi/TASKS.md", TASKS);
    r.write(".enallagi/SPEC.md", "# spec\n");
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
    let text = std::fs::read_to_string(r.root.join(".enallagi/TASKS.md")).expect("TASKS.md");
    let blocks = enallagi::queue::parse(&text).expect("parse");
    let t001 = blocks.iter().find(|b| b.id == "T-001").expect("T-001");
    enallagi::queue::field(t001, "status")
}

#[test]
fn an_unsupported_done_is_forced_back_to_ready() {
    let r = nested("echo '(fail) alpha'\nexit 1\n", "");

    let (digest, events) = go(&r, &opts(1));
    assert!(digest.landed.is_empty(), "{events:#?}");
    assert_eq!(nested_status(&r).as_deref(), Some("ready"));
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
fn a_nested_install_refuses_a_grown_baseline() {
    let r = nested("exit 0\n", "echo alpha >>.enallagi/.check-baseline\n");

    let (digest, events) = go(&r, &opts(1));
    assert!(digest.landed.is_empty(), "{events:#?}");
    assert_eq!(nested_status(&r).as_deref(), Some("ready"));
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

#[test]
fn a_landed_iteration_leaves_no_instance_path() {
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
                    "TASKS.md",
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
const FILED: &str = "\n## [T-009] the fence scan reads one language only\n\nscope: src/thing.ts\nrows: none — harness\nstatus: proposed\ncriteria:\n  - the scan reads every fence\n";

const RESTATED: &str = "\n## [T-009] the check was red at HEAD\n\nscope: src/thing.ts\nrows: none — harness\nstatus: proposed\ncriteria:\n  - the check was red at HEAD\n";

const REJECTION: &str = "the check was red at HEAD";

const NOTED_REJECTION: &str =
    "the fence scan reads one language only and nothing in the queue says so";

const NOTED_RESTATED: &str = "\n## [T-009] the fence scan reads one language only\n\nscope: src/thing.ts\nrows: none — harness\nstatus: proposed\ncriteria:\n  - the fence scan reads one language only and nothing in the queue says so\n";

const ATTENDED_TASKS: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: ready
criteria:
  - it happens

## [T-002] the one a human runs

scope: src/other.ts
rows: none — harness
status: ready
attended: true
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
            "{bin} tasks set-status T-001 ready '{REJECTION}'\n\
             cat src/filed.md >>TASKS.md\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    )
}

// the role prompt has the verifier edit the block and append `REJECTED` to notes; only set-status writes a `gate:` line
fn verifier_rejecting_in_notes(r: &Repo) -> String {
    script(
        r,
        "src/fakeverify.sh",
        &format!(
            "sed -i.bak 's/^status: review$/status: ready/' TASKS.md\n\
             rm -f TASKS.md.bak\n\
             printf 'notes: Verifier: REJECTED on criterion 1. {NOTED_REJECTION}. Re-run it.\\n' >>TASKS.md\n\
             cat src/filed.md >>TASKS.md\n\
             {QUIET}"
        ),
    )
}

fn adjudicator(r: &Repo, body: &str) -> String {
    script(r, "src/fakeadj.sh", &format!("{body}{QUIET}"))
}

fn promotes(id: &str) -> String {
    format!(
        "{bin} tasks set-status {id} ready 'promoted'\n",
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
    r.write("TASKS.md", TASKS);
    r.write("src/filed.md", filed);
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

fn status_of(r: &Repo, id: &str) -> Option<String> {
    let text = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    let blocks = enallagi::queue::parse(&text).expect("parse");
    blocks
        .iter()
        .find(|b| b.id == id)
        .and_then(|b| enallagi::queue::field(b, "status"))
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
    assert_eq!(status_of(&r, "T-001").as_deref(), Some("ready"));
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
    r.write("TASKS.md", TASKS);
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

// one fixture, both arms: what the verifier files decides whether the stage spawns at all
fn adjudicating(filed: &str) -> Digest {
    let r = repo("", "");
    draining(&r, filed, "");
    go(&r, &opts(1)).0
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
fn a_stage_at_its_turn_cap_is_named_in_the_digest() {
    let r = repo(&base_toml(""), "");
    r.write("TASKS.md", "# queue\n");
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
    r.write("TASKS.md", "# queue\n");
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
fn an_uncapped_stage_is_not_named_as_a_turn_cap() {
    let toml = base_toml("").replace(
        r#"command = ["./src/fakeagent.sh", "{prompt}", "{turns}"]"#,
        r#"command = ["./src/fakeagent.sh", "{prompt}"]"#,
    );
    let r = repo(&toml, "");
    r.write("TASKS.md", "# queue\n");
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
    r.write("TASKS.md", "# queue\n");
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
    assert_eq!(status_of(&r, "T-009").as_deref(), Some("proposed"));
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
fn no_scout_still_reaches_the_adjudicator() {
    let r = repo("", "");
    draining(&r, FILED, &promotes("T-009"));
    let toml = std::fs::read_to_string(r.root.join("enallagi.toml")).expect("enallagi.toml");
    let toml = toml
        .replace(
            "[[pipeline]]\nname = \"discover\"\nwhen = \"!queue.takeable\"\nstages = [\"scout\", \"adjudicate\"]\nend_after_dry_rounds = 2\n",
            "",
        )
        .replace("[[stage]]\nname = \"scout\"\nrole = \"scout\"\nturns = 5\n", "");
    assert!(!toml.contains("scout"), "{toml}");
    r.write("enallagi.toml", &toml);
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
fn the_shipped_task_pipeline_adjudicates() {
    let r = Repo::new();
    r.write("enallagi.toml", "[check]\ncommand = \"true\"\n");
    let cfg = enallagi::config::load(&r.root).expect("load");
    let task = cfg
        .pipeline
        .iter()
        .find(|p| p.name == "task")
        .expect("the task pipeline");
    assert_eq!(task.stages, vec!["implement", "verify", "adjudicate"]);
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
    r.write("TASKS.md", ATTENDED_TASKS);
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
fn the_adjudicator_prompt_names_the_filed_ids() {
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
status: ready
criteria:
  - it happens
notes: REJECTED: the red deleted the whole call site, so it covered no branch.
";

fn implementing(r: &Repo, tasks: &str) {
    let implement = implementer(r, "printf '%s' \"$1\" >implement-prompt\n");
    let verify = verifier(r);
    write_toml(r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", tasks);
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
    r.write("TASKS.md", TASKS);
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
    assert_eq!(status_of(&r, "T-009").as_deref(), Some("proposed"));
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
    format!("\n## [T-{n}] a standing proposal\n\nscope: src/thing.ts\nrows: none — harness\nstatus: proposed\ncriteria:\n  - it happens\n")
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
    r.write("TASKS.md", TASKS);
    r.write("src/filed.md", FILED);
    r.commit_all("stubs");

    // one commit each, oldest first: the state commit that added a heading is what orders the pile
    for n in 101..=110 {
        let mut tasks = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
        tasks.push_str(&standing(n));
        r.write("TASKS.md", &tasks);
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
    r.write("TASKS.md", &format!("{TASKS}{}", standing(100)));
    r.commit_all("stubs");

    // far more commits than proposed_rounds, so only T-100 is past the bound whatever the round commits
    for n in 1..=30 {
        r.write("filler.txt", &n.to_string());
        r.commit_all(&format!("filler {n}"));
    }
    let tasks = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    r.write(
        "TASKS.md",
        &format!("{tasks}{}{}", standing(101), standing(102)),
    );
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

// no TASKS.md and a scout stub: `!queue.takeable` holds and discover is the round's pipeline
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

#[test]
fn a_sigterm_ends_the_limit_wait() {
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

    let sent = std::process::Command::new("kill")
        .args(["-TERM", &launcher.id().to_string()])
        .status()
        .expect("signal the launcher");
    assert!(sent.success(), "TERM was not delivered");
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
    assert!(exited, "the launcher outlived the signal by 10s");

    let log = read();
    assert_eq!(limits(&log), 1, "{log:#?}");
    assert!(
        matches!(log.last().map(|e| &e.kind), Some(Kind::RunEnd { .. })),
        "{log:#?}"
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
fn a_claude_lane_skips_no_permission_by_default() {
    let r = claude_args_repo("");
    let _ = try_go(&r, &opts(1));
    let args = std::fs::read_to_string(r.root.join("args.txt")).expect("the scout ran");
    assert!(!args.contains("--dangerously-skip-permissions"), "{args}");
}

#[test]
fn the_skip_key_adds_the_bypass_flag() {
    let r = claude_args_repo("dangerously_skip_permissions = true\n");
    let _ = try_go(&r, &opts(1));
    let args = std::fs::read_to_string(r.root.join("args.txt")).expect("the scout ran");
    assert!(args.contains("--dangerously-skip-permissions"), "{args}");
}

#[test]
fn a_skip_with_no_bypass_flag_refuses_the_run() {
    let extra = "dangerously_skip_permissions = true\n";
    let toml = base_toml("").replacen("[agent]\n", &format!("[agent]\n{extra}"), 1);
    let r = repo(&toml, TASKS);
    let (digest, events) = try_go(&r, &opts(1));
    let err = digest.expect_err("the run is refused");
    assert!(err.to_string().contains("custom"), "{err}");
    assert!(ends(&events).is_empty(), "nothing may spawn");
}

fn skipped(events: &[Event]) -> Option<bool> {
    events.iter().find_map(|e| match &e.kind {
        Kind::RunStart {
            permissions_skipped,
            ..
        } => Some(*permissions_skipped),
        _ => None,
    })
}

#[test]
fn the_skip_flag_adds_the_bypass_flag() {
    let r = claude_args_repo("");
    let run = RunOpts {
        dangerously_skip_permissions: true,
        ..opts(1)
    };
    let (_, events) = try_go(&r, &run);
    let args = std::fs::read_to_string(r.root.join("args.txt")).expect("the scout ran");
    assert!(args.contains("--dangerously-skip-permissions"), "{args}");
    assert_eq!(skipped(&events), Some(true), "{events:#?}");
}

#[test]
fn the_run_start_records_no_skip() {
    let r = claude_args_repo("");
    let (_, events) = try_go(&r, &opts(1));
    assert_eq!(skipped(&events), Some(false), "{events:#?}");
}

#[test]
fn a_skip_flag_with_no_bypass_flag_is_refused() {
    let r = repo(&base_toml(""), TASKS);
    let run = RunOpts {
        dangerously_skip_permissions: true,
        ..opts(1)
    };
    let (digest, events) = try_go(&r, &run);
    let err = digest.expect_err("the run is refused");
    assert!(err.to_string().contains("custom"), "{err}");
    assert!(ends(&events).is_empty(), "nothing may spawn");
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
status: ready
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
             {bin} tasks set-status T-001 ready 'REJECTED: assert!(true) replaced an assertion'\n\
             else\n\
             {bin} tasks set-status T-001 done 'stub verified'\n\
             fi\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
        ),
    )
}

#[test]
fn a_tautologised_test_does_not_reach_done() {
    let r = repo("", "");
    let implement = script(
        &r,
        "src/fakeimpl.sh",
        &format!(
            "sed -i.bak 's/assert_eq!(1 + 1, 2);/assert!(true);/' src/thing.rs\n\
             rm -f src/thing.rs.bak\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>PROGRESS.md\n\
             git add src/thing.rs >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {QUIET}",
            bin = env!("CARGO_BIN_EXE_enallagi"),
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
    r.write("TASKS.md", TAUTOLOGY_TASK);
    r.commit_all("stubs");

    go(&r, &opts(1));
    let thing = std::fs::read_to_string(r.root.join("src/thing.rs")).expect("thing.rs");
    assert!(thing.contains("assert!(true);"), "{thing}");
    let tasks = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    assert!(!tasks.contains("status: done"), "{tasks}");
    assert!(tasks.contains("REJECTED: assert!(true)"), "{tasks}");
    let role = std::fs::read_to_string(r.root.join(".enallagi/run/roles/verifier.md"))
        .expect("the rendered verifier role");
    assert!(role.contains("git diff $BASE -- 'src/*.rs'"), "{role}");
}

struct Pulls {
    repo: Repo,
    origin: tempfile::TempDir,
    tools: tempfile::TempDir,
}

// a bare origin at the fixture's HEAD, and a gh on PATH that records its argv and its parent's pid
fn pulls(extra: &str, check: &str) -> Pulls {
    let r = repo("", "");
    script(&r, "src/fakecheck.sh", check);
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(
        &r,
        &base_toml(&format!("{}{extra}", role_commands(&implement, &verify))),
    );
    r.write("TASKS.md", TASKS);
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
    let gh = tools.path().join("gh");
    std::fs::write(
        &gh,
        format!(
            "#!/usr/bin/env bash\necho $PPID >{dir}/gh.ppid\nprintf '%s\\n' \"$@\" >{dir}/gh.log\necho https://example.test/pull/1\n",
            dir = tools.path().display()
        ),
    )
    .expect("gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    Pulls {
        repo: r,
        origin,
        tools,
    }
}

impl Pulls {
    fn run(&self, args: &[&str]) -> (u32, String) {
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

    fn gh(&self) -> Option<String> {
        std::fs::read_to_string(self.tools.path().join("gh.log")).ok()
    }

    fn remote_branches(&self) -> String {
        enallagi::git::git(
            &self.origin.path().join("origin.git"),
            &["branch", "--list"],
        )
        .expect("branches")
    }
}

#[test]
fn pr_per_task_opens_one_from_the_launcher() {
    let p = pulls("", "exit 0\n");
    let (pid, out) = p.run(&["--pr-per-task"]);
    assert!(out.contains("tasks landed: T-001"), "{out}");
    let args = p.gh().unwrap_or_else(|| panic!("gh never ran:\n{out}"));
    assert!(args.starts_with("pr\ncreate\n"), "{args}");
    assert!(args.contains("--head\ntask/T-001\n"), "{args}");
    let ppid = std::fs::read_to_string(p.tools.path().join("gh.ppid")).expect("gh.ppid");
    assert_eq!(
        ppid.trim(),
        pid.to_string(),
        "gh was not the launcher's child"
    );
    assert!(p.remote_branches().contains("task/T-001"), "{out}");
    let pulls = out.split("pull requests:").nth(1).expect(&out);
    assert!(
        pulls.contains("T-001: https://example.test/pull/1"),
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
fn a_run_without_pr_per_task_opens_nothing() {
    let p = pulls("", "exit 0\n");
    let (_, out) = p.run(&[]);
    assert!(out.contains("tasks landed: T-001"), "{out}");
    assert!(p.gh().is_none(), "gh ran:\n{out}");
    assert!(!p.remote_branches().contains("task/"), "{out}");
    assert!(!out.contains("pull requests:"), "{out}");
}

#[test]
fn pr_per_task_config_opens_one() {
    let p = pulls("\n[pr]\nper_task = true\n", "exit 0\n");
    let (_, out) = p.run(&[]);
    assert!(p.gh().is_some(), "gh never ran:\n{out}");
    assert!(out.contains("T-001: https://example.test/pull/1"), "{out}");
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
    assert!(p.gh().is_none(), "gh ran:\n{out}");
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
