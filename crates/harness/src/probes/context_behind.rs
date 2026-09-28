//! A Commands line the context template renders that the installed context file lacks: init keeps a file the project owns, so a line the template gained reaches new installs only.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::config;

const CONTEXT: &str = include_str!("../../../../templates/AGENTS.md");

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    let file = &ctx.cfg.layout.context_file;
    if !common::is_file(ctx.root, file) {
        return ProbeResult::Off(format!("{file} does not exist"));
    }
    common::result(find(ctx, file))
}

fn commands(text: &str) -> Vec<&str> {
    text.lines()
        .skip_while(|l| l.trim() != "## Commands")
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .filter(|l| !l.trim().is_empty())
        .collect()
}

fn find(ctx: &ProbeCtx, file: &str) -> Res<Vec<Finding>> {
    let installed = common::read(ctx.root, file)?;
    let carried: Vec<&str> = installed.lines().map(str::trim_end).collect();
    let heading = carried
        .iter()
        .position(|l| l.trim() == "## Commands")
        .map_or(0, |i| i + 1);
    // the Commands lines carry only check tokens, so config::subst renders them as init does
    let rendered = config::subst(CONTEXT, ctx.cfg);
    Ok(commands(&rendered)
        .into_iter()
        .filter(|line| !carried.contains(&line.trim_end()))
        .map(|line| {
            common::finding(
                file,
                heading,
                format!("the context template's Commands section carries a line this file lacks; add it by hand: {line}"),
            )
        })
        .collect())
}
