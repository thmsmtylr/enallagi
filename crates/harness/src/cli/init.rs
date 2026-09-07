#[allow(dead_code)]
pub struct Args {
    pub adapter: Option<String>,
    pub dry_run: bool,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: init not implemented");
    Ok(2)
}
