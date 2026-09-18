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
    let full = format!("{toml}{}", r.local_skills(toml));
    // built, never written literally: the floor test greps this tree for the literal
    let remote = concat!("source = \"", "github:");
    assert!(
        !full.contains(remote) && !full.contains("source = \"git+"),
        "a fixture config may not name a remote skill source:\n{full}"
    );
    r.write("enallagi.toml", &full);
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
    let cfg = enallagi::config::load(&repo.root).expect("load");
    pipeline::plan(&repo.root, &cfg).expect("plan")
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
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--no-tui", "--iterations", "1"])
        .current_dir(&r.root)
        .output()
        .expect("run enallagi run");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
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
