//! The queue against the spec, both directions: neither spec-untested nor queue-hygiene asks whether the two agree.

use super::common::{self, Res};
use super::spec_untested::untested_rows;
use super::{Finding, ProbeCtx, ProbeResult};
use crate::git;
use std::collections::BTreeSet;

const OPEN_STATUS: &[&str] = &["proposed", "ready", "blocked", "review"];

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let row_ref = common::re(&format!(
        r"([\w./-]+{})::",
        ctx.cfg.layout.test_file_suffix_re
    ))?;
    let defined: BTreeSet<(String, String)> = common::spec_rows(ctx)?
        .into_iter()
        .map(|r| (r.name, r.test))
        .collect();
    let mut claimed: BTreeSet<(String, String)> = BTreeSet::new();
    let mut found = Vec::new();

    let tasks = common::instance(ctx, "TASKS.md");
    let mut tree: Option<Vec<String>> = None;
    for block in common::task_blocks(ctx)? {
        let status = common::field(&block, "status").map(|(_, v)| v);
        if !status.is_some_and(|s| OPEN_STATUS.contains(&s.as_str())) {
            continue;
        }
        found.extend(stale_criteria(ctx, &block, &tasks, &mut tree)?);
        let Some((rows_at, rows)) = common::field(&block, "rows") else {
            continue;
        };
        let refs: Vec<_> = row_ref.captures_iter(&rows).collect();
        for (at, m) in refs.iter().enumerate() {
            let (Some(name), Some(whole)) = (m.get(1), m.get(0)) else {
                continue;
            };
            // a test name may hold a comma, so a claim runs to the next reference, not to the next comma
            let stop = refs
                .get(at + 1)
                .and_then(|next| next.get(0))
                .map_or(rows.len(), |g| g.start());
            let test = rows[whole.end()..stop]
                .split(['`', '\n'])
                .next()
                .unwrap_or_default()
                .trim()
                .trim_end_matches(',')
                .trim();
            if test.is_empty() {
                continue;
            }
            let reference = (name.as_str().to_string(), test.to_string());
            claimed.insert(reference.clone());
            if !defined.contains(&reference) {
                found.push(common::finding(
                    &tasks,
                    rows_at,
                    format!(
                        "{} claims row {}::{} and the exit criteria define no such row",
                        block.id, reference.0, reference.1
                    ),
                ));
            }
        }
    }

    let spec = &common::instance(ctx, &ctx.cfg.layout.spec);
    for row in untested_rows(ctx)? {
        if !claimed.contains(&(row.name.clone(), row.test.clone())) {
            found.push(common::finding(
                spec,
                row.line,
                format!(
                    "{}::{} is untested and no open task names it ({})",
                    row.name, row.test, row.message
                ),
            ));
        }
    }
    Ok(found)
}

// a name absent from the tree is quiet unless the history once held it, so a test its own task has yet to write is not reported
fn stale_criteria(
    ctx: &ProbeCtx,
    block: &common::TaskBlock,
    tasks: &str,
    tree: &mut Option<Vec<String>>,
) -> Res<Vec<Finding>> {
    let src = &ctx.cfg.layout.source_root;
    if src.is_empty() {
        return Ok(Vec::new());
    }
    let token = common::re(r"\b[a-z][a-z0-9]*(_[a-z0-9]+)+\b")?;
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    let mut inside = false;
    for (line, text) in &block.body {
        if text.starts_with("criteria:") {
            inside = true;
        } else if !text.starts_with(' ') {
            inside = false;
        }
        if !inside {
            continue;
        }
        for m in token.find_iter(text) {
            let name = m.as_str();
            if !seen.insert(name.to_string()) {
                continue;
            }
            let files = match tree {
                Some(files) => files,
                None => tree.insert(source_texts(ctx)?),
            };
            if files.iter().any(|f| f.contains(name)) {
                continue;
            }
            let pick = format!("-S{name}");
            let commits = git::git(ctx.root, &["log", "--format=%h", &pick, "--", src])
                .map_err(|e| e.to_string())?;
            let n = commits.lines().filter(|l| !l.is_empty()).count();
            if n > 0 {
                found.push(common::finding(
                    tasks,
                    *line,
                    format!(
                        "{} criteria name {name}, which no file under {src} declares and {n} commits under it touched",
                        block.id
                    ),
                ));
            }
        }
    }
    Ok(found)
}

fn source_texts(ctx: &ProbeCtx) -> Res<Vec<String>> {
    let src = &ctx.cfg.layout.source_root;
    let paths = git::git(
        ctx.root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            src,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(paths
        .lines()
        .filter_map(|p| common::read(ctx.root, p).ok())
        .collect())
}
