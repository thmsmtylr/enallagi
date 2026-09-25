//! `enallagi review <ref>` -- appends one block per open review thread, and exits 1 when more than a page went unread.

use crate::config;
use crate::queue::Queue;
use crate::review::{self, ReviewError};

pub struct Args {
    pub reference: String,
    pub dry_run: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    let dir = config::harness_dir(&root);
    let path = config::instance_path(&root, &dir, "TASKS.md");
    let queue = Queue { path };
    let decisions = std::fs::read_to_string(config::instance_path(&root, &dir, "DECISIONS.md"))
        .unwrap_or_default();
    let found = match review::read(&args.reference) {
        Ok(found) => found,
        Err(err @ (ReviewError::BadRef(_) | ReviewError::Gh { .. })) => {
            eprintln!("{err}");
            return Ok(1);
        }
        Err(err) => return Err(err.into()),
    };
    let appended = review::append(&queue.read()?, &decisions, &found)?;
    for block in &appended.blocks {
        if args.dry_run {
            println!("{block}\n");
        } else {
            println!("  proposed: {}", block.lines().next().unwrap_or_default());
        }
    }
    for (task, url) in &appended.carried {
        println!("  carried: {task} {url}");
    }
    println!("  skipped: {} with no path", found.unanchored);
    println!("  skipped: {} resolved or outdated", found.settled);
    println!("  folded: {} later in a thread", found.folded);
    // what was read is kept; the exit code is what says the rest was never read
    if !args.dry_run {
        queue.write(&appended.queue)?;
        println!("  queue: {}", queue.path.display());
    }
    if found.short {
        eprintln!(
            "enallagi review: more than one page of reviews, threads or comments, and only the first 100 of each were read"
        );
        return Ok(1);
    }
    Ok(0)
}
