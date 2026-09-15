//! The launcher: the dry plan, the halts, the run log and the budget, with stub agents in place of a coding agent.

use harness::events::{Event, Kind};
use harness::fixture::Repo;
use harness::pipeline::{self, Digest, RunOpts};
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

const QUIET: &str = "echo '{\"total_cost_usd\":0.5}'\n";

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

[check]
command = "./src/fakecheck.sh"

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
post = ["commit-round", "adjudicator-halt", "dry-round"]
{extra}
"#
    )
}

// Every fixture config must carry the `path:` skills `repo()` wrote: rewriting harness.toml with
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
    r.write("harness.toml", &full);
}

fn repo(toml: &str, tasks: &str) -> Repo {
    let repo = Repo::new();
    // what `harness init` ignores under the harness dir; vendored skills and roles are committed
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
            bin = env!("CARGO_BIN_EXE_harness"),
        ),
    )
}

fn verifier(repo: &Repo) -> String {
    script(
        repo,
        "src/fakeverify.sh",
        &format!(
            "{bin} tasks set-status T-001 done 'stub verified'\n{QUIET}",
            bin = env!("CARGO_BIN_EXE_harness"),
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
    let cfg = harness::config::load(&repo.root).expect("load");
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

#[test]
fn the_launcher_spawns_the_configured_agent_not_a_hardcoded_one() {
    let r = repo(&base_toml(""), TASKS);
    assert!(plan_of(&r).contains("via ./src/fakeagent.sh"));
}

#[test]
fn the_implement_stage_points_the_agent_at_its_role_file() {
    let r = repo(&base_toml(""), TASKS);
    assert!(plan_of(&r).contains(".enallagi/run/roles/implementer.md"));
}

#[test]
fn a_role_with_its_own_agent_command_is_spawned_with_it() {
    let extra =
        "\n[agent.verifier]\ncommand = [\"./src/fakeverifier.sh\", \"{prompt}\", \"{turns}\"]\n";
    let r = repo(&base_toml(extra), TASKS);
    assert!(plan_of(&r).contains("as role verifier via ./src/fakeverifier.sh"));
}

#[test]
fn a_role_with_no_agent_command_falls_back_to_the_default() {
    let extra =
        "\n[agent.verifier]\ncommand = [\"./src/fakeverifier.sh\", \"{prompt}\", \"{turns}\"]\n";
    let r = repo(&base_toml(extra), TASKS);
    assert!(plan_of(&r).contains("as role implementer via ./src/fakeagent.sh"));
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
fn a_backticked_marker_in_the_template_does_not_halt_it() {
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
fn every_spawned_stage_appends_one_record_to_the_run_log() {
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
fn and_the_record_carries_the_role_the_seconds_and_the_reported_cost() {
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
fn the_loop_stops_before_a_stage_that_would_exceed_the_budget() {
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
        &base_toml("").replace("cost = \"total_cost_usd\"", "turns = \"turns\""),
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
fn a_stage_on_a_preset_with_no_turn_cap_and_no_timeout_is_refused() {
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
fn a_stage_refuses_to_start_on_an_unresolved_skill_under_frozen() {
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
        "stages = [\"implement\", \"verify\"]",
        "stages = [\"note\"]",
    ) + "\n[[stage]]\nname = \"note\"\ncommand = \"printf '%s %s %s' \\\"$HARNESS_TASK\\\" \\\"$HARNESS_STAGE\\\" \\\"$HARNESS_ITERATION\\\" >env.txt\"\nturns = 1\n";
    let r = repo(&toml, TASKS);
    go(&r, &opts(1));
    let seen = std::fs::read_to_string(r.root.join("env.txt")).expect("the command stage ran");
    assert_eq!(seen, "T-001 note 1");
}

#[test]
fn queue_empty_fails_closed_on_an_unparseable_tasks_file() {
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
            bin = env!("CARGO_BIN_EXE_harness"),
        ),
    );
    let toml = base_toml(&role_commands(&implement, "./src/fakeagent.sh")).replace(
        "stages = [\"implement\", \"verify\"]",
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
fn harness_run_without_a_tty_prints_one_line_per_event_and_exits_0() {
    let r = repo("", "");
    let implement = implementer(&r, "");
    let verify = verifier(&r);
    write_toml(&r, &base_toml(&role_commands(&implement, &verify)));
    r.write("TASKS.md", TASKS);
    r.commit_all("stubs");

    // this asserts what a tty-less run prints; CI would make the run --frozen and refuse the
    // fixture's unvendored skills, which is `a_stage_refuses_to_start_on_an_unresolved_skill_under_frozen`
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["run", "--no-tui", "--iterations", "1"])
        .current_dir(&r.root)
        .env_remove("CI")
        .output()
        .expect("run harness run");
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
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["run", "--no-tui", "--iterations", "1"])
        .current_dir(&r.root)
        .output()
        .expect("run harness run");
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
fn rendering_a_role_never_eats_the_source_it_rendered_from() {
    let toml = base_toml(SKILL).replace(
        "stages = [\"implement\", \"verify\"]",
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
fn a_fetched_skill_is_committed_before_the_stage_that_needs_it() {
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
            bin = env!("CARGO_BIN_EXE_harness"),
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
            bin = env!("CARGO_BIN_EXE_harness"),
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
    let lock = harness::skills::read_lock(&r.root).expect("lock");
    assert_eq!(lock.role.len(), 1, "{lock:?}");
    assert_eq!(lock.role[0].id, "implementer");
}

#[test]
fn a_red_check_that_names_nothing_is_a_finding_not_an_error() {
    let r = repo(&base_toml(""), "");
    script(&r, "src/fakecheck.sh", "echo boom\nexit 1\n");
    r.write("TASKS.md", "# queue\n");
    r.commit_all("a red check");

    let plan = plan_of(&r);
    assert!(plan.contains("FINDING check-red"), "{plan}");
    assert!(!plan.contains("PROBE check-red ERROR"), "{plan}");
}

#[test]
fn a_block_left_proposed_whose_fix_names_the_contract_halts() {
    let r = repo("", "");
    let adjudicate = script(
        &r,
        "src/fakeadj.sh",
        "printf '\\n## [T-002] the schema is wrong\\nstatus: proposed\\nprobe: spec-untested\\noutput: SPEC.md does not say which store\\n' >>TASKS.md\n",
    );
    let extra = format!(
        "\n[agent.adjudicator]\ncommand = [\"{adjudicate}\", \"{{prompt}}\", \"{{turns}}\"]\n"
    );
    write_toml(&r, &base_toml(&extra));
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
fn a_stages_output_reaches_the_sink_while_the_stage_is_still_running() {
    let toml = base_toml("").replace(
        "stages = [\"implement\", \"verify\"]",
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
fn the_implementer_marking_its_own_task_done_skips_the_verify_stage() {
    let r = repo("", "");
    let implement = implementer(
        &r,
        &format!(
            "{bin} tasks set-status T-001 done 'I verified myself'\n",
            bin = env!("CARGO_BIN_EXE_harness"),
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
fn an_implementer_that_stops_short_of_review_skips_the_verify_stage() {
    let r = repo("", "");
    let implement = implementer(
        &r,
        &format!(
            "{bin} tasks set-status T-001 blocked 'the fixture is missing'\n",
            bin = env!("CARGO_BIN_EXE_harness"),
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
fn a_task_stranded_at_review_gets_its_verify_stage_on_the_next_run() {
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
    let blocks = harness::queue::parse(&text).expect("parse");
    let t001 = blocks.iter().find(|b| b.id == "T-001").expect("T-001");
    assert_eq!(
        harness::queue::field(t001, "status").as_deref(),
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
fn a_verify_only_iteration_leaves_one_progress_entry_written_by_the_launcher() {
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
        harness::git::porcelain(&r.root).is_empty(),
        "the entry is committed"
    );
}

#[test]
fn and_an_iteration_whose_implementer_wrote_one_gets_no_second_entry() {
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
fn a_task_at_review_wins_over_one_ready_for_the_pipeline_choice() {
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
    let blocks = harness::queue::parse(&text).expect("parse");
    let t001 = blocks.iter().find(|b| b.id == "T-001").expect("T-001");
    let t002 = blocks.iter().find(|b| b.id == "T-002").expect("T-002");
    assert_eq!(
        harness::queue::field(t001, "status").as_deref(),
        Some("done")
    );
    assert_eq!(
        harness::queue::field(t002, "status").as_deref(),
        Some("ready"),
        "T-002 is untouched: the task pipeline never ran"
    );
}

#[test]
fn the_run_archives_the_task_it_landed_in_its_last_iteration() {
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
            bin = env!("CARGO_BIN_EXE_harness"),
        ),
    )
}

fn t001_status(r: &Repo) -> Option<String> {
    let text = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    let blocks = harness::queue::parse(&text).expect("parse");
    let t001 = blocks.iter().find(|b| b.id == "T-001").expect("T-001");
    harness::queue::field(t001, "status")
}

#[test]
fn a_verdict_that_defers_a_finding_in_notes_with_no_proposed_block_is_refused() {
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
        harness::git::porcelain(&r.root).is_empty(),
        "the refused verdict is committed, not left in the tree"
    );
}

#[test]
fn and_the_same_verdict_with_the_finding_proposed_is_committed() {
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
fn a_verdict_saying_no_findings_minor_or_otherwise_is_committed() {
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
fn an_implementer_note_saying_minor_is_not_read_as_the_verdicts() {
    let r = repo("", "");
    let implement = implementer(&r, IMPLEMENTER_NOTE);
    a_clean_verdict_after_the_implementer(&r, &implement);
}

#[test]
fn an_uncommitted_implementer_note_saying_minor_is_not_read_as_the_verdicts() {
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
            bin = env!("CARGO_BIN_EXE_harness"),
        ),
    );
    a_clean_verdict_after_the_implementer(&r, &implement);
}
