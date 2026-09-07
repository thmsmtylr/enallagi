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
    let harness_dir = dir.path().join(".harness");
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
    let harness_dir = dir.path().join(".harness");
    std::fs::create_dir_all(&harness_dir).unwrap();
    // Keys out of `Kind::Halt`'s declared order, plus a field no variant
    // declares at all — both are legal JSONL, and `--json` must reproduce
    // this exact line rather than a re-serialization of the parsed `Event`.
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
    let harness_dir = dir.path().join(".harness");
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
