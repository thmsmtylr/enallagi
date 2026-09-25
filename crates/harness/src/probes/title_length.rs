//! A block title past 72 characters: a pull request subject is cut at 72, and a title that runs to a sentence reads as an essay.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub const CAP: usize = 72;

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let tasks = common::instance(ctx, "TASKS.md");
    let text = common::read(ctx.root, &tasks)?;
    let heading = common::re(r"^## \[T-(\d+)\] (.*)$")?;
    // titles written before the cap are the record they were written as; every citation names them verbatim
    let from = ctx.cfg.queue.title_cap_from;
    let mut found = Vec::new();
    for (at, line) in text.lines().enumerate() {
        let Some(caps) = heading.captures(line) else {
            continue;
        };
        let (Some(number), Some(title)) = (caps.get(1), caps.get(2)) else {
            continue;
        };
        let number: u32 = number
            .as_str()
            .parse()
            .map_err(|e| format!("{tasks}: {e}"))?;
        let chars = title.as_str().trim().chars().count();
        if number >= from && chars > CAP {
            found.push(common::finding(
                &tasks,
                at + 1,
                format!(
                    "T-{:03} title runs to {chars} characters, past the {CAP} cap",
                    number
                ),
            ));
        }
    }
    Ok(found)
}
