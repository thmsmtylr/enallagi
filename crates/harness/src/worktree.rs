//! One lane runs in its own git worktree, fast-forwarded back onto the parent branch when its history allows it.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::git;

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("worktree: the parent checkout is detached; check out a branch first")]
    Detached,
    #[error("worktree: could not create {0}")]
    Create(PathBuf),
    #[error(
        "worktree: {repo} has uncommitted work; commit it before a lane branches from it: {files}"
    )]
    Dirty { repo: PathBuf, files: String },
    #[error("worktree: {repo} fast-forwarded to {branch}, a later merge was refused, and `git reset --keep {pre}` failed: {stderr}. Both lane worktrees are left")]
    Rollback {
        repo: PathBuf,
        branch: String,
        pre: String,
        stderr: String,
    },
    #[error(transparent)]
    Git(#[from] git::GitError),
}

pub struct LaneReport {
    pub merged: bool,
    pub left: Option<PathBuf>,
    // the lane's worktree of the harness directory's own repository, inside `left` at the harness directory's path
    pub state: Option<PathBuf>,
    pub reason: String,
    pub branch: String,
    // recorded but never used to decide whether the lane merges: only the worktree's committed state does that
    pub run_error: Option<String>,
}

// (parent repository, lane worktree) pairs, the state repository first: it sits inside the product worktree
type Pairs = Vec<(PathBuf, PathBuf)>;

fn create_worktree(
    root: &Path,
    cfg: &Config,
    base: &str,
) -> Result<(String, PathBuf, Pairs), WorktreeError> {
    let harness_dir = &cfg.layout.harness_dir;
    let state_root = git::state_root(root, harness_dir);
    let mut last_dir = PathBuf::new();
    for n in 0..=10u32 {
        let name = if n == 0 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        let branch = format!("lane/{name}");
        let dir = root
            .join(harness_dir)
            .join("worktrees")
            .join(format!("lane-{name}"));
        let mut pairs = vec![(root.to_path_buf(), dir.clone())];
        if state_root != root {
            pairs.insert(0, (state_root.clone(), dir.join(harness_dir)));
        }
        let mut made: Vec<&(PathBuf, PathBuf)> = Vec::new();
        for pair in pairs.iter().rev() {
            let (repo, wt) = pair;
            if !git::git_ok(
                repo,
                &["worktree", "add", "-b", &branch, &wt.to_string_lossy()],
            ) {
                break;
            }
            made.push(pair);
        }
        if made.len() == pairs.len() {
            return Ok((branch, dir, pairs));
        }
        for (repo, wt) in made.into_iter().rev() {
            let _ = git::git(
                repo,
                &["worktree", "remove", "--force", &wt.to_string_lossy()],
            );
            let _ = git::git(repo, &["branch", "-D", &branch]);
        }
        last_dir = dir;
    }
    Err(WorktreeError::Create(last_dir))
}

// what happens next is decided by the worktrees' own committed state, never run's exit status
pub fn lane(
    root: &Path,
    cfg: &Config,
    run: &mut dyn FnMut(&Path) -> anyhow::Result<()>,
) -> Result<LaneReport, WorktreeError> {
    let parent_branch = git::git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if parent_branch == "HEAD" {
        return Err(WorktreeError::Detached);
    }

    // a lane branches from the parent's HEAD, so work uncommitted there is work it never sees
    let state_root = git::state_root(root, &cfg.layout.harness_dir);
    let mut parents = vec![root.to_path_buf()];
    if state_root != root {
        parents.insert(0, state_root);
    }
    for repo in parents {
        let files = git::porcelain(&repo);
        if !files.is_empty() {
            return Err(WorktreeError::Dirty {
                repo,
                files: files.join(", "),
            });
        }
    }

    let ts = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    let base = format!("{ts}-{}", std::process::id());
    let (branch, dir, pairs) = create_worktree(root, cfg, &base)?;
    let state = (pairs.len() > 1).then(|| pairs[0].1.clone());

    let run_error = run(&dir).err().map(|e| e.to_string());
    let left = |reason: String| LaneReport {
        merged: false,
        left: Some(dir.clone()),
        state: state.clone(),
        reason,
        branch: branch.clone(),
        run_error: run_error.clone(),
    };

    // Uncommitted work in the lane is the lane's to finish, not ours to throw away.
    if let Some((_, wt)) = pairs.iter().find(|(_, wt)| !git::porcelain(wt).is_empty()) {
        return Ok(left(format!(
            "{branch} has uncommitted work in {}",
            wt.display()
        )));
    }

    // checked for every repository before any merge, so one that cannot fast-forward merges neither
    let stuck: Vec<String> = pairs
        .iter()
        .filter(|(repo, _)| !git::git_ok(repo, &["merge-base", "--is-ancestor", "HEAD", &branch]))
        .map(|(repo, _)| format!("{branch} cannot fast-forward into {}", repo.display()))
        .collect();
    if !stuck.is_empty() {
        return Ok(left(stuck.join("\n")));
    }

    let mut said = Vec::new();
    let mut merged: Vec<(&PathBuf, String)> = Vec::new();
    for (repo, _) in &pairs {
        let pre = git::git(repo, &["rev-parse", "HEAD"])?;
        match git::git(repo, &["merge", "--ff-only", &branch]) {
            Ok(out) => {
                said.push(out);
                merged.push((repo, pre));
            }
            Err(err) => {
                // a merge git refuses after the ancestry check (a dirty parent tree) undoes the ones before it
                for (done, pre) in merged {
                    if let Err(e) = git::git(done, &["reset", "--keep", &pre]) {
                        return Err(WorktreeError::Rollback {
                            repo: done.clone(),
                            branch,
                            pre,
                            stderr: e.to_string(),
                        });
                    }
                }
                match err {
                    git::GitError::Failed { stderr, .. } => return Ok(left(stderr)),
                    e => return Err(e.into()),
                }
            }
        }
    }
    for (repo, wt) in &pairs {
        let _ = git::git(repo, &["worktree", "remove", &wt.to_string_lossy()]);
        let _ = git::git(repo, &["branch", "-d", &branch]);
    }
    Ok(LaneReport {
        merged: true,
        left: None,
        state,
        reason: said.join("\n"),
        branch,
        run_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;

    fn repo_with_harness_dir() -> Repo {
        let r = Repo::new();
        r.write(".gitignore", ".enallagi/worktrees/\n");
        r.write("enallagi.toml", "");
        r.commit_all("gitignore worktrees");
        r
    }

    fn cfg(r: &Repo) -> Config {
        crate::config::load(&r.root).expect("load config")
    }

    fn commit_all_in(dir: &Path, msg: &str) {
        git::git(dir, &["add", "-A"]).expect("add");
        git::git(
            dir,
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", msg],
        )
        .expect("commit");
    }

    fn commit_in(dir: &Path, msg: &str) {
        git::git(dir, &["add", "-A"]).expect("add");
        git::git(
            dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                msg,
            ],
        )
        .expect("commit");
    }

    fn nested_repo() -> Repo {
        let r = Repo::new();
        crate::init::install(&r.root, &crate::init::InitOpts::default()).expect("install");
        r.write(
            ".enallagi/TASKS.md",
            "# TASKS\n\n## [T-001] one\nscope: one.txt\nstatus: ready\n\n## [T-002] two\nscope: two.txt\nstatus: ready\n",
        );
        commit_in(&r.root, "install");
        commit_in(&r.root.join(".enallagi"), "queue");
        r
    }

    fn land(wt: &Path, id: &str, file: &str) {
        std::fs::write(wt.join(file), id).expect("write");
        commit_in(wt, &format!("{id} work"));
        let state = wt.join(".enallagi");
        let tasks = std::fs::read_to_string(state.join("TASKS.md")).expect("the lane's TASKS.md");
        let at = tasks.find(&format!("## [{id}]")).expect("the block");
        let verdict = tasks[at..].replacen("status: ready", "status: done", 1);
        std::fs::write(state.join("TASKS.md"), format!("{}{verdict}", &tasks[..at]))
            .expect("write");
        commit_in(&state, &format!("verify {id}"));
        std::fs::write(state.join("events.jsonl"), "{}\n").expect("an ignored run log");
    }

    fn remove_left(r: &Repo, report: &LaneReport) {
        let state = r.root.join(".enallagi");
        if let Some(left) = &report.left {
            let _ = git::git(
                &state,
                &[
                    "worktree",
                    "remove",
                    "--force",
                    &left.join(".enallagi").to_string_lossy(),
                ],
            );
            let _ = git::git(
                &r.root,
                &["worktree", "remove", "--force", &left.to_string_lossy()],
            );
        }
        let _ = git::git(&state, &["branch", "-D", &report.branch]);
        let _ = git::git(&r.root, &["branch", "-D", &report.branch]);
    }

    #[test]
    fn two_lanes_merge_and_both_verdicts_land() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");

        for (id, file) in [("T-001", "one.txt"), ("T-002", "two.txt")] {
            let report = lane(&r.root, &cfg, &mut |wt| {
                land(wt, id, file);
                Ok(())
            })
            .expect("lane");
            assert!(report.merged, "{id}: {}", report.reason);
        }

        let tasks = std::fs::read_to_string(state.join("TASKS.md")).expect("TASKS.md");
        assert!(
            tasks.contains("## [T-001] one\nscope: one.txt\nstatus: done"),
            "{tasks}"
        );
        assert!(
            tasks.contains("## [T-002] two\nscope: two.txt\nstatus: done"),
            "{tasks}"
        );
        assert!(r.root.join("one.txt").exists() && r.root.join("two.txt").exists());
        for repo in [&r.root, &state] {
            let branches = git::git(repo, &["branch", "--list", "lane/*"]).expect("branch list");
            assert!(branches.trim().is_empty(), "{}: {branches}", repo.display());
        }
    }

    #[test]
    fn a_stuck_state_branch_merges_neither() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let pre_head = git::head(&r.root);

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            std::fs::write(state.join("moved.txt"), "y").expect("write");
            commit_in(&state, "a second writer moved the state");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        assert!(report.reason.contains(&report.branch), "{}", report.reason);
        assert!(
            report.reason.contains(&state.display().to_string()),
            "{}",
            report.reason
        );
        assert_eq!(git::head(&r.root), pre_head);
        assert!(!r.root.join("one.txt").exists());
        let left = report.left.clone().expect("left in place");
        assert!(left.join("one.txt").exists());
        let lane_tasks =
            std::fs::read_to_string(left.join(".enallagi/TASKS.md")).expect("TASKS.md");
        assert!(lane_tasks.contains("status: done"));
        remove_left(&r, &report);
    }

    #[test]
    fn a_stuck_product_branch_merges_neither() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let pre_state = git::head(&state);

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            r.write("moved.txt", "y");
            commit_in(&r.root, "a second writer moved the parent");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        assert_eq!(git::head(&state), pre_state);
        let tasks = std::fs::read_to_string(state.join("TASKS.md")).expect("TASKS.md");
        assert!(!tasks.contains("status: done"), "{tasks}");
        remove_left(&r, &report);
    }

    #[test]
    fn a_dirty_product_parent_refuses_the_lane() {
        let r = nested_repo();
        let cfg = cfg(&r);
        r.write("scratch.txt", "an operator's uncommitted work");

        let mut ran = false;
        let err = lane(&r.root, &cfg, &mut |_wt| {
            ran = true;
            Ok(())
        })
        .err()
        .expect("refused");

        assert!(!ran, "the lane ran against a dirty parent");
        let msg = err.to_string();
        assert!(msg.contains("scratch.txt"), "{msg}");
        assert!(msg.contains(&r.root.display().to_string()), "{msg}");
        let listed = git::git(&r.root, &["worktree", "list"]).expect("worktree list");
        assert!(!listed.contains("/worktrees/lane-"), "{listed}");
    }

    #[test]
    fn a_dirty_state_parent_refuses_the_lane() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        std::fs::write(
            state.join("TASKS.md"),
            "# TASKS\n\nan uncommitted queue edit\n",
        )
        .expect("write");

        let mut ran = false;
        let err = lane(&r.root, &cfg, &mut |_wt| {
            ran = true;
            Ok(())
        })
        .err()
        .expect("refused");

        assert!(!ran, "the lane ran against a dirty state repository");
        let msg = err.to_string();
        assert!(msg.contains("TASKS.md"), "{msg}");
        assert!(msg.contains(&state.display().to_string()), "{msg}");
        let listed = git::git(&state, &["worktree", "list"]).expect("worktree list");
        assert!(!listed.contains("/worktrees/lane-"), "{listed}");
    }

    #[test]
    fn a_locked_parent_index_leaves_both_worktrees() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let lock = r.root.join(".git/index.lock");
        let (pre_root, pre_state) = (git::head(&r.root), git::head(&state));

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            // another process holding the parent index is what the merge hits
            std::fs::write(&lock, "").expect("write the lock");
            Ok(())
        })
        .expect("lane");
        std::fs::remove_file(&lock).expect("release the lock");

        assert!(!report.merged, "reason: {}", report.reason);
        assert!(report.reason.contains("index.lock"), "{}", report.reason);
        assert_eq!(git::head(&r.root), pre_root);
        assert_eq!(git::head(&state), pre_state, "reason: {}", report.reason);
        let left = report.left.clone().expect("left in place");
        assert!(left.exists(), "{}", left.display());
        let left_state = report.state.clone().expect("the state worktree");
        assert!(left_state.exists(), "{}", left_state.display());
        remove_left(&r, &report);
    }

    #[test]
    fn a_refused_product_merge_rolls_back() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let (pre_root, pre_state) = (git::head(&r.root), git::head(&state));

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            r.write("one.txt", "an operator's own untracked edit");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        assert_eq!(git::head(&r.root), pre_root);
        assert_eq!(git::head(&state), pre_state, "reason: {}", report.reason);
        let tasks = std::fs::read_to_string(state.join("TASKS.md")).expect("TASKS.md");
        assert!(!tasks.contains("status: done"), "{tasks}");
        remove_left(&r, &report);
    }

    #[test]
    fn a_lane_leaves_the_parent_untouched() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);
        let pre_head = git::head(&r.root);

        let mut observed_head_inside: Option<String> = None;
        let mut observed_porcelain_inside: Vec<String> = Vec::new();
        let report = lane(&r.root, &cfg, &mut |wt| {
            observed_head_inside = git::head(&r.root);
            observed_porcelain_inside = git::porcelain(&r.root);
            let _ = wt;
            Ok(())
        })
        .expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert_eq!(observed_head_inside, pre_head);
        assert!(observed_porcelain_inside.is_empty());
    }

    #[test]
    fn a_lanes_worktree_lands_under_the_harness_directory() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);

        let mut seen_dir: Option<PathBuf> = None;
        let mut seen_in_worktree_list = false;
        lane(&r.root, &cfg, &mut |wt| {
            seen_dir = Some(wt.to_path_buf());
            let listed = git::git(&r.root, &["worktree", "list"]).unwrap_or_default();
            seen_in_worktree_list = listed.contains(&wt.to_string_lossy().into_owned());
            Ok(())
        })
        .expect("lane");

        let dir = seen_dir.expect("closure ran");
        assert!(dir.starts_with(r.root.join(&cfg.layout.harness_dir).join("worktrees")));
        assert!(
            seen_in_worktree_list,
            "worktree not listed while the lane ran"
        );
    }

    #[test]
    fn the_parent_branch_fast_forwarded_to_the_lane() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);
        let pre_head = git::head(&r.root);

        let report = lane(&r.root, &cfg, &mut |wt| {
            std::fs::write(wt.join("lane-work.txt"), "x").expect("write");
            commit_all_in(wt, "feat: lane work");
            Ok(())
        })
        .expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert_ne!(git::head(&r.root), pre_head);
        assert!(r.root.join("lane-work.txt").exists());
    }

    #[test]
    fn the_worktree_is_removed_once_it_has_merged() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);

        let report = lane(&r.root, &cfg, &mut |_wt| Ok(())).expect("lane");

        assert!(report.merged);
        assert!(report.left.is_none());
        let listed = git::git(&r.root, &["worktree", "list"]).expect("worktree list");
        assert!(!listed.contains("lane/"));
        let branches = git::git(&r.root, &["branch", "--list", "lane/*"]).expect("branch list");
        assert!(branches.trim().is_empty());
    }

    #[test]
    fn a_stuck_lane_is_left_with_its_work() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);
        let pre_head = git::head(&r.root);

        let report = lane(&r.root, &cfg, &mut |wt| {
            std::fs::write(wt.join("lane-work.txt"), "x").expect("write");
            commit_all_in(wt, "feat: lane work");
            // The parent moves under the lane: a second writer, so the merge can't fast-forward.
            r.write("moved.txt", "y");
            r.commit_all("a second writer moved the parent");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        let left = report.left.clone().expect("left in place");
        assert!(left.exists());
        assert!(left.join("lane-work.txt").exists());
        assert_ne!(git::head(&r.root), pre_head); // the parent's own commit landed, not the lane's
        assert!(!report.reason.is_empty());

        // cleanup so the fixture's tempdir can be dropped without a lingering worktree lock
        let _ = git::git(
            &r.root,
            &["worktree", "remove", "--force", &left.to_string_lossy()],
        );
        let _ = git::git(&r.root, &["branch", "-D", &report.branch]);
    }

    #[test]
    fn a_run_that_errors_still_merges() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);

        let report = lane(&r.root, &cfg, &mut |_wt| {
            anyhow::bail!("the lane's run failed")
        })
        .expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert_eq!(report.run_error.as_deref(), Some("the lane's run failed"));
    }

    #[test]
    fn two_lanes_in_the_same_second_do_not_collide() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);

        let first = lane(&r.root, &cfg, &mut |wt| {
            std::fs::write(wt.join("first.txt"), "x").expect("write");
            commit_all_in(wt, "feat: first lane");
            Ok(())
        })
        .expect("first lane");
        let second = lane(&r.root, &cfg, &mut |wt| {
            std::fs::write(wt.join("second.txt"), "x").expect("write");
            commit_all_in(wt, "feat: second lane");
            Ok(())
        })
        .expect("second lane");

        assert!(first.merged, "reason: {}", first.reason);
        assert!(second.merged, "reason: {}", second.reason);
        assert!(r.root.join("first.txt").exists());
        assert!(r.root.join("second.txt").exists());
    }
}
