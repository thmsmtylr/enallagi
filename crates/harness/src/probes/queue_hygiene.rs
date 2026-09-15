//! The queue read against itself: a repeated id, a missing status, an undefined blocker, or an open block whose scope matches nothing.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use std::collections::BTreeSet;

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let blocks = common::task_blocks(ctx.root)?;
    let ids: BTreeSet<&str> = blocks.iter().map(|b| b.id.as_str()).collect();
    let task_id = common::re(r"T-\d+")?;
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut tree: Option<Vec<String>> = None;
    let mut found = Vec::new();

    for block in &blocks {
        if seen.contains(block.id.as_str()) {
            found.push(common::finding(
                "TASKS.md",
                block.line,
                format!("a second block is numbered {}", block.id),
            ));
        }
        seen.insert(block.id.as_str());

        let status = common::field(block, "status").map(|(_, v)| v);
        if status.is_none() {
            found.push(common::finding(
                "TASKS.md",
                block.line,
                format!("{} has no status line", block.id),
            ));
        }

        if let Some((blocked_at, blocked)) = common::field(block, "blockedBy") {
            for m in task_id.find_iter(&blocked) {
                if !ids.contains(m.as_str()) {
                    found.push(common::finding(
                        "TASKS.md",
                        blocked_at,
                        format!(
                            "{} is blocked by {}, which no block defines",
                            block.id,
                            m.as_str()
                        ),
                    ));
                }
            }
        }

        // a done block's scope is history; an open block whose scope names nothing is a task nobody can take
        if !matches!(
            status.as_deref(),
            Some("ready" | "review" | "blocked" | "needs-spec")
        ) {
            continue;
        }
        let Some((scope_at, scope)) = common::field(block, "scope") else {
            continue;
        };
        for pattern in scope.split(',') {
            let pattern = pattern.trim().trim_matches('`');
            if pattern.is_empty() {
                continue;
            }
            let paths = tree.get_or_insert_with(|| common::walk(ctx.root));
            if common::matches(paths, pattern)?.is_empty() {
                found.push(common::finding(
                    "TASKS.md",
                    scope_at,
                    format!(
                        "{} is open and its scope {pattern} matches no file",
                        block.id
                    ),
                ));
            }
        }
    }
    Ok(found)
}
