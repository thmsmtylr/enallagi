//! `enallagi eject`: a repository the harness was brought into, ran in and left looks as it did before.

use enallagi::fixture::Repo;
use std::path::{Path, PathBuf};
use std::process::Command;

const TASKS: &str = "\
## [T-001] do the thing

scope: src/thing.ts
rows: none — harness
status: ready
criteria:
  - it happens
";

const SKILLS: [&str; 8] = [
    "tdd",
    "ponytail",
    "debugging",
    "review-received",
    "verify-before-done",
    "review-requested",
    "brainstorming",
    "caveman-commit",
];

// where an eject in these tests keeps its record: never the user's own data directory
fn data_home() -> &'static Path {
    static DATA: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    DATA.get_or_init(|| tempfile::tempdir().expect("data home"))
        .path()
}

fn harness(root: &Path, args: &[&str]) -> (i32, String) {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(args)
        .current_dir(root)
        .env_remove("ENALLAGI_DIR")
        // CI sets frozen, which refuses the fixture's path: skills before anything locks them
        .env_remove("CI")
        .env("XDG_DATA_HOME", data_home())
        .output()
        .expect("spawn harness");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

fn git(root: &Path, args: &[&str]) -> String {
    enallagi::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

fn exclude(root: &Path) -> String {
    let path = git(root, &["rev-parse", "--git-path", "info/exclude"]);
    std::fs::read_to_string(root.join(path)).unwrap_or_default()
}

fn snapshot(root: &Path) -> (String, String, String) {
    (
        git(root, &["status", "--porcelain", "--ignored"]),
        git(root, &["ls-files", "-s"]),
        exclude(root),
    )
}

fn exec(path: &Path, body: &str) -> String {
    std::fs::write(path, format!("#!/usr/bin/env bash\nset -u\n{body}")).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path.display().to_string()
}

// the stubs and skills live outside the repository, so nothing but init and the run touches it
fn stub_config(tools: &Path) -> String {
    let bin = env!("CARGO_BIN_EXE_enallagi");
    let quiet = "echo '{\"total_cost_usd\":0.5}'\n";
    let check = exec(&tools.join("check.sh"), "exit 0\n");
    let agent = exec(&tools.join("agent.sh"), quiet);
    let implement = exec(
        &tools.join("implement.sh"),
        &format!(
            "echo work >src/thing.ts\n\
             {bin} tasks set-status T-001 review 'stub implemented'\n\
             echo 'iteration' >>.enallagi/PROGRESS.md\n\
             git add src/thing.ts >/dev/null 2>&1\n\
             git -c commit.gpgsign=false commit -qm 'T-001: stub' >/dev/null 2>&1\n\
             {quiet}"
        ),
    );
    let verify = exec(
        &tools.join("verify.sh"),
        &format!("{bin} tasks set-status T-001 done 'stub verified'\n{quiet}"),
    );
    let mut toml = format!(
        r#"
[agent]
preset = "custom"
command = ["{agent}", "{{prompt}}", "{{turns}}"]

[agent.usage]
cost = "total_cost_usd"

[agent.implementer]
command = ["{implement}", "{{prompt}}", "{{turns}}"]

[agent.verifier]
command = ["{verify}", "{{prompt}}", "{{turns}}"]

[check]
command = "{check}"

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
"#
    );
    for id in SKILLS {
        let dir = tools.join("vendor").join(id);
        std::fs::create_dir_all(&dir).expect("skill dir");
        std::fs::write(dir.join("SKILL.md"), format!("# {id}\n")).expect("skill");
        toml.push_str(&format!(
            "\n[[skill]]\nid = \"{id}\"\nsource = \"path:{}\"\npath = \"\"\ngate = \"none\"\nwhy = \"fixture\"\n",
            dir.display()
        ));
    }
    toml
}

fn removed(out: &str, verb: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.trim().strip_prefix(&format!("{verb}: ")))
        .map(String::from)
        .collect()
}

#[test]
fn an_ejected_repository_looks_untouched() {
    let r = Repo::new();
    let tools = tempfile::tempdir().expect("tools");
    let (status, index, excluded) = snapshot(&r.root);

    let (code, out) = harness(&r.root, &["init", "--adapter", "claude"]);
    assert_eq!(code, 0, "{out}");
    std::fs::write(
        r.root.join(".enallagi/enallagi.toml"),
        stub_config(tools.path()),
    )
    .expect("config");
    std::fs::write(r.root.join(".enallagi/TASKS.md"), TASKS).expect("queue");
    // the run refuses an install that has not seen this config, so init runs once more over it
    let (code, out) = harness(&r.root, &["init", "--adapter", "claude"]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = harness(&r.root, &["run", "--iterations", "1", "--no-tui"]);
    assert_eq!(code, 0, "{out}");
    let log = git(&r.root, &["log", "--format=%s"]);
    assert!(log.lines().any(|s| s == "T-001: stub"), "{log}\n{out}");

    let (code, dry) = harness(&r.root, &["eject", "--dry-run", "--delete"]);
    assert_eq!(code, 0, "{dry}");
    assert!(r.root.join(".enallagi").is_dir(), "{dry}");
    let (code, out) = harness(&r.root, &["eject", "--delete"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        removed(&out, "removed").contains(&".enallagi".to_string()),
        "{out}"
    );
    assert_eq!(removed(&dry, "would remove"), removed(&out, "removed"));

    assert_eq!(
        git(&r.root, &["status", "--porcelain", "--ignored"]),
        status
    );
    assert_eq!(exclude(&r.root), excluded);
    let after = git(&r.root, &["ls-files", "-s"]);
    let product: Vec<&str> = after
        .lines()
        .filter(|l| !l.ends_with("\tsrc/thing.ts"))
        .collect();
    assert_eq!(product.join("\n"), index);
    let paths = git(&r.root, &["log", "--name-only", "--format="]);
    assert!(paths.lines().any(|p| p == "src/thing.ts"), "{paths}");
    for path in paths.lines().filter(|p| !p.is_empty()) {
        assert!(
            path == "src/thing.ts" || path == "src/schema.ts",
            "{path} is in the product history"
        );
    }
}

#[test]
fn eject_removes_only_untracked_entry_points() {
    let r = Repo::new();
    let (code, out) = harness(&r.root, &["init", "--adapter", "gemini"]);
    assert_eq!(code, 0, "{out}");
    assert!(r.root.join("GEMINI.md").is_file() && r.root.join(".gemini/settings.json").is_file());
    git(&r.root, &["add", "-f", "GEMINI.md"]);
    r.commit_all("keep the pointer");

    let (code, out) = harness(&r.root, &["eject"]);
    assert_eq!(code, 0, "{out}");
    let gone = removed(&out, "removed");
    assert!(gone.contains(&".gemini/settings.json".to_string()), "{out}");
    assert!(!gone.contains(&"GEMINI.md".to_string()), "{out}");
    assert!(!r.root.join(".gemini").exists(), "{out}");
    assert!(r.root.join("GEMINI.md").is_file());
    assert!(!exclude(&r.root).contains("# >>> harness"));
    assert_eq!(git(&r.root, &["status", "--porcelain", "--ignored"]), "");
}

struct Reaper(std::process::Child);

impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn eject_refuses_a_live_lane_or_loop() {
    let r = Repo::new();
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    let lane = r.root.join(".enallagi/worktrees/lane-x");
    git(
        &r.root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "lane/x",
            &lane.to_string_lossy(),
        ],
    );
    let child = Command::new("sleep").arg("30").spawn().expect("sleep");
    let pid = child.id();
    let _reaper = Reaper(child);
    std::fs::write(r.root.join(".enallagi/loop.pid"), pid.to_string()).expect("pid");

    for args in [&["eject"][..], &["eject", "--dry-run"]] {
        let (code, out) = harness(&r.root, args);
        assert_ne!(code, 0, "{out}");
        assert!(out.contains("lane-x"), "{out}");
        assert!(out.contains(&pid.to_string()), "{out}");
        assert!(r.root.join(".enallagi/TASKS.md").is_file(), "{out}");
    }
    let _ = git(
        &r.root,
        &["worktree", "remove", "--force", &lane.to_string_lossy()],
    );
}

#[test]
fn eject_refuses_its_own_ancestor_loop() {
    let r = Repo::new();
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");

    for flag in ["", "--dry-run"] {
        let out = enallagi::fixture::command("bash")
            .args(["-c", "echo $$ > .enallagi/loop.pid; \"$0\" eject $1"])
            .arg(env!("CARGO_BIN_EXE_enallagi"))
            .arg(flag)
            .current_dir(&r.root)
            .env_remove("ENALLAGI_DIR")
            .output()
            .expect("spawn bash");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_ne!(out.status.code(), Some(0), "{text}");
        assert!(text.contains("a loop is live"), "{text}");
        assert!(r.root.join(".enallagi/TASKS.md").is_file(), "{text}");
    }
}

#[test]
fn eject_keep_record_moves_the_directory_out() {
    let r = Repo::new();
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    let away = tempfile::tempdir().expect("record");
    let record: PathBuf = away.path().join("record");

    let (code, out) = harness(
        &r.root,
        &[
            "eject",
            "--keep-record",
            &r.root.join("inside").to_string_lossy(),
        ],
    );
    assert_ne!(code, 0, "{out}");
    assert!(r.root.join(".enallagi/TASKS.md").is_file(), "{out}");

    let (code, out) = harness(
        &r.root,
        &["eject", "--keep-record", &record.to_string_lossy()],
    );
    assert_eq!(code, 0, "{out}");
    assert!(record.join("TASKS.md").is_file(), "{out}");
    assert!(record.join(".git").exists(), "{out}");
    assert!(!r.root.join(".enallagi").exists(), "{out}");
    assert_eq!(git(&r.root, &["status", "--porcelain", "--ignored"]), "");
    assert!(!exclude(&r.root).contains("# >>> harness"));
}

// the record outlives the install: the events, verdicts and progress are what says why a run
// ended where it did, and only --delete removes them
#[test]
fn eject_keeps_the_record_in_the_data_directory() {
    let r = Repo::new();
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    let events = "{\"kind\":\"run.start\"}\n{\"kind\":\"run.end\"}\n";
    r.write(".enallagi/events.jsonl", events);
    r.write(".enallagi/PROGRESS.md", "# progress\n\nT-001 landed\n");
    r.write(".enallagi/DECISIONS.md", "# decisions\n\n## [T-001] done\n");
    let data = tempfile::tempdir().expect("data");

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["eject"])
        .current_dir(&r.root)
        .env_remove("ENALLAGI_DIR")
        .env_remove("CI")
        .env("XDG_DATA_HOME", data.path())
        .output()
        .expect("spawn harness");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{text}");
    let ejected = data.path().join("enallagi").join("ejected");
    let records: Vec<PathBuf> = std::fs::read_dir(&ejected)
        .expect("ejected dir")
        .map(|e| e.expect("entry").path())
        .collect();
    assert_eq!(records.len(), 1, "{records:?}");
    let record = &records[0];
    assert!(text.contains(&record.display().to_string()), "{text}");
    for (name, body) in [
        ("events.jsonl", events),
        ("PROGRESS.md", "# progress\n\nT-001 landed\n"),
        ("DECISIONS.md", "# decisions\n\n## [T-001] done\n"),
    ] {
        assert_eq!(
            std::fs::read_to_string(record.join(name)).expect(name),
            body,
            "{name}"
        );
    }
    assert!(!r.root.join(".enallagi").exists(), "{text}");
    assert_eq!(git(&r.root, &["status", "--porcelain", "--ignored"]), "");
}

#[test]
fn eject_delete_leaves_no_record() {
    let r = Repo::new();
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    let data = tempfile::tempdir().expect("data");

    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["eject", "--delete"])
        .current_dir(&r.root)
        .env_remove("ENALLAGI_DIR")
        .env_remove("CI")
        .env("XDG_DATA_HOME", data.path())
        .output()
        .expect("spawn harness");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(!r.root.join(".enallagi").exists(), "{text}");
    assert!(!data.path().join("enallagi").exists(), "{text}");
    assert!(text.contains("removed: .enallagi"), "{text}");
}
