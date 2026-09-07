//! worktree: one lane runs in its own git worktree, isolated from the parent checkout, and is
//! fast-forwarded back onto the parent branch when its history allows it. Ported from
//! `harness/worktree.sh`.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::git;

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("worktree: the parent checkout is detached; check out a branch first")]
    Detached,
    #[error("worktree: could not create {0}")]
    Create(PathBuf),
    #[error(transparent)]
    Git(#[from] git::GitError),
}

pub struct LaneReport {
    pub merged: bool,
    pub left: Option<PathBuf>,
    pub reason: String,
    pub branch: String,
}

/// Run one lane in its own git worktree, branched off the current branch's HEAD. `run` is called
/// with the worktree's path; its gates, commits and STOP file are all the lane's. Whatever `run`
/// returns, what decides what happens next is the worktree's own state -- committed or not --
/// never the run's exit status, mirroring `worktree.sh`, which checks `git status --porcelain`
/// regardless of the loop's exit code.
pub fn lane(
    root: &Path,
    cfg: &Config,
    run: &mut dyn FnMut(&Path) -> anyhow::Result<()>,
) -> Result<LaneReport, WorktreeError> {
    let parent_branch = git::git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if parent_branch == "HEAD" {
        return Err(WorktreeError::Detached);
    }

    let ts = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    let branch = format!("lane/{ts}");
    let dir = root
        .join(&cfg.layout.harness_dir)
        .join("worktrees")
        .join(format!("lane-{ts}"));
    let dir_str = dir.to_string_lossy().into_owned();

    git::git(root, &["worktree", "add", "-b", &branch, &dir_str])
        .map_err(|_| WorktreeError::Create(dir.clone()))?;

    let _ = run(&dir);

    // Uncommitted work in the lane is the lane's to finish, not ours to throw away.
    if !git::porcelain(&dir).is_empty() {
        return Ok(LaneReport {
            merged: false,
            left: Some(dir),
            reason: format!("{branch} has uncommitted work"),
            branch,
        });
    }

    // Fast-forward or nothing. A merge commit here would mean the parent moved under the lane,
    // which is a human's call, never this function's.
    match git::git(root, &["merge", "--ff-only", &branch]) {
        Ok(reason) => {
            let _ = git::git(root, &["worktree", "remove", &dir_str]);
            let _ = git::git(root, &["branch", "-d", &branch]);
            Ok(LaneReport {
                merged: true,
                left: None,
                reason,
                branch,
            })
        }
        Err(git::GitError::Failed { stderr, .. }) => Ok(LaneReport {
            merged: false,
            left: Some(dir),
            reason: stderr,
            branch,
        }),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;

    /// `worktrees/` under the harness dir, gitignored the way `init` writes it -- tests write the
    /// line themselves since `fixture::init_harness` doesn't run `init` yet.
    fn repo_with_harness_dir() -> Repo {
        let r = Repo::new();
        r.write(".gitignore", ".harness/worktrees/\n");
        r.write("harness.toml", "");
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

    #[test]
    fn a_worktree_lane_leaves_the_parent_checkout_untouched() {
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
        lane(&r.root, &cfg, &mut |wt| {
            seen_dir = Some(wt.to_path_buf());
            Ok(())
        })
        .expect("lane");

        let dir = seen_dir.expect("closure ran");
        assert!(dir.starts_with(r.root.join(".harness").join("worktrees")));
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
    fn a_lane_that_cannot_fast_forward_is_left_for_a_human_and_its_work_is_still_there() {
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
}
