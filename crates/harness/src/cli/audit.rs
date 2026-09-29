//! `enallagi audit` -- prints each learning it proposed, each it merged into a standing entry, and each it refused with the reason.

use crate::{audit, config, git};

pub fn run() -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => std::path::PathBuf::from(top),
        Err(_) => cwd,
    };
    let cfg = config::load(&root)?;
    let report = match audit::run(&root, &cfg) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("{err}");
            return Ok(1);
        }
    };
    for proposal in &report.proposed {
        println!(
            "audit: proposed `{}` from {} instances",
            proposal.class,
            proposal.instances.len()
        );
    }
    for line in report.merged.iter().chain(&report.refused) {
        println!("audit: {line}");
    }
    if report.proposed.is_empty() && report.merged.is_empty() {
        println!("audit: nothing written");
    } else {
        println!("audit: wrote {}", report.decisions);
    }
    Ok(0)
}
