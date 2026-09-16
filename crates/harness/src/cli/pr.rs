use crate::pr::{self, PrError, PrOpts};

pub struct Args {
    pub tasks: Vec<String>,
    pub push: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    let opts = PrOpts { push: args.push };
    let report = match pr::build(&root, &args.tasks, &opts) {
        Ok(report) => report,
        Err(
            err @ (PrError::Refused(_)
            | PrError::Conflict { .. }
            | PrError::Check(_)
            | PrError::Gh(_)),
        ) => {
            eprintln!("{err}");
            return Ok(1);
        }
        Err(err) => return Err(err.into()),
    };
    println!("  branch: {}", report.branch);
    println!("  description: {}", report.description.display());
    if let Some(opened) = &report.opened {
        println!("  pushed: {opened}");
    }
    Ok(0)
}
