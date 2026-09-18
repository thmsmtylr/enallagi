//! The context file's Commands section must name the configured check, or every lane verifies with the wrong command silently.
//!
//! ponytail: the context file only; widen to the spec or LEARNINGS.md once a second document is measured to have drifted.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let check = &ctx.cfg.check.command;
    let file = &ctx.cfg.layout.context_file;
    if check.is_empty() {
        return Ok(vec![common::finding(
            file,
            0,
            "enallagi.toml sets no check.command, so the context file names no check; set it, then run `enallagi init`",
        )]);
    }
    if !common::exists(ctx.root, file) {
        return Ok(vec![common::finding(
            file,
            0,
            "the context file every lane reads does not exist",
        )]);
    }
    // scoped to the Commands section only: a match elsewhere in the file isn't a match
    let mut commands: Vec<String> = Vec::new();
    let mut start = 0usize;
    for (index, line) in common::lines_of(ctx.root, file)?.iter().enumerate() {
        let at = index + 1;
        if let Some(heading) = line.strip_prefix("## ") {
            if !commands.is_empty() || start > 0 {
                break;
            }
            if heading.trim().to_lowercase() == "commands" {
                start = at;
            }
            continue;
        }
        if start > 0 {
            commands.push(line.clone());
        }
    }
    if start == 0 {
        return Ok(vec![common::finding(
            file,
            0,
            "has no \"## Commands\" section, so no lane is told how to verify",
        )]);
    }
    if commands.iter().any(|line| line.contains(check.as_str())) {
        return Ok(vec![]);
    }
    Ok(vec![common::finding(
        file,
        start,
        format!(
            "the Commands section names no command matching enallagi.toml check ({check}), so every lane is told to verify with something else"
        ),
    )])
}
