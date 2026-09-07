//! The forced check, reported rather than gated. A check that could not run is
//! ERROR, never a count of zero.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let Some(check) = ctx.check else {
        return Err("the check was not run".to_string());
    };
    if !check.ran {
        return Err(check.output.trim().to_string());
    }
    if !check.red {
        return Ok(vec![]);
    }
    let log = &check.output;
    let first = common::re(r"(?m)^\s*(\S+#\S+):\s+ERROR")?
        .captures(log)
        .or(common::re(r"(?m)^\s*Failed:\s*(\S+)")?.captures(log))
        .and_then(|m| m.get(1).map(|g| g.as_str().to_string()));
    let named = common::re(&format!("(?m){}", ctx.cfg.check.fail_name))?
        .captures(log)
        .and_then(|m| m.get(1).map(|g| g.as_str().to_string()));

    let path = ctx
        .cfg
        .layout
        .harness_files
        .first()
        .cloned()
        .unwrap_or_else(|| "check".to_string());
    Ok(vec![common::finding(
        path,
        0,
        format!(
            "the check exited non-zero, first failing task {}{}",
            first.unwrap_or_else(|| "unnamed".to_string()),
            named
                .map(|n| format!(", first failure {n}"))
                .unwrap_or_default()
        ),
    )])
}
