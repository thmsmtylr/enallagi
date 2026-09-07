//! The context file is read by every lane at the start of every task, and it is
//! seeded ONCE -- init writes a document only when it is absent, so a later
//! change to `check.command` never reaches it. This repository ran for six
//! rounds with the context file telling every lane to verify with a command
//! that did not exist in it; a lane hit it, worked around it locally, and the
//! source stayed wrong (DECISIONS.md:709). Nothing caught it because twelve
//! probes read these documents for SHAPE -- that an entry names a file, that a
//! row names a test -- and none for TRUTH.
//!
//! ponytail: the context file only. The same staleness in the spec or
//! LEARNINGS.md is real and is not reported here, because detecting "a command
//! that is not the check" needs to know what a check looks like, and a fuzzy
//! match on a document full of shell examples cries wolf. Widen it when a
//! second document is measured to have drifted.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let check = &ctx.cfg.check.command;
    if check.is_empty() {
        return Ok(vec![]);
    }
    let file = &ctx.cfg.layout.context_file;
    if !common::exists(ctx.root, file) {
        return Ok(vec![common::finding(
            file,
            0,
            "the context file every lane reads does not exist",
        )]);
    }
    // The Commands section only. The check is usually named again further down,
    // where the `green` rail is explained, and a document that still explains
    // the rail correctly while telling a lane to run the wrong command is
    // exactly the state this probe exists to catch -- so a match anywhere in
    // the file is not a match.
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
            "the Commands section names no command matching harness.toml check ({check}), so every lane is told to verify with something else"
        ),
    )])
}
