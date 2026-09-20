//! `enallagi pr <tasks>` -- builds the branch and the description, and prints a refusal or a conflict instead of raising it.

use crate::pr::{self, PrError, PrOpts};

pub struct Args {
    pub tasks: Vec<String>,
    pub push: bool,
    pub policy_read: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    let opts = PrOpts {
        push: args.push,
        policy_read: args.policy_read,
        stack_on: Vec::new(),
    };
    let report = match pr::build(&root, &args.tasks, &opts) {
        Ok(report) => report,
        Err(
            err @ (PrError::Refused(_)
            | PrError::Conflict { .. }
            | PrError::Check(_)
            | PrError::Policy { .. }
            | PrError::Gh(_)),
        ) => {
            eprintln!("{err}");
            return Ok(1);
        }
        Err(err) => return Err(err.into()),
    };
    println!("  branch: {}", report.branch);
    println!("  description: {}", report.description.display());
    for f in &report.policy {
        println!("  contribution policy: {}", pr::cite(f));
    }
    if let Some(opened) = &report.opened {
        println!("  pushed: {opened}");
    }
    Ok(0)
}
