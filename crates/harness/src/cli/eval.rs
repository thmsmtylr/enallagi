//! `enallagi eval [names]` -- runs the suite, or one gate's cases, exiting 1 on a failure and 2 when it could not run.

use crate::eval;

pub struct Args {
    pub gate: Option<String>,
    pub names: Vec<String>,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = super::root()?;

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
