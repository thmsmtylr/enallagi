//! `enallagi audit` -- prints each learning it proposed and each recurring class a kill or an acceptance already answers.

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
    for settled in &report.settled {
        println!("audit: {settled}, nothing proposed");
    }
    if report.proposed.is_empty() {
        println!("audit: nothing written");
    } else {
        println!("audit: wrote {}", report.decisions);
    }
    Ok(0)
}
