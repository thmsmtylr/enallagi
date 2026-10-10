//! `enallagi audit` -- prints each learning it proposed, each it merged into a standing entry, and each it refused with the reason. With `--harness`, prints each rule two installs earned as a `[seed]` line.

use crate::{audit, config, git};

pub fn run(harness: &[std::path::PathBuf]) -> anyhow::Result<i32> {
    if !harness.is_empty() {
        return shared(harness);
    }
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

fn shared(dirs: &[std::path::PathBuf]) -> anyhow::Result<i32> {
    if dirs.len() < 2 {
        eprintln!("enallagi audit: --harness needs two or more install directories");
        return Ok(1);
    }
    let shared = match audit::shared(dirs) {
        Ok(shared) => shared,
        Err(err) => {
            eprintln!("{err}");
            return Ok(1);
        }
    };
    if shared.is_empty() {
        println!("audit: no rule earned in two installs");
    }
    for rule in &shared {
        println!(
            "audit: `{}` earned in {} installs",
            rule.class,
            rule.earned.len()
        );
        for (path, line, first) in &rule.earned {
            println!("  {}:{line} {first}", path.display());
        }
        println!("{}\n", rule.seed);
    }
    Ok(0)
}
