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
    /// Install the harness into this repository: seed the documents, write harness.toml, wire the adapter
    Init {
        /// Agent preset whose hook and instruction files to write (claude, codex, gemini, copilot, cursor, qwen, …)
        #[arg(long)]
        adapter: Option<String>,
        /// Print what would be written and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Run the pipelines: a takeable task is implemented then verified; a task at review is verified; an empty queue runs the scout and adjudicator
    Run {
        /// Iterations to run; a run also ends after two discovery rounds that leave nothing takeable
        #[arg(short = 'n', long = "iterations", default_value_t = 3)]
        iterations: u32,
        /// Dollar budget for the run; overrides BUDGET_USD when given
        #[arg(long = "budget-usd")]
        budget_usd: Option<f64>,
        /// Wall-clock budget in seconds for the run; overrides BUDGET_SECONDS when given
        #[arg(long = "budget-seconds")]
        budget_seconds: Option<u64>,
        /// Token budget for the run; overrides BUDGET_TOKENS when given
        #[arg(long = "budget-tokens")]
        budget_tokens: Option<u64>,
        /// Run without the live view, even when stdout is a tty
        #[arg(long)]
        no_tui: bool,
        /// Print the planned pipeline and probe output; spawn no stage
        #[arg(long)]
        dry_run: bool,
        /// Refuse a stage whose skills are not already vendored and locked; never fetch
        #[arg(long)]
        frozen: bool,
    },
    /// Attach read-only to a running loop's event log and queue
    Watch,
    /// Run the probes and print PROBE and FINDING lines; exit 2 if any probe could not run
    Probe {
        /// Probes to run; empty runs all of them
        names: Vec<String>,
    },
    /// Run one gate against a task; exit 0 on pass, 2 on fail
    Gate {
        /// Which gate to run
        which: GateWhich,
        /// Task id the gate judges
        task: String,
        /// Commit-ish the task's diff is measured against (default HEAD~1)
        #[arg(long)]
        base: Option<String>,
    },
    /// Agent lifecycle hook entry point; reads the tool's JSON on stdin, exits 2 to refuse
    Hook {
        /// Hook to run (immutable, one-writer, verify-done, skills)
        name: String,
    },
    /// Resolve declared skills: check (frozen), sync (fetch), list
    Skills {
        /// Which operation to perform
        cmd: SkillsCmd,
        /// Refuse a stage whose skills are not already vendored and locked; never fetch
        #[arg(long)]
        frozen: bool,
    },
    /// Query or edit TASKS.md: list, ready, ids-at, block, field, set-status, unblock, rejections
    Tasks {
        /// Subcommand to run
        cmd: String,
        /// Arguments for the subcommand
        args: Vec<String>,
    },
    /// Run evals; --gate runs the three-condition admission check for a candidate rule
    Eval {
        /// Rule name to run the admission gate against, instead of a plain eval run
        #[arg(long)]
        gate: Option<String>,
        /// Eval packages to run; empty runs all of them
        names: Vec<String>,
    },
    /// Query the event log
    Events {
        /// Keep only events for this role, with their surrounding stage
        #[arg(long)]
        role: Option<String>,
        /// Keep only events for this task id
        #[arg(long)]
        task: Option<String>,
        /// Keep only events at or after this timestamp
        #[arg(long)]
        since: Option<String>,
        /// Print one JSON object per line instead of the rendered form
        #[arg(long)]
        json: bool,
    },
    /// Run one lane in its own git worktree and fast-forward the branch
    Worktree {
        /// Iterations the lane's run performs
        n: Option<u32>,
    },
}

pub fn run(cli: Cli) -> anyhow::Result<i32> {
    match cli.command {
        Command::Init { adapter, dry_run } => init::run(&init::Args { adapter, dry_run }),
        Command::Run {
            iterations,
            budget_usd,
            budget_seconds,
            budget_tokens,
            no_tui,
            dry_run,
            frozen,
        } => run::run(&run::Args {
            iterations,
            budget_usd,
            budget_seconds,
            budget_tokens,
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
