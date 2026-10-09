//! A block still at `status: review` whose id a product commit on the remote's default branch names: merged, and nothing moved it on.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::git;

const HEAD: &str = "refs/remotes/origin/HEAD";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    // origin/HEAD is a local ref, so no remote or host tool is asked
    match git::git(ctx.root, &["symbolic-ref", "--short", HEAD]) {
        Ok(default) => common::result(find(ctx, &default)),
        Err(_) => ProbeResult::Off(format!("{HEAD} names no default branch")),
    }
}

fn find(ctx: &ProbeCtx, default: &str) -> Res<Vec<Finding>> {
    let log = git::git(ctx.root, &["log", "--format=%h %s", default]).map_err(|e| e.to_string())?;
    let commits: Vec<(&str, &str)> = log.lines().filter_map(|l| l.split_once(' ')).collect();
    let tasks = common::instance(ctx, "TASKS.md");
    let mut found = Vec::new();
    for block in common::task_blocks(ctx)? {
        if common::field(&block, "status").is_none_or(|(_, status)| status != "review") {
            continue;
        }
        let Some((sha, _)) = commits.iter().find(|(_, subject)| {
            // an in-tree install's state commits name the task on the product's branch too
            git::recorded_sha(subject).is_none() && crate::gates::names_task(subject, &block.id)
        }) else {
            continue;
        };
        found.push(common::finding(
            &tasks,
            block.line,
            format!("{} is at review and {default} carries {sha}", block.id),
        ));
    }
    Ok(found)
}
