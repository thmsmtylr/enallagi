#[allow(dead_code)]
pub struct Args {
    pub name: String,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: hook not implemented");
    Ok(2)
}
