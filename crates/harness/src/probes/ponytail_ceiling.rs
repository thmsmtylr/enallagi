//! A ponytail marker is not re-reported once a kill line names its TEXT (quoted, or via `path:line`) — text not line number, since a moved marker is still the same marker.

use super::common::{self, Res};
use super::learning_ungated::DATED;
use super::{Finding, ProbeCtx, ProbeResult};
use std::collections::{BTreeMap, BTreeSet};

const REJECTED: &str = "## Rejected findings";

// lines, not their task ids: an id is spent once but the same finding recurs under a new one
pub fn kill_lines(ctx: &ProbeCtx) -> Res<Vec<String>> {
    let decisions = common::instance(ctx, "DECISIONS.md");
    if !common::exists(ctx.root, &decisions) {
        return Ok(vec![]);
    }
    let dated = common::re(DATED)?;
    let mut inside = false;
    let mut found = Vec::new();
    for line in common::lines_of(ctx.root, &decisions)? {
        if line.starts_with("## ") {
            inside = line.trim() == REJECTED;
        } else if inside && dated.is_match(&line) {
            found.push(line);
        }
    }
    Ok(found)
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    // never the literal, or this file reports itself
    let marker = concat!("ponytail", ":");
    let quoted = kill_lines(ctx)?.join("\n");
    let reference = common::re(r"`([\w./-]+):(\d+)`")?;

    let mut settled: BTreeSet<String> = BTreeSet::new();
    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in reference.captures_iter(&quoted) {
        let (Some(path), Some(at)) = (m.get(1), m.get(2)) else {
            continue;
        };
        let path = path.as_str().to_string();
        if !seen.contains_key(&path) {
            let body = if common::is_file(ctx.root, &path) {
                common::lines_of(ctx.root, &path)?
            } else {
                vec![]
            };
            seen.insert(path.clone(), body);
        }
        let Ok(index) = at.as_str().parse::<usize>() else {
            continue;
        };
        let body = seen.get(&path).map(Vec::as_slice).unwrap_or(&[]);
        if index >= 1 {
            if let Some(line) = body.get(index - 1) {
                if line.contains(marker) {
                    settled.insert(line.trim().to_string());
                }
            }
        }
    }

    let ext = &ctx.cfg.layout.source_ext;
    let mut found = Vec::new();
    for path in common::tracked(ctx.root)? {
        if !ext.iter().any(|e| path.ends_with(e.as_str())) || !common::is_file(ctx.root, &path) {
            continue;
        }
        for (index, line) in common::lines_of(ctx.root, &path)?.iter().enumerate() {
            let text = line.trim();
            if line.contains(marker) && !settled.contains(text) && !quoted.contains(text) {
                found.push(common::finding(&path, index + 1, common::cut(text, 100)));
            }
        }
    }
    Ok(found)
}
