//! `enallagi eval [names]` -- runs the suite, or one gate's cases, exiting 1 on a failure and 2 when it could not run.

use crate::{eval, git};
use std::path::PathBuf;

pub struct Args {
    pub gate: Option<String>,
    pub names: Vec<String>,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => PathBuf::from(top),
        Err(_) => cwd,
    };

    let outcome = match &args.gate {
        Some(name) => eval::gate(&root, name, None),
        None => eval::run(&root, &args.names, None),
    };

    Ok(match outcome {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => 2,
    })
}
