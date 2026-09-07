#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SkillsCmd {
    Check,
    Sync,
    List,
}

#[allow(dead_code)]
pub struct Args {
    pub cmd: SkillsCmd,
    pub frozen: bool,
}

pub fn run(_args: &Args) -> anyhow::Result<i32> {
    eprintln!("harness: skills not implemented");
    Ok(2)
}
