//! `harness pr`: a landed task becomes a branch off the upstream default branch holding only its own product change.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const THING: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve\n";

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    origin: PathBuf,
    tools: PathBuf,
}

fn git(root: &Path, args: &[&str]) -> String {
    harness::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

fn exec(path: &Path, body: &str) -> String {
    fs::write(path, format!("#!/usr/bin/env bash\n{body}")).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path.display().to_string()
}

fn commit(root: &Path, rel: &str, text: &str, msg: &str) -> String {
    write(root, rel, text);
    git(root, &["add", "--", rel]);
    git(root, &["-c", "commit.gpgsign=false", "commit", "-qm", msg]);
    git(root, &["rev-parse", "--short", "HEAD"])
}

fn replace(text: &str, from: &str, to: &str) -> String {
    text.replacen(from, to, 1)
}

impl Fixture {
    // a bare origin at main, a round branch, and a harness directory with its own repository excluded from the product's
    fn new(tasks: &str, check: &str) -> Fixture {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let root = dir.path().join("product");
        let origin = dir.path().join("origin.git");
        let tools = dir.path().join("tools");
        fs::create_dir_all(&root).expect("mkdir");
        fs::create_dir_all(&tools).expect("mkdir");
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.name", "Pat Operator"]);
        git(&root, &["config", "user.email", "pat@example.test"]);
        commit(&root, "src/thing.txt", THING, "init");
        git(
            dir.path(),
            &["clone", "-q", "--bare", "product", "origin.git"],
        );
        git(
            &root,
            &["remote", "add", "origin", &origin.display().to_string()],
        );
        git(&root, &["fetch", "-q", "origin"]);
        git(&root, &["checkout", "-q", "-b", "dogfood/round-1"]);

        let state = root.join(".enallagi");
        fs::create_dir_all(&state).expect("mkdir");
        git(&state, &["init", "-q"]);
        let exclude = root.join(git(&root, &["rev-parse", "--git-path", "info/exclude"]));
        fs::write(exclude, "# >>> harness\n.enallagi/\n# <<< harness\n").expect("exclude");
        let check = exec(&tools.join("check.sh"), check);
        write(
            &state,
            "harness.toml",
            &format!("[check]\ncommand = \"{check}\"\n"),
        );
        write(&state, "TASKS.md", tasks);
        git(&state, &["add", "-A"]);
        git(
            &state,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "queue",
            ],
        );
        Fixture {
            _dir: dir,
            root,
            origin,
            tools,
        }
    }

    fn harness(&self, args: &[&str]) -> (i32, String) {
        let path = format!(
            "{}:{}",
            self.tools.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = Command::new(env!("CARGO_BIN_EXE_harness"))
            .args(args)
            .current_dir(&self.root)
            .env_remove("HARNESS_DIR")
            .env("PATH", path)
            .output()
            .expect("spawn harness");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.code().unwrap_or(-1), text)
    }

    fn thing(&self) -> String {
        fs::read_to_string(self.root.join("src/thing.txt")).expect("read thing")
    }

    fn remote_branches(&self) -> String {
        git(&self.origin, &["branch", "--list"])
    }
}

const DONE_AND_REJECTED: &str = "\
# TASKS

## [T-001] the thing says two twice
scope: src/thing.txt
blockedBy: none
status: done
archived: DECISIONS.md

## [T-002] the thing says five loudly
scope: src/thing.txt
blockedBy: none
status: ready
notes: verifier: rejected, it shouts
";

const DECISIONS: &str = "\
# DECISIONS

## [T-001] the thing says two twice
scope: src/thing.txt
blockedBy: none
status: done
criteria:
  - src/thing.txt says two twice
notes: implementer: doubled two
  verifier, first pass: REJECTED, three does not mention two
  implementer: three mentions two
  verifier, second pass: VERIFIED. `grep -c two src/thing.txt` → 2
";

// the done task lands in two commits around a rejected task's commit inside the second one's context, so only a three-way apply takes it
fn landed(check: &str) -> (Fixture, Vec<String>) {
    let f = Fixture::new(DONE_AND_REJECTED, check);
    write(&f.root, ".enallagi/DECISIONS.md", DECISIONS);
    let first = commit(
        &f.root,
        "src/thing.txt",
        &replace(THING, "two\n", "two two\n"),
        "feat(thing): T-001 two says itself twice",
    );
    let rejected = commit(
        &f.root,
        "src/thing.txt",
        &replace(&f.thing(), "five\n", "FIVE\n"),
        "feat(thing): T-002 five shouts",
    );
    let second = commit(
        &f.root,
        "src/thing.txt",
        &replace(&f.thing(), "three\n", "three, after two two\n"),
        "fix(thing): T-001 answer the rejection",
    );
    (f, vec![first, rejected, second])
}

#[test]
fn pr_refuses_a_task_that_is_not_done() {
    let (f, _) = landed("exit 0\n");
    let (code, out) = f.harness(&["pr", "T-002"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("T-002") && out.contains("not done"), "{out}");
    assert!(!out.contains("task/T-002\n"), "{out}");
    assert!(git(&f.root, &["branch", "--list", "task/*"]).is_empty());
}

#[test]
fn the_task_branch_holds_the_done_tasks_change_and_not_the_rejected_one() {
    let (f, shas) = landed("grep -q 'two two' src/thing.txt\n");
    let (code, out) = f.harness(&["pr", "T-001"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("task/T-001"), "{out}");

    let thing = git(&f.root, &["show", "task/T-001:src/thing.txt"]);
    assert!(
        thing.contains("two two\n") && thing.contains("three, after two two\n"),
        "{thing}"
    );
    assert!(
        thing.contains("five\n") && !thing.contains("FIVE"),
        "{thing}"
    );
    assert_eq!(
        git(&f.root, &["rev-list", "--count", "origin/main..task/T-001"]),
        "1"
    );
    assert_eq!(
        git(&f.root, &["log", "-1", "--format=%an <%ae>", "task/T-001"]),
        "Pat Operator <pat@example.test>"
    );
    assert_eq!(
        git(
            &f.root,
            &["log", "-1", "--format=%(trailers)", "task/T-001"]
        ),
        ""
    );
    let subject = git(&f.root, &["log", "-1", "--format=%s", "task/T-001"]);
    assert!(subject.contains("the thing says two twice"), "{subject}");
    assert!(git(&f.root, &["worktree", "list"]).lines().count() == 1);
    assert!(f.remote_branches().lines().all(|b| !b.contains("task/")));
    assert_eq!(
        git(&f.root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "dogfood/round-1"
    );

    let description = f.root.join(".enallagi/pr/T-001.md");
    assert!(
        out.contains(&description.display().to_string()) || out.contains(".enallagi/pr/T-001.md"),
        "{out}"
    );
    let text = fs::read_to_string(&description).expect("description");
    assert!(text.contains("the thing says two twice"), "{text}");
    assert!(text.contains("src/thing.txt says two twice"), "{text}");
    assert!(text.contains(&shas[0]) && text.contains(&shas[2]), "{text}");
    assert!(!text.contains(&shas[1]), "{text}");
    let verification = text.split("### Verification").nth(1).expect("verification");
    assert!(
        verification.contains("`grep -c two src/thing.txt` → 2"),
        "{text}"
    );
    assert!(!verification.contains("first pass"), "{text}");
}

#[test]
fn pr_refuses_while_a_blockers_change_is_not_on_the_upstream_default_branch() {
    let (f, _) = landed("exit 0\n");
    let tasks = format!(
        "{DONE_AND_REJECTED}\n## [T-003] the thing says twelve at the end\nscope: src/other.txt\nblockedBy: T-001\nstatus: done\n"
    );
    write(&f.root, ".enallagi/TASKS.md", &tasks);
    commit(
        &f.root,
        "src/other.txt",
        "twelve\n",
        "feat(other): T-003 twelve",
    );

    let (code, out) = f.harness(&["pr", "T-003"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("T-001"), "{out}");
    assert!(git(&f.root, &["branch", "--list", "task/*"]).is_empty());

    let (code, out) = f.harness(&["pr", "T-001", "T-003"]);
    assert_eq!(code, 0, "{out}");
    let branch = "task/T-001-T-003";
    assert!(out.contains(branch), "{out}");
    assert_eq!(
        git(&f.root, &["show", &format!("{branch}:src/other.txt")]),
        "twelve"
    );
    assert!(git(&f.root, &["show", &format!("{branch}:src/thing.txt")]).contains("two two"));
    assert_eq!(
        git(
            &f.root,
            &["rev-list", "--count", &format!("origin/main..{branch}")]
        ),
        "1"
    );

    // the blocker lands upstream under another sha, named in its message
    git(&f.root, &["checkout", "-q", "main"]);
    commit(
        &f.root,
        "src/thing.txt",
        &replace(THING, "two\n", "two two\n"),
        "feat(thing): the thing says two twice\n\nT-001",
    );
    git(&f.root, &["push", "-q", "origin", "main"]);
    git(&f.root, &["checkout", "-q", "dogfood/round-1"]);
    let (code, out) = f.harness(&["pr", "T-003"]);
    assert_eq!(code, 0, "{out}");
    assert!(git(&f.root, &["show", "task/T-003:src/other.txt"]).contains("twelve"));
}

#[test]
fn a_conflicting_diff_exits_non_zero_naming_the_files_and_leaves_no_branch() {
    let (f, _) = landed("exit 0\n");
    git(&f.root, &["checkout", "-q", "main"]);
    commit(
        &f.root,
        "src/thing.txt",
        &replace(THING, "two\n", "TWO\n"),
        "upstream moved two",
    );
    git(&f.root, &["push", "-q", "origin", "main"]);
    git(&f.root, &["checkout", "-q", "dogfood/round-1"]);

    let (code, out) = f.harness(&["pr", "T-001", "--push"]);
    assert_ne!(code, 0, "{out}");
    assert!(
        out.contains("conflict") && out.contains("src/thing.txt"),
        "{out}"
    );
    assert!(git(&f.root, &["branch", "--list", "task/*"]).is_empty());
    assert!(f.remote_branches().lines().all(|b| !b.contains("task/")));
    assert!(git(&f.root, &["worktree", "list"]).lines().count() == 1);
}

#[test]
fn a_red_check_in_the_worktree_exits_non_zero_with_its_output_and_pushes_nothing() {
    let (f, _) = landed("echo 'the check saw FIVE missing'\nexit 3\n");
    let (code, out) = f.harness(&["pr", "T-001", "--push"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("the check saw FIVE missing"), "{out}");
    assert!(f.remote_branches().lines().all(|b| !b.contains("task/")));
    assert!(git(&f.root, &["branch", "--list", "task/*"]).is_empty());
}

#[test]
fn push_pushes_the_branch_and_opens_the_pull_request_with_gh() {
    let (f, _) = landed("exit 0\n");
    let log = f.tools.join("gh.log");
    exec(
        &f.tools.join("gh"),
        &format!("printf '%s\\n' \"$@\" >{}\n", log.display()),
    );

    let (code, out) = f.harness(&["pr", "T-001", "--push"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        f.remote_branches().contains("task/T-001"),
        "{}",
        f.remote_branches()
    );
    let args = fs::read_to_string(&log).expect("gh ran");
    let args: Vec<&str> = args.lines().collect();
    assert_eq!(&args[..2], ["pr", "create"], "{args:?}");
    for pair in [["--base", "main"], ["--head", "task/T-001"]] {
        assert!(args.windows(2).any(|w| w == pair), "{args:?}");
    }
    let body = args
        .windows(2)
        .find(|w| w[0] == "--body-file")
        .expect("--body-file")[1];
    assert!(fs::read_to_string(body)
        .expect("body")
        .contains("the thing says two twice"));
}

#[test]
fn pr_refuses_a_done_task_no_round_commit_names() {
    let (f, _) = landed("exit 0\n");
    let tasks = format!("{DONE_AND_REJECTED}\n## [T-004] only the queue moved\nscope: TASKS.md\nblockedBy: none\nstatus: done\n");
    write(&f.root, ".enallagi/TASKS.md", &tasks);
    let (code, out) = f.harness(&["pr", "T-004"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("T-004") && out.contains("no commit"), "{out}");
    assert!(git(&f.root, &["branch", "--list", "task/*"]).is_empty());
}
