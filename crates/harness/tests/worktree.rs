//! `enallagi worktree`: the lane it creates is named on stderr before the lane's first stage runs.

use enallagi::fixture::Repo;
use std::path::Path;

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

fn git(root: &Path, args: &[&str]) -> String {
    enallagi::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
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

// the stub agent and the stub skills live outside the repository, so the lane's run makes no network call
fn stub_config(tools: &Path) -> String {
    let agent = exec(&tools.join("agent.sh"), "echo '{\"total_cost_usd\":0}'\n");
    let check = exec(&tools.join("check.sh"), "exit 0\n");
    let mut toml = format!(
        r#"
[agent]
preset = "custom"
command = ["{agent}", "{{prompt}}", "{{turns}}"]

[agent.usage]
cost = "total_cost_usd"

[check]
command = "{check}"

[[pipeline]]
name = "discover"
when = "!queue.takeable"
stages = ["scout"]
end_after_dry_rounds = 1

[[stage]]
name = "scout"
role = "scout"
turns = 1
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

fn harness(root: &Path, args: &[&str]) -> (i32, String) {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(args)
        .current_dir(root)
        .env_remove("ENALLAGI_DIR")
        .env_remove("CI")
        .output()
        .expect("spawn enallagi");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn the_lane_worktree_is_named_for_watch() {
    let r = Repo::new();
    let tools = tempfile::tempdir().expect("tools");
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    std::fs::write(
        r.root.join(".enallagi/enallagi.toml"),
        stub_config(tools.path()),
    )
    .expect("config");
    std::fs::write(r.root.join(".enallagi/TASKS.md"), "# TASKS\n").expect("queue");
    let state = r.root.join(".enallagi");
    // the nested state repo is created by init with no identity, and a CI runner has no global one
    for (key, value) in [("user.email", "t@t"), ("user.name", "t")] {
        git(&state, &["config", key, value]);
    }
    git(&state, &["add", "-A"]);
    git(
        &state,
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "queue"],
    );

    let (_code, out) = harness(&r.root, &["worktree", "1"]);

    let line = out
        .lines()
        .find(|l| l.starts_with("worktree: cd "))
        .unwrap_or_else(|| panic!("no watch line in:\n{out}"));
    let dir = Path::new(
        line.trim_start_matches("worktree: cd ")
            .trim_end_matches(" && enallagi watch"),
    );
    let worktrees = state.join("worktrees").canonicalize().expect("worktrees");
    assert!(dir.starts_with(&worktrees), "{line}\n{out}");
    assert!(
        dir.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("lane-")),
        "{line}\n{out}"
    );
    assert!(line.ends_with(" && enallagi watch"), "{line}");

    for repo in [&r.root, &state] {
        if let Ok(listed) = enallagi::git::git(repo, &["worktree", "list"]) {
            for wt in listed.lines().filter_map(|l| l.split_whitespace().next()) {
                if wt.contains("/worktrees/lane-") {
                    let _ = enallagi::git::git(repo, &["worktree", "remove", "--force", wt]);
                }
            }
        }
        let _ = enallagi::git::git(repo, &["branch", "-D", "--", "lane"]);
    }
}
