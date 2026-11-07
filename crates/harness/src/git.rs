//! every git the harness runs; the repository, not the spawning process, sets commit identity

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git {args}: {stderr}")]
    Failed { args: String, stderr: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn cmd(root: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    // a lane's inherited GIT_AUTHOR_*/GIT_COMMITTER_* would beat the repository's `user.email`
    for var in [
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ] {
        c.env_remove(var);
    }
    c.current_dir(root).args(args);
    c
}

pub fn git(root: &Path, args: &[&str]) -> Result<String, GitError> {
    if enters_gitlinks(args) {
        gitlinks_safe(root, args)?;
    }
    raw(root, args)
}

fn raw(root: &Path, args: &[&str]) -> Result<String, GitError> {
    let out = cmd(root, args).output()?;
    if !out.status.success() {
        return Err(GitError::Failed {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

// the subcommands that run git inside a populated gitlink, and with it that repository's config
fn enters_gitlinks(args: &[&str]) -> bool {
    let mut words = args.iter();
    while let Some(w) = words.next() {
        if *w == "-c" || *w == "-C" {
            words.next();
        } else if !w.starts_with('-') {
            return matches!(
                *w,
                "status"
                    | "add"
                    | "diff"
                    | "diff-index"
                    | "diff-files"
                    | "describe"
                    | "commit"
                    | "stash"
            );
        }
    }
    false
}

// ponytail: one index listing per such call; cache by index mtime if it shows in a profile
fn gitlinks_safe(root: &Path, args: &[&str]) -> Result<(), GitError> {
    let staged = raw(root, &["ls-files", "-s", "-z"])?;
    let links: Vec<&str> = staged
        .split('\0')
        .filter_map(|e| e.strip_prefix("160000 ")?.split_once('\t').map(|(_, p)| p))
        .filter(|p| root.join(p).join(".git").exists())
        .collect();
    if links.is_empty() {
        return Ok(());
    }
    // a real submodule's repository sits under modules/, which the sandbox denies a lane
    let common = raw(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let modules = std::fs::canonicalize(Path::new(&common).join("modules")).ok();
    for link in links {
        let dotgit = root.join(link).join(".git");
        let target = std::fs::read_to_string(&dotgit)
            .ok()
            .and_then(|t| Some(dotgit.parent()?.join(t.strip_prefix("gitdir:")?.trim())))
            .and_then(|p| std::fs::canonicalize(p).ok());
        let held = matches!((&target, &modules), (Some(t), Some(m)) if t.starts_with(m));
        if !held {
            return Err(GitError::Failed {
                args: args.join(" "),
                stderr: format!(
                    "{link} is a gitlink to a repository inside the checkout, whose config a lane \
                     can write; remove it (`git rm --cached {link}`) and run again"
                ),
            });
        }
    }
    Ok(())
}

pub fn git_ok(root: &Path, args: &[&str]) -> bool {
    git(root, args).is_ok()
}

pub fn head(root: &Path) -> Option<String> {
    git(root, &["rev-parse", "--short", "HEAD"]).ok()
}

pub fn porcelain(root: &Path) -> Vec<String> {
    git(root, &["status", "--porcelain"])
        .map(|s| s.lines().map(String::from).collect())
        .unwrap_or_default()
}

pub fn diff_names(root: &Path, base: &str) -> Vec<String> {
    git(root, &["diff", "--name-only", base, "HEAD"])
        .map(|s| {
            s.lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn commit_paths(root: &Path, paths: &[&str], msg: &str) -> Result<bool, GitError> {
    // `git add -- a b` fails whole on one missing path, so missing paths are filtered first
    for path in paths.iter().filter(|p| root.join(p).exists()) {
        git(root, &["add", "--", path])?;
    }
    if git_ok(root, &["diff", "--cached", "--quiet"]) {
        return Ok(false);
    }
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", msg],
    )?;
    Ok(true)
}

/// commits `path` alone; the rest of the index stays staged
pub fn commit_only(root: &Path, path: &str, msg: &str) -> Result<(), GitError> {
    git(root, &["add", "-A", "--", path])?;
    git_as(root, root, &["commit", "-q", "-m", msg, "--", path])?;
    Ok(())
}

pub fn ignore_line(dir: &str) -> String {
    format!("/{dir}/")
}

// None when already the main worktree; a linked worktree's files go when its lane ends
pub fn main_worktree_path(dir: &Path) -> Option<PathBuf> {
    // prefix asked first: git() trims a trailing newline but would lose an empty leading prefix
    let out = git(dir, &["rev-parse", "--show-prefix", "--show-toplevel"]).ok()?;
    let (prefix, top) = out.split_once('\n')?;
    let listed = git(dir, &["worktree", "list", "--porcelain"]).ok()?;
    // git lists the main worktree first
    let main = listed.lines().next()?.strip_prefix("worktree ")?;
    if main == top {
        return None;
    }
    let mut path = PathBuf::from(main);
    if !prefix.is_empty() {
        path.push(prefix.trim_end_matches('/'));
    }
    Some(path)
}

// the harness directory's own repository once it has one, else the product's
pub fn state_root(root: &Path, harness_dir: &str) -> PathBuf {
    let dir = root.join(harness_dir);
    if !harness_dir.is_empty() && dir.join(".git").exists() {
        dir
    } else {
        root.to_path_buf()
    }
}

// a path relative to the product root, as it is named inside the repository that holds it
pub fn locate(root: &Path, harness_dir: &str, path: &str) -> (PathBuf, String, bool) {
    let state = state_root(root, harness_dir);
    match path.strip_prefix(&format!("{harness_dir}/")) {
        Some(inner) if state != root => (state, inner.to_string(), true),
        _ => (root.to_path_buf(), path.to_string(), false),
    }
}

/// `git args` in `dir` under `root`'s identity, unsigned; the state repository has none of its own
pub fn git_as(root: &Path, dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let mut all = identity_args(root);
    all.extend(["-c", "commit.gpgsign=false"].map(String::from));
    all.extend(args.iter().map(|a| a.to_string()));
    let all: Vec<&str> = all.iter().map(String::as_str).collect();
    git(dir, &all)
}

pub fn identity_args(root: &Path) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    for key in ["user.name", "user.email"] {
        if let Ok(value) = git(root, &["config", key]) {
            args.extend(["-c".to_string(), format!("{key}={value}")]);
        }
    }
    args
}

// the subject names the product HEAD, which `enallagi base` reads back
thread_local! {
    // the loop's `<run>/<iteration>`, so expiry counts an iteration's state commits as one round
    pub static ITERATION: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn stamped(msg: &str, sha: &str) -> String {
    match ITERATION.with_borrow(Clone::clone) {
        Some(it) => format!("{msg} at {sha}\n\nIteration: {it}"),
        None => format!("{msg} at {sha}"),
    }
}

pub fn commit_instance(
    root: &Path,
    harness_dir: &str,
    names: &[&str],
    msg: &str,
) -> Result<bool, GitError> {
    let state = state_root(root, harness_dir);
    if state == root && names.is_empty() {
        return Ok(false);
    }
    let sha = git(root, &["rev-parse", "HEAD"])?;
    let msg = stamped(msg, &sha);
    if state == root {
        let paths: Vec<String> = names
            .iter()
            .map(|name| crate::config::instance_rel(harness_dir, name))
            .collect();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        return commit_paths(root, &paths, &msg);
    }
    // an old .gitignore lacks the STOP line, which would commit the operator's halt into every lane
    git(&state, &["add", "-A"])?;
    git(
        &state,
        &["rm", "--cached", "-q", "--ignore-unmatch", "--", "STOP"],
    )?;
    if git_ok(&state, &["diff", "--cached", "--quiet"]) {
        return Ok(false);
    }
    git_as(root, &state, &["commit", "-q", "-m", &msg])?;
    Ok(true)
}

// the named paths only: `enallagi pr` must not commit the operator's uncommitted queue edits
pub fn commit_instance_only(
    root: &Path,
    harness_dir: &str,
    names: &[&str],
    msg: &str,
) -> Result<bool, GitError> {
    let state = state_root(root, harness_dir);
    if state == root {
        return commit_instance(root, harness_dir, names, msg);
    }
    let sha = git(root, &["rev-parse", "HEAD"])?;
    let msg = stamped(msg, &sha);
    let mut add = vec!["add", "--"];
    add.extend(names);
    git(&state, &add)?;
    let mut diff = vec!["diff", "--cached", "--quiet", "--"];
    diff.extend(names);
    if git_ok(&state, &diff) {
        return Ok(false);
    }
    let mut args = vec!["commit", "-q", "-m", &msg, "--"];
    args.extend(names);
    git_as(root, &state, &args)?;
    Ok(true)
}

// the first state commit recorded at a product revision, never a revision the state repo lacks
pub fn state_rev(root: &Path, harness_dir: &str, rev: &str) -> Result<String, String> {
    let state = state_root(root, harness_dir);
    if state == root {
        return Ok(rev.to_string());
    }
    let sha = git(
        root,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )
    .map_err(|e| e.to_string())?;
    let found = git(
        &state,
        &[
            "log",
            "--reverse",
            "--format=%H",
            &format!("--grep= at {sha}$"),
        ],
    )
    .map_err(|e| e.to_string())?;
    found
        .lines()
        .next()
        .map(String::from)
        .ok_or_else(|| format!("no commit in {harness_dir} records product revision {rev} ({sha})"))
}

// the product sha to diff a task against, read off the subject of the commit that queued it
pub fn task_base(root: &Path, harness_dir: &str, task: &str) -> Result<String, String> {
    let specs = crate::config::instance_rel(harness_dir, crate::tracker::SPECS);
    let (repo, _, nested) = locate(root, harness_dir, &specs);
    let Some(commit) = crate::tracker::queued_commit(root, harness_dir, task)? else {
        return Ok(String::new());
    };
    if !nested {
        return Ok(commit);
    }
    let subject = git(&repo, &["log", "-1", "--format=%s", &commit]).map_err(|e| e.to_string())?;
    recorded_sha(&subject).map(String::from).ok_or_else(|| {
        format!("the state commit that added {task}, \"{subject}\", records no product sha")
    })
}

/// the product sha a state commit's subject ends with; None on a product commit
pub fn recorded_sha(subject: &str) -> Option<&str> {
    subject
        .rsplit_once(" at ")
        .map(|(_, sha)| sha)
        .filter(|sha| sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;

    #[test]
    fn nested_fsmonitor_inert() {
        let r = Repo::new();
        let raw = |dir: &Path, args: &[&str]| {
            let ok = Command::new("git")
                .current_dir(dir)
                .args(args)
                .output()
                .expect("git")
                .status
                .success();
            assert!(ok, "{args:?}");
        };
        let x = r.root.join("x");
        std::fs::create_dir_all(&x).unwrap();
        raw(&x, &["init", "-q"]);
        r.write("x/f", "hi\n");
        raw(&x, &["add", "f"]);
        raw(
            &x,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "f",
            ],
        );
        raw(&r.root, &["add", "x"]);
        let marker = r.root.join("ran");
        r.write(
            "hook.sh",
            &format!("#!/bin/sh\ntouch {}\n", marker.display()),
        );
        raw(&r.root, &["update-index", "--chmod=+x", "--add", "hook.sh"]);
        std::fs::set_permissions(
            r.root.join("hook.sh"),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let hook = r.root.join("hook.sh").display().to_string();
        raw(&x, &["config", "core.fsmonitor", &hook]);
        let err = git(&r.root, &["status", "--porcelain"]).unwrap_err();
        assert!(err.to_string().contains("x"), "{err}");
        assert!(porcelain(&r.root).is_empty());
        assert!(!marker.exists());
    }

    #[test]
    fn head_and_porcelain_read_a_fixture() {
        let r = Repo::new();
        assert!(head(&r.root).is_some());
        assert!(porcelain(&r.root).is_empty());
        r.write("src/a.ts", "x");
        assert_eq!(porcelain(&r.root), vec!["?? src/a.ts".to_string()]);
        assert!(commit_paths(&r.root, &["src/a.ts"], "add").unwrap());
        assert!(!commit_paths(&r.root, &["src/a.ts"], "again").unwrap());
        assert_eq!(diff_names(&r.root, "HEAD~1"), vec!["src/a.ts".to_string()]);
    }

    #[test]
    fn identity_args_follow_the_repo_config() {
        let r = Repo::new();
        git(&r.root, &["config", "user.name", "Lane"]).expect("config");
        git(&r.root, &["config", "user.email", "lane@example.test"]).expect("config");
        assert_eq!(
            identity_args(&r.root),
            ["-c", "user.name=Lane", "-c", "user.email=lane@example.test"]
        );
        let bare = tempfile::TempDir::new().expect("tempdir");
        git(bare.path(), &["init", "-q"]).expect("init");
        git(bare.path(), &["config", "--unset-all", "user.name"]).ok();
        git(bare.path(), &["config", "--unset-all", "user.email"]).ok();
        // a repository that sets neither key falls through to the machine's global identity, if any
        let mut global = Vec::new();
        for key in ["user.name", "user.email"] {
            if let Ok(value) = git(bare.path(), &["config", "--global", key]) {
                global.extend(["-c".to_string(), format!("{key}={value}")]);
            }
        }
        assert_eq!(identity_args(bare.path()), global);
    }

    #[test]
    fn a_failed_add_for_an_existing_path_is_an_err() {
        let r = Repo::new();
        r.write(".gitignore", "src/a.ts\n");
        assert!(commit_paths(&r.root, &[".gitignore"], "ignore").unwrap());
        r.write("src/a.ts", "x");
        assert!(commit_paths(&r.root, &["src/a.ts"], "add ignored").is_err());
    }

    // the process is shared, so the identity variables come off the child, not the process
    #[test]
    fn a_git_command_carries_no_ambient_identity() {
        let c = cmd(Path::new("."), &["status"]);
        let dropped: Vec<String> = c
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect();
        for var in [
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GIT_COMMITTER_NAME",
            "GIT_COMMITTER_EMAIL",
        ] {
            assert!(
                dropped.iter().any(|k| k == var),
                "{var} reaches git: {dropped:?}"
            );
        }
    }

    #[test]
    fn a_linked_worktree_resolves_to_the_main_one() {
        let r = Repo::new();
        let wt = r.root.join("lane");
        git(
            &r.root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "lane/x",
                &wt.to_string_lossy(),
            ],
        )
        .expect("worktree add");
        let real = |p: &Path| std::fs::canonicalize(p).expect("canonicalize");

        assert_eq!(main_worktree_path(&r.root), None);
        assert_eq!(main_worktree_path(&wt), Some(real(&r.root)));
        std::fs::create_dir_all(wt.join(".enallagi")).expect("harness dir");
        assert_eq!(
            main_worktree_path(&wt.join(".enallagi")),
            Some(real(&r.root).join(".enallagi"))
        );

        git(
            &r.root,
            &["worktree", "remove", "--force", &wt.to_string_lossy()],
        )
        .expect("remove");
    }

    #[test]
    fn unqueued_block_has_no_commit() {
        let r = Repo::new();
        r.queue(
            ".enallagi",
            "## [T-001] first\nscope: src/a.ts\nstatus: ready-for-agent\n",
        );
        r.commit_all("queue: T-001");
        assert!(crate::tracker::queued_commit(&r.root, ".enallagi", "T-001")
            .expect("the queued commit")
            .is_some());
        assert_eq!(
            crate::tracker::queued_commit(&r.root, ".enallagi", "T-002"),
            Ok(None)
        );
        assert_eq!(task_base(&r.root, ".enallagi", "T-002"), Ok(String::new()));
    }

    #[test]
    fn queued_commit_is_the_first_add() {
        let r = Repo::new();
        r.write(".git/info/exclude", ".enallagi/\n");
        let block = "## [T-001] first\nscope: src/a.ts\nstatus: ready-for-agent\n";
        r.queue(".enallagi", block);
        let state = r.root.join(".enallagi");
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
        ] {
            git(&state, args).expect("state repo");
        }
        let commit = |msg: &str| {
            git(&state, &["add", "-A"]).expect("add");
            git(
                &state,
                &["-c", "commit.gpgsign=false", "commit", "-qm", msg],
            )
            .expect("commit");
            git(&state, &["rev-parse", "HEAD"]).expect("head")
        };
        let queued = commit("queue at 0000000");
        r.queue(".enallagi", "");
        commit("archive at 1111111");
        r.queue(".enallagi", block);
        let readded = commit("queue at 2222222");

        assert_ne!(queued, readded);
        assert_eq!(
            crate::tracker::queued_commit(&r.root, ".enallagi", "T-001"),
            Ok(Some(queued))
        );
    }

    // an install whose .gitignore predates the STOP line still keeps it out of the state repository
    #[test]
    fn state_commit_skips_stop() {
        let r = Repo::new();
        r.write(".git/info/exclude", ".enallagi/\n");
        r.queue(
            ".enallagi",
            "## [T-001] first\nscope: src/a.ts\nstatus: ready-for-agent\n",
        );
        let state = r.root.join(".enallagi");
        git(&state, &["init", "-q"]).expect("state repo");
        r.write(".enallagi/STOP", "");

        assert!(commit_instance(&r.root, ".enallagi", &[], "round").expect("commit"));
        let tracked = git(&state, &["ls-files"]).expect("ls-files");
        assert!(tracked.contains("issues/T-001"), "{tracked}");
        assert!(!tracked.lines().any(|l| l == "STOP"), "{tracked}");
    }

    #[test]
    fn missing_path_commits_others() {
        let r = Repo::new();
        r.write("PROGRESS.md", "queue\n");
        assert!(commit_paths(&r.root, &["PROGRESS.md", "DECISIONS.md"], "queue").unwrap());
        assert_eq!(
            diff_names(&r.root, "HEAD~1"),
            vec!["PROGRESS.md".to_string()]
        );
    }
}
