//! The queue read against itself: a repeated id, a killed id reused, a missing status, an undefined blocker, a block in review whose scope matches nothing, or a scope entry naming a bare directory.

use super::common::{self, Res};
use super::ponytail_ceiling::kill_lines;
use super::{Finding, ProbeCtx, ProbeResult};
use crate::block::{self, QueueError};
use std::collections::BTreeSet;

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let tracker = common::tracker(ctx)?;
    // the tracker refuses a repeated id, so the repeat is the only finding a queue holding one gets
    let blocks = match tracker.blocks() {
        Ok(blocks) => blocks,
        Err(QueueError::DuplicateId { id, second, .. }) => {
            let tasks = common::cite(ctx, &tracker, &id);
            return Ok(vec![repeated(&tasks, second, &id)]);
        }
        Err(QueueError::DuplicateTicket { id, second, .. }) => {
            let path = common::rel(ctx, std::path::Path::new(&second));
            return Ok(vec![repeated(&path, 1, &id)]);
        }
        Err(e) => return Err(e.to_string()),
    };
    let ids: BTreeSet<&str> = blocks.iter().map(|b| b.id.as_str()).collect();
    let task_id = common::re(r"T-\d+")?;
    // the id opening a kill line, never one it cites: a duplicate kill names the live block it duplicates
    let opening = common::re(r"^- \[[^\]]*\] (T-\d+)\b")?;
    let killed: BTreeSet<String> = kill_lines(ctx)?
        .iter()
        .filter_map(|l| opening.captures(l).map(|m| m[1].to_string()))
        .collect();
    let mut tree: Option<Vec<String>> = None;
    let mut found = Vec::new();

    for block in &blocks {
        let tasks = common::cite(ctx, &tracker, &block.id);
        let status = block::field_at(block, "status").map(|(_, v)| v);
        if status.is_none() {
            found.push(common::finding(
                &tasks,
                block.line,
                format!("{} has no status line", block.id),
            ));
        }

        if let Some((blocked_at, blocked)) = block::field_at(block, "blockedBy") {
            for m in task_id.find_iter(&blocked) {
                if !ids.contains(m.as_str()) {
                    found.push(common::finding(
                        &tasks,
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

        if status.as_deref() == Some("done") {
            continue;
        }
        if killed.contains(&block.id) {
            found.push(common::finding(
                &tasks,
                block.line,
                format!("{} reuses the id of a killed finding", block.id),
            ));
        }
        let Some((scope_at, scope)) = block::field_at(block, "scope") else {
            continue;
        };
        for pattern in scope.split(',') {
            let pattern = pattern.trim().trim_matches('`');
            if pattern.is_empty() {
                continue;
            }
            // the scope gate's glob matches a bare directory against itself and never its files
            let bare = pattern.trim_end_matches('/');
            if !bare.contains(['*', '?', '[', '{']) && ctx.root.join(bare).is_dir() {
                found.push(common::finding(
                    &tasks,
                    scope_at,
                    format!(
                        "{}'s scope {bare} names a directory and would match none of its files: write `{bare}/**`",
                        block.id
                    ),
                ));
                continue;
            }
            // before review a missing scope path is a file the task creates
            if status.as_deref() != Some("review") {
                continue;
            }
            let paths = tree.get_or_insert_with(|| common::walk(ctx.root));
            if common::matches(paths, pattern)?.is_empty() {
                found.push(common::finding(
                    &tasks,
                    scope_at,
                    format!(
                        "{} is in review and its scope {pattern} matches no file",
                        block.id
                    ),
                ));
            }
        }
    }
    Ok(found)
}

fn repeated(path: &str, line: usize, id: &str) -> Finding {
    common::finding(path, line, format!("a second block is numbered {id}"))
}
