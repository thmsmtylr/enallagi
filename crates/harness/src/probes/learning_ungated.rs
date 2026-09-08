//! A dated rule must name the eval that holds it, and the library is capacity-bounded. `[seed]` entries predate the gate and are exempt.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub const DATED: &str = r"^- \[\d{4}-\d{2}-\d{2}\]";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let dated = common::re(DATED)?;
    let names = common::re(r"evals/([\w.-]+)")?;
    let cap = ctx.cfg.layout.learnings_cap;
    let entries = common::learning_entries(ctx.root)?;
    let mut found = Vec::new();

    for (at, text) in &entries {
        if !dated.is_match(text.trim()) {
            continue;
        }
        let named: Vec<String> = names
            .captures_iter(text)
            .filter_map(|m| m.get(1).map(|g| g.as_str().to_string()))
            .collect();
        if named.is_empty() {
            found.push(common::finding(
                "LEARNINGS.md",
                *at,
                format!(
                    "dated rule names no eval, so nothing decided it was worth its place: {}",
                    common::cut(text.trim(), 80)
                ),
            ));
            continue;
        }
        for name in named {
            if !common::is_dir(ctx.root, &format!("evals/{name}")) {
                found.push(common::finding(
                    "LEARNINGS.md",
                    *at,
                    format!("names evals/{name}, which does not exist"),
                ));
            }
        }
    }
    if entries.len() > cap {
        found.push(common::finding(
            "LEARNINGS.md",
            0,
            format!(
                "the rule library holds {} entries against a cap of {cap}. Every entry is read at the start of every task; adding one means removing one",
                entries.len()
            ),
        ));
    }
    Ok(found)
}
