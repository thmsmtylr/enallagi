//! A block carrying a rejection that is not back in the queue, and a block
//! parked at `needs-spec`.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let mut found = Vec::new();
    for block in common::task_blocks(ctx.root)? {
        let Some((line, status)) = common::field(&block, "status") else {
            continue;
        };
        let notes_at = common::field(&block, "notes").map(|(at, _)| at);
        let notes: String = block
            .body
            .iter()
            .filter(|(at, _)| notes_at.is_some_and(|start| *at >= start))
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if notes.contains("REJECTED") && status != "ready" {
            found.push(common::finding(
                "TASKS.md",
                line,
                format!("{} notes carry REJECTED while status is {status}", block.id),
            ));
        }
        if status == "needs-spec" {
            found.push(common::finding(
                "TASKS.md",
                line,
                format!("{} is parked at needs-spec", block.id),
            ));
        }
    }
    Ok(found)
}
