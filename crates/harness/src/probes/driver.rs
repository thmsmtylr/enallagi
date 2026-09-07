//! The one probe that does not read text: it drives the built artifact through
//! the surface a user touches and prints what fell short. The other fifteen
//! read the repo, so the floor and the direction signal are the same instrument
//! and capability shortfall is invisible to them.
//!
//! Off unless `layout.driver_command` is set AND `HARNESS_DRIVER` is `1`,
//! because it costs wall-clock on every scout round and has to earn it.
//!
//! Contract: the command exits 0 when it REACHED the artifact, whatever it
//! found there, and prints one line per shortfall beginning `FINDING `. A
//! non-zero exit means it could not reach the artifact at all -- that is `PROBE
//! driver ERROR`, and nothing is proposed from a probe that did not run.
//!
//! It gets a throwaway working directory and a stripped environment: if the
//! thing you drive is itself an agent, that is what stops it inheriting this
//! loop's context, settings and tools. A relative path in `driver_command`
//! therefore cannot work; `$HARNESS_ROOT` is exported for it to resolve
//! against. Watch the persistent effect, not the answer -- diff the store, the
//! file, the row it was supposed to change.

use super::{common, ProbeCtx, ProbeResult};
use std::process::Command;

pub const OFF: &str = "no driverCommand in harness.toml, or HARNESS_DRIVER is unset. Nothing here exercised the built artifact.";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    let command = &ctx.cfg.layout.driver_command;
    if command.is_empty() || !ctx.driver {
        // not a count of zero: a probe that did not run has found nothing,
        // which is no evidence about the tree (LEARNINGS.md, zero-as-pass).
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
