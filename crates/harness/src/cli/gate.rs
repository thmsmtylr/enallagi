use crate::config;
use crate::events::{Log, Writer};
use crate::gates::{self, GateCtx};

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum GateWhich {
    Verdict,
    Scope,
}

pub struct Args {
    pub which: GateWhich,
    pub task: String,
    pub base: Option<String>,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    let cfg = config::load(&root)?;
    let mut events = Writer::new(Log::open(&root.join(&cfg.layout.harness_dir)));
    let mut warnings = Vec::new();
    let mut halts = Vec::new();
    let mut dry_rounds = 0;
    let name = match args.which {
        GateWhich::Verdict => "verdict",
        GateWhich::Scope => "scope",
    };
    let outcome = gates::run(
        name,
        &mut GateCtx {
            root: &root,
            cfg: &cfg,
            task: Some(args.task.clone()),
            iter_base: Some(args.base.clone().unwrap_or_else(|| "HEAD~1".to_string())),
            state_base: Some(args.base.clone().unwrap_or_else(|| "HEAD~1".to_string())),
            stage_output: String::new(),
            events: &mut events,
            dry_run: false,
            warnings: &mut warnings,
            halts: &mut halts,
            dry_rounds: &mut dry_rounds,
        },
    );
    println!("{}", outcome.reason);
    for w in &warnings {
        eprintln!("{w}");
    }
    Ok(if outcome.pass { 0 } else { 2 })
}
