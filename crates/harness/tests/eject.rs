//! `harness eject`: a repository the harness was brought into, ran in and left looks as it did before.

use harness::fixture::Repo;
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

const SKILLS: [&str; 7] = [
    "tdd",
    "ponytail",
    "debugging",
    "review-received",
    "verify-before-done",
    "review-requested",
    "brainstorming",
];

fn harness(root: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(args)
        .current_dir(root)
        .env_remove("HARNESS_DIR")
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
    harness::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
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
    let bin = env!("CARGO_BIN_EXE_harness");
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
fn a_repository_the_harness_ran_in_and_was_ejected_from_looks_as_it_did_before() {
    let r = Repo::new();
    let tools = tempfile::tempdir().expect("tools");
    let (status, index, excluded) = snapshot(&r.root);

    let (code, out) = harness(&r.root, &["init", "--adapter", "claude"]);
    assert_eq!(code, 0, "{out}");
    std::fs::write(
        r.root.join(".enallagi/harness.toml"),
        stub_config(tools.path()),
    )
    .expect("config");
    std::fs::write(r.root.join(".enallagi/TASKS.md"), TASKS).expect("queue");
    let (code, out) = harness(&r.root, &["run", "--iterations", "1", "--no-tui"]);
    assert_eq!(code, 0, "{out}");
    let log = git(&r.root, &["log", "--format=%s"]);
    assert!(log.lines().any(|s| s == "T-001: stub"), "{log}\n{out}");

    let (code, dry) = harness(&r.root, &["eject", "--dry-run"]);
    assert_eq!(code, 0, "{dry}");
    assert!(r.root.join(".enallagi").is_dir(), "{dry}");
    let (code, out) = harness(&r.root, &["eject"]);
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
fn eject_removes_an_untracked_entry_point_and_keeps_one_the_product_now_tracks() {
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
fn eject_refuses_while_a_lane_worktree_exists_or_a_loop_is_live_naming_each() {
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
fn eject_refuses_when_the_live_loop_is_an_ancestor_of_the_eject_process() {
    let r = Repo::new();
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");

    for flag in ["", "--dry-run"] {
        let out = Command::new("bash")
            .args(["-c", "echo $$ > .enallagi/loop.pid; \"$0\" eject $1"])
            .arg(env!("CARGO_BIN_EXE_harness"))
            .arg(flag)
            .current_dir(&r.root)
            .env_remove("HARNESS_DIR")
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
fn eject_keep_record_moves_the_harness_directory_outside_the_repository() {
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
