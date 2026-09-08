//! The one probe that drives the built artifact instead of reading text. Off unless `layout.driver_command` is set and `HARNESS_DRIVER=1`.

use super::{common, ProbeCtx, ProbeResult};
use std::process::Command;

pub const OFF: &str = "no driver_command in harness.toml, or HARNESS_DRIVER is unset. Nothing here exercised the built artifact.";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    let command = &ctx.cfg.layout.driver_command;
    if command.is_empty() || !ctx.driver {
        // not Count(0): a probe that did not run is no evidence about the tree
        return ProbeResult::Off(OFF.to_string());
    }
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => return ProbeResult::Error(format!("no working directory for the driver: {e}")),
    };
    let mut child = Command::new("sh");
    child.arg("-c").arg(command).current_dir(dir.path());
    child.env_clear();
    for key in ["PATH", "HOME"] {
        if let Some(value) = std::env::var_os(key) {
            child.env(key, value);
        }
    }
    child.env("HARNESS_ROOT", ctx.root);

    let out = match child.output() {
        Ok(out) => out,
        Err(e) => return ProbeResult::Error(format!("the driver could not be started: {e}")),
    };
    let mut log = String::from_utf8_lossy(&out.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        let code = out
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "on a signal".to_string());
        return ProbeResult::Error(format!(
            "the driver exited {code} without reaching the artifact: {}",
            common::cut(&log.trim().replace('\n', " "), 200)
        ));
    }
    ProbeResult::Count(
        log.split('\n')
            .filter_map(|line| line.strip_prefix("FINDING "))
            .map(|line| common::finding(command, 0, line.trim()))
            .collect(),
    )
}
