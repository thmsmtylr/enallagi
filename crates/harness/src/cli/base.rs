//! `enallagi base <task>` -- prints the commit a task's work starts from, and nothing at all when there is none.

use crate::{config, git};

pub struct Args {
    pub task: String,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    match git::task_base(&root, &config::harness_dir(&root), &args.task) {
        Ok(sha) => {
            if !sha.is_empty() {
                println!("{sha}");
            }
            Ok(0)
        }
        Err(err) => {
            eprintln!("enallagi base: {err}");
            Ok(1)
        }
    }
}
