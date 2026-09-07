#[allow(dead_code)]
pub struct Args {
    pub cmd: String,
    pub args: Vec<String>,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: tasks not implemented");
    Ok(2)
}
