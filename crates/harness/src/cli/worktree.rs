//! `enallagi worktree [n]` -- runs a round in a lane of its own, and reports what came back to the parent branch.

use std::path::Path;
use std::process::Command;

use crate::{config, git, worktree};

pub struct Args {
    pub n: Option<u32>,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = &super::root()?;
    let cfg = config::load(root)?;
    // refused here too, so a lane is not created only for its run to refuse
    crate::pipeline::refuse_unfilled(root, &cfg)?;
    let n = args.n.unwrap_or(3);
    let parent_branch = git::git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|_| "HEAD".to_string());
    if cfg.parallel.writers.get() > 1 {
        return writers(root, &cfg, n);
    }

    let report = worktree::lane(root, &cfg, &mut |wt| {
        eprintln!("worktree: cd {} && enallagi watch", wt.display());
        let status = Command::new(std::env::current_exe()?)
            .args(["run", "--iterations", &n.to_string(), "--no-tui"])
            .env("ENALLAGI_DIR", wt.join(&cfg.layout.harness_dir))
            .current_dir(wt)
            .status()?;
        if !status.success() {
            anyhow::bail!("lane run exited with {status}");
        }
        Ok(())
    })?;

    // a checkout that did not follow its upstream is a failure even when the lane's own work landed:
    // the next lane starts behind its upstream and every `enallagi pr` replay conflicts there
    if let Some(err) = &report.follow_error {
        eprintln!("worktree: {err}");
        if report.merged {
            eprintln!(
                "worktree: {} landed, so the work is in the repository; the checkout is not up to date",
                report.branch
            );
        }
        return Ok(1);
    }
    if report.merged {
        let landing = if worktree::by_branch(root, &cfg) {
            format!("left {} for `enallagi pr`", report.branch)
        } else {
            format!("fast-forwarded to {}", report.branch)
        };
        if let Some(err) = &report.run_error {
            eprintln!("worktree: {landing} but the lane's run failed: {err}");
            return Ok(1);
        }
        eprintln!("worktree: {landing} -- {}", report.reason);
        return Ok(0);
    }

    eprintln!(
        "{}",
        left_message(root, &cfg.layout.harness_dir, &parent_branch, &report)
    );
    Ok(1)
}

fn writers(root: &Path, cfg: &config::Config, n: u32) -> anyhow::Result<i32> {
    // a stop reaches every lane's run, and the launcher still lands what came back
    crate::agent::catch_stop_signals();
    crate::pipeline::vendor_lanes(root, cfg).map_err(anyhow::Error::msg)?;
    let n = n.to_string();
    let reports = worktree::launch(root, cfg, &|wt, task, env| {
        eprintln!("worktree: cd {} && enallagi watch", wt.display());
        let mut args = vec!["run", "--iterations", &n, "--no-tui"];
        match task {
            Some((task, worktree::Hold::Writer)) => {
                args.extend(["--task", task, "--pipeline", "task", "--pipeline", "review"])
            }
            Some((task, worktree::Hold::Reader)) => {
                args.extend(["--task", task, "--pipeline", "review"])
            }
            None => {}
        }
        let status = Command::new(std::env::current_exe()?)
            .args(args)
            .env("ENALLAGI_DIR", wt.join(&cfg.layout.harness_dir))
            .envs(env.iter().map(|(k, v)| (k, v)))
            .current_dir(wt)
            .status()?;
        if !status.success() {
            anyhow::bail!("lane run exited with {status}");
        }
        Ok(())
    })?;
    for r in &reports {
        let task = r.task.as_deref().unwrap_or("unpinned");
        let outcome = match (&r.left, r.merged) {
            (_, true) => "landed".to_string(),
            (Some(dir), false) => format!("left in {}", dir.display()),
            (None, false) => "sent back".to_string(),
        };
        eprintln!("worktree: {task} {outcome} -- {}", r.reason);
    }
    Ok(i32::from(!reports.iter().all(|r| r.merged)))
}

fn left_message(
    root: &Path,
    harness_dir: &str,
    parent_branch: &str,
    report: &worktree::LaneReport,
) -> String {
    let branch = &report.branch;
    let mut repos = vec![(root.to_path_buf(), report.left.clone().unwrap_or_default())];
    if let Some(state) = &report.state {
        repos.insert(0, (root.join(harness_dir), state.clone()));
    }
    let said: String = report.reason.lines().map(|l| format!("  {l}\n")).collect();
    let mut msg = format!(
        "worktree: {branch} did not fast-forward into {parent_branch}. git said:\n{said}Nothing was merged, nothing was removed:"
    );
    for (repo, dir) in &repos {
        let (repo, dir) = (repo.display(), dir.display());
        msg.push_str(&format!(
            "\n  cd {dir}\n  git -C {repo} merge {branch}                 # keep the work\n  git -C {repo} worktree remove {dir} && git -C {repo} branch -D {branch}   # discard it"
        ));
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unmerged_lane_names_both_repositories() {
        let report = worktree::LaneReport {
            merged: false,
            left: Some("/r/.enallagi/worktrees/lane-x".into()),
            state: Some("/r/.enallagi/worktrees/lane-x/.enallagi".into()),
            reason: "lane/x cannot fast-forward into /r/.enallagi".to_string(),
            branch: "lane/x".to_string(),
            run_error: None,
            follow_error: None,
            task: None,
        };
        let msg = left_message(Path::new("/r"), ".enallagi", "main", &report);
        for want in [
            "cd /r/.enallagi/worktrees/lane-x\n",
            "cd /r/.enallagi/worktrees/lane-x/.enallagi\n",
            "git -C /r merge lane/x",
            "git -C /r/.enallagi merge lane/x",
        ] {
            assert!(msg.contains(want), "missing `{want}` in:\n{msg}");
        }
    }
}
