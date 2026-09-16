//! The queue against the spec, both directions: neither spec-untested nor queue-hygiene asks whether the two agree.

use super::common::{self, Res};
use super::spec_untested::untested_rows;
use super::{Finding, ProbeCtx, ProbeResult};
use std::collections::BTreeSet;

const OPEN_STATUS: &[&str] = &["proposed", "ready", "blocked", "review"];

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let row_ref = common::re(&format!(
        r"([\w./-]+{})::([^`,\n]+)",
        ctx.cfg.layout.test_file_suffix_re
    ))?;
    let defined: BTreeSet<(String, String)> = common::spec_rows(ctx)?
        .into_iter()
        .map(|r| (r.name, r.test))
        .collect();
    let mut claimed: BTreeSet<(String, String)> = BTreeSet::new();
    let mut found = Vec::new();

    let tasks = common::instance(ctx, "TASKS.md");
    for block in common::task_blocks(ctx)? {
        let status = common::field(&block, "status").map(|(_, v)| v);
        let rows = common::field(&block, "rows");
        let (Some((rows_at, rows)), Some(status)) = (rows, status) else {
            continue;
        };
        if !OPEN_STATUS.contains(&status.as_str()) {
            continue;
        }
        for m in row_ref.captures_iter(&rows) {
            let (Some(name), Some(test)) = (m.get(1), m.get(2)) else {
                continue;
            };
            let reference = (
                name.as_str().to_string(),
                test.as_str()
                    .trim()
                    .trim_end_matches('`')
                    .trim()
                    .to_string(),
            );
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
