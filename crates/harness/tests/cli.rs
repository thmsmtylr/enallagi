use std::process::Command;

#[test]
fn help_exits_0_and_lists_subcommands() {
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
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
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["run", "--help"])
        .output()
        .expect("run harness run --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--iterations"), "{stdout}");
    assert!(stdout.contains("--budget-usd"), "{stdout}");
}

#[test]
fn run_rejects_a_bare_iteration_count() {
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["run", "7"])
        .output()
        .expect("run harness run 7");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--iterations"), "{stderr}");
}

#[test]
fn events_with_no_log_exits_0_with_no_output() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .arg("events")
        .current_dir(dir.path())
        .output()
        .expect("run harness events");
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

    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["events", "--task", "T-1", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run harness events --task T-1 --json");
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

    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["events", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run harness events --json");
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

    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["events", "--since", "2026-09-07T00:00:01Z", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run harness events --since ...");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), 1);
    assert!(stdout.contains("\"reason\":\"late\""));
}

fn events_in(dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["events", "--json"])
        .current_dir(dir)
        .output()
        .expect("run harness events --json")
}

#[test]
fn a_legacy_harness_directory_is_read_with_a_warning_naming_the_move() {
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
    assert!(stderr.contains("harness init --move"), "{stderr}");
}

#[test]
fn the_enallagi_directory_is_read_without_a_warning_when_both_exist() {
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
fn skills_sync_works_under_a_custom_preset_which_has_no_directory_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut toml = String::from(
        "[agent]\npreset = \"custom\"\ncommand = [\"./lane.sh\", \"{prompt}\", \"{turns}\"]\n\n",
    );
    for id in [
        "tdd",
        "ponytail",
        "debugging",
        "review-received",
        "verify-before-done",
        "review-requested",
        "brainstorming",
    ] {
        toml.push_str(&format!(
            "[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"x\"\n\n"
        ));
        std::fs::create_dir_all(root.join(format!("vendor/{id}"))).unwrap();
        std::fs::write(root.join(format!("vendor/{id}/SKILL.md")), "body\n").unwrap();
    }
    std::fs::write(root.join("harness.toml"), toml).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["skills", "sync"])
        .current_dir(root)
        .output()
        .expect("run harness skills sync");
    assert!(out.status.success(), "{out:?}");
    assert!(root.join(".enallagi/skills/tdd/SKILL.md").is_file());
    assert!(root.join("harness.lock").is_file());
}

#[test]
fn skills_check_refuses_what_sync_then_locks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // the shipped roles name every one of these, and `skills check` validates the config first
    const IDS: [&str; 7] = [
        "tdd",
        "ponytail",
        "debugging",
        "review-received",
        "verify-before-done",
        "review-requested",
        "brainstorming",
    ];
    let mut toml = String::new();
    for id in IDS {
        toml.push_str(&format!(
            "[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"x\"\n\n"
        ));
        std::fs::create_dir_all(root.join(format!("vendor/{id}"))).unwrap();
        std::fs::write(root.join(format!("vendor/{id}/SKILL.md")), "body\n").unwrap();
    }
    std::fs::write(root.join("harness.toml"), toml).unwrap();

    let harness = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_harness"))
            .args(args)
            .current_dir(root)
            .output()
            .expect("run harness skills")
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
    assert!(root.join(".claude/skills/tdd/SKILL.md").is_file());
    assert!(root.join("harness.lock").is_file());

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
fn the_shipped_documents_describe_and_do_not_argue() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |rel: &str| std::fs::read_to_string(root.join(rel)).expect(rel);
    let argues = regex::Regex::new(
        r"\b(because|which is why|the reason|worth|deliberately|on purpose|we |our )\b",
    )
    .expect("regex");
    let mut offences = Vec::new();

    let readme = read("README.md");
    if readme.lines().count() > 280 {
        offences.push(format!("README.md is {} lines", readme.lines().count()));
    }
    let intent = read("docs/intent.md");
    for (name, text) in [("README.md", &readme), ("docs/intent.md", &intent)] {
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

    let toml = harness::config::DEFAULT_TOML;
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
    let cfg = harness::config::load(&root).expect("harness.toml");
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

    // `harness probe` reads check-unnamed 0 and litter 0
    let green = harness::probes::CheckOutcome {
        ran: true,
        red: false,
        output: String::new(),
    };
    let ctx = harness::probes::ProbeCtx {
        root: &root,
        cfg: &cfg,
        check: Some(&green),
        driver: false,
    };
    if root.join(&cfg.layout.context_file).is_file() {
        for (name, result) in harness::probes::run_all(&ctx, &["check-unnamed".to_string()]) {
            match result {
                harness::probes::ProbeResult::Count(found) if found.is_empty() => {}
                other => offences.push(format!("{name}: {other:?}")),
            }
        }
    }
    if !cfg.layout.docs.iter().any(|d| d == "test-hashes.json") {
        offences.push("harness.toml docs omits test-hashes.json, which litter flags".to_string());
    }
    assert!(offences.is_empty(), "{}", offences.join("\n"));
}

#[test]
fn tasks_archive_moves_a_done_block_and_names_it() {
    let r = harness::fixture::Repo::new();
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

    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["tasks", "archive"])
        .current_dir(&r.root)
        .output()
        .expect("run harness tasks archive");
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

    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["tasks", "archive"])
        .current_dir(&r.root)
        .output()
        .expect("run harness tasks archive twice");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "nothing to archive"
    );
}
