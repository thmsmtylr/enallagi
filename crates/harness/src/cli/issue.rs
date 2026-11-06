//! `enallagi issue <ref>` -- appends the block one GitHub issue becomes, or prints it and writes nothing under --dry-run.

use crate::config;
use crate::issue::{self, IssueError};
use crate::tracker::{self, Tracker};

pub struct Args {
    pub reference: String,
    pub dry_run: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    let dir = config::harness_dir(&root);
    let t = Tracker::open(&root, &dir);
    let decisions = std::fs::read_to_string(config::instance_path(&root, &dir, "DECISIONS.md"))
        .unwrap_or_default();
    let appended = issue::read(&args.reference)
        .and_then(|found| issue::append(&t.blocks()?, &decisions, &found));
    let block = match appended {
        Ok(appended) => appended,
        Err(err @ (IssueError::BadRef(_) | IssueError::Gh { .. } | IssueError::Queued { .. })) => {
            eprintln!("{err}");
            return Ok(1);
        }
        Err(err) => return Err(err.into()),
    };
    if args.dry_run {
        println!("{block}");
        return Ok(0);
    }
    let path = t.append(&tracker::repo_slug(&root), &block)?;
    let id = block.lines().next().unwrap_or_default();
    println!("  proposed: {id}");
    println!("  queue: {}", path.display());
    Ok(0)
}
