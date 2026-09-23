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
fn run_help_mentions_iterations_and_budget_usd() {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--help"])
        .output()
        .expect("run enallagi run --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--iterations"), "{stdout}");
    assert!(stdout.contains("--budget-usd"), "{stdout}");
    assert!(
        stdout.contains("--dangerously-skip-permissions"),
        "{stdout}"
    );
    assert!(stdout.contains("--pr-per-task"), "{stdout}");
}

#[test]
fn events_prints_whether_permissions_were_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let harness_dir = dir.path().join(".enallagi");
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("events.jsonl"),
        r#"{"ts":"2026-09-18T00:00:00Z","run":"r","iter":0,"seq":1,"kind":"run.start","config_sha256":"a","pipeline":null,"permissions_skipped":true}"#.to_string() + "\n",
    )
    .unwrap();
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("events")
        .current_dir(dir.path())
        .output()
        .expect("run enallagi events");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("permissions_skipped=true"), "{stdout}");
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
    // keys out of order plus an undeclared field: both legal JSONL, and --json must echo them raw, not re-serialize
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

fn events_in(dir: &std::path::Path) -> std::process::Output {
    enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["events", "--json"])
        .current_dir(dir)
        .output()
        .expect("run enallagi events --json")
}

#[test]
fn a_legacy_directory_is_read_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let line = r#"{"ts":"2026-09-07T00:00:00Z","run":"r","iter":0,"seq":1,"kind":"halt","halt":"a","reason":"legacy"}"#;
    std::fs::create_dir_all(dir.path().join(".harness")).unwrap();
    std::fs::write(
        dir.path().join(".harness/events.jsonl"),
        format!("{line}\n"),
    )
    .unwrap();

    let out = events_in(dir.path());
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), line);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains("enallagi init --move"), "{stderr}");
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
fn skills_sync_works_under_a_custom_preset() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut toml = String::from(
        "[check]\ncommand = \"true\"\n\n[agent]\npreset = \"custom\"\ncommand = [\"./lane.sh\", \"{prompt}\", \"{turns}\"]\n\n",
    );
    for id in [
        "tdd",
        "ponytail",
        "debugging",
        "review-received",
        "verify-before-done",
        "review-requested",
        "brainstorming",
        "caveman-commit",
    ] {
        toml.push_str(&format!(
            "[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"x\"\n\n"
        ));
        std::fs::create_dir_all(root.join(format!("vendor/{id}"))).unwrap();
        std::fs::write(root.join(format!("vendor/{id}/SKILL.md")), "body\n").unwrap();
    }
    std::fs::write(root.join("enallagi.toml"), toml).unwrap();

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["skills", "sync"])
        .current_dir(root)
        .output()
        .expect("run enallagi skills sync");
    assert!(out.status.success(), "{out:?}");
    assert!(root.join(".enallagi/skills/tdd/SKILL.md").is_file());
    assert!(root.join(".enallagi/harness.lock").is_file());
}

#[test]
fn skills_check_refuses_what_sync_then_locks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // the shipped roles name every one of these, and `skills check` validates the config first
    const IDS: [&str; 8] = [
        "tdd",
        "ponytail",
        "debugging",
        "review-received",
        "verify-before-done",
        "review-requested",
        "brainstorming",
        "caveman-commit",
    ];
    let mut toml = String::from("[check]\ncommand = \"true\"\n\n");
    for id in IDS {
        toml.push_str(&format!(
            "[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"x\"\n\n"
        ));
        std::fs::create_dir_all(root.join(format!("vendor/{id}"))).unwrap();
        std::fs::write(root.join(format!("vendor/{id}/SKILL.md")), "body\n").unwrap();
    }
    std::fs::write(root.join("enallagi.toml"), toml).unwrap();

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
fn the_reference_tables_live_in_docs() {
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
        ["Pipelines", "Gates", "Probes", "Configuration", "Files"]
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
fn the_shipped_documents_describe_and_do_not_argue() {
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
    let cfg = enallagi::config::load(&root).expect("enallagi.toml");
    let mut elsewhere = vec![
        cfg.layout.context_file.clone(),
        "templates/pointer.md".to_string(),
        "templates/RAILS.md".to_string(),
        format!("{}/RAILS.md", cfg.layout.harness_dir),
    ];
    elsewhere.extend(cfg.layout.pointer_files.iter().cloned());
    for rel in &elsewhere {
        // the installed copies exist only while a dogfood round is open
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

    // `enallagi probe` reads check-unnamed 0 and litter 0
    let green = enallagi::probes::CheckOutcome {
        ran: true,
        red: false,
        output: String::new(),
    };
    let ctx = enallagi::probes::ProbeCtx {
        root: &root,
        cfg: &cfg,
        check: Some(&green),
        driver: false,
    };
    if root.join(&cfg.layout.context_file).is_file() {
        for (name, result) in enallagi::probes::run_all(&ctx, &["check-unnamed".to_string()]) {
            match result {
                enallagi::probes::ProbeResult::Count(found) if found.is_empty() => {}
                other => offences.push(format!("{name}: {other:?}")),
            }
        }
    }
    if !cfg.layout.docs.iter().any(|d| d == "test-hashes.json") {
        offences.push("enallagi.toml docs omits test-hashes.json, which litter flags".to_string());
    }
    assert!(offences.is_empty(), "{}", offences.join("\n"));
}

#[test]
fn tasks_ready_finds_the_queue_config_refused() {
    let r = enallagi::fixture::Repo::new();
    r.write(
        ".enallagi/TASKS.md",
        "# TASKS\n\n## [T-001] open\nscope: src/a.ts\nstatus: ready\n",
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
fn a_task_block_carries_model_and_effort() {
    let r = enallagi::fixture::Repo::new();
    r.write(
        ".enallagi/TASKS.md",
        "# TASKS\n\n\
         ## [T-001] picked per task\n\
         scope: src/a.ts\n\
         blockedBy: none\n\
         model: claude-haiku-4-5\n\
         effort: low\n\
         status: ready\n\
         \n\
         ## [T-002] picked per role\n\
         scope: src/b.ts\n\
         blockedBy: none\n\
         status: ready\n",
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
    assert_eq!(field("T-001", "blockedBy"), "none");
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

#[test]
fn tasks_archive_moves_a_done_block_and_names_it() {
    let r = enallagi::fixture::Repo::new();
    r.write(
        "TASKS.md",
        "# TASKS\n\n\
         ## [T-001] finished work\n\
         scope: src/schema.ts\n\
         status: done\n\
         notes: the verifier's own words\n\
         \n\
         ## [T-002] still open\n\
         scope: src/schema.ts\n\
         status: ready\n",
    );
    r.write("DECISIONS.md", "# DECISIONS\n");
    r.commit_all("seed the queue");

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "archive"])
        .current_dir(&r.root)
        .output()
        .expect("run enallagi tasks archive");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        vec!["T-001"],
        "{stdout}"
    );

    let tasks = std::fs::read_to_string(r.root.join("TASKS.md")).expect("TASKS.md");
    assert!(tasks.contains("archived: DECISIONS.md"), "{tasks}");
    assert!(!tasks.contains("notes: the verifier"), "{tasks}");
    assert!(tasks.contains("## [T-002] still open"), "{tasks}");
    let decisions = std::fs::read_to_string(r.root.join("DECISIONS.md")).expect("DECISIONS.md");
    assert!(
        decisions.contains("notes: the verifier's own words"),
        "{decisions}"
    );

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "archive"])
        .current_dir(&r.root)
        .output()
        .expect("run enallagi tasks archive twice");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "nothing to archive"
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
fn base_prints_the_state_commits_product_sha() {
    let r = enallagi::fixture::Repo::new();
    let git = |dir: &std::path::Path, args: &[&str]| enallagi::git::git(dir, args).expect("git");
    r.write(".git/info/exclude", ".enallagi/\n");
    let queued = git(&r.root, &["rev-parse", "HEAD"]);
    let state = r.root.join(".enallagi");
    r.write(
        ".enallagi/TASKS.md",
        "# TASKS\n\n## [T-001] open\nscope: src/a.ts\nstatus: ready\n",
    );
    git(&state, &["init", "-q"]);
    git(&state, &["config", "user.name", "t"]);
    git(&state, &["config", "user.email", "t@t"]);
    git(&state, &["add", "-A"]);
    git(&state, &["commit", "-qm", &format!("queue at {queued}")]);

    r.write("src/a.ts", "work\n");
    r.commit_all("T-001 work");
    let worked = git(&r.root, &["rev-parse", "HEAD"]);
    r.write(
        ".enallagi/TASKS.md",
        "# TASKS\n\n## [T-001] open\nscope: src/a.ts\nstatus: review\nnotes: `git log -S '## [T-001]'`\n\n## [T-002] by hand\nstatus: ready\n",
    );
    git(&state, &["add", "-A"]);
    git(
        &state,
        &["commit", "-qm", &format!("implement T-001 at {worked}")],
    );

    let out = base_of(&r.root, "T-001");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), queued);

    r.write(
        ".enallagi/TASKS.md",
        "# TASKS\n\n## [T-001] open\nstatus: review\n\n## [T-002] by hand\nstatus: ready\n\n## [T-003] by hand\nstatus: ready\n",
    );
    git(&state, &["commit", "-qam", "queue: T-003"]);
    let out = base_of(&r.root, "T-003");
    assert_ne!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("queue: T-003"),
        "{out:?}"
    );
}

#[test]
fn base_in_the_root_layout_matches_the_pickaxe() {
    let r = enallagi::fixture::Repo::new();
    r.write("TASKS.md", "# TASKS\n\n## [T-001] open\nstatus: ready\n");
    r.commit_all("queue: T-001");
    r.write("src/a.ts", "work\n");
    r.commit_all("T-001 work");

    let pickaxe = enallagi::git::git(
        &r.root,
        &[
            "log",
            "-1",
            "--format=%H",
            "-S",
            "## [T-001]",
            "--",
            "TASKS.md",
        ],
    )
    .expect("pickaxe");
    assert!(!pickaxe.is_empty());
    let out = base_of(&r.root, "T-001");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), pickaxe);
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
fn gate_scope_refuses_a_grown_nested_baseline() {
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
    let tasks = std::fs::read_to_string(state.join("TASKS.md")).expect("TASKS.md");
    let block = "\n## [T-900] grow\nscope: a.txt\nstatus: review\n";
    r.write(".enallagi/TASKS.md", &format!("{tasks}{block}"));
    commit_state(&format!("queue at {queued}"));

    r.write("a.txt", "work\n");
    r.commit_all("T-900 work");
    let worked = git(&r.root, &["rev-parse", "HEAD"]);
    let baseline = std::fs::read_to_string(state.join(".check-baseline")).unwrap_or_default();
    r.write(".enallagi/.check-baseline", &format!("{baseline}alpha\n"));
    let done = block.replace("status: review", "status: done");
    r.write(".enallagi/TASKS.md", &format!("{tasks}{done}"));
    commit_state(&format!("verify T-900 at {worked}"));

    let base = base_of(&r.root, "T-900");
    assert_eq!(String::from_utf8_lossy(&base.stdout).trim(), queued);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("added a line to .check-baseline"),
        "{out:?}"
    );

    // init's own state commit records the revision it installed at, so an unrecorded one is a
    // product commit the state never saw
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

// a nested install with T-900 verified done and `demo` locked at the base; `at_base` writes the
// vendored skill before the base commit, so the vendor commit rewrites it instead of adding it
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
    let tasks = std::fs::read_to_string(state.join("TASKS.md")).expect("TASKS.md");
    let block = "\n## [T-900] vendor\nscope: a.txt\nrows: none — harness\nstatus: review\n";
    r.write(".enallagi/TASKS.md", &format!("{tasks}{block}"));
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
    r.write(".enallagi/TASKS.md", &format!("{tasks}{done}"));
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

// a nested install with T-900 verified done; `product` rides the work commit and `state` the
// verify commit, the way the launcher's state commit carries a STOP written mid-run
fn verified_install(product: &[&str], state: &[&str]) -> (enallagi::fixture::Repo, String) {
    verified_install_as(product, state, "scope: a.txt\nstatus: done\n")
}

// `verdict` is the block's body as the verifier leaves it, so a test can widen its scope: line
fn verified_install_as(
    product: &[&str],
    state: &[&str],
    verdict: &str,
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
    let tasks = std::fs::read_to_string(state_dir.join("TASKS.md")).expect("TASKS.md");
    let block = "\n## [T-900] halt\nscope: a.txt\nstatus: review\n";
    r.write(".enallagi/TASKS.md", &format!("{tasks}{block}"));
    commit_state(&format!("queue at {queued}"));

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
    r.write(".enallagi/TASKS.md", &format!("{tasks}{done}"));
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
fn gate_scope_rejects_a_silent_widening() {
    let verdict = "scope: a.txt, b.txt\nstatus: done\n";
    let (r, queued) = verified_install_as(&["b.txt"], &[], verdict);
    let out = in_harness(&r.root, &["gate", "scope", "T-900", "--base", &queued]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("widened scope: with b.txt"), "{out:?}");
    assert!(stdout.contains("a `widened:` line"), "{out:?}");
    let tasks = std::fs::read_to_string(r.root.join(".enallagi/TASKS.md")).expect("TASKS.md");
    let block = &tasks[tasks.find("## [T-900]").expect("T-900")..];
    assert!(block.contains("status: ready"), "{block}");
    assert!(
        block.contains("gate: ") && block.contains("widened scope: with b.txt"),
        "{block}"
    );
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
fn tasks_ready_prefers_the_harness_dir_queue() {
    let r = enallagi::fixture::Repo::new();
    r.write(
        ".enallagi/TASKS.md",
        "# TASKS\n\n## [T-001] parent\nscope: src/a.ts\nstatus: ready\n",
    );
    r.write(
        "lane/.enallagi/TASKS.md",
        "# TASKS\n\n## [T-002] lane\nscope: src/a.ts\nstatus: ready\n",
    );
    let outside = tempfile::TempDir::new().expect("tempdir");
    std::fs::write(
        outside.path().join("TASKS.md"),
        "# TASKS\n\n## [T-003] outside\nscope: src/a.ts\nstatus: ready\n",
    )
    .expect("write");

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
fn the_immutable_hook_reads_nested_hashes() {
    let r = enallagi::fixture::Repo::new();
    r.write(".enallagi/TASKS.md", "# TASKS\n");
    r.write("lane/.enallagi/TASKS.md", "# TASKS\n");
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
fn a_legacy_variable_exits_2_naming_the_new_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "ready"])
        .env(format!("{}DIR", enallagi::config::LEGACY_ENV), dir.path())
        .current_dir(dir.path())
        .output()
        .expect("run tasks ready");
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ENALLAGI_DIR"), "{stderr}");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
}

#[test]
fn the_new_variable_admits_the_legacy_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["tasks", "ready"])
        .env(format!("{}DIR", enallagi::config::LEGACY_ENV), dir.path())
        .env("ENALLAGI_DIR", dir.path())
        .current_dir(dir.path())
        .output()
        .expect("run tasks ready");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn a_legacy_config_name_warns_about_the_rename() {
    let r = enallagi::fixture::Repo::new();
    r.write(".enallagi/TASKS.md", "# TASKS\n");
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

const DOC_UNRUNNABLE: [(&str, &str); 12] = [
    (
        "enallagi init --issue https://github.com/owner/repo/issues/12",
        "reads a GitHub issue over the network with gh",
    ),
    (
        "enallagi run --pr-per-task --iterations 1",
        "spawns the agent CLI, which no test may call",
    ),
    (
        "curl -LO https://github.com/thmsmtylr/enallagi/releases/latest/download/enallagi-aarch64-apple-darwin",
        "downloads a release asset over the network",
    ),
    (
        "chmod +x enallagi-aarch64-apple-darwin",
        "the asset the curl line downloads is not in the tree",
    ),
    (
        "sudo mv enallagi-aarch64-apple-darwin /usr/local/bin/enallagi",
        "installs system-wide as root",
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
    ("enallagi watch", "attaches a live view and does not exit on its own"),
    (
        "enallagi pr T-001 --push",
        "pushes a branch and opens a pull request",
    ),
];

// A fence that is not commands, named by its first line, never by its tag.
const DOC_DATA_FENCES: [(&str, &str); 3] = [
    (
        "## [T-001] the date parser drops a timezone",
        "a task block the reader pastes into TASKS.md",
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

// The documents whose fences run, in order, in one fixture repository each.
const RUN_DOCS: [&str; 2] = ["README.md", "docs/setup.md"];

// A fence is commands whatever its tag, so `sh`, `text` and an untagged fence all run.
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
fn a_failing_fence_of_any_tag_fails_the_readme_run() {
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

struct GhRepo {
    repo: enallagi::fixture::Repo,
    tools: tempfile::TempDir,
}

impl GhRepo {
    // the instance is committed in its own repository, so its porcelain lists only what the command wrote
    fn new(gh_body: &str) -> GhRepo {
        GhRepo::with_state(gh_body, &[])
    }

    fn with_state(gh_body: &str, files: &[(&str, &str)]) -> GhRepo {
        let repo = enallagi::fixture::Repo::new();
        repo.init_harness("");
        let state = repo.root.join(".enallagi");
        // init seeds no block; one standing task gives every appended block the next id
        let tasks = state.join("TASKS.md");
        let seeded = std::fs::read_to_string(&tasks).expect("TASKS.md");
        std::fs::write(
            &tasks,
            format!(
                "{}\n## [T-001] the first task\nscope: src/a.ts\nblockedBy: none\nstatus: ready\nrows: none \u{2014} harness\ncriteria:\n  - it happens\nnotes:\n",
                seeded.trim_end()
            ),
        )
        .expect("TASKS.md");
        for (name, text) in files {
            std::fs::write(state.join(name), text).expect("write state");
        }
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
        if !gh_body.is_empty() {
            let gh = tools.path().join("gh");
            std::fs::write(&gh, format!("#!/bin/sh\n{gh_body}")).expect("write gh");
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        GhRepo { repo, tools }
    }

    // shell builtins only: PATH holds nothing but the stub
    fn gh_printing(json: &str) -> String {
        assert!(
            !json.contains('\''),
            "the fixture cannot sit in single quotes"
        );
        format!("printf '%s\\n' \"$@\" >\"${{0%/*}}/gh.log\"\nprintf '%s' '{json}'\n")
    }

    fn fixture_gh() -> String {
        GhRepo::gh_printing(include_str!("fixtures/issues/help-wanted.json"))
    }

    // PATH is the tools directory alone, so a missing stub is a missing gh
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
        std::fs::read_to_string(self.repo.root.join(".enallagi/TASKS.md")).expect("TASKS.md")
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
fn issue_appends_one_proposed_block() {
    let f = GhRepo::new(&GhRepo::fixture_gh());
    let before = f.tasks();

    let out = f.issue(&["owner/repo#12"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    let args = std::fs::read_to_string(f.tools.path().join("gh.log")).expect("gh ran");
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            "issue",
            "view",
            "12",
            "--repo",
            "owner/repo",
            "--json",
            "number,title,body,url,labels"
        ]
    );
    assert_eq!(f.porcelain(), " M TASKS.md\n");

    let after = f.tasks();
    assert!(
        after.starts_with(&before),
        "the queue was rewritten, not appended to"
    );
    let blocks = enallagi::queue::parse(&after).expect("parse");
    let last = blocks.last().expect("a block");
    assert_eq!(last.id, "T-002");
    assert_eq!(last.title, "a file uploaded unzipped cannot be downloaded");
    let block = enallagi::queue::block_text(last);
    let want = "\
scope: src/thing.ts, src/thing.test.ts
blockedBy:
status: proposed
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
    assert!(block.contains(want), "{block}");
}

#[test]
fn issue_dry_run_prints_and_writes_nothing() {
    let f = GhRepo::new(&GhRepo::fixture_gh());
    let before = f.tasks();

    let out = f.issue(&["https://github.com/owner/repo/issues/12", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("## [T-002] a file uploaded unzipped cannot be downloaded\n"),
        "{stdout}"
    );
    assert!(stdout.contains("status: proposed"), "{stdout}");
    let args = std::fs::read_to_string(f.tools.path().join("gh.log")).expect("gh ran");
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            "issue",
            "view",
            "https://github.com/owner/repo/issues/12",
            "--json",
            "number,title,body,url,labels"
        ]
    );
    assert_eq!(f.tasks(), before);
    assert_eq!(f.porcelain(), "");
}

#[test]
fn issue_refuses_an_issue_already_queued() {
    let f = GhRepo::new(&GhRepo::fixture_gh());
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

const LANDED_STUB: &str = "# TASKS\n\n## [T-001] landed\nstatus: done\narchived: DECISIONS.md\n";

#[test]
fn issue_refuses_an_issue_already_archived() {
    let decisions = "# DECISIONS\n\n## Rejected findings\n\n## [T-001] landed\nstatus: done\n\
        notes: https://github.com/owner/repo/issues/12\n";
    let f = GhRepo::with_state(
        &GhRepo::fixture_gh(),
        &[("TASKS.md", LANDED_STUB), ("DECISIONS.md", decisions)],
    );
    for args in [&["owner/repo#12"][..], &["owner/repo#12", "--dry-run"][..]] {
        let out = f.issue(args);
        assert_ne!(out.status.code(), Some(0), "{args:?}: {out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("T-001"), "{args:?}: {stderr}");
        assert!(
            stderr.contains("https://github.com/owner/repo/issues/12"),
            "{args:?}: {stderr}"
        );
        assert_eq!(f.tasks(), LANDED_STUB, "{args:?}");
        assert_eq!(f.porcelain(), "", "{args:?}");
    }
}

#[test]
fn issue_requeues_an_expired_finding() {
    let decisions = "# DECISIONS\n\n## Rejected findings\n\n## Expired findings\n\n\
        ## [T-001] landed\nstatus: proposed\nnotes: https://github.com/owner/repo/issues/12\n";
    let f = GhRepo::with_state(
        &GhRepo::fixture_gh(),
        &[("TASKS.md", LANDED_STUB), ("DECISIONS.md", decisions)],
    );
    let out = f.issue(&["owner/repo#12"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let blocks = enallagi::queue::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.last().expect("a block").id, "T-002");
}

#[test]
fn issue_names_the_gh_command_that_failed() {
    for gh in [
        "",
        "echo 'To get started with GitHub CLI, please run:  gh auth login' >&2\nexit 4\n",
        "echo 'GraphQL: Could not resolve to an issue with the number of 12.' >&2\nexit 1\n",
        "echo 'not json'\n",
    ] {
        let f = GhRepo::new(gh);
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

const REVIEW_QUERY: &str = concat!(
    "query($owner:String!,$repo:String!,$number:Int!){",
    "repository(owner:$owner,name:$repo){pullRequest(number:$number){",
    "reviews(first:100){pageInfo{hasNextPage} nodes{body}}",
    "reviewThreads(first:100){pageInfo{hasNextPage} nodes{isResolved isOutdated ",
    "comments(first:100){pageInfo{hasNextPage} nodes{path body url}}}}",
    "}}}",
);

fn review_gh(fixture: &str) -> String {
    GhRepo::gh_printing(fixture)
}

const SUMMARY: &str = include_str!("fixtures/reviews/summary.json");
const SETTLED: &str = include_str!("fixtures/reviews/settled.json");
const GENERATED: &str = include_str!("fixtures/reviews/generated.json");
const PLAIN: &str = include_str!("fixtures/reviews/plain.json");
const SHORT: &str = include_str!("fixtures/reviews/short.json");

// everything a proposed block states before the comment's own words
fn scaffold_of(block: &str) -> Vec<&str> {
    block
        .lines()
        .skip(1)
        .take_while(|l| !l.starts_with("notes:"))
        .collect()
}

#[test]
fn a_short_read_is_kept_and_reported() {
    let f = GhRepo::new(&review_gh(SHORT));
    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("only the first 100 of each were read"),
        "{said}"
    );
    let blocks = enallagi::queue::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.len(), 2, "the comment it did read is queued");
}

#[test]
fn review_appends_one_block_per_comment() {
    let f = GhRepo::new(&review_gh(SUMMARY));
    let before = f.tasks();

    let out = f.review(&["owner/repo#13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    let args = std::fs::read_to_string(f.tools.path().join("gh.log")).expect("gh ran");
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            "api",
            "graphql",
            "-f",
            &format!("query={REVIEW_QUERY}"),
            "-f",
            "owner=owner",
            "-f",
            "repo=repo",
            "-F",
            "number=13",
        ]
    );

    let after = f.tasks();
    assert!(after.starts_with(&before), "the queue was rewritten");
    let blocks = enallagi::queue::parse(&after).expect("parse");
    assert_eq!(blocks.len(), 3, "{after}");
    let first = enallagi::queue::block_text(&blocks[1]);
    assert_eq!(
        first.trim_end(),
        "\
## [T-002] Refactor suggestion
scope: src/date.rs
blockedBy:
status: proposed
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
  > ```",
        "{first}"
    );
    assert_eq!(blocks[2].id, "T-003");
    assert_eq!(
        enallagi::queue::field(&blocks[2], "scope").as_deref(),
        Some("src/cli.rs")
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("  proposed: ## [T-002] "), "{stdout}");
    assert!(stdout.contains("  proposed: ## [T-003] "), "{stdout}");
    assert!(stdout.contains("  skipped: 1 with no path"), "{stdout}");
}

#[test]
fn review_skips_a_resolved_or_outdated_thread() {
    let f = GhRepo::new(&review_gh(SETTLED));
    let out = f.review(&["https://github.com/owner/repo/pull/13"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    let args = std::fs::read_to_string(f.tools.path().join("gh.log")).expect("gh ran");
    assert!(args.contains("number=13"), "{args}");

    let blocks = enallagi::queue::parse(&f.tasks()).expect("parse");
    assert_eq!(blocks.len(), 2, "{}", f.tasks());
    assert_eq!(
        enallagi::queue::field(&blocks[1], "scope").as_deref(),
        Some("src/cli.rs")
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("  skipped: 2 resolved or outdated"),
        "{stdout}"
    );
}

#[test]
fn review_run_twice_appends_nothing() {
    let f = GhRepo::new(&review_gh(SUMMARY));
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
fn review_dry_run_prints_and_writes_nothing() {
    let f = GhRepo::new(&review_gh(SUMMARY));
    let before = f.tasks();

    let out = f.review(&["owner/repo#13", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("## [T-002] Refactor suggestion\n"),
        "{stdout}"
    );
    assert!(stdout.contains("\n## [T-003] "), "{stdout}");
    assert!(stdout.contains("status: proposed"), "{stdout}");
    assert_eq!(f.tasks(), before);
    assert_eq!(f.porcelain(), "");
}

#[test]
fn a_generated_and_a_plain_comment_share_a_shape() {
    let mut shapes = Vec::new();
    for fixture in [GENERATED, PLAIN] {
        let f = GhRepo::new(&review_gh(fixture));
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

// a name in the source would be a filter on who reviewed; the command reads every open thread
#[test]
fn the_review_source_names_no_comment_author() {
    const SOURCE: [(&str, &str); 2] = [
        ("src/review.rs", include_str!("../src/review.rs")),
        ("src/cli/review.rs", include_str!("../src/cli/review.rs")),
    ];
    for (name, text) in SOURCE {
        let tokens: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .collect();
        for named in ["author", "login", "user", "bot", "reviewer"] {
            assert!(!tokens.contains(&named), "{name} says `{named}`");
        }
    }
}
