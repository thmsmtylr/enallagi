//! `harness init`, then every read-only subcommand, against one fresh install: the binary an operator actually types, not a module call.

use harness::fixture::Repo;
use std::process::Command;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn harness(repo: &Repo, args: &[&str]) -> Out {
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(args)
        .current_dir(&repo.root)
        .output()
        .expect("run harness");
    Out {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

fn installed() -> Repo {
    let repo = Repo::new();
    let out = harness(&repo, &["init"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(repo.root.join(".harness/RAILS.md").is_file());
    repo
}

#[test]
fn probe_exits_0_on_a_fresh_install() {
    let repo = installed();
    let out = harness(&repo, &["probe"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout
            .lines()
            .filter(|l| l.starts_with("PROBE "))
            .count(),
        harness::probes::NAMES.len()
    );
    assert_eq!(
        out.stdout.lines().filter(|l| l.contains(" ERROR ")).count(),
        0,
        "{}",
        out.stdout
    );
}

#[test]
fn run_dry_run_prints_the_plan_and_exits_0() {
    let repo = installed();
    let out = harness(&repo, &["run", "1", "--dry-run"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    // discover's `when` negates task's, so exactly one of the pair is ever planned
    assert!(
        out.stdout
            .contains("=== pipeline task (queue.takeable) ==="),
        "{}",
        out.stdout
    );
    for stage in ["implement as role implementer", "verify as role verifier"] {
        assert!(out.stdout.contains(stage), "no {stage} in:\n{}", out.stdout);
    }
}

#[test]
fn tasks_list_prints_the_seeded_queue() {
    let repo = installed();
    let out = harness(&repo, &["tasks", "list"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let rows: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(rows.len(), 1, "{}", out.stdout);
    assert!(rows[0].starts_with("T-001  "), "{}", rows[0]);
    assert!(rows[0].ends_with("\u{2192} ready"), "{}", rows[0]);
}

#[test]
fn skills_list_prints_the_seven_declared_skills() {
    let repo = installed();
    let out = harness(&repo, &["skills", "list"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let ids: Vec<&str> = out
        .stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let cfg = harness::config::load(&repo.root).expect("config");
    assert_eq!(cfg.skill.len(), 7);
    assert_eq!(
        ids,
        cfg.skill.iter().map(|s| s.id.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn events_on_an_empty_log_prints_nothing_and_exits_0() {
    let repo = installed();
    let out = harness(&repo, &["events"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "");
}
