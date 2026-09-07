#[allow(dead_code)]
pub struct Args {
    pub gate: Option<String>,
    pub names: Vec<String>,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: eval not implemented");
    Ok(2)
}
