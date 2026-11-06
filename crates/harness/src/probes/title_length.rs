//! A block title past 72 characters: a pull request subject is cut at 72, and a title that runs to a sentence reads as an essay.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::{queue, tracker};

pub const CAP: usize = 72;

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    // titles written before the cap are the record they were written as; every citation names them verbatim
    let from = ctx.cfg.queue.title_cap_from;
    let mut found = Vec::new();
    for (tasks, b) in common::tickets(ctx)? {
        // an imported issue keeps its author's title until the adjudicator writes the task's at promotion
        if tracker::state(&b).as_deref() == Some("needs-triage")
            && queue::field(&b, "probe").as_deref() == Some("issue")
        {
            continue;
        }
        let number: u32 = b.id[2..].parse().map_err(|e| format!("{tasks}: {e}"))?;
        let chars = b.title.chars().count();
        if number >= from && chars > CAP {
            found.push(common::finding(
                &tasks,
                b.line,
                format!(
                    "T-{:03} title runs to {chars} characters, past the {CAP} cap",
                    number
                ),
            ));
        }
    }
    Ok(found)
}
