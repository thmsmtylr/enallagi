//! `enallagi worktree`: the lane it creates is named on stderr before the lane's first stage runs.

use enallagi::fixture::Repo;
use std::path::Path;

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

# the stubs sit in /tmp outside the lane, which bwrap masks
[sandbox]
mode = "off"

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
    for id in enallagi::fixture::SKILL_IDS {
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

fn lane_repo(tools: &Path) -> Repo {
    let r = Repo::new();
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    std::fs::write(r.root.join(".enallagi/enallagi.toml"), stub_config(tools)).expect("config");
    // the rendered files name the configured check, so the install is rebuilt on the stub one or
    // the lane's run refuses before its first stage
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    let state = r.root.join(".enallagi");
    // the nested state repo is created by init with no identity, and a CI runner has no global one
    for (key, value) in [("user.email", "t@t"), ("user.name", "t")] {
        git(&state, &["config", key, value]);
    }
    // init commits the state itself; the second init over the stub config may leave nothing more
    git(&state, &["add", "-A"]);
    git(
        &state,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "queue",
        ],
    );
    r
}

fn remove_lanes(r: &Repo) {
    for repo in [&r.root, &r.root.join(".enallagi")] {
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

#[test]
fn the_lane_worktree_is_named_for_watch() {
    let tools = tempfile::tempdir().expect("tools");
    let r = lane_repo(tools.path());
    let state = r.root.join(".enallagi");

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

    remove_lanes(&r);
}

// the lane worktree is removed when the lane ends and the log is ignored, so a log the lane wrote
// inside it is a run nobody can read back
#[test]
fn a_lanes_events_land_in_the_parents_log() {
    let tools = tempfile::tempdir().expect("tools");
    let r = lane_repo(tools.path());
    let state = r.root.join(".enallagi");

    let (code, out) = harness(&r.root, &["worktree", "1"]);
    assert_eq!(code, 0, "{out}");

    let log = std::fs::read_to_string(state.join("events.jsonl")).expect("the parent's log");
    for kind in [r#""kind":"run.start""#, r#""kind":"run.end""#] {
        assert!(log.contains(kind), "{kind} missing from:\n{log}");
    }

    // `enallagi events` knows nothing about worktrees: it reads the log the lane wrote
    let (code, printed) = harness(&r.root, &["events"]);
    assert_eq!(code, 0, "{printed}");
    assert!(printed.contains("run.start"), "{printed}");

    let tracked = git(&state, &["ls-files"]);
    assert!(!tracked.contains("events.jsonl"), "{tracked}");

    remove_lanes(&r);
}

// the operator's halt marker is local to one checkout: tracked, it reaches every later lane
#[test]
fn stop_never_committed() {
    let tools = tempfile::tempdir().expect("tools");
    let r = lane_repo(tools.path());
    let state = r.root.join(".enallagi");
    let once = tools.path().join("stopped");
    exec(
        &tools.path().join("agent.sh"),
        &format!(
            "if [ ! -e '{once}' ]; then touch '{once}' .enallagi/STOP; d=$(dirname .enallagi/specs/*/spec.md)/issues; mkdir -p \"$d\"; printf '# T-001: edited\\n\\n**Status:** wontfix\\n**Blocked by:** None\\n' > \"$d/T-001-edited.md\"; fi\necho '{{\"total_cost_usd\":0}}'\n",
            once = once.display()
        ),
    );

    let (_code, out) = harness(&r.root, &["worktree", "1"]);
    assert!(once.exists(), "the stub agent never ran:\n{out}");
    let tracked = git(&state, &["ls-files"]);
    assert!(
        tracked.contains("issues/T-001-edited.md"),
        "the lane's state never merged:\n{tracked}\n{out}"
    );
    assert_eq!(git(&state, &["ls-files", "STOP"]), "", "{out}");
    assert!(!state.join("STOP").exists(), "{out}");

    let (_code, out) = harness(&r.root, &["worktree", "1"]);
    assert!(!out.contains("STOP file found"), "{out}");

    remove_lanes(&r);
}

#[test]
fn init_untracks_stop() {
    let tools = tempfile::tempdir().expect("tools");
    let r = lane_repo(tools.path());
    let state = r.root.join(".enallagi");
    std::fs::write(state.join("STOP"), "").expect("stop");
    git(&state, &["add", "-f", "STOP"]);
    git(
        &state,
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "stop"],
    );

    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");

    assert_eq!(git(&state, &["ls-files", "STOP"]), "", "{out}");
    assert!(out.contains("STOP"), "init did not say so:\n{out}");
    assert!(state.join("STOP").exists(), "the operator's file is kept");
    let ignore = std::fs::read_to_string(state.join(".gitignore")).expect("ignore");
    assert!(ignore.lines().any(|l| l == "STOP"), "{ignore}");
}

// one STOP in the operator's checkout halts every lane, not only a lane that writes its own
#[test]
fn parent_stop_halts_lane() {
    let tools = tempfile::tempdir().expect("tools");
    let r = lane_repo(tools.path());
    let state = r.root.join(".enallagi");
    let toml = std::fs::read_to_string(state.join("enallagi.toml")).expect("config");
    let toml = toml.replace(r#"stages = ["scout"]"#, r#"stages = ["scout", "again"]"#)
        + "\n[[stage]]\nname = \"again\"\nrole = \"scout\"\nturns = 1\n";
    std::fs::write(state.join("enallagi.toml"), toml).expect("config");
    let (code, out) = harness(&r.root, &["init"]);
    assert_eq!(code, 0, "{out}");
    git(&state, &["add", "-A"]);
    git(
        &state,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "two stages",
        ],
    );
    let calls = tools.path().join("calls");
    exec(
        &tools.path().join("agent.sh"),
        &format!(
            "echo x >> '{calls}'; touch '{stop}'\necho '{{\"total_cost_usd\":0}}'\n",
            calls = calls.display(),
            stop = state.join("STOP").display()
        ),
    );

    let (_code, out) = harness(&r.root, &["worktree", "1"]);
    let ran = std::fs::read_to_string(&calls).unwrap_or_default();
    assert_eq!(ran.lines().count(), 1, "{out}");
    assert!(out.contains("STOP file found"), "{out}");

    remove_lanes(&r);
}

// two writers, a stub implementing and verifying the pinned ticket; `wait` runs before its commit
fn writers_repo(tools: &Path, queue: &str, wait: &str) -> Repo {
    let r = lane_repo(tools);
    let state = r.root.join(".enallagi");
    let log = tools.join("log");
    exec(
        &tools.join("agent.sh"),
        &format!(
            r#"bin='{bin}'
case "$1" in
  *"Complete exactly ONE ticket: T-"*)
    id=$(printf '%s' "$1" | grep -o 'ONE ticket: T-[0-9]*' | grep -o 'T-[0-9]*')
    echo "start $id $(ls src 2>/dev/null | tr '\n' ' ')" >> '{log}'
    {wait}
    mkdir -p src; echo "$id" > "src/$id.txt"
    git add -- "src/$id.txt"; git -c commit.gpgsign=false commit -qm "$id: work"
    "$bin" tasks set-status "$id" review 'stub implemented'
    ;;
  *"Verify T-"*)
    id=$(printf '%s' "$1" | grep -o 'Verify T-[0-9]*' | grep -o 'T-[0-9]*')
    echo "verify $id $(pwd)" >> '{log}'
    "$bin" tasks set-status "$id" done 'stub verified'
    ;;
  *"Verify every ticket at review"*)
    for id in $("$bin" tasks ids-at review); do "$bin" tasks set-status "$id" done 'stub verified'; done
    ;;
esac
echo '{{"total_cost_usd":0}}'
"#,
            bin = env!("CARGO_BIN_EXE_enallagi"),
            log = log.display(),
        ),
    );
    let toml = std::fs::read_to_string(state.join("enallagi.toml")).expect("config")
        + r#"
[parallel]
writers = 2

[[pipeline]]
name = "review"
when = "queue.reviewing"
stages = ["verify"]

[[pipeline]]
name = "task"
when = "queue.takeable"
stages = ["implement", "verify"]

[[stage]]
name = "implement"
role = "implementer"
turns = 1

[[stage]]
name = "verify"
role = "verifier"
turns = 1
"#;
    std::fs::write(state.join("enallagi.toml"), toml).expect("config");
    // vendored up front; lanes vendoring different role sets conflict on harness.lock
    let (code, out) = harness(&r.root, &["init", "--sync"]);
    assert_eq!(code, 0, "{out}");
    r.queue(".enallagi", queue);
    git(&state, &["add", "-A"]);
    git(
        &state,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "writers",
        ],
    );
    r
}

fn status_of(r: &Repo, id: &str) -> Option<String> {
    let t = enallagi::tracker::Tracker::open(&r.root, ".enallagi");
    enallagi::tracker::state(&t.get(id).expect("queue")?)
}

#[test]
fn disjoint_writers_land() {
    let tools = tempfile::tempdir().expect("tools");
    let marks = tools.path().join("marks");
    std::fs::create_dir_all(&marks).expect("marks");
    // each implementer waits for its sibling's mark, so a pass means both ran at once
    let wait = format!(
        "touch '{m}/'$id; for i in $(seq 1 300); do [ $(ls '{m}' | wc -l) -ge 2 ] && break; sleep 0.1; done; [ $(ls '{m}' | wc -l) -ge 2 ] || echo \"alone $id\" >> '{log}'",
        m = marks.display(),
        log = tools.path().join("log").display()
    );
    let r = writers_repo(
        tools.path(),
        "## [T-001] one\nscope: src/T-001.txt\nstatus: ready-for-agent\n\n## [T-002] two\nscope: src/T-002.txt\nstatus: ready-for-agent\n",
        &wait,
    );

    let (code, out) = harness(&r.root, &["worktree", "1"]);

    let log = std::fs::read_to_string(tools.path().join("log")).unwrap_or_default();
    assert!(!log.contains("alone"), "{log}\n{out}");
    for id in ["T-001", "T-002"] {
        assert_eq!(status_of(&r, id).as_deref(), Some("done"), "{id}\n{out}");
        assert!(r.root.join(format!("src/{id}.txt")).exists(), "{id}\n{out}");
    }
    assert_eq!(code, 0, "{out}");
    remove_lanes(&r);
}

#[test]
fn overlapping_tickets_serialize() {
    let tools = tempfile::tempdir().expect("tools");
    let r = writers_repo(
        tools.path(),
        "## [T-001] one\nscope: src/**\nstatus: ready-for-agent\n\n## [T-002] two\nscope: src/T-002.txt\nstatus: ready-for-agent\n",
        "",
    );

    let (code, out) = harness(&r.root, &["worktree", "1"]);

    let log = std::fs::read_to_string(tools.path().join("log")).unwrap_or_default();
    let second = log
        .lines()
        .find(|l| l.starts_with("start T-002"))
        .unwrap_or_else(|| panic!("T-002 never started:\n{log}\n{out}"));
    assert!(second.contains("T-001.txt"), "{log}\n{out}");
    for id in ["T-001", "T-002"] {
        assert_eq!(status_of(&r, id).as_deref(), Some("done"), "{id}\n{out}");
    }
    assert_eq!(code, 0, "{out}");
    remove_lanes(&r);
}

#[test]
fn readers_fan_out() {
    let tools = tempfile::tempdir().expect("tools");
    let log = tools.path().join("log");
    // the writer waits for both readers, so a pass means they ran beside it
    let wait = format!(
        "for i in $(seq 1 300); do [ $(grep -c '^verify T-00[12]' '{l}') -ge 2 ] && break; sleep 0.1; done; [ $(grep -c '^verify T-00[12]' '{l}') -ge 2 ] || echo \"alone $id\" >> '{l}'",
        l = log.display()
    );
    let r = writers_repo(
        tools.path(),
        "## [T-001] one\nscope: src/one.txt\nstatus: review\n\n## [T-002] two\nscope: src/two.txt\nstatus: review\n\n## [T-003] three\nscope: src/T-003.txt\nstatus: ready-for-agent\n",
        &wait,
    );

    let (code, out) = harness(&r.root, &["worktree", "1"]);

    let log = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!log.contains("alone"), "{log}\n{out}");
    let dirs: Vec<&str> = ["T-001", "T-002"]
        .iter()
        .map(|id| {
            log.lines()
                .find_map(|l| l.strip_prefix(&format!("verify {id} ")))
                .unwrap_or_else(|| panic!("{id} never verified:\n{log}\n{out}"))
        })
        .collect();
    assert_ne!(dirs[0], dirs[1], "{log}");
    for id in ["T-001", "T-002", "T-003"] {
        assert_eq!(status_of(&r, id).as_deref(), Some("done"), "{id}\n{out}");
    }
    assert_eq!(code, 0, "{out}");
    remove_lanes(&r);
}

#[test]
fn one_writer_unclaimed() {
    let tools = tempfile::tempdir().expect("tools");
    let r = lane_repo(tools.path());

    let (code, out) = harness(&r.root, &["worktree", "1"]);

    assert_eq!(code, 0, "{out}");
    assert!(!r.root.join(".enallagi/run/claims.json").exists(), "{out}");
    remove_lanes(&r);
}
