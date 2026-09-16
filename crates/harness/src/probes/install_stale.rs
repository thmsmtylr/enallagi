//! An installed file drifted from what init would write now (not the raw template, whose substitution tokens would false-positive every file). Seeded documents are excluded: seed never overwrites one, so drift there is legitimate.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::agent;
use crate::init::{self, InitOpts};

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let planned = init::planned_files(
        ctx.root,
        &InitOpts {
            adapter: None,
            dry_run: true,
        },
    )
    .map_err(|e| format!("enallagi init could not plan this tree: {e}"))?;

    let presets = agent::presets();
    let skills = init::skills_root(ctx.cfg, presets.get(&ctx.cfg.agent.preset));
    let dirs = [
        format!("{}/", ctx.cfg.layout.harness_dir),
        format!("{}/", skills.to_string_lossy().replace('\\', "/")),
    ];

    let mut found = Vec::new();
    for (path, content) in planned {
        if !dirs.iter().any(|dir| path.starts_with(dir.as_str())) {
            continue;
        }
        let message = if !common::is_file(ctx.root, &path) {
            "enallagi init writes this file and the instance does not have it; re-run `enallagi init`"
        } else if common::read(ctx.root, &path)? != content {
            "the installed copy differs from the source it was built from; re-run `enallagi init`"
        } else {
            continue;
        };
        found.push(common::finding(&path, 0, message.to_string()));
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}
