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

    let report = worktree::lane(root, &cfg, &mut |wt| {
        let status = Command::new(std::env::current_exe()?)
            .args(["run", &n.to_string(), "--no-tui"])
            .current_dir(wt)
            .status()?;
        if !status.success() {
            anyhow::bail!("lane run exited with {status}");
        }
        Ok(())
    })?;

    if report.merged {
        eprintln!(
            "worktree: fast-forwarded to {} -- {}",
            report.branch, report.reason
        );
        Ok(0)
    } else {
        let left = report
            .left
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        eprintln!(
            "worktree: {} did not fast-forward. git said:\n  {}\nNothing was merged and nothing was removed. Left in place: {left}",
            report.branch, report.reason,
        );
        Ok(1)
    }
}
