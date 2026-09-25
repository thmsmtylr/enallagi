use std::path::PathBuf;

use crate::eject::{self, EjectOpts};

pub struct Args {
    pub dry_run: bool,
    pub keep_record: Option<PathBuf>,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match crate::git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => PathBuf::from(top),
        Err(_) => cwd.clone(),
    };
    let opts = EjectOpts {
        dry_run: args.dry_run,
        keep_record: args.keep_record.as_ref().map(|dir| cwd.join(dir)),
    };
    let dir = crate::config::harness_dir(&root);
    let report = match eject::eject(&root, &opts) {
        Ok(report) => report,
        Err(eject::EjectError::Refused(why)) => {
            eprintln!("{}", eject::EjectError::Refused(why));
            return Ok(1);
        }
        Err(e) => return Err(e.into()),
    };

    let verb = if args.dry_run {
        "would remove"
    } else {
        "removed"
    };
    for path in &report.removed {
        match &opts.keep_record {
            Some(record) if *path == dir => {
                let moved = if args.dry_run { "would move" } else { "moved" };
                println!("  {moved}: {path} -> {}", record.display());
            }
            _ => println!("  {verb}: {path}"),
        }
    }
    for path in &report.kept {
        println!("  kept: {path} (tracked by the product)");
    }
    if let Some(exclude) = &report.exclude {
        println!("  {verb}: the harness block in {exclude}");
    }
    Ok(0)
}
