#[allow(dead_code)]
pub struct Args {
    pub role: Option<String>,
    pub task: Option<String>,
    pub since: Option<String>,
    pub json: bool,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: events not implemented");
    Ok(2)
}
