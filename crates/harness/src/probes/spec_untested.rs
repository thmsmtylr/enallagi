//! Every exit-criteria row whose test does not exist.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub struct Untested {
    pub name: String,
    pub test: String,
    pub line: usize,
    pub message: String,
}

pub fn untested_rows(ctx: &ProbeCtx) -> Res<Vec<Untested>> {
    let src = &ctx.cfg.layout.source_root;
    let mut out = Vec::new();
    for row in common::spec_rows(ctx)? {
        // SPEC.md §12: rows live under source_root, so a name with a slash is tried there first
        let under_root = format!("{src}/{}", row.name);
        let path = if row.name.contains('/') && !common::exists(ctx.root, &under_root) {
            row.name.clone()
        } else {
            under_root
        };
        let message = if !common::exists(ctx.root, &path) {
            format!("no file {path} for criterion {}::{}", row.name, row.test)
        } else if !common::declares(
            ctx.root,
            &path,
            &row.test,
            &ctx.cfg.layout.test_decl_patterns,
        )? {
            format!("{path} declares no test named {}", row.test)
        } else {
            continue;
        };
        out.push(Untested {
            name: row.name,
            test: row.test,
            line: row.line,
            message,
        });
    }
    Ok(out)
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let spec = &common::instance(ctx, &ctx.cfg.layout.spec);
    Ok(untested_rows(ctx)?
        .into_iter()
        .map(|row| common::finding(spec, row.line, row.message))
        .collect())
}
