//! `enallagi init`, then every read-only subcommand, against one fresh install: the binary an operator actually types, not a module call.

use enallagi::fixture::Repo;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(repo: &Repo, args: &[&str]) -> Out {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(args)
        .current_dir(&repo.root)
        .output()
        .expect("run enallagi");
    Out {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

fn installed() -> Repo {
    let repo = Repo::new();
    repo.write(
        "enallagi.toml",
        "[check]\ncommand = \"true\"\nfail_name = \"x\"\n",
    );
    let out = run(&repo, &["init"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(repo.root.join(".enallagi/RAILS.md").is_file());
    repo
}

#[test]
fn probe_exits_0_on_a_fresh_install() {
    let repo = installed();
    let out = run(&repo, &["probe"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout
            .lines()
            .filter(|l| l.starts_with("PROBE "))
            .count(),
        enallagi::probes::NAMES.len()
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
    let out = run(&repo, &["run", "--iterations", "1", "--dry-run"]);
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
    let out = run(&repo, &["tasks", "list"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let rows: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(rows.len(), 1, "{}", out.stdout);
    assert!(rows[0].starts_with("T-001  "), "{}", rows[0]);
    assert!(rows[0].ends_with("\u{2192} ready"), "{}", rows[0]);
}

#[test]
fn skills_list_prints_every_declared_skill() {
    let repo = installed();
    let out = run(&repo, &["skills", "list"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let ids: Vec<&str> = out
        .stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let cfg = enallagi::config::load(&repo.root).expect("config");
    assert_eq!(cfg.skill.len(), 8);
    assert_eq!(
        ids,
        cfg.skill.iter().map(|s| s.id.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn events_on_an_empty_log_prints_nothing_and_exits_0() {
    let repo = installed();
    let out = run(&repo, &["events"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "");
}

#[test]
fn version_names_the_crate_version() {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("--version")
        .output()
        .expect("run enallagi --version");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("enallagi {}", env!("CARGO_PKG_VERSION"))
    );
}
