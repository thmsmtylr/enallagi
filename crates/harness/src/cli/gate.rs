#[derive(clap::ValueEnum, Clone, Debug)]
pub enum GateWhich {
    Verdict,
    Scope,
}

#[allow(dead_code)]
pub struct Args {
    pub which: GateWhich,
    pub task: String,
    pub base: Option<String>,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: gate not implemented");
    Ok(2)
}
