use crate::probes::{self, ProbeCtx, ProbeResult};
use crate::{config, git};

pub struct Args {
    pub names: Vec<String>,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => std::path::PathBuf::from(top),
        Err(_) => cwd,
    };
    let cfg = config::load(&root)?;
    let ctx = ProbeCtx {
        root: &root,
        cfg: &cfg,
        check: None,
        driver: std::env::var("ENALLAGI_DRIVER").as_deref() == Ok("1"),
    };
    let results = probes::run_all(&ctx, &args.names);
    print!("{}", probes::render(&results));
    let errored = results
        .iter()
        .any(|(_, r)| matches!(r, ProbeResult::Error(_)));
    Ok(if errored { 2 } else { 0 })
}
