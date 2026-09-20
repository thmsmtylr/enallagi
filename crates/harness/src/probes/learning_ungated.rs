//! `LEARNINGS.md` holds the seed rules an install shipped. A rule the loop earned belongs under `## Earned rules` in `DECISIONS.md`, the store the adjudicator writes, and the two are capped together.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub const DATED: &str = r"^- \[\d{4}-\d{2}-\d{2}\]";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let dated = common::re(DATED)?;
    let cap = ctx.cfg.layout.learnings_cap;
    let entries = common::learning_entries(ctx)?;
    let learnings = common::instance(ctx, "LEARNINGS.md");
    let decisions = common::instance(ctx, "DECISIONS.md");
    let mut found = Vec::new();

    for (at, text) in &entries {
        if !dated.is_match(text.trim()) {
            continue;
        }
        found.push(common::finding(
            &learnings,
            *at,
            format!(
                "a dated rule in the seed library: move it under `## Earned rules` in {decisions}, with the `enallagi eval --gate` run that admitted it or the command that showed its cost: {}",
                common::cut(text.trim(), 80)
            ),
        ));
    }

    let held = entries.len() + earned(ctx, &decisions)?;
    if held > cap {
        found.push(common::finding(
            &learnings,
            0,
            format!(
                "the rule library holds {held} entries against a cap of {cap}. Every entry is read at the start of every task; adding one means removing one"
            ),
        ));
    }
    Ok(found)
}

fn earned(ctx: &ProbeCtx, decisions: &str) -> Res<usize> {
    if !common::is_file(ctx.root, decisions) {
        return Ok(0);
    }
    Ok(common::lines_of(ctx.root, decisions)?
        .iter()
        .skip_while(|line| !line.starts_with("## Earned rules"))
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .filter(|line| line.starts_with("- "))
        .count())
}
