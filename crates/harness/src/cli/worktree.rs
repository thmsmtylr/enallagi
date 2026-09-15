use std::path::Path;
use std::process::Command;

use crate::{config, git, worktree};

pub struct Args {
    pub n: Option<u32>,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = git::git(Path::new("."), &["rev-parse", "--show-toplevel"])?;
    let root = Path::new(root.trim());
    let cfg = config::load(root)?;
    let n = args.n.unwrap_or(3);
    let parent_branch = git::git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|_| "HEAD".to_string());

    let report = worktree::lane(root, &cfg, &mut |wt| {
        let status = Command::new(std::env::current_exe()?)
            .args(["run", "--iterations", &n.to_string(), "--no-tui"])
            .current_dir(wt)
            .status()?;
        if !status.success() {
            anyhow::bail!("lane run exited with {status}");
        }
        Ok(())
    })?;

    if report.merged {
        if let Some(err) = &report.run_error {
            eprintln!(
                "worktree: fast-forwarded to {} but the lane's run failed: {err}",
                report.branch
            );
            return Ok(1);
        }
        eprintln!(
            "worktree: fast-forwarded to {} -- {}",
            report.branch, report.reason
        );
        return Ok(0);
    }

    let dir = report
        .left
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let said: String = report.reason.lines().map(|l| format!("  {l}\n")).collect();
    eprintln!(
        "worktree: {branch} did not fast-forward into {parent_branch}. git said:\n{said}Nothing was merged and nothing was removed. A merge here is a decision, not a step:\n  cd {dir}                       # the lane's work, intact\n  git -C {root} merge {branch}       # if you want the merge commit\n  git -C {root} worktree remove {dir} && git -C {root} branch -D {branch}   # if you do not want the work",
        branch = report.branch,
        root = root.display(),
    );
    Ok(1)
}
