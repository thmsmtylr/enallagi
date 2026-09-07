#[allow(dead_code)]
pub struct Args {
    pub names: Vec<String>,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: probe not implemented");
    Ok(2)
}
