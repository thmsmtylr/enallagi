#[allow(dead_code)]
pub struct Args {
    pub n: Option<u32>,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: worktree not implemented");
    Ok(2)
}
