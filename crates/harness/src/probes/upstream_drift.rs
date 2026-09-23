//! The checkout's branch and its upstream each carry commits the other lacks, so one change exists as two.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::git;

const UPSTREAM: [&str; 4] = [
    "rev-parse",
    "--abbrev-ref",
    "--symbolic-full-name",
    "@{upstream}",
];

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    let branch = match git::git(ctx.root, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(branch) => branch,
        Err(err) => return ProbeResult::Error(err.to_string()),
    };
    match git::git(ctx.root, &UPSTREAM) {
        Ok(upstream) => common::result(find(ctx, &branch, &upstream)),
        Err(_) => ProbeResult::Off(format!("{branch} tracks no upstream branch")),
    }
}

fn find(ctx: &ProbeCtx, branch: &str, upstream: &str) -> Res<Vec<Finding>> {
    let count = |range: &str| {
        git::git(ctx.root, &["rev-list", "--count", range]).map_err(|e| e.to_string())
    };
    let ahead = format!("{upstream}..{branch}");
    let behind = format!("{branch}..{upstream}");
    let (mine, theirs) = (count(&ahead)?, count(&behind)?);
    if mine == "0" || theirs == "0" {
        return Ok(Vec::new());
    }
    Ok(vec![common::finding(
        ".git/HEAD",
        0,
        format!(
            "`git rev-list --count {ahead}` -> {mine} and `git rev-list --count {behind}` -> {theirs}"
        ),
    )])
}
