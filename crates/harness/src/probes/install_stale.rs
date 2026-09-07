//! An installed file that has drifted from the source it was built from.
//!
//! Substitution is what makes a plain diff useless: a template carries
//! `__HARNESS_DIR__` where the installed copy carries `.harness`, so comparing
//! the trees raw reports every file forever. The comparison is therefore
//! against what the installer would write now --
//! `init::planned_files(root, dry_run)` -- rather than against the templates.
//!
//! Only the files `init` always overwrites are compared. The seeded documents
//! -- TASKS.md, PROGRESS.md, the context file, evals/README.md -- are
//! deliberately absent: `seed` never overwrites one, so a repository's own
//! record legitimately differs from the template it started as.

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
    .map_err(|e| format!("harness init could not plan this tree: {e}"))?;

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
            "harness init writes this file and the instance does not have it; re-run `harness init`"
        } else if common::read(ctx.root, &path)? != content {
            "the installed copy differs from the source it was built from; re-run `harness init`"
        } else {
            continue;
        };
        found.push(common::finding(&path, 0, message.to_string()));
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}
