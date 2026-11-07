#[test]
fn help_exits_0_and_lists_subcommands() {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("--help")
        .output()
        .expect("run harness --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in [
        "init", "run", "watch", "probe", "gate", "hook", "skills", "tasks", "eval", "events",
    ] {
        assert!(
            stdout.contains(sub),
            "help missing subcommand {sub}: {stdout}"
        );
    }
}

#[test]
fn run_help_names_budget() {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--help"])
        .output()
        .expect("run enallagi run --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--iterations"), "{stdout}");
    assert!(stdout.contains("--budget-usd"), "{stdout}");
    assert!(stdout.contains("--pr-per-task"), "{stdout}");
}

#[test]
fn events_print_sandbox() {
    let dir = tempfile::tempdir().unwrap();
    let harness_dir = dir.path().join(".enallagi");
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("events.jsonl"),
        r#"{"ts":"2026-09-18T00:00:00Z","run":"r","iter":0,"seq":1,"kind":"run.start","config_sha256":"a","pipeline":null,"sandbox":"off"}"#.to_string() + "\n",
    )
    .unwrap();
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("events")
        .current_dir(dir.path())
        .output()
        .expect("run enallagi events");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("sandbox=off"), "{stdout}");
}

#[test]
fn events_renders_the_gate_tally() {
    let dir = tempfile::tempdir().unwrap();
    let harness_dir = dir.path().join(".enallagi");
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("events.jsonl"),
        r#"{"ts":"2026-09-18T00:00:00Z","run":"r","iter":1,"seq":1,"kind":"gate","gate":"verdict","task":"T-1","pass":true,"reason":"ok","tally":{"passed":476,"failed":0,"ignored":5,"lines":14}}"#.to_string() + "\n",
    )
    .unwrap();
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("events")
        .current_dir(dir.path())
        .output()
        .expect("run enallagi events");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(r#"tally={"passed":476,"failed":0,"ignored":5,"lines":14}"#),
        "{stdout}"
    );
}

#[test]
fn run_rejects_a_bare_iteration_count() {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "7"])
        .output()
        .expect("run enallagi run 7");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--iterations"), "{stderr}");
}

#[test]
fn events_with_no_log_exits_0_with_no_output() {
    let dir = tempfile::tempdir().unwrap();
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("events")
        .current_dir(dir.path())
        .output()
        .expect("run enallagi events");
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn events_json_prints_raw_lines_and_filters_by_task() {
    let dir = tempfile::tempdir().unwrap();
    let harness_dir = dir.path().join(".enallagi");
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("events.jsonl"),
        concat!(
            r#"{"ts":"2026-09-07T00:00:00Z","run":"r","iter":0,"seq":1,"kind":"gate","gate":"scope","task":"T-1","pass":true,"reason":"ok"}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:01Z","run":"r","iter":0,"seq":2,"kind":"gate","gate":"scope","task":"T-2","pass":true,"reason":"ok"}"#,
            "\n",
        ),
    )
    .unwrap();

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["events", "--task", "T-1", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run enallagi events --task T-1 --json");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), 1);
    assert!(stdout.contains("\"task\":\"T-1\""));
}

#[test]
fn events_json_echoes_the_file_line_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let harness_dir = dir.path().join(".enallagi");
    std::fs::create_dir_all(&harness_dir).unwrap();
    // keys out of order plus an undeclared field: legal JSONL that --json must echo raw
    let line = r#"{"kind":"halt","reason":"boom","extra_field":"unexpected","seq":1,"iter":0,"run":"r","ts":"2026-09-07T00:00:00Z","halt":"stop"}"#;
    std::fs::write(harness_dir.join("events.jsonl"), format!("{line}\n")).unwrap();

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["events", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run enallagi events --json");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim_end(),
        line,
        "must echo the raw line byte-for-byte"
    );
}

#[test]
fn events_since_keeps_only_events_at_or_after() {
    let dir = tempfile::tempdir().unwrap();
    let harness_dir = dir.path().join(".enallagi");
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("events.jsonl"),
        concat!(
            r#"{"ts":"2026-09-07T00:00:00Z","run":"r","iter":0,"seq":1,"kind":"halt","halt":"a","reason":"early"}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:02Z","run":"r","iter":0,"seq":2,"kind":"halt","halt":"b","reason":"late"}"#,
            "\n",
        ),
    )
    .unwrap();

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["events", "--since", "2026-09-07T00:00:01Z", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run enallagi events --since ...");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), 1);
    assert!(stdout.contains("\"reason\":\"late\""));
}

#[test]
fn events_summary_counts_an_overturned_done() {
    let dir = tempfile::tempdir().unwrap();
    let harness_dir = dir.path().join(".enallagi");
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("events.jsonl"),
        concat!(
            r#"{"ts":"2026-09-07T00:00:00Z","run":"r","iter":1,"seq":1,"kind":"stage.end","stage":"implement","task":"T-1","seconds":10,"exit":0,"cost":1.5,"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":3,"cache_read_input_tokens":4,"turns":1}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:01Z","run":"r","iter":1,"seq":2,"kind":"stage.end","stage":"verify","task":"T-1","seconds":5,"exit":0,"cost":0.5,"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":3,"cache_read_input_tokens":4,"turns":1}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:02Z","run":"r","iter":1,"seq":3,"kind":"gate","gate":"verdict","task":"T-1","pass":true,"reason":"ok","tally":{"passed":1,"failed":0,"ignored":0,"lines":1}}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:03Z","run":"r","iter":2,"seq":4,"kind":"stage.end","stage":"verify","task":"T-2","seconds":7,"exit":0,"cost":1.0,"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":30,"cache_read_input_tokens":40,"turns":1}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:04Z","run":"r","iter":2,"seq":5,"kind":"task.status","task":"T-2","from":"done","to":"ready","reason":"red","by":"verdict"}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:05Z","run":"r","iter":2,"seq":6,"kind":"gate","gate":"verdict","task":"T-2","pass":false,"reason":"red","tally":{"passed":0,"failed":1,"ignored":0,"lines":1}}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:06Z","run":"r","iter":3,"seq":7,"kind":"stage.end","stage":"verify","task":"T-2","seconds":3,"exit":0,"cost":null,"input_tokens":null,"output_tokens":null,"turns":null}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:00:07Z","run":"r","iter":3,"seq":8,"kind":"gate","gate":"verdict","task":"T-2","pass":true,"reason":"not done"}"#,
            "\n",
        ),
    )
    .unwrap();
    let summary = |json: bool| {
        let mut args = vec!["events", "--summary"];
        if json {
            args.push("--json");
        }
        let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(&args)
            .current_dir(dir.path())
            .output()
            .expect("run enallagi events --summary");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    let text = summary(false);
    let cells = |task: &str| -> Vec<String> {
        let line = text.lines().find(|l| l.starts_with(task)).unwrap();
        line.split_whitespace().skip(1).map(String::from).collect()
    };
    assert_eq!(
        cells("T-1"),
        ["2", "1", "1", "0", "0", "15", "2.00", "2", "4", "6", "8"],
        "{text}"
    );
    assert_eq!(
        cells("T-2"),
        ["2", "2", "1", "1", "1", "10", "1.00", "10", "20", "30", "40"],
        "{text}"
    );
    assert!(
        text.contains("false-completion rate: 1/2 = 0.500"),
        "{text}"
    );

    let json = summary(true);
    let rows: Vec<serde_json::Value> = json
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 3, "{json}");
    assert_eq!(rows[0]["task"], "T-1");
    assert_eq!(rows[0]["overturns"], 0);
    assert_eq!(rows[1]["task"], "T-2");
    assert_eq!(rows[1]["overturns"], 1);
    assert_eq!(rows[1]["rejections"], 1);
    assert_eq!(rows[1]["cache_read_input_tokens"], 40);
    assert_eq!(rows[2]["false_completion_rate"], 0.5);
}

// one implementer stage with usage and a rendered role file, one verifier stage with neither
fn prefix_in(dir: &std::path::Path, json: bool) -> String {
    let harness_dir = dir.join(".enallagi");
    std::fs::create_dir_all(harness_dir.join("run/roles")).unwrap();
    std::fs::write(harness_dir.join("run/roles/implementer.md"), "0123456789").unwrap();
    std::fs::write(
        harness_dir.join("events.jsonl"),
        concat!(
            r#"{"ts":"2026-09-07T00:00:00Z","run":"r","iter":1,"seq":1,"kind":"stage.start","stage":"implement","role":"implementer","command":"claude","task":"T-1"}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:10:00Z","run":"r","iter":1,"seq":2,"kind":"stage.end","stage":"implement","task":"T-1","seconds":600,"exit":0,"cost":null,"input_tokens":22,"output_tokens":6233,"cache_creation_input_tokens":54825,"cache_read_input_tokens":505740,"turns":9}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:11:00Z","run":"r","iter":1,"seq":3,"kind":"stage.start","stage":"verify","role":"verifier","command":"claude","task":"T-1"}"#,
            "\n",
            r#"{"ts":"2026-09-07T00:20:00Z","run":"r","iter":1,"seq":4,"kind":"stage.end","stage":"verify","task":"T-1","seconds":540,"exit":143,"cost":null,"input_tokens":null,"output_tokens":null,"turns":null}"#,
            "\n",
        ),
    )
    .unwrap();
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["events", "--prefix"])
        .args(json.then_some("--json"))
        .current_dir(dir)
        .output()
        .expect("run enallagi events --prefix");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn row<'a>(report: &'a str, role: &str) -> Vec<&'a str> {
    report
        .lines()
        .find(|l| l.starts_with(&format!("{role} ")))
        .unwrap_or_else(|| panic!("no {role} row: {report}"))
        .split_whitespace()
        .collect()
}

#[test]
fn events_prefix_prints_each_role_s_usage() {
    let dir = tempfile::tempdir().unwrap();
    let report = prefix_in(dir.path(), false);
    let header = row(&report, "role");
    let implementer = row(&report, "implementer");
    let at = |name: &str| {
        let i = header.iter().position(|h| *h == name);
        implementer[i.unwrap_or_else(|| panic!("no {name} column: {report}"))]
    };
    assert_eq!(at("input_tokens"), "22", "{report}");
    assert_eq!(at("cache_creation_input_tokens"), "54825", "{report}");
    assert_eq!(at("cache_read_input_tokens"), "505740", "{report}");
    assert_eq!(at("repaid"), "54847", "{report}");
    assert_eq!(at("cached"), "505740", "{report}");
    // the rendered role file on disk is 10 bytes, and not what any logged stage was given
    assert_eq!(at("handed_bytes"), "-", "{report}");
}

#[test]
fn events_prefix_json_prints_one_object_per_row() {
    let dir = tempfile::tempdir().unwrap();
    let report = prefix_in(dir.path(), true);
    let rows: Vec<serde_json::Value> = report
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}")))
        .collect();
    assert_eq!(rows.len(), 4, "{report}");
    assert_eq!(rows[0]["role"], "implementer", "{report}");
    assert_eq!(rows[0]["repaid"], 54847, "{report}");
    assert_eq!(rows[0]["cached"], 505740, "{report}");
    assert!(rows[0]["handed_bytes"].is_null(), "{report}");
    assert!(rows[1]["input_tokens"].is_null(), "{report}");
    assert_eq!(rows[2]["total"], "implementer", "{report}");
    assert_eq!(rows[2]["repaid"], 54847, "{report}");
}

#[test]
fn events_prefix_prints_a_dash_for_no_usage() {
    let dir = tempfile::tempdir().unwrap();
    let report = prefix_in(dir.path(), false);
    let verifier = row(&report, "verifier");
    assert_eq!(verifier[..3], ["verifier", "verify", "T-1"], "{report}");
    assert!(verifier[3..].iter().all(|v| *v == "-"), "{report}");
}

#[test]
fn events_prefix_names_its_window_and_no_rate() {
    let dir = tempfile::tempdir().unwrap();
    let report = prefix_in(dir.path(), false);
    let window = report.lines().next().unwrap_or_default();
    assert!(
        window.contains("2026-09-07T00:00:00Z") && window.contains("2026-09-07T00:20:00Z"),
        "{report}"
    );
    for rate in ["per day", "per hour", "per run", "/day", "/hour"] {
        assert!(!report.contains(rate), "{rate}: {report}");
    }
}

fn events_in(dir: &std::path::Path) -> std::process::Output {
    enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["events", "--json"])
        .current_dir(dir)
        .output()
        .expect("run enallagi events --json")
}

#[test]
fn the_new_directory_wins_without_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let line = r#"{"ts":"2026-09-07T00:00:00Z","run":"r","iter":0,"seq":1,"kind":"halt","halt":"a","reason":"current"}"#;
    std::fs::create_dir_all(dir.path().join(".harness")).unwrap();
    std::fs::create_dir_all(dir.path().join(".enallagi")).unwrap();
    std::fs::write(dir.path().join(".harness/events.jsonl"), "{}\n").unwrap();
    std::fs::write(
        dir.path().join(".enallagi/events.jsonl"),
        format!("{line}\n"),
    )
    .unwrap();

    let out = events_in(dir.path());
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), line);
    assert!(out.stderr.is_empty(), "{out:?}");
}

#[test]
fn skills_sync_custom_preset() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut toml = String::from(
        "[check]\ncommand = \"true\"\n\n[agent]\npreset = \"custom\"\ncommand = [\"./lane.sh\", \"{prompt}\", \"{turns}\"]\n\n",
    );
    for id in enallagi::fixture::SKILL_IDS {
        toml.push_str(&format!(
            "[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"x\"\n\n"
        ));
        std::fs::create_dir_all(root.join(format!("vendor/{id}"))).unwrap();
        std::fs::write(root.join(format!("vendor/{id}/SKILL.md")), "body\n").unwrap();
    }
    std::fs::create_dir_all(root.join(".enallagi")).unwrap();
    std::fs::write(root.join(".enallagi/enallagi.toml"), toml).unwrap();

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["skills", "sync"])
        .current_dir(root)
        .output()
        .expect("run enallagi skills sync");
    assert!(out.status.success(), "{out:?}");
    assert!(root.join(".enallagi/skills/tdd/SKILL.md").is_file());
    assert!(root.join(".enallagi/harness.lock").is_file());
}

// implementer transcript invokes tdd, verifier transcript caveman-commit
fn cost_in(root: &std::path::Path, window: &[&str]) -> String {
    let mut toml = String::from("[check]\ncommand = \"true\"\n\n");
    for id in enallagi::fixture::SKILL_IDS {
        toml.push_str(&format!(
            "[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"x\"\n\n"
        ));
        std::fs::create_dir_all(root.join(format!("vendor/{id}"))).unwrap();
        std::fs::write(root.join(format!("vendor/{id}/SKILL.md")), "body\n").unwrap();
    }
    std::fs::create_dir_all(root.join(".enallagi")).unwrap();
    std::fs::write(root.join(".enallagi/enallagi.toml"), toml).unwrap();
    let harness = |args: &[&str]| {
        enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(args)
            .current_dir(root)
            .output()
            .expect("run enallagi skills")
    };
    assert!(harness(&["skills", "sync"]).status.success());
    let used = |id: &str| {
        serde_json::json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "name": "Skill", "input": {"skill": format!("harness:{id}")}}
        ]}})
        .to_string()
    };
    let lines = [
        r#"{"ts":"2026-09-07T00:00:00Z","run":"r","iter":1,"seq":1,"kind":"stage.start","stage":"implement","role":"implementer","command":"claude","task":"T-1"}"#.to_string(),
        serde_json::json!({"ts":"2026-09-07T00:01:00Z","run":"r","iter":1,"seq":2,"kind":"stage.output","stage":"implement","chunk": format!("{}\n", used("tdd"))}).to_string(),
        r#"{"ts":"2026-09-07T00:10:00Z","run":"r","iter":1,"seq":3,"kind":"stage.end","stage":"implement","task":"T-1","seconds":600,"exit":0,"cost":null,"input_tokens":null,"output_tokens":null,"turns":null}"#.to_string(),
        r#"{"ts":"2026-09-07T00:11:00Z","run":"r","iter":1,"seq":4,"kind":"stage.start","stage":"verify","role":"verifier","command":"claude","task":"T-1"}"#.to_string(),
        serde_json::json!({"ts":"2026-09-07T00:12:00Z","run":"r","iter":1,"seq":5,"kind":"stage.output","stage":"verify","chunk": format!("{}\n", used("caveman-commit"))}).to_string(),
        r#"{"ts":"2026-09-07T00:20:00Z","run":"r","iter":1,"seq":6,"kind":"stage.end","stage":"verify","task":"T-1","seconds":540,"exit":0,"cost":null,"input_tokens":null,"output_tokens":null,"turns":null}"#.to_string(),
    ];
    std::fs::write(root.join(".enallagi/events.jsonl"), lines.join("\n") + "\n").unwrap();
    let out = harness(&[&["skills", "--cost"], window].concat());
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn cost_counts_loads() {
    let dir = tempfile::tempdir().unwrap();
    let report = cost_in(dir.path(), &[]);
    let row = |id: &str| -> Vec<String> {
        report
            .lines()
            .find(|l| l.starts_with(&format!("{id} ")))
            .unwrap_or_else(|| panic!("no {id} row: {report}"))
            .split_whitespace()
            .map(String::from)
            .collect()
    };
    assert_eq!(
        row("skill"),
        ["skill", "bytes", "loaded", "used"],
        "{report}"
    );
    assert_eq!(row("tdd"), ["tdd", "5", "1", "1"], "{report}");
    assert_eq!(row("ponytail"), ["ponytail", "5", "1", "0"], "{report}");
    assert_eq!(
        row("ponytail-review"),
        ["ponytail-review", "5", "1", "0"],
        "{report}"
    );
    assert_eq!(
        row("caveman-commit"),
        ["caveman-commit", "5", "2", "1"],
        "{report}"
    );
    assert_eq!(row("to-tickets"), ["to-tickets", "5", "0", "0"], "{report}");
    assert_eq!(
        row("codebase-design"),
        ["codebase-design", "5", "1", "0"],
        "{report}"
    );
}

// the log splits at the swap: a stage counts in the window it started in
#[test]
fn skills_cost_by_period() {
    let row = |report: &str, id: &str| -> Vec<String> {
        report
            .lines()
            .find(|l| l.starts_with(&format!("{id} ")))
            .unwrap_or_else(|| panic!("no {id} row: {report}"))
            .split_whitespace()
            .skip(2)
            .map(String::from)
            .collect()
    };
    let split = "2026-09-07T00:11:00Z";
    let dir = tempfile::tempdir().unwrap();
    let before = cost_in(dir.path(), &["--until", split]);
    assert_eq!(row(&before, "tdd"), ["1", "1"], "{before}");
    assert_eq!(row(&before, "caveman-commit"), ["1", "0"], "{before}");
    let dir = tempfile::tempdir().unwrap();
    let after = cost_in(dir.path(), &["--since", split]);
    assert_eq!(row(&after, "tdd"), ["0", "0"], "{after}");
    assert_eq!(row(&after, "caveman-commit"), ["1", "1"], "{after}");
}

#[test]
fn cost_flags_unused_skill() {
    let dir = tempfile::tempdir().unwrap();
    let report = cost_in(dir.path(), &[]);
    let unproven: Vec<&str> = report
        .lines()
        .filter(|l| l.contains("unproven"))
        .filter_map(|l| l.split(':').next())
        .collect();
    assert_eq!(
        unproven,
        [
            "diagnosing-bugs",
            "codebase-design",
            "writing-for-agents",
            "domain-modeling",
            "code-review",
            "ponytail",
            "ponytail-review",
            "receiving-code-review",
            "verification-before-completion"
        ],
        "{report}"
    );
    assert!(!report.contains("waste"), "{report}");
}

#[test]
fn a_report_flag_refuses_a_flag_it_would_ignore() {
    let dir = tempfile::tempdir().unwrap();
    let pairs: [&[&str]; 3] = [
        &["skills", "sync", "--cost"],
        &["skills", "--frozen", "--cost"],
        &["events", "--prefix", "--summary"],
    ];
    for args in pairs {
        let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(args)
            .current_dir(dir.path())
            .output()
            .expect("run enallagi");
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("cannot be used with"), "{args:?}: {stderr}");
    }
}

#[test]
fn skills_check_after_sync() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut toml = String::from("[check]\ncommand = \"true\"\n\n");
    for id in enallagi::fixture::SKILL_IDS {
        toml.push_str(&format!(
            "[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"x\"\n\n"
        ));
        std::fs::create_dir_all(root.join(format!("vendor/{id}"))).unwrap();
        std::fs::write(root.join(format!("vendor/{id}/SKILL.md")), "body\n").unwrap();
    }
    std::fs::create_dir_all(root.join(".enallagi")).unwrap();
    std::fs::write(root.join(".enallagi/enallagi.toml"), toml).unwrap();

    let harness = |args: &[&str]| {
        enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(args)
            .current_dir(root)
            .output()
            .expect("run enallagi skills")
    };

    let out = harness(&["skills", "check"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("tdd"));

    let out = harness(&["skills", "list"]);
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l == "tdd  path:vendor/tdd  unlocked"));

    let out = harness(&["skills", "sync"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l == "tdd  fetched"));
    assert!(root
        .join(".enallagi/adapters/claude/skills/tdd/SKILL.md")
        .is_file());
    assert!(root.join(".enallagi/harness.lock").is_file());

    let out = harness(&["skills", "check"]);
    assert!(out.status.success(), "{out:?}");

    let out = harness(&["skills", "list"]);
    let listed = String::from_utf8_lossy(&out.stdout);
    assert!(
        listed
            .lines()
            .any(|l| l.starts_with("tdd  path:vendor/tdd  sha256:")),
        "{listed}"
    );
}

#[test]
fn run_refuses_an_unset_check() {
    let repo = enallagi::fixture::Repo::new();
    let harness = |args: &[&str]| {
        enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(args)
            .current_dir(&repo.root)
            .output()
            .expect("run enallagi")
    };
    let out = harness(&["init"]);
    assert!(out.status.success(), "{out:?}");

    let out = harness(&["run", "--no-tui", "--iterations", "1"]);
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr
            .lines()
            .any(|l| l.contains("check.command") && l.contains("enallagi init")),
        "{stderr}"
    );
    let log = std::fs::read_to_string(repo.root.join(".enallagi/events.jsonl")).unwrap_or_default();
    assert!(!log.contains("stage.start"), "{log}");
}

#[test]
fn run_names_an_unset_check_as_unset() {
    let repo = enallagi::fixture::Repo::new();
    let harness = |args: &[&str]| {
        enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(args)
            .current_dir(&repo.root)
            .output()
            .expect("run enallagi")
    };
    let out = harness(&["init"]);
    assert!(out.status.success(), "{out:?}");

    let out = harness(&["run", "--no-tui", "--iterations", "1"]);
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let first = stderr.lines().next().unwrap_or_default();
    assert!(
        first.contains("check.command") && first.contains("enallagi.toml"),
        "{stderr}"
    );
    assert!(!first.contains("wrong check"), "{stderr}");
    let log = std::fs::read_to_string(repo.root.join(".enallagi/events.jsonl")).unwrap_or_default();
    assert!(!log.contains("stage.start"), "{log}");
}

#[test]
fn reference_tables_in_docs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md");
    let reference =
        std::fs::read_to_string(root.join("docs/reference.md")).expect("docs/reference.md");
    let rows = |text: &str| text.lines().filter(|l| l.starts_with('|')).count();
    assert_eq!(rows(&readme), 0, "README.md carries a table");
    assert!(
        readme.contains("](docs/reference.md)"),
        "README.md never links docs/reference.md"
    );
    // b008d8a's README carried 19 table rows, and none may be lost in the move
    assert!(
        rows(&reference) >= 19,
        "docs/reference.md has {} rows",
        rows(&reference)
    );
    let headings: Vec<&str> = reference
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .collect();
    assert_eq!(
        headings,
        [
            "Pipelines",
            "Gates",
            "Probes",
            "Configuration",
            "Files",
            "Twins"
        ]
    );
}

#[test]
fn the_gate_table_names_every_gate() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let gates =
        std::fs::read_to_string(root.join("crates/harness/src/gates.rs")).expect("gates.rs");
    let reference =
        std::fs::read_to_string(root.join("docs/reference.md")).expect("docs/reference.md");
    let body = gates
        .split("let outcome = match name {")
        .nth(1)
        .and_then(|rest| rest.split("other =>").next())
        .expect("run() in gates.rs matches on name");
    let arm = regex::Regex::new(r#"(?m)^\s*"([a-z-]+)" =>"#).unwrap();
    let names: Vec<&str> = arm
        .captures_iter(body)
        .map(|c| c.get(1).unwrap().as_str())
        .collect();
    assert!(!names.is_empty(), "no gate arm read from gates.rs");
    for gate in names {
        assert!(
            reference
                .lines()
                .any(|l| l.starts_with(&format!("| `{gate}`"))),
            "docs/reference.md has no row for the gate {gate}"
        );
    }
}

#[test]
fn readme_links_the_three_guides() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md");
    for guide in ["docs/setup.md", "docs/configuration.md", "docs/pipeline.md"] {
        assert!(
            readme.contains(&format!("]({guide})")),
            "README.md never links {guide}"
        );
    }
}

#[test]
fn shipped_docs_describe() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |rel: &str| std::fs::read_to_string(root.join(rel)).expect(rel);
    let argues = regex::Regex::new(
        r"\b(because|which is why|the reason|worth|deliberately|on purpose|we |our )\b",
    )
    .expect("regex");
    let mut offences = Vec::new();

    let readme = read("README.md");
    if readme.lines().count() >= 150 {
        offences.push(format!("README.md is {} lines", readme.lines().count()));
    }
    let mut sections: Vec<&str> = Vec::new();
    let mut fenced = false;
    for line in readme.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
        } else if let Some(h) = line.strip_prefix("## ").filter(|_| !fenced) {
            sections.push(h);
        }
    }
    let order = [
        "Highlights",
        "Install",
        "From install to a landed task",
        "How a run works",
        "Configuration",
        "Commands",
        "License",
    ];
    if sections != order {
        offences.push(format!("README.md headings {sections:?}"));
    }
    let intent = read("docs/intent.md");
    let guides =
        ["docs/setup.md", "docs/configuration.md", "docs/pipeline.md"].map(|rel| (rel, read(rel)));
    let texts = [("README.md", &readme), ("docs/intent.md", &intent)]
        .into_iter()
        .chain(guides.iter().map(|(rel, text)| (*rel, text)));
    for (name, text) in texts {
        for (i, line) in text.lines().enumerate() {
            if argues.is_match(line) {
                offences.push(format!("{name}:{} {line}", i + 1));
            }
        }
    }

    let headings: Vec<&str> = intent
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .collect();
    let wanted = [
        "Problem",
        "Proposed outcome",
        "What exists today",
        "Where it goes",
        "Affected users and systems",
        "Constraints",
        "Open questions",
        "References",
    ];
    if headings != wanted {
        offences.push(format!("docs/intent.md headings {headings:?}"));
    }

    let toml = enallagi::config::DEFAULT_TOML;
    let mut run = 0usize;
    for (i, line) in toml.lines().enumerate() {
        run = if line.trim_start().starts_with('#') {
            run + 1
        } else {
            0
        };
        if run == 2 {
            offences.push(format!(
                "harness.default.toml:{} comment runs past one line",
                i
            ));
        }
        if line.contains("arXiv") {
            offences.push(format!("harness.default.toml:{} cites a paper", i + 1));
        }
    }

    // the citations moved to docs/intent.md appear in no other shipped document
    let cfg: enallagi::config::Config =
        toml::from_str(enallagi::config::DEFAULT_TOML).expect("defaults");
    let mut elsewhere = vec![
        "templates/pointer.md".to_string(),
        "templates/RAILS.md".to_string(),
    ];
    elsewhere.extend(cfg.layout.pointer_files.iter().cloned());
    for rel in &elsewhere {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if ["34235", "2605.29668", "2602.11988", "agents.md"]
                .iter()
                .any(|cite| line.contains(cite))
            {
                offences.push(format!("{rel}:{} cites a paper docs/intent.md owns", i + 1));
            }
        }
    }
    if !cfg.layout.docs.iter().any(|d| d == "test-hashes.json") {
        offences.push("enallagi.toml docs omits test-hashes.json, which litter flags".to_string());
    }
    assert!(offences.is_empty(), "{}", offences.join("\n"));
}

#[test]
fn tasks_ready_bad_config() {
    let r = enallagi::fixture::Repo::new();
    r.queue(
        ".enallagi",
        "## [T-001] open\nscope: src/a.ts\nstatus: ready-for-agent\n",
    );
    r.write(".enallagi/enallagi.toml", "[check]\ncomand = \"x\"\n");

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "ready"])
        .current_dir(&r.root)
        .output()
        .expect("run enallagi tasks ready");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "T-001",
        "{out:?}"
    );
}

#[test]
fn tasks_set_status() {
    let r = enallagi::fixture::Repo::new();
    r.queue(
        ".enallagi",
        "## [T-001] first\nstatus: review\n\n## [T-002] waits\nblockedBy: T-001\nstatus: ready-for-agent\n\n## [T-003] a person's\nstatus: ready-for-human\n",
    );
    let tasks = |args: &[&str]| {
        enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .arg("tasks")
            .args(args)
            .current_dir(&r.root)
            .output()
            .expect("run enallagi tasks")
    };
    let stdout = |args: &[&str]| {
        String::from_utf8_lossy(&tasks(args).stdout)
            .trim()
            .to_string()
    };

    assert_eq!(stdout(&["ready"]), "");
    let out = tasks(&["set-status", "T-001", "done", "verified"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(stdout(&["ready"]), "T-002");
    assert_eq!(stdout(&["ids-at", "ready-for-human"]), "T-003");
    assert_eq!(stdout(&["ids-at", "done"]), "T-001");

    let out = tasks(&["set-status", "T-009", "done", ""]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("no block T-009"));

    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    let state = |id: &str| {
        t.get(id)
            .unwrap()
            .as_ref()
            .and_then(enallagi::tracker::state)
    };
    assert_eq!(state("T-001").as_deref(), Some("done"));
    assert_eq!(state("T-002").as_deref(), Some("ready-for-agent"));
    assert_eq!(state("T-003").as_deref(), Some("ready-for-human"));
    let gate = t
        .get("T-001")
        .unwrap()
        .and_then(|b| enallagi::block::field(&b, "gate"));
    assert_eq!(gate.as_deref(), Some("verified"));
}

#[test]
fn block_model_effort() {
    let r = enallagi::fixture::Repo::new();
    r.queue(
        ".enallagi",
        "\
         ## [T-001] picked per task\n\
         scope: src/a.ts\n\
         blockedBy: none\n\
         model: claude-haiku-4-5\n\
         effort: low\n\
         status: ready-for-agent\n\
         \n\
         ## [T-002] picked per role\n\
         scope: src/b.ts\n\
         blockedBy: none\n\
         status: ready-for-agent\n",
    );

    let field = |id: &str, key: &str| {
        let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(["tasks", "field", id, key])
            .current_dir(&r.root)
            .output()
            .expect("run enallagi tasks field");
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    assert_eq!(field("T-001", "model"), "claude-haiku-4-5");
    assert_eq!(field("T-001", "effort"), "low");
    assert_eq!(field("T-001", "scope"), "src/a.ts");
    // a ticket file writes `none` as an empty `Blocked by:`
    assert!(matches!(field("T-001", "blockedBy").as_str(), "none" | ""));
    assert_eq!(field("T-002", "model"), "");
    assert_eq!(field("T-002", "effort"), "");

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "ready"])
        .current_dir(&r.root)
        .output()
        .expect("run enallagi tasks ready");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "T-001",
        "{out:?}"
    );
}

fn base_of(root: &std::path::Path, task: &str) -> std::process::Output {
    enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["base", task])
        .current_dir(root)
        .output()
        .expect("run enallagi base")
}

#[test]
fn base_prints_product_sha() {
    let r = enallagi::fixture::Repo::new();
    let git = |dir: &std::path::Path, args: &[&str]| enallagi::git::git(dir, args).expect("git");
    r.write(".git/info/exclude", ".enallagi/\n");
    let queued = git(&r.root, &["rev-parse", "HEAD"]);
    let state = r.root.join(".enallagi");
    r.queue(
        ".enallagi",
        "## [T-001] open\nscope: src/a.ts\nstatus: ready-for-agent\n",
    );
    git(&state, &["init", "-q"]);
    git(&state, &["config", "user.name", "t"]);
    git(&state, &["config", "user.email", "t@t"]);
    git(&state, &["add", "-A"]);
    git(&state, &["commit", "-qm", &format!("queue at {queued}")]);

    r.write("src/a.ts", "work\n");
    r.commit_all("T-001 work");
    let worked = git(&r.root, &["rev-parse", "HEAD"]);
    r.queue(
        ".enallagi",
        "## [T-001] open\nscope: src/a.ts\nstatus: review\n\n## [T-002] by hand\nstatus: ready-for-agent\n",
    );
    git(&state, &["add", "-A"]);
    git(
        &state,
        &["commit", "-qm", &format!("implement T-001 at {worked}")],
    );

    let out = base_of(&r.root, "T-001");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), queued);

    r.queue(
        ".enallagi",
        "## [T-001] open\nstatus: review\n\n## [T-002] by hand\nstatus: ready-for-agent\n\n## [T-003] by hand\nstatus: ready-for-agent\n",
    );
    git(&state, &["add", "-A"]);
    git(&state, &["commit", "-qm", "queue: T-003"]);
    let out = base_of(&r.root, "T-003");
    assert_ne!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("queue: T-003"),
        "{out:?}"
    );
}

fn in_harness(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("run harness")
}

#[test]
fn prune_defaults_prints_no_install_header() {
    let r = enallagi::fixture::Repo::new();
    let init = in_harness(&r.root, &["init"]);
    assert_eq!(init.status.code(), Some(0), "{init:?}");
    assert!(
        String::from_utf8_lossy(&init.stdout).contains("installing the harness into"),
        "{init:?}"
    );
    let prune = in_harness(&r.root, &["init", "--prune-defaults"]);
    assert_eq!(prune.status.code(), Some(0), "{prune:?}");
    assert!(
        !String::from_utf8_lossy(&prune.stdout).contains("installing the harness into"),
        "{prune:?}"
    );
}

#[test]
fn prune_defaults_without_config_exits_1() {
    let r = enallagi::fixture::Repo::new();
    let out = in_harness(&r.root, &["init", "--prune-defaults"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("enallagi.toml not found"),
        "{out:?}"
    );
    assert!(!r.root.join(".enallagi").exists(), "{out:?}");
}

#[test]
fn prune_defaults_says_when_nothing_dropped() {
    let r = enallagi::fixture::Repo::new();
    let init = in_harness(&r.root, &["init"]);
    assert_eq!(init.status.code(), Some(0), "{init:?}");
    let config = enallagi::config::config_path(&r.root);
    std::fs::write(&config, "[check]\ncommand = \"make check\"\n").expect("write config");
    let out = in_harness(&r.root, &["init", "--prune-defaults"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("no key equals its default"),
        "{out:?}"
    );
}

#[test]
fn gate_scope_grown_baseline() {
    let r = enallagi::fixture::Repo::new();
    let git = |dir: &std::path::Path, args: &[&str]| enallagi::git::git(dir, args).expect("git");
    let out = in_harness(&r.root, &["init"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    git(
        &r.root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "install",
        ],
    );
    let queued = git(&r.root, &["rev-parse", "HEAD"]);
    let state = r.root.join(".enallagi");
    // CI runners have no global git identity, so the nested repository needs its own
    git(&state, &["config", "user.name", "t"]);
    git(&state, &["config", "user.email", "t@t"]);
    let commit_state = |msg: &str| {
        git(&state, &["add", "-A"]);
        git(
            &state,
            &["-c", "commit.gpgsign=false", "commit", "-qm", msg],
        );
    };
    let block = "\n## [T-900] grow\nscope: a.txt\nstatus: review\n";
    r.queue(".enallagi", block);
    commit_state(&format!("queue at {queued}"));

    r.write("a.txt", "work\n");
    r.commit_all("T-900 work");
    let worked = git(&r.root, &["rev-parse", "HEAD"]);
    let baseline = std::fs::read_to_string(state.join(".check-baseline")).unwrap_or_default();
    r.write(".enallagi/.check-baseline", &format!("{baseline}alpha\n"));
    let done = block.replace("status: review", "status: done");
    r.queue(".enallagi", &done);
    commit_state(&format!("verify T-900 at {worked}"));

    let base = base_of(&r.root, "T-900");
    assert_eq!(String::from_utf8_lossy(&base.stdout).trim(), queued);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("added a line to .check-baseline"),
        "{out:?}"
    );

    // init's state commit records its revision, so an unrecorded one is a product commit
    r.write("b.txt", "later\n");
    r.commit_all("unrelated");
    let unrecorded = git(&r.root, &["rev-parse", "HEAD"]);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &unrecorded]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("records product revision"),
        "{out:?}"
    );
}

// T-900 verified, `demo` locked at base; `at_base` vendors early, so the vendor commit rewrites it
fn vendored_demo(at_base: bool) -> (enallagi::fixture::Repo, String) {
    let r = enallagi::fixture::Repo::new();
    let git = |dir: &std::path::Path, args: &[&str]| enallagi::git::git(dir, args).expect("git");
    let out = in_harness(&r.root, &["init"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    git(
        &r.root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "install",
        ],
    );
    let queued = git(&r.root, &["rev-parse", "HEAD"]);
    let state = r.root.join(".enallagi");
    // CI runners have no global git identity, so the nested repository needs its own
    git(&state, &["config", "user.name", "t"]);
    git(&state, &["config", "user.email", "t@t"]);
    let commit_state = |msg: &str| {
        git(&state, &["add", "-A"]);
        git(
            &state,
            &["-c", "commit.gpgsign=false", "commit", "-qm", msg],
        );
    };
    let skill = ".enallagi/adapters/claude/skills/demo/SKILL.md";
    let block = "\n## [T-900] vendor\nscope: a.txt\nrows: none — harness\nstatus: review\n";
    r.queue(".enallagi", block);
    r.write(
        ".enallagi/harness.lock",
        "version = 1\n\n[[skill]]\nid = \"demo\"\nsource = \"path:vendor/demo\"\nsha256 = \"aaa\"\n",
    );
    if at_base {
        r.write(skill, "# demo\n");
    }
    commit_state(&format!("queue at {queued}"));

    r.write(skill, "# demo, re-vendored\n");
    commit_state("chore(vendor): T-900 demo");

    r.write("a.txt", "work\n");
    r.commit_all("T-900 work");
    let worked = git(&r.root, &["rev-parse", "HEAD"]);
    let done = block.replace("status: review", "status: done");
    r.queue(".enallagi", &done);
    commit_state(&format!("verify T-900 at {worked}"));
    (r, queued)
}

#[test]
fn gate_scope_exempts_a_revendored_locked_skill() {
    let (r, queued) = vendored_demo(false);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("T-900 stayed inside its scope."),
        "{out:?}"
    );
}

#[test]
fn gate_scope_rejects_a_rewritten_vendored_skill() {
    let (r, queued) = vendored_demo(true);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains(".enallagi/adapters/claude/skills/demo/SKILL.md"),
        "{out:?}"
    );
}

// T-900 verified; `product` rides the work commit and `state` the verify commit, like a mid-run STOP
fn verified_install(product: &[&str], state: &[&str]) -> (enallagi::fixture::Repo, String) {
    verified_install_as(product, state, "scope: a.txt\nstatus: done\n")
}

// `verdict` is the block's body as the verifier leaves it, so a test can widen its scope: line
fn verified_install_as(
    product: &[&str],
    state: &[&str],
    verdict: &str,
) -> (enallagi::fixture::Repo, String) {
    verified_install_promoted(product, state, verdict, None)
}

// `promoted` queues the block proposed with a placeholder scope, then promotes it with this one
fn verified_install_promoted(
    product: &[&str],
    state: &[&str],
    verdict: &str,
    promoted: Option<&str>,
) -> (enallagi::fixture::Repo, String) {
    let r = enallagi::fixture::Repo::new();
    let git = |dir: &std::path::Path, args: &[&str]| enallagi::git::git(dir, args).expect("git");
    let out = in_harness(&r.root, &["init"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    git(
        &r.root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "install",
        ],
    );
    let queued = git(&r.root, &["rev-parse", "HEAD"]);
    let state_dir = r.root.join(".enallagi");
    // CI runners have no global git identity, so the nested repository needs its own
    git(&state_dir, &["config", "user.name", "t"]);
    git(&state_dir, &["config", "user.email", "t@t"]);
    let commit_state = |msg: &str| {
        git(&state_dir, &["add", "-A"]);
        git(
            &state_dir,
            &["-c", "commit.gpgsign=false", "commit", "-qm", msg],
        );
    };
    match promoted {
        Some(scope) => {
            let block = "\n## [T-900] halt\nscope: src/thing.ts\nstatus: needs-triage\n";
            r.queue(".enallagi", block);
            commit_state(&format!("queue at {queued}"));
            let block = format!("\n## [T-900] halt\nscope: {scope}\nstatus: ready-for-agent\n");
            r.queue(".enallagi", &block);
            commit_state(&format!("promote at {queued}"));
        }
        None => {
            let block = "\n## [T-900] halt\nscope: a.txt\nstatus: review\n";
            r.queue(".enallagi", block);
            commit_state(&format!("queue at {queued}"));
        }
    }

    r.write("a.txt", "work\n");
    for f in product {
        r.write(f, "work\n");
    }
    r.commit_all("T-900 work");
    let worked = git(&r.root, &["rev-parse", "HEAD"]);
    for f in state {
        r.write(f, "");
    }
    let done = format!("\n## [T-900] halt\n{verdict}");
    r.queue(".enallagi", &done);
    commit_state(&format!("verify T-900 at {worked}"));
    (r, queued)
}

#[test]
fn gate_scope_exempts_the_halt_marker() {
    let (r, queued) = verified_install(&[], &[".enallagi/STOP"]);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("T-900 stayed inside its scope."),
        "{out:?}"
    );
}

#[test]
fn gate_scope_rejects_a_file_off_the_scope_line() {
    let (r, queued) = verified_install(&["b.txt"], &[".enallagi/STOP"]);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("b.txt"), "{out:?}");
    assert!(!stdout.contains("STOP"), "{out:?}");
}

#[test]
fn gate_scope_silent_widening() {
    let verdict = "scope: a.txt, b.txt\nstatus: done\n";
    let (r, queued) = verified_install_as(&["b.txt"], &[], verdict);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("widened scope: with b.txt"), "{out:?}");
    assert!(stdout.contains("a `widened:` line"), "{out:?}");
    let tasks = queue_text(&r.root);
    let block = &tasks[tasks.find("## [T-900]").expect("T-900")..];
    assert!(block.contains("status: ready-for-agent"), "{block}");
    assert!(
        block.contains("gate: ") && block.contains("widened scope: with b.txt"),
        "{block}"
    );
}

#[test]
fn gate_scope_counts_widening_from_promotion() {
    let verdict = "scope: a.txt, b.txt\nstatus: done\n";
    let (r, queued) = verified_install_promoted(&["b.txt"], &[], verdict, Some("a.txt, b.txt"));
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

// the base a review-only round runs on: the product commit the implement round left behind
#[test]
fn gate_scope_rejects_a_widening_from_the_work() {
    let verdict = "scope: a.txt, b.txt\nstatus: done\n";
    let (r, _) = verified_install_as(&["b.txt"], &[], verdict);
    let worked = enallagi::git::git(&r.root, &["rev-parse", "HEAD"]).expect("git");
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &worked]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("widened scope: with b.txt"),
        "{out:?}"
    );
}

#[test]
fn gate_scope_names_a_widening_with_a_reason() {
    let verdict = "scope: a.txt, b.txt\nwidened: b.txt declares the flag\nstatus: done\n";
    let (r, queued) = verified_install_as(&["b.txt"], &[], verdict);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("T-900 stayed inside its scope."), "{out:?}");
    assert!(stdout.contains("widened scope: with b.txt"), "{out:?}");
    assert!(stdout.contains("b.txt declares the flag"), "{out:?}");
}

#[test]
fn tasks_ready_harness_dir() {
    let r = enallagi::fixture::Repo::new();
    r.queue(
        ".enallagi",
        "## [T-001] parent\nscope: src/a.ts\nstatus: ready-for-agent\n",
    );
    r.queue(
        "lane/.enallagi",
        "## [T-002] lane\nscope: src/a.ts\nstatus: ready-for-agent\n",
    );
    let outside = tempfile::TempDir::new().expect("tempdir");
    r.queue(
        &outside.path().to_string_lossy(),
        "## [T-003] outside\nscope: src/a.ts\nstatus: ready-for-agent\n",
    );

    let ready = |dir: &std::path::Path| {
        let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(["tasks", "ready"])
            .env("ENALLAGI_DIR", dir)
            .current_dir(&r.root)
            .output()
            .expect("run enallagi tasks ready");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(ready(&r.root.join("lane/.enallagi")), "T-002");
    assert_eq!(ready(outside.path()), "T-001");
}

#[test]
fn immutable_reads_nested_hashes() {
    let r = enallagi::fixture::Repo::new();
    r.write(
        "lane/.enallagi/test-hashes.json",
        r#"{"src/schema.ts":"deadbeef"}"#,
    );

    let mut child = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["hook", "immutable"])
        .env("ENALLAGI_DIR", r.root.join("lane/.enallagi"))
        .env_remove("CLAUDE_PROJECT_DIR")
        .current_dir(&r.root)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run enallagi hook immutable");
    use std::io::Write;
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(br#"{"tool_input":{"file_path":"src/schema.ts"}}"#)
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

#[test]
fn legacy_config_name_warns() {
    let r = enallagi::fixture::Repo::new();
    r.write(".enallagi/harness.toml", "[check]\ncommand = \"true\"\n");
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "list"])
        .current_dir(&r.root)
        .output()
        .expect("run tasks list");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("enallagi.toml"), "{stderr}");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
}

const DOC_UNRUNNABLE: [(&str, &str); 10] = [
    (
        "enallagi init --issue https://github.com/owner/repo/issues/12",
        "reads a GitHub issue over the network with gh",
    ),
    (
        "enallagi run --pr-per-task --iterations 1",
        "spawns the agent CLI, which no test may call",
    ),
    (
        "curl -fsSL https://raw.githubusercontent.com/thmsmtylr/enallagi/main/install.sh | sh",
        "downloads a release asset over the network",
    ),
    (
        "enallagi run --pipeline task --iterations 1",
        "spawns the agent CLI, which no test may call",
    ),
    (
        "enallagi run --pr-per-task --iterations 3",
        "spawns the agent CLI, which no test may call",
    ),
    (
        "enallagi issue owner/repo#12",
        "reads a GitHub issue over the network with gh",
    ),
    (
        "cargo install --locked --git https://github.com/thmsmtylr/enallagi enallagi",
        "builds from the network and installs outside the tree",
    ),
    (
        "enallagi worktree 1",
        "spawns the agent CLI in a worktree, which no test may call",
    ),
    (
        "enallagi watch",
        "attaches a live view and does not exit on its own",
    ),
    (
        "enallagi pr T-001 --push",
        "pushes a branch and opens a pull request",
    ),
];

// a fence that is not commands, named by its first line, never by its tag
const DOC_DATA_FENCES: [(&str, &str); 3] = [
    (
        "# T-001: the date parser drops a timezone",
        "a ticket file the reader writes under .enallagi/specs/",
    ),
    (
        r#"  "test": "node --import tsx --test src/*.test.ts""#,
        "a line of the example repository's package.json",
    ),
    (
        r#"  detected: check.command = "npm test" (package.json:4)"#,
        "what enallagi init prints on the example repository",
    ),
];

// the documents whose fences run, in order, in one fixture repository each
const RUN_DOCS: [&str; 2] = ["README.md", "docs/setup.md"];

// a fence is commands whatever its tag, so `sh`, `text` and an untagged fence all run
fn readme_commands(readme: &str) -> Vec<String> {
    let mut fenced: Vec<String> = Vec::new();
    let mut inside = false;
    let mut first = false;
    let mut data = false;
    for line in readme.lines() {
        if line.starts_with("```") {
            inside = !inside;
            first = inside;
            data = false;
            continue;
        }
        if first {
            data = DOC_DATA_FENCES.iter().any(|(named, _)| *named == line);
            first = false;
        }
        let cmd = line.split_once(" #").map_or(line, |(c, _)| c).trim();
        if inside && !data && !cmd.is_empty() {
            fenced.push(cmd.to_string());
        }
    }
    fenced
}

fn readme_command_failures(readme: &str) -> Vec<String> {
    let repo = enallagi::fixture::Repo::new();
    let mut failures = Vec::new();
    for cmd in readme_commands(readme) {
        if DOC_UNRUNNABLE.iter().any(|(named, _)| *named == cmd) {
            continue;
        }
        let args: Vec<&str> = cmd.split_whitespace().collect();
        let mut run = match args[0] {
            "enallagi" => {
                let mut c = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"));
                c.args(&args[1..]);
                c
            }
            // a quoted commit message survives only a shell
            "git" => {
                let mut c = std::process::Command::new("sh");
                c.args(["-c", &cmd]);
                c
            }
            _ => {
                failures.push(format!("{cmd} has no runner and no reason"));
                continue;
            }
        };
        // a runner has no global git identity, and the documents' commits must still run
        let out = run
            .current_dir(&repo.root)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap_or_else(|e| panic!("{cmd}: {e}"));
        if out.status.code() != Some(0) {
            failures.push(format!("{cmd}: {out:?}"));
        }
    }
    failures
}

#[test]
fn every_readme_command_runs_or_is_named() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md");
    let fenced = readme_commands(&readme);
    assert!(fenced.len() > 8, "README fenced commands: {fenced:?}");
    let failures = readme_command_failures(&readme);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn every_setup_command_runs_or_is_named() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let setup = std::fs::read_to_string(root.join("docs/setup.md")).expect("docs/setup.md");
    let fenced = readme_commands(&setup);
    assert!(
        fenced.len() > 8,
        "docs/setup.md fenced commands: {fenced:?}"
    );
    let failures = readme_command_failures(&setup);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn every_named_fence_is_in_a_run_doc() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let docs: Vec<String> = RUN_DOCS
        .iter()
        .map(|rel| std::fs::read_to_string(root.join(rel)).expect(rel))
        .collect();
    let fenced: Vec<String> = docs.iter().flat_map(|d| readme_commands(d)).collect();
    for (cmd, _) in &DOC_UNRUNNABLE {
        assert!(
            fenced.iter().any(|f| f == cmd),
            "no document command reads {cmd}"
        );
    }
    for (first, _) in &DOC_DATA_FENCES {
        assert!(
            docs.iter().any(|d| d.lines().any(|l| l == *first)),
            "no document fence opens with {first}"
        );
    }
}

#[test]
fn readme_fence_fails() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md");
    for tag in ["sh", "text", "markdown", "toml", ""] {
        let appended = format!("{readme}\n```{tag}\nenallagi tasks nope\n```\n");
        let failures = readme_command_failures(&appended);
        assert!(
            failures
                .iter()
                .any(|f| f.starts_with("enallagi tasks nope")),
            "tag {tag:?}: {failures:#?}"
        );
    }
}

// a ticket file keeps every line of a block, but in its own order
fn assert_lines(block: &str, want: &str) {
    for line in want.lines() {
        assert!(
            block.lines().any(|l| l.trim_end() == line.trim_end()),
            "{line}\n{block}"
        );
    }
}

// every ticket rendered as a block in id order
fn queue_text(root: &std::path::Path) -> String {
    let t = enallagi::tracker::Tracker::open(root, ".enallagi");
    let blocks = t.blocks().expect("the queue");
    let text: Vec<String> = blocks.iter().map(enallagi::block::block_text).collect();
    text.join("\n\n")
}

struct GhRepo {
    repo: enallagi::fixture::Repo,
    tools: tempfile::TempDir,
    twin: Option<std::path::PathBuf>,
}

use enallagi::fixture::twin::{Response, State};

fn answering(responses: Vec<Response>) -> Option<State> {
    Some(State {
        responses,
        ..State::default()
    })
}

// the recorded issue, answered for both forms of its reference
fn issue_twin() -> Option<State> {
    let json = include_str!("fixtures/issues/help-wanted.json");
    answering(
        ["owner/repo#12", "https://github.com/owner/repo/issues/12"]
            .map(|r| Response::to(&enallagi::issue::gh_args(r).expect("args"), json))
            .to_vec(),
    )
}

fn review_twin(json: &str) -> Option<State> {
    let args = enallagi::review::gh_args("owner/repo#13").expect("args");
    answering(vec![Response::to(&args, json)])
}

impl GhRepo {
    // the instance is committed in its own repository, so its porcelain lists only what the command wrote
    fn new(twin: Option<State>) -> GhRepo {
        GhRepo::with_state(twin, &[])
    }

    fn with_state(twin: Option<State>, files: &[(&str, &str)]) -> GhRepo {
        let repo = enallagi::fixture::Repo::new();
        repo.init_harness("");
        let state = repo.root.join(".enallagi");
        // init seeds no ticket; one standing task gives every appended block the next id
        let mut queue = "## [T-001] the first task\nscope: src/a.ts\nblockedBy: none\nstatus: ready-for-agent\nrows: none \u{2014} harness\ncriteria:\n  - it happens\nnotes:\n".to_string();
        for (name, text) in files {
            match *name {
                "queue" => queue = text.to_string(),
                _ => std::fs::write(state.join(name), text).expect("write state"),
            }
        }
        repo.queue(".enallagi", &queue);
        for args in [
            &["add", "-A"][..],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "state",
            ][..],
        ] {
            enallagi::git::git(&state, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"));
        }
        let tools = tempfile::tempdir().expect("tempdir");
        let twin = twin.map(|state| {
            enallagi::fixture::twin::install(tools.path(), env!("CARGO_BIN_EXE_enallagi"), &state).1
        });
        GhRepo { repo, tools, twin }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        enallagi::fixture::twin::load(self.twin.as_ref().expect("a twin")).calls
    }

    // PATH is the tools directory alone, so no twin is a missing gh
    fn command(&self, name: &str, args: &[&str]) -> std::process::Output {
        enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .arg(name)
            .args(args)
            .current_dir(&self.repo.root)
            .env_remove("ENALLAGI_DIR")
            .env("PATH", self.tools.path())
            .output()
            .expect("run enallagi")
    }

    fn issue(&self, args: &[&str]) -> std::process::Output {
        self.command("issue", args)
    }

    fn review(&self, args: &[&str]) -> std::process::Output {
        self.command("review", args)
    }

    fn tasks(&self) -> String {
        queue_text(&self.repo.root)
    }

    fn porcelain(&self) -> String {
        let out = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(self.repo.root.join(".enallagi"))
            .output()
            .expect("git status");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

#[test]
fn issue_appends_block() {
    let f = GhRepo::new(issue_twin());
    let before = f.tasks();

    let out = f.issue(&["owner/repo#12"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    assert_eq!(
        f.calls(),
        [[
            "issue",
            "view",
            "12",
            "--repo",
            "owner/repo",
            "--json",
            "number,title,body,url,labels,state"
        ]]
    );
    assert!(f.porcelain().starts_with("?? specs/"), "{}", f.porcelain());

    let after = f.tasks();
    assert!(
        after.starts_with(&before),
        "the queue was rewritten, not appended to"
    );
    let blocks = enallagi::block::parse(&after).expect("parse");
    let last = blocks.last().expect("a block");
    assert_eq!(last.id, "T-002");
    assert_eq!(last.title, "a file uploaded unzipped cannot be downloaded");
    let block = enallagi::block::block_text(last);
    let want = "\
scope: <written by the adjudicator at promotion>
blockedBy:
status: needs-triage
probe: issue
command: `gh issue view https://github.com/owner/repo/issues/12 --json state,title --jq '.state + \": \" + .title'`
output: OPEN: a file uploaded unzipped cannot be downloaded
rows: none — harness
criteria:
  - <objective, and naming the command whose output changes when it is done>
notes: https://github.com/owner/repo/issues/12
  labels: enhancement, help wanted
  > ### Describe the bug
  >
  > A download of a file uploaded unzipped fails with `archive: false`.
  >
  > ### Affected version
  >
  > ```
  > tool version 2.88.1 (2026-03-12)
  > ```
  >
  > ## Steps to reproduce the behavior
  >
  > 1. Upload the file unzipped.
  > 2. Download it.";
    assert_lines(&block, want);
}

#[test]
fn issue_dry_run() {
    let f = GhRepo::new(issue_twin());
    let before = f.tasks();

    let out = f.issue(&["https://github.com/owner/repo/issues/12", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("## [T-002] a file uploaded unzipped cannot be downloaded\n"),
        "{stdout}"
    );
    assert!(stdout.contains("status: needs-triage"), "{stdout}");
    assert_eq!(
        f.calls(),
        [[
            "issue",
            "view",
            "https://github.com/owner/repo/issues/12",
            "--json",
            "number,title,body,url,labels,state"
        ]]
    );
    assert_eq!(f.tasks(), before);
    assert_eq!(f.porcelain(), "");
}

#[test]
fn issue_refuses_queued() {
    let f = GhRepo::new(issue_twin());
    assert_eq!(f.issue(&["owner/repo#12"]).status.code(), Some(0));
    let once = f.tasks();

    let out = f.issue(&["owner/repo#12"]);
    assert_ne!(out.status.code(), Some(0), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("T-002"), "{stderr}");
    assert!(
        stderr.contains("https://github.com/owner/repo/issues/12"),
        "{stderr}"
    );
    assert_eq!(f.tasks(), once);
}

const LANDED_STUB: &str = "## [T-001] landed\nstatus: done\narchived: DECISIONS.md\n";

#[test]
fn issue_refuses_archived() {
    let decisions = "# DECISIONS\n\n## Rejected findings\n\n## [T-001] landed\nstatus: done\n\
        notes: https://github.com/owner/repo/issues/12\n";
    let f = GhRepo::with_state(
        issue_twin(),
        &[("queue", LANDED_STUB), ("DECISIONS.md", decisions)],
    );
    let before = f.tasks();
    for args in [&["owner/repo#12"][..], &["owner/repo#12", "--dry-run"][..]] {
        let out = f.issue(args);
        assert_ne!(out.status.code(), Some(0), "{args:?}: {out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("T-001"), "{args:?}: {stderr}");
        assert!(
            stderr.contains("https://github.com/owner/repo/issues/12"),
            "{args:?}: {stderr}"
        );
        assert_eq!(f.tasks(), before, "{args:?}");
        assert_eq!(f.porcelain(), "", "{args:?}");
    }
}

#[test]
fn issue_requeues_expired() {
    let decisions = "# DECISIONS\n\n## Rejected findings\n\n## Expired findings\n\n\
        ## [T-001] landed\nstatus: needs-triage\nnotes: https://github.com/owner/repo/issues/12\n";
    let f = GhRepo::with_state(
        issue_twin(),
        &[("queue", LANDED_STUB), ("DECISIONS.md", decisions)],
    );
    let out = f.issue(&["owner/repo#12"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let blocks = enallagi::block::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.last().expect("a block").id, "T-002");
}

#[test]
fn issue_names_failed_command() {
    let args = enallagi::issue::gh_args("owner/repo#12").expect("args");
    let failing = |stderr: &str, exit: i32| Response {
        stderr: stderr.to_string(),
        exit,
        ..Response::to(&args, "")
    };
    for twin in [
        None,
        answering(vec![failing(
            "To get started with GitHub CLI, please run:  gh auth login\n",
            4,
        )]),
        answering(vec![failing(
            "GraphQL: Could not resolve to an issue with the number of 12.\n",
            1,
        )]),
        answering(vec![Response::to(&args, "not json\n")]),
    ] {
        let gh = format!("{twin:?}");
        let f = GhRepo::new(twin);
        let before = f.tasks();
        let out = f.issue(&["owner/repo#12"]);
        assert_ne!(out.status.code(), Some(0), "{gh}: {out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("gh issue view 12 --repo owner/repo"),
            "{gh}: {stderr}"
        );
        assert_eq!(f.tasks(), before, "{gh}");
        assert_eq!(f.porcelain(), "", "{gh}");
    }
}

#[test]
fn reference_config_links_the_guide() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reference =
        std::fs::read_to_string(root.join("docs/reference.md")).expect("docs/reference.md");
    let section = reference
        .split("\n## Configuration\n")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("docs/reference.md has a Configuration section");
    let rows: Vec<&str> = section.lines().filter(|l| l.starts_with('|')).collect();
    assert!(rows.is_empty(), "Configuration keeps a table: {rows:?}");
    assert!(
        section.contains("](configuration.md)"),
        "Configuration never links configuration.md"
    );
}

const SUMMARY: &str = include_str!("fixtures/reviews/summary.json");
const SETTLED: &str = include_str!("fixtures/reviews/settled.json");
const GENERATED: &str = include_str!("fixtures/reviews/generated.json");
const PLAIN: &str = include_str!("fixtures/reviews/plain.json");
const SHORT: &str = include_str!("fixtures/reviews/short.json");
const THREAD: &str = include_str!("fixtures/reviews/thread.json");
const AGREED: &str = include_str!("fixtures/reviews/agreed.json");

// everything a proposed block states before the comment's own words
fn scaffold_of(block: &str) -> Vec<&str> {
    block
        .lines()
        .skip(1)
        .take_while(|l| !l.starts_with("notes:"))
        .collect()
}

#[test]
fn short_read_kept() {
    let f = GhRepo::new(review_twin(SHORT));
    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("only the first 100 of each were read"),
        "{said}"
    );
    let blocks = enallagi::block::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.len(), 2, "the comment it did read is queued");
}

#[test]
fn review_block_per_thread() {
    let f = GhRepo::new(review_twin(SUMMARY));
    let before = f.tasks();

    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    assert_eq!(
        f.calls(),
        [enallagi::review::gh_args("owner/repo#13").expect("args")]
    );

    let after = f.tasks();
    assert!(after.starts_with(&before), "the queue was rewritten");
    let blocks = enallagi::block::parse(&after).expect("parse");
    assert_eq!(blocks.len(), 4, "{after}");
    let first = enallagi::block::block_text(&blocks[1]);
    let want = "\
## [T-002] Refactor suggestion
scope: src/date.rs
blockedBy:
status: needs-triage
rows: none — harness
criteria:
  - <objective, and naming the command whose output changes when it is done>
notes: https://github.com/owner/repo/pull/13#discussion_r1000000001
  > ### Refactor suggestion
  >
  > `parse` drops the timezone when the offset is absent.
  >
  > ```suggestion
  >     let offset = offset.unwrap_or(UTC);
  > ```";
    assert_lines(&first, want);
    assert_eq!(blocks[2].id, "T-003");
    assert_eq!(
        enallagi::block::field(&blocks[2], "scope").as_deref(),
        Some("src/cli.rs")
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("  proposed: ## [T-002] "), "{stdout}");
    assert!(stdout.contains("  proposed: ## [T-003] "), "{stdout}");
    assert!(
        stdout.contains("  proposed: ## [T-004] **Actionable comments posted: 2**"),
        "{stdout}"
    );
    assert_eq!(
        enallagi::block::field(&blocks[3], "scope").as_deref(),
        Some(enallagi::block::SCOPE_PLACEHOLDER)
    );
    assert!(stdout.contains("  folded: 0 later in a thread"), "{stdout}");
}

#[test]
fn review_folds_thread() {
    let f = GhRepo::new(review_twin(THREAD));
    let before = enallagi::block::parse(&f.tasks()).expect("parse").len();

    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let blocks = enallagi::block::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.len(), before + 1, "{}", f.tasks());
    let block = enallagi::block::block_text(&blocks[before]);
    assert!(
        block.starts_with("## [T-002] parse drops the timezone\n"),
        "{block}"
    );
    assert_eq!(
        enallagi::block::field(&blocks[before], "notes").as_deref(),
        Some("https://github.com/owner/repo/pull/13#discussion_r1000000007")
    );
    assert!(
        block.contains("  > parse drops the timezone\n  >\n  > Thanks, fixed in the next push."),
        "{block}"
    );
    assert!(block.contains("\n  thread: PRRT_kwDOA1\n"), "{block}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("  folded: 1 later in a thread"), "{stdout}");

    let once = f.tasks();
    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(f.tasks(), once);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(
            "  carried: T-002 https://github.com/owner/repo/pull/13#discussion_r1000000007"
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("discussion_r1000000008"), "{stdout}");
}

#[test]
fn repeated_finding_one_block() {
    let f = GhRepo::new(review_twin(AGREED));
    let before = enallagi::block::parse(&f.tasks()).expect("parse").len();

    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let blocks = enallagi::block::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.len(), before + 1, "{}", f.tasks());
    let block = enallagi::block::block_text(&blocks[before]);
    assert!(
        block.contains(
            "notes: https://github.com/owner/repo/pull/13#discussion_r1000000009\n  \
             > https://github.com/owner/repo/pull/13#discussion_r1000000009\n  \
             > https://github.com/owner/repo/pull/13#discussion_r1000000010\n"
        ),
        "{block}"
    );

    let once = f.tasks();
    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(f.tasks(), once);
}

#[test]
fn different_finding_same_line() {
    let differs = AGREED.replacen("drops the timezone", "drops the offset", 1);
    let f = GhRepo::new(review_twin(&differs));
    let before = enallagi::block::parse(&f.tasks()).expect("parse").len();

    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let blocks = enallagi::block::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.len(), before + 2, "{}", f.tasks());
    assert!(
        !f.tasks().contains("  > https://"),
        "a block raised once lists no raiser: {}",
        f.tasks()
    );
}

#[test]
fn review_skips_resolved() {
    let f = GhRepo::new(review_twin(SETTLED));
    let out = f.review(&["https://github.com/owner/repo/pull/13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    let calls = f.calls();
    assert!(calls[0].contains(&"number=13".to_string()), "{calls:?}");

    let blocks = enallagi::block::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.len(), 2, "{}", f.tasks());
    assert_eq!(
        enallagi::block::field(&blocks[1], "scope").as_deref(),
        Some("src/cli.rs")
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("  skipped: 2 resolved or outdated"),
        "{stdout}"
    );
}

#[test]
fn review_rerun_appends_nothing() {
    let f = GhRepo::new(review_twin(SUMMARY));
    assert_eq!(f.review(&["owner/repo#13"]).status.code(), Some(0));
    let once = f.tasks();

    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(f.tasks(), once);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("proposed:"), "{stdout}");
    assert!(
        stdout.contains(
            "  carried: T-002 https://github.com/owner/repo/pull/13#discussion_r1000000001"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "  carried: T-003 https://github.com/owner/repo/pull/13#discussion_r1000000002"
        ),
        "{stdout}"
    );
}

#[test]
fn review_dry_run() {
    let f = GhRepo::new(review_twin(SUMMARY));
    let before = f.tasks();

    let out = f.review(&["owner/repo#13", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("## [T-002] Refactor suggestion\n"),
        "{stdout}"
    );
    assert!(stdout.contains("\n## [T-003] "), "{stdout}");
    assert!(stdout.contains("status: needs-triage"), "{stdout}");
    assert_eq!(f.tasks(), before);
    assert_eq!(f.porcelain(), "");
}

#[test]
fn comments_share_shape() {
    let mut shapes = Vec::new();
    for fixture in [GENERATED, PLAIN] {
        let f = GhRepo::new(review_twin(fixture));
        let out = f.review(&["owner/repo#13", "--dry-run"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            stdout.contains("notes: https://github.com/owner/repo/pull/13#discussion_r1000000003"),
            "{stdout}"
        );
        shapes.push(
            scaffold_of(&stdout)
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(shapes[0], shapes[1]);
    assert_eq!(shapes[0][0], "scope: src/date.rs");
}

// a name outside the bot table would be a filter on who reviewed; the table maps, it never drops
#[test]
fn vendors_in_bot_table() {
    let text = include_str!("../src/review.rs");
    let start = text.find("\nmod bots {").expect("the bot table");
    let end = start + text[start..].find("\n}\n").expect("its end");
    let vendors = [
        "kilo",
        "greptile",
        "copilot",
        "bugbot",
        "coderabbit",
        "claude",
    ];
    let named = |part: &str| -> Vec<String> {
        let lower = part.to_lowercase();
        let tokens: Vec<&str> = part
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .collect();
        ["author", "login"]
            .into_iter()
            .filter(|w| tokens.contains(w))
            .chain(vendors.into_iter().filter(|v| lower.contains(v)))
            .map(String::from)
            .collect()
    };
    // the unit tests drive the table through the twin's recordings, so they name vendors
    let tests = text.find("\n#[cfg(test)]").unwrap_or(text.len());
    let outside = format!("{}{}", &text[..start], &text[end..tests]);
    assert_eq!(named(&outside), Vec::<String>::new());
    assert_eq!(
        named(include_str!("../src/cli/review.rs")),
        Vec::<String>::new()
    );
    let mut inside = named(&text[start..end]);
    inside.sort();
    assert_eq!(
        inside,
        [
            "author",
            "bugbot",
            "claude",
            "coderabbit",
            "copilot",
            "greptile",
            "kilo",
            "login"
        ]
    );
}

#[test]
fn run_refuses_unfilled() {
    let r = enallagi::fixture::Repo::new();
    let out = in_harness(&r.root, &["init", "--check", "true", "--fail-name", "(.+)"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let block = format!(
        "\n## [T-001] a\nscope: {}\nstatus: ready-for-agent\n",
        enallagi::block::SCOPE_PLACEHOLDER
    );
    r.queue(".enallagi", &block);
    for args in [
        &["run", "--iterations", "1", "--no-tui"][..],
        &["worktree", "1"],
    ] {
        let out = in_harness(&r.root, args);
        assert_ne!(out.status.code(), Some(0), "{args:?} {out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("T-001 is ready but still carries"),
            "{args:?} {stderr}"
        );
    }
    assert!(!r.root.join(".enallagi/worktrees").exists());
}

const SPEC: &str = "# App\n\n**Status:** ready-for-agent\n\n### Exit criteria\n\n| Behaviour | Test |\n|---|---|\n| sums | `src/a.test.ts::adds` |\n\n## Out of Scope\n";

fn draft(num: &str, blocked: &str, criterion: &str) -> String {
    format!("# {num}: Step {num}\n\n**What to build:** step {num}.\n\n**Blocked by:** {blocked}\n\n**Status:** ready-for-agent\n\n- [ ] {criterion}\n")
}

// a committed accepted spec `app` and a ticket T-001 still open
fn decompose_repo() -> enallagi::fixture::Repo {
    let r = enallagi::fixture::Repo::new();
    r.queue(".enallagi", "## [T-001] open\nstatus: ready-for-agent\n");
    r.write(".enallagi/specs/app/spec.md", SPEC);
    r.commit_all("spec");
    r
}

type Drafts<'a> = Vec<(&'a str, String)>;

fn check(r: &enallagi::fixture::Repo, drafts: &[(&str, String)]) -> (Option<i32>, String) {
    let issues = r.root.join(".enallagi/specs/app/issues");
    let _ = std::fs::remove_dir_all(&issues);
    for (name, text) in drafts {
        r.write(&format!(".enallagi/specs/app/issues/{name}"), text);
    }
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "check", "app"])
        .current_dir(&r.root)
        .output()
        .expect("run enallagi tasks check");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

#[test]
fn check_flags_drafts() {
    let r = decompose_repo();
    let good = [
        (
            "01-first.md",
            draft("01", "None (can start immediately)", "`adds` passes"),
        ),
        ("02-second.md", draft("02", "01, T-001", "it shows")),
    ];
    assert_eq!(check(&r, &good), (Some(0), String::new()));

    let cases: Vec<(Drafts, &[&str])> = vec![
        (vec![], &["no drafts"]),
        (
            vec![("01-bare.md", "01 untitled\n".to_string())],
            &[
                "01-bare.md: no `# NN: title` heading",
                "01-bare.md: no **Status:** line",
                "01-bare.md: no **Blocked by:** line",
                "01-bare.md: no **What to build:** paragraph",
                "01-bare.md: no `- [ ]` criterion",
            ],
        ),
        (
            vec![("01-first.md", draft("01", "07", "`adds` passes"))],
            &["01-first.md: blocked by 07, which is no draft or ticket"],
        ),
        (
            vec![
                ("01-first.md", draft("01", "02", "`adds` passes")),
                ("02-second.md", draft("02", "01", "it shows")),
            ],
            &["blocking cycle through 01, 02"],
        ),
        (
            vec![("01-first.md", draft("01", "T-001", "`adds` passes"))],
            &["no draft is free to start"],
        ),
        (
            vec![("01-first.md", draft("01", "None", "it shows"))],
            &["`adds` is named by no draft's criteria"],
        ),
        (
            vec![good[0].clone(), ("notes.md", "scratch\n".to_string())],
            &[".enallagi/specs/app/issues/notes.md: changed beside the drafts"],
        ),
        (
            vec![
                good[0].clone(),
                ("01-again.md", draft("01", "None", "it shows")),
            ],
            &["01-first.md: 01 used twice"],
        ),
    ];
    for (drafts, want) in cases {
        let (code, out) = check(&r, &drafts);
        assert_eq!(code, Some(2), "{drafts:?}: {out}");
        for line in want {
            assert!(
                out.lines().any(|l| l.ends_with(line)),
                "{line} not in:\n{out}"
            );
        }
    }

    // a blocker whose pull request is open has landed for every draft after it
    r.queue(
        ".enallagi",
        "## [T-001] open\nstatus: ready-for-agent\n\n## [T-002] pulled\nstatus: open\n",
    );
    std::fs::remove_dir_all(r.root.join(".enallagi/specs/app/issues")).expect("rm drafts");
    r.commit_all("pulled");
    let after = [("01-first.md", draft("01", "T-002", "`adds` passes"))];
    assert_eq!(check(&r, &after), (Some(0), String::new()));

    r.write("src/b.ts", "export const y = 2\n");
    let (code, out) = check(&r, &good);
    assert_eq!(code, Some(2), "{out}");
    assert!(out.contains("src/b.ts: changed beside the drafts"), "{out}");
}

#[test]
fn check_refuses_spec_edit() {
    let r = decompose_repo();
    r.write(
        ".enallagi/specs/app/spec.md",
        &SPEC.replace("sums", "adds up"),
    );
    let good = [("01-first.md", draft("01", "None", "`adds` passes"))];
    let (code, out) = check(&r, &good);
    assert_eq!(code, Some(2), "{out}");
    assert_eq!(
        out.trim(),
        ".enallagi/specs/app/spec.md: changed; the decomposer never edits the spec"
    );
}

// the tickets gate appends `## Harness`, so a draft carrying one would end with two
#[test]
fn check_refuses_trailer() {
    let r = decompose_repo();
    let text = draft("01", "None", "`adds` passes") + "\n## Harness\n\nprobe: decompose\n";
    let (code, out) = check(&r, &[("01-first.md", text)]);
    assert_eq!(code, Some(2), "{out}");
    assert_eq!(
        out.trim(),
        "01-first.md: carries `## Harness`; the tickets gate writes it"
    );
}

// a lane's shell can sit inside a nested instance repo, whose toplevel is not the product's
fn nest(r: &enallagi::fixture::Repo) -> std::path::PathBuf {
    let git = |dir: &std::path::Path, args: &[&str]| enallagi::git::git(dir, args).expect("git");
    r.write(".git/info/exclude", ".enallagi/\n");
    let state = r.root.join(".enallagi");
    git(&r.root, &["rm", "-rq", "--cached", ".enallagi"]);
    r.commit_all("instance moves out");
    git(&state, &["init", "-q"]);
    git(&state, &["add", "-A"]);
    git(&state, &["commit", "-qm", "instance"]);
    state
}

#[test]
fn check_from_instance() {
    let r = decompose_repo();
    let state = nest(&r);
    r.write(
        ".enallagi/specs/app/issues/01-first.md",
        &draft("01", "None", "`adds` passes"),
    );
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "check", "app"])
        .current_dir(&state)
        .output()
        .expect("run enallagi tasks check");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn root_from_instance() {
    let r = decompose_repo();
    let state = nest(&r);
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["probe", "spec-untested"])
        .current_dir(&state)
        .output()
        .expect("run enallagi probe");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

const BIN: &str = env!("CARGO_BIN_EXE_enallagi");

fn twin_call(state: &std::path::Path, args: &[&str]) -> std::process::Output {
    enallagi::fixture::command(BIN)
        .args(["twin", "gh"])
        .args(args)
        .env(enallagi::fixture::twin::ENV, state)
        .output()
        .expect("run enallagi twin")
}

fn reviewed_pull() -> enallagi::fixture::twin::State {
    use enallagi::fixture::twin::{Pull, Remark, State, Thread};
    State {
        repo: "owner/repo".to_string(),
        default_branch: "main".to_string(),
        pulls: vec![Pull {
            number: 7,
            url: "https://github.com/owner/repo/pull/7".to_string(),
            head: "task/T-001".to_string(),
            base: "main".to_string(),
            state: "OPEN".to_string(),
            threads: vec![Thread {
                id: "PRRT_1".to_string(),
                comments: vec![Remark {
                    author: "kilo-code-bot".to_string(),
                    path: Some("src/date.rs".to_string()),
                    line: Some(3),
                    body: "parse drops the timezone".to_string(),
                    url: "https://github.com/owner/repo/pull/7#discussion_r1".to_string(),
                    ..Remark::default()
                }],
                ..Thread::default()
            }],
            ..Pull::default()
        }],
        ..State::default()
    }
}

#[test]
fn twin_serves_review_threads() {
    let f = GhRepo::new(Some(reviewed_pull()));
    let out = f.review(&["https://github.com/owner/repo/pull/7"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let blocks = enallagi::block::parse(&f.tasks()).expect("parse");
    let last = blocks.last().expect("a block");
    assert_eq!(last.title, "parse drops the timezone");
    assert_eq!(
        enallagi::block::field(last, "scope").as_deref(),
        Some("src/date.rs")
    );
    assert_eq!(enallagi::review::thread_of(last).as_deref(), Some("PRRT_1"));
}

#[test]
fn twin_settle_resolves_thread() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (_, path) = enallagi::fixture::twin::install(dir.path(), BIN, &reviewed_pull());
    let reply = format!("query={}", enallagi::review::REPLY);
    let resolve = format!("query={}", enallagi::review::RESOLVE);
    for args in [
        &[
            "api",
            "graphql",
            "-f",
            &reply,
            "-f",
            "thread=PRRT_1",
            "-f",
            "body=fixed",
        ][..],
        &["api", "graphql", "-f", &resolve, "-f", "thread=PRRT_1"],
    ] {
        let out = twin_call(&path, args);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
    }
    let state = enallagi::fixture::twin::load(&path);
    let thread = &state.pulls[0].threads[0];
    assert!(thread.resolved);
    assert_eq!(thread.comments.len(), 2);
    assert_eq!(thread.comments[1].body, "fixed");
}

#[test]
fn twin_push_round() {
    let dir = tempfile::tempdir().expect("tempdir");
    let work = dir.path().join("work");
    let origin = dir.path().join("origin.git");
    let r = enallagi::fixture::Repo::new();
    let git = |root: &std::path::Path, args: &[&str]| {
        enallagi::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    };
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--bare",
            &r.root.display().to_string(),
            "origin.git",
        ],
    );
    git(
        dir.path(),
        &["clone", "-q", &origin.display().to_string(), "work"],
    );
    git(&work, &["checkout", "-q", "-b", "task/T-001"]);
    git(&work, &["push", "-q", "origin", "task/T-001"]);
    let first = git(&work, &["rev-parse", "HEAD"]);
    let mut state = reviewed_pull();
    state.origin = origin.clone();
    state.pulls[0].head_sha = first;
    state.script = vec![enallagi::fixture::twin::Round {
        head: "task/T-001".to_string(),
        round: 1,
        checks: vec![
            serde_json::json!({"name": "test", "status": "COMPLETED", "conclusion": "SUCCESS"}),
        ],
        bots: vec![enallagi::fixture::twin::Finding {
            bot: "greptile".to_string(),
            path: "src/date.rs".to_string(),
            line: 4,
            text: "The offset is dropped".to_string(),
        }],
        ..Default::default()
    }];
    let (_, path) = enallagi::fixture::twin::install(&dir.path().join("tools"), BIN, &state);
    let greptile = |path: &std::path::Path| {
        let state = enallagi::fixture::twin::load(path);
        state.pulls[0]
            .comments
            .iter()
            .any(|c| c.author == "greptile-apps" && c.body.contains("<!-- greptile_summary -->"))
    };
    let out = twin_call(&path, &["pr", "view", "7", "--json", "url"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(!greptile(&path), "posted before the push");

    git(
        &work,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "push",
        ],
    );
    git(&work, &["push", "-q", "origin", "task/T-001"]);
    let out = twin_call(
        &path,
        &["pr", "view", "7", "--json", "headRefOid,statusCheckRollup"],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let state = enallagi::fixture::twin::load(&path);
    let pull = &state.pulls[0];
    assert_eq!(pull.round, 1);
    assert_eq!(pull.head_sha, git(&work, &["rev-parse", "HEAD"]));
    assert_eq!(pull.checks[0]["conclusion"], "SUCCESS");
    assert!(String::from_utf8_lossy(&out.stdout).contains(&pull.head_sha));
    assert!(greptile(&path), "{:?}", pull.comments);
    assert_eq!(pull.threads.len(), 2, "{:?}", pull.threads);
    assert!(pull.threads[1].comments[0]
        .body
        .contains("The offset is dropped"));
}

#[test]
fn twin_refuses_unknown_call() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (_, path) = enallagi::fixture::twin::install(dir.path(), BIN, &reviewed_pull());
    let out = twin_call(&path, &["repo", "fork"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no response for gh repo fork"), "{stderr}");
    assert_eq!(
        enallagi::fixture::twin::load(&path).calls,
        [["repo", "fork"]]
    );
}

#[test]
fn twin_merge_retargets_stacked() {
    use enallagi::fixture::twin::Pull;
    let dir = tempfile::tempdir().expect("tempdir");
    let work = dir.path().join("work");
    let origin = dir.path().join("origin.git");
    let r = enallagi::fixture::Repo::new();
    let git = |root: &std::path::Path, args: &[&str]| {
        enallagi::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    };
    let src = r.root.display().to_string();
    git(dir.path(), &["clone", "-q", "--bare", &src, "origin.git"]);
    let url = origin.display().to_string();
    git(dir.path(), &["clone", "-q", &url, "work"]);
    let main = git(&work, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let commit = |file: &str| {
        std::fs::write(work.join(file), file).expect("write");
        git(&work, &["add", "-A"]);
        let id = ["-c", "user.name=t", "-c", "user.email=t@t"];
        let mut args = id.to_vec();
        args.extend(["-c", "commit.gpgsign=false", "commit", "-q", "-m", file]);
        git(&work, &args);
        git(&work, &["rev-parse", "HEAD"])
    };
    git(&work, &["checkout", "-q", "-b", "task/T-001"]);
    let one = commit("one.txt");
    git(&work, &["checkout", "-q", "-b", "task/T-002"]);
    let two = commit("two.txt");
    git(&work, &["push", "-q", "origin", "task/T-001", "task/T-002"]);
    let pull = |number: u64, head: &str, base: &str, sha: &str| Pull {
        number,
        url: format!("https://github.com/owner/repo/pull/{number}"),
        head: head.to_string(),
        base: base.to_string(),
        head_sha: sha.to_string(),
        state: "OPEN".to_string(),
        ..Pull::default()
    };
    let state = enallagi::fixture::twin::State {
        repo: "owner/repo".to_string(),
        origin: origin.clone(),
        default_branch: main.clone(),
        pulls: vec![
            pull(7, "task/T-001", &main, &one),
            pull(8, "task/T-002", "task/T-001", &two),
        ],
        ..Default::default()
    };
    let (_, path) = enallagi::fixture::twin::install(&dir.path().join("tools"), BIN, &state);
    let merge = |sha: &str| {
        let args = ["pr", "merge", "7", "--merge", "--match-head-commit", sha];
        twin_call(&path, &[&args[..], &["--delete-branch"]].concat())
    };

    let stale = merge(&two);
    assert_eq!(stale.status.code(), Some(1), "{stale:?}");
    assert!(String::from_utf8_lossy(&stale.stderr).contains("Head branch was modified"));
    let out = merge(&one);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    let tip = format!("refs/heads/{main}");
    let parents = git(&origin, &["rev-list", "--parents", "-n", "1", &tip]);
    assert!(parents.ends_with(&format!(" {one}")), "{parents}");
    assert_eq!(parents.split(' ').count(), 3, "{parents}");
    let gone = enallagi::git::git(
        &origin,
        &["rev-parse", "--verify", "-q", "refs/heads/task/T-001"],
    );
    assert!(gone.is_err(), "{gone:?}");
    let state = enallagi::fixture::twin::load(&path);
    assert_eq!(state.pulls[0].state, "MERGED");
    assert_eq!(state.pulls[1].base, main);
    let again = merge(&one);
    assert!(String::from_utf8_lossy(&again.stderr).contains("already merged"));
}

// from a subdirectory each command reads the root's instance, never a fresh one under the cwd
#[test]
fn subdir_finds_root() {
    let r = enallagi::fixture::Repo::new();
    enallagi::fixture::queue(
        &r.root,
        ".enallagi",
        "## [T-001] a task\nstatus: ready-for-agent\n",
    );
    r.write(
        ".enallagi/events.jsonl",
        &(r#"{"ts":"2026-09-18T00:00:00Z","run":"r","iter":0,"seq":1,"kind":"run.start","config_sha256":"a","pipeline":null,"sandbox":"off"}"#.to_string() + "\n"),
    );
    r.write("src/lib.rs", "");
    for (args, want) in [
        (&["tasks", "block", "T-001"][..], "a task"),
        (&["events"][..], "sandbox=off"),
    ] {
        let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
            .args(args)
            .current_dir(r.root.join("src"))
            .output()
            .expect("run enallagi");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && text.contains(want),
            "{args:?}: {out:?}"
        );
    }
}
