//! A rule that names nothing runnable is unenforceable.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

const CITED: &str = r"^(bun|bunx|git|npm|turbo|node|ps|sed|grep|touch|rm|chmod)\b";

pub fn cites_something(text: &str) -> Res<bool> {
    let cited = common::re(CITED)?;
    let dotted = common::re(r"\.\w")?;
    Ok(common::backticked(text)
        .iter()
        .any(|token| token.contains('/') || dotted.is_match(token) || cited.is_match(token)))
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let mut found = Vec::new();
    for (at, text) in common::learning_entries(ctx.root)? {
        if !cites_something(&text)? {
            found.push(common::finding(
                "LEARNINGS.md",
                at,
                format!(
                    "entry names no file, command or hook: {}",
                    common::cut(text.trim(), 90)
                ),
            ));
        }
    }
    Ok(found)
}
