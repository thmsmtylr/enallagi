//! probes: what the tree says about itself.
//!
//! Sixteen probes read the repository and one drives the built artifact. A
//! probe REPORTS; it never gates, and nothing here is wired into a hook. A
//! probe that could not run says ERROR and the scout proposes nothing from it
//! -- a count of zero is a claim about the tree, and a probe that did not run
//! has not made one (LEARNINGS.md, zero-as-pass).
//!
//! Exit 0 when every probe ran, non-zero only when one could not.

pub mod common;

mod check_red;
mod check_unnamed;
mod driver;
mod friction_repeat;
mod hash_uncovered;
mod install_stale;
mod learning_unenforced;
mod learning_ungated;
mod litter;
mod ponytail_ceiling;
mod queue_hygiene;
mod queue_uncovered;
mod rail_unenforced;
mod rejection_stale;
mod skill_ungated;
mod spec_untested;

use crate::config::Config;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub message: String,
}

#[derive(Debug)]
pub enum ProbeResult {
    Count(Vec<Finding>),
    Error(String),
    Off(String),
}

/// The forced check, run once for the whole probe run -- the force variant,
/// because a cached green is a green nobody ran (LEARNINGS.md, 2026-08-27).
/// `ran` is false when the command could not be started, was not found, or the
/// run is nested inside a build cache that would recurse into it.
pub struct CheckOutcome {
    pub ran: bool,
    pub red: bool,
    pub output: String,
}

pub struct ProbeCtx<'a> {
    pub root: &'a Path,
    pub cfg: &'a Config,
    /// `None` makes `run_all` run the check itself.
    pub check: Option<&'a CheckOutcome>,
    /// `HARNESS_DRIVER` is set to `1`.
    pub driver: bool,
}

pub const NAMES: &[&str] = &[
    "spec-untested",
    "queue-uncovered",
    "rail-unenforced",
    "hash-uncovered",
    "check-unnamed",
    "learning-unenforced",
    "learning-ungated",
    "skill-ungated",
    "ponytail-ceiling",
    "rejection-stale",
    "queue-hygiene",
    "friction-repeat",
    "check-red",
    "litter",
    "install-stale",
    "verdict-flip",
    "rejection-repeat",
    "stage-outlier",
    "turns-exhausted",
    "limit-repeat",
    "driver",
];

type ProbeFn = fn(&ProbeCtx) -> ProbeResult;

/// The order the scout reads them in. The five telemetry probes over
/// `events.jsonl` are Task 10's and are not here yet; naming one explicitly
/// reports ERROR rather than a silent zero.
fn registry() -> [(&'static str, ProbeFn); 16] {
    [
        ("spec-untested", spec_untested::probe),
        ("queue-uncovered", queue_uncovered::probe),
        ("rail-unenforced", rail_unenforced::probe),
        ("hash-uncovered", hash_uncovered::probe),
        ("check-unnamed", check_unnamed::probe),
        ("learning-unenforced", learning_unenforced::probe),
        ("learning-ungated", learning_ungated::probe),
        ("skill-ungated", skill_ungated::probe),
        ("ponytail-ceiling", ponytail_ceiling::probe),
        ("rejection-stale", rejection_stale::probe),
        ("queue-hygiene", queue_hygiene::probe),
        ("friction-repeat", friction_repeat::probe),
        ("check-red", check_red::probe),
        ("litter", litter::probe),
        ("install-stale", install_stale::probe),
        ("driver", driver::probe),
    ]
}

pub fn run_all(ctx: &ProbeCtx, names: &[String]) -> Vec<(String, ProbeResult)> {
    let table = registry();
    let mut wanted: Vec<String> = if names.is_empty() {
        table.iter().map(|(name, _)| name.to_string()).collect()
    } else {
        names.to_vec()
    };
    // the driver is the expensive one and the one the scout reads last
    wanted.sort_by_key(|name| u8::from(name == "driver"));

    let computed = match ctx.check {
        Some(_) => None,
        None => Some(force_check(ctx)),
    };
    let ctx = ProbeCtx {
        root: ctx.root,
        cfg: ctx.cfg,
        check: ctx.check.or(computed.as_ref()),
        driver: ctx.driver,
    };

    wanted
        .into_iter()
        .map(|name| {
            let result = match table.iter().find(|(known, _)| *known == name) {
                Some((_, probe)) => catch(*probe, &ctx),
                None => ProbeResult::Error("not implemented".to_string()),
            };
            (name, result)
        })
        .collect()
}

/// One probe that blows up is one ERROR line, not fifteen probes nobody ran.
fn catch(probe: ProbeFn, ctx: &ProbeCtx) -> ProbeResult {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| probe(ctx))) {
        Ok(result) => result,
        Err(_) => ProbeResult::Error("the probe panicked".to_string()),
    }
}

fn force_check(ctx: &ProbeCtx) -> CheckOutcome {
    if std::env::var_os("TURBO_HASH").is_some() {
        return CheckOutcome {
            ran: false,
            red: true,
            output: "nested under turbo, the check would recurse".to_string(),
        };
    }
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&ctx.cfg.check.force)
        .current_dir(ctx.root)
        .output();
    match out {
        Err(e) => CheckOutcome {
            ran: false,
            red: true,
            output: format!("the check could not be run: {e}"),
        },
        Ok(out) => {
            let mut log = String::from_utf8_lossy(&out.stdout).into_owned();
            log.push_str(&String::from_utf8_lossy(&out.stderr));
            if out.status.code() == Some(127) {
                return CheckOutcome {
                    ran: false,
                    red: true,
                    output: format!(
                        "the check could not be run: {}",
                        common::cut(log.trim(), 120)
                    ),
                };
            }
            CheckOutcome {
                ran: true,
                red: !out.status.success(),
                output: log,
            }
        }
    }
}

pub fn render(results: &[(String, ProbeResult)]) -> String {
    let mut out = String::new();
    for (name, result) in results {
        match result {
            ProbeResult::Count(found) => {
                out.push_str(&format!("PROBE {name} {}\n", found.len()));
                for f in found {
                    out.push_str(&format!(
                        "FINDING {name} {}:{} {}\n",
                        f.path,
                        f.line,
                        f.message.replace('\n', " ")
                    ));
                }
            }
            ProbeResult::Error(reason) => {
                out.push_str(&format!(
                    "PROBE {name} ERROR {}\n",
                    reason.replace('\n', " ")
                ));
            }
            ProbeResult::Off(message) => {
                out.push_str(&format!("PROBE {name} OFF -- {message}\n"));
            }
        }
    }
    out
}
