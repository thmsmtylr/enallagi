#[allow(dead_code)]
pub struct Args {
    pub n: Option<u32>,
    pub no_tui: bool,
    pub dry_run: bool,
    pub frozen: bool,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: run not implemented");
    Ok(2)
}
