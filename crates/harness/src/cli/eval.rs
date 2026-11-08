//! `enallagi eval [names]` -- the suite or one gate's cases; exit 1 on a failure, 2 when it cannot run

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
