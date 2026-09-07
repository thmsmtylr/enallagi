//! cli: argument parsing (clap derive) and dispatch to per-subcommand modules.

mod eval;
mod events;
mod gate;
mod hook;
mod init;
mod probe;
mod run;
mod skills;
mod tasks;
mod watch;
mod worktree;

pub use gate::GateWhich;
pub use skills::SkillsCmd;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "harness",
    about = "An autonomous task loop for a coding agent, installed into any git repository."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Install the harness into the current repository.
    Init {
        #[arg(long)]
        adapter: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Run the task loop.
    Run {
        n: Option<u32>,
        #[arg(long)]
        no_tui: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        frozen: bool,
    },
    /// Attach to a running loop's live view.
    Watch,
    /// Run probes over the repository.
    Probe { names: Vec<String> },
    /// Evaluate a gate against a task.
    Gate {
        which: GateWhich,
        task: String,
        #[arg(long)]
        base: Option<String>,
    },
    /// Run a hook by name, reading its input from stdin.
    Hook { name: String },
    /// Manage skill locks.
    Skills {
        cmd: SkillsCmd,
        #[arg(long)]
        frozen: bool,
    },
    /// Operate on the task queue.
    Tasks { cmd: String, args: Vec<String> },
    /// Run eval packages.
    Eval {
        #[arg(long)]
        gate: Option<String>,
        names: Vec<String>,
    },
    /// Inspect the event log.
    Events {
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Manage worktrees.
    Worktree { n: Option<u32> },
}

pub fn run(cli: Cli) -> anyhow::Result<i32> {
    match cli.command {
        Command::Init { adapter, dry_run } => init::run(&init::Args { adapter, dry_run }),
        Command::Run {
            n,
            no_tui,
            dry_run,
            frozen,
        } => run::run(&run::Args {
            n,
            no_tui,
            dry_run,
            frozen,
        }),
        Command::Watch => watch::run(),
        Command::Probe { names } => probe::run(&probe::Args { names }),
        Command::Gate { which, task, base } => gate::run(&gate::Args { which, task, base }),
        Command::Hook { name } => hook::run(&hook::Args { name }),
        Command::Skills { cmd, frozen } => skills::run(&skills::Args { cmd, frozen }),
        Command::Tasks { cmd, args } => tasks::run(&tasks::Args { cmd, args }),
        Command::Eval { gate, names } => eval::run(&eval::Args { gate, names }),
        Command::Events {
            role,
            task,
            since,
            json,
        } => events::run(&events::Args {
            role,
            task,
            since,
            json,
        }),
        Command::Worktree { n } => worktree::run(&worktree::Args { n }),
    }
}
