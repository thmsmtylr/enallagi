//! A proposed learning waits in `DECISIONS.md` for the adjudicator, and expires after `queue.proposed_rounds` rounds undecided.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let decisions = common::instance(ctx, "DECISIONS.md");
    let rounds = ctx.cfg.queue.proposed_rounds;
    Ok(crate::audit::waiting(ctx.root, ctx.cfg)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|w| {
            common::finding(
                &decisions,
                w.line,
                format!(
                    "`{}` has stood {} rounds, {} left before it expires",
                    w.class,
                    w.age,
                    rounds.saturating_sub(w.age)
                ),
            )
        })
        .collect())
}
