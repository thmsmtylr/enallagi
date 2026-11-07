//! A rejection nothing has answered: the `verdict:` line reads `rejected` and the status is not ready. A ticket parked at `needs-info` is reported too.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::block;
use crate::tracker::{self, Verdict};

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let mut found = Vec::new();
    for (tasks, block) in common::tickets(ctx)? {
        let Some((line, status)) = block::field_at(&block, "status") else {
            continue;
        };
        // the whole value: `ready-for-agent -- a reason` was never an answer
        let ready = tracker::READY.contains(&status.as_str());
        if matches!(tracker::verdict(&block), Some((Verdict::Rejected(_), _))) && !ready {
            found.push(common::finding(
                &tasks,
                line,
                format!(
                    "{} carries an unanswered rejection while status is {status}",
                    block.id
                ),
            ));
        }
        if status == "needs-info" {
            found.push(common::finding(
                &tasks,
                line,
                format!("{} is parked at {status}", block.id),
            ));
        }
    }
    Ok(found)
}
