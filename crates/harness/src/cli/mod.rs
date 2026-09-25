//! The parser and the dispatcher: one clap `Command` variant per subcommand, and `run` hands each to the sibling module of the same name.

mod base;
mod eject;
mod eval;
mod events;
mod gate;
mod hook;
mod init;
mod issue;
mod pr;
mod probe;
mod review;
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
    name = "enallagi",
    version,
    disable_help_subcommand = true,
    about = "An autonomous task loop for a coding agent, installed into any git repository."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Install into this repository in one pass
    ///
    /// Detects the check, asks for what it cannot decide, writes enallagi.toml and every rendered
    /// file, wires the configured agent's hooks, vendors the skills, commits the state, and probes.
    Init {
        /// Agent preset whose hook and instruction files to write; defaults to agent.preset
        #[arg(long)]
        adapter: Option<String>,
        /// Print what would be written and write nothing
        #[arg(long)]
        dry_run: bool,
        /// Move instance files at the repository root, or in a legacy .harness/, into the harness directory, and install nothing
        #[arg(long = "move")]
        move_files: bool,
        /// Rewrite enallagi.toml without the keys that equal their default
        #[arg(long)]
        prune_defaults: bool,
        /// Take every default without asking; a stdin that is not a terminal does the same
        #[arg(long, short = 'y')]
        yes: bool,
        /// Answer agent.preset without being asked
        #[arg(long)]
        preset: Option<String>,
        /// Answer check.command without being asked
        #[arg(long)]
        check: Option<String>,
        /// Answer check.fail_name without being asked
        #[arg(long)]
        fail_name: Option<String>,
        /// Answer layout.source_root without being asked
        #[arg(long)]
        source_root: Option<String>,
        /// Vendor the declared skills even when stdin is not a terminal
        #[arg(long, conflicts_with = "frozen")]
        sync: bool,
        /// Vendor nothing, as CI does
        #[arg(long)]
        frozen: bool,
        /// Append this GitHub issue as a proposed block once installed
        #[arg(long, value_name = "URL")]
        issue: Option<String>,
    },
    /// Remove from this repository and leave no trace
    ///
    /// Removes the untracked entry points init wrote and the exclude block, and moves the harness
    /// directory, the run's record, under $XDG_DATA_HOME/enallagi/ejected.
    Eject {
        /// Print what would be removed and remove nothing
        #[arg(long)]
        dry_run: bool,
        /// Move the harness directory to this path outside the repository instead of the data directory
        #[arg(long, conflicts_with = "delete")]
        keep_record: Option<std::path::PathBuf>,
        /// Delete the harness directory instead of keeping it; the record is gone with it
        #[arg(long)]
        delete: bool,
    },
    /// Run the pipelines in this checkout
    ///
    /// A takeable task is implemented then verified. A task at review is verified. An empty queue
    /// runs the scout and the adjudicator.
    Run {
        /// Iterations to run; a run also ends after two discovery rounds that leave nothing takeable
        #[arg(short = 'n', long = "iterations", default_value_t = 3)]
        iterations: u32,
        /// Pipeline from enallagi.toml to run, repeatable; none given runs whichever one's `when` holds
        #[arg(long = "pipeline", value_name = "NAME")]
        pipeline: Vec<String>,
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
        /// Add the agent preset's bypass flag to every lane, as `[agent] dangerously_skip_permissions` does
        #[arg(long)]
        dangerously_skip_permissions: bool,
        /// Push each landed task to its own branch and open its pull request, as `[pr] per_task` does
        #[arg(long)]
        pr_per_task: bool,
    },
    /// Attach read-only to a running loop's event log and queue
    Watch,
    /// Run the probes and print PROBE and FINDING lines
    ///
    /// Exits 2 if any probe could not run.
    Probe {
        /// Probes to run; empty runs all of them
        names: Vec<String>,
    },
    /// Build a pull-request branch for the named tasks
    ///
    /// Branches off the upstream default branch and carries only the product commits whose subject
    /// names one of the tasks.
    Pr {
        /// Done task ids; several build one branch, for tasks that cannot land apart
        #[arg(required = true)]
        tasks: Vec<String>,
        /// Push the branch and open the pull request with gh when it is installed; never merges
        #[arg(long)]
        push: bool,
        /// Push past a contribution guide that conditions generated changes, once you have read it
        #[arg(long)]
        policy_read: bool,
    },
    /// Append a proposed block to TASKS.md from a GitHub issue
    ///
    /// Reads the issue with gh. Scope and criteria stay placeholders until you write them.
    Issue {
        /// Issue URL, or owner/repo#n
        reference: String,
        /// Print the block and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Append a proposed block for each open comment on a pull request review
    ///
    /// Reads the comments with gh. A comment with no file anchor is skipped, and so is a resolved
    /// or outdated one. Each block takes its scope from the comment's path.
    Review {
        /// Pull request URL, or owner/repo#n
        reference: String,
        /// Print the blocks and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the product commit a task was queued against
    ///
    /// That commit is the base a gate measures the task's diff from.
    Base {
        /// Task id whose base to print
        task: String,
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
    /// Agent lifecycle hook entry point
    ///
    /// Reads the tool's JSON on stdin and exits 2 to refuse.
    Hook {
        /// Hook to run (immutable, one-writer, verify-done, skills)
        name: String,
    },
    /// Resolve the declared skills: check, sync, list
    ///
    /// check verifies the vendored content against harness.lock and never fetches. sync fetches.
    /// list prints each declared skill with its source and its locked commit or hash.
    Skills {
        /// Which operation to perform
        cmd: SkillsCmd,
        // `check` is this flag: `sync --frozen` and `check` take the same arm. Hidden here, and
        // accepted for one release so a caller written against the old spelling still runs.
        #[arg(long, hide = true)]
        frozen: bool,
    },
    /// Query or edit TASKS.md
    ///
    /// Subcommands: list, ready, ready-unattended, ids-at, block, field, set-status, unblock,
    /// rejections, archive.
    Tasks {
        /// Subcommand to run
        cmd: String,
        /// Arguments for the subcommand
        args: Vec<String>,
    },
    /// Run the evals
    ///
    /// --gate runs the three-condition admission check for a candidate rule instead.
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
        Command::Init {
            adapter,
            dry_run,
            move_files,
            prune_defaults,
            yes,
            preset,
            check,
            fail_name,
            source_root,
            sync,
            frozen,
            issue,
        } => init::run(&init::Args {
            adapter,
            dry_run,
            move_files,
            prune_defaults,
            yes,
            answers: crate::init::Answers {
                preset,
                check,
                fail_name,
                source_root,
            },
            sync,
            frozen,
            issue,
        }),
        Command::Eject {
            dry_run,
            keep_record,
            delete,
        } => eject::run(&eject::Args {
            dry_run,
            keep_record,
            delete,
        }),
        Command::Run {
            iterations,
            pipeline,
            budget_usd,
            budget_seconds,
            budget_tokens,
            no_tui,
            dry_run,
            frozen,
            dangerously_skip_permissions,
            pr_per_task,
        } => run::run(&run::Args {
            iterations,
            pipelines: pipeline,
            budget_usd,
            budget_seconds,
            budget_tokens,
            no_tui,
            dry_run,
            frozen,
            dangerously_skip_permissions,
            pr_per_task,
        }),
        Command::Watch => watch::run(),
        Command::Probe { names } => probe::run(&probe::Args { names }),
        Command::Pr {
            tasks,
            push,
            policy_read,
        } => pr::run(&pr::Args {
            tasks,
            push,
            policy_read,
        }),
        Command::Issue { reference, dry_run } => issue::run(&issue::Args { reference, dry_run }),
        Command::Review { reference, dry_run } => review::run(&review::Args { reference, dry_run }),
        Command::Base { task } => base::run(&base::Args { task }),
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    // include_str!, not a runtime read: a document a test reads is a declared cache input
    const README: &str = include_str!("../../../../README.md");
    const CI: &str = include_str!("../../../../.github/workflows/ci.yml");
    const RELEASE: &str = include_str!("../../../../.github/workflows/release.yml");

    const SUBCOMMANDS: [&str; 16] = [
        "init", "eject", "run", "watch", "probe", "pr", "issue", "review", "base", "gate", "hook",
        "skills", "tasks", "eval", "events", "worktree",
    ];

    const FLAGS: [&str; 35] = [
        "init --adapter",
        "init --dry-run",
        "init --move",
        "init --prune-defaults",
        "init --yes",
        "init --preset",
        "init --check",
        "init --fail-name",
        "init --source-root",
        "init --sync",
        "init --frozen",
        "init --issue",
        "eject --dry-run",
        "eject --keep-record",
        "eject --delete",
        "run --iterations",
        "run --pipeline",
        "run --budget-usd",
        "run --budget-seconds",
        "run --budget-tokens",
        "run --no-tui",
        "run --dry-run",
        "run --frozen",
        "run --dangerously-skip-permissions",
        "run --pr-per-task",
        "pr --push",
        "pr --policy-read",
        "issue --dry-run",
        "review --dry-run",
        "gate --base",
        "eval --gate",
        "events --role",
        "events --task",
        "events --since",
        "events --json",
    ];

    fn visible_flags() -> Vec<String> {
        let cmd = Cli::command();
        let mut found = Vec::new();
        for sub in cmd.get_subcommands() {
            for arg in sub.get_arguments() {
                let Some(long) = arg.get_long() else { continue };
                if arg.is_hide_set() || long == "help" || long == "version" {
                    continue;
                }
                found.push(format!("{} --{long}", sub.get_name()));
            }
        }
        found
    }

    // the job id, then only the step-level `name:` and `run:` lines under its `steps:`. The job's
    // own `name:`, a `uses:` path, a `with:` value and a comment are all left out, so nothing but
    // what the job runs can stand in for the id. Step keys are the two indents a step writes them
    // at: `      - ` for the first key of a step and `        ` for the rest.
    fn job_steps(workflow: &str) -> Vec<(String, String)> {
        let mut jobs = Vec::new();
        let mut in_jobs = false;
        let mut in_steps = false;
        for line in workflow.lines() {
            if line == "jobs:" {
                in_jobs = true;
            } else if !line.starts_with(' ') && !line.trim().is_empty() {
                in_jobs = false;
            } else if in_jobs {
                if let Some(id) = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':')) {
                    if !id.starts_with(' ') && !id.starts_with('#') {
                        jobs.push((id.to_string(), String::new()));
                        in_steps = false;
                        continue;
                    }
                }
                if line.starts_with("    steps:") {
                    in_steps = true;
                    continue;
                }
                let key = line
                    .strip_prefix("      - ")
                    .or_else(|| line.strip_prefix("        "));
                if let (true, Some(key), Some((_, steps))) = (in_steps, key, jobs.last_mut()) {
                    if key.starts_with("name:") || key.starts_with("run:") {
                        steps.push_str(&key.to_lowercase());
                        steps.push('\n');
                    }
                }
            }
        }
        jobs
    }

    // `grep -w`'s boundary: `rust` inside `rustfmt` is not the word, `build` in `cross-build` is.
    fn says_word(steps: &str, word: &str) -> bool {
        steps
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|token| token == word)
    }

    #[test]
    fn a_word_inside_a_longer_token_is_not_said() {
        let workflow = "\
jobs:
  shell:
    runs-on: ubuntu-latest
    steps:
      - name: shellcheck is on the runner
        run: shellcheck --version
";
        let jobs = job_steps(workflow);
        assert!(
            !says_word(&jobs[0].1, "shell"),
            "`shell` passed on `shellcheck`"
        );
        assert!(
            says_word("cross-build linker\n", "build"),
            "a hyphen is a boundary"
        );
    }

    fn subcommand_names() -> Vec<String> {
        Cli::command()
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .collect()
    }

    #[test]
    fn every_subcommand_is_in_the_reviewed_set() {
        assert_eq!(subcommand_names(), SUBCOMMANDS);
    }

    #[test]
    fn every_subcommand_is_named_in_the_readme() {
        let listed = README.split("## Commands").nth(1).expect("## Commands");
        for name in subcommand_names() {
            assert!(listed.contains(&format!("`{name}`")), "README omits {name}");
        }
    }

    #[test]
    fn no_two_subcommands_state_one_job() {
        let mut jobs: Vec<String> = Cli::command()
            .get_subcommands()
            .map(|s| s.get_about().expect("about").to_string())
            .collect();
        jobs.sort();
        let before = jobs.len();
        jobs.dedup();
        assert_eq!(before, jobs.len(), "two subcommands state one job");
    }

    #[test]
    fn every_flag_is_in_the_reviewed_vocabulary() {
        assert_eq!(visible_flags(), FLAGS);
    }

    #[test]
    fn run_takes_the_pipeline_flag_more_than_once() {
        let cli = Cli::try_parse_from([
            "enallagi",
            "run",
            "--pipeline",
            "task",
            "--pipeline",
            "review",
        ])
        .expect("parse");
        let Command::Run { pipeline, .. } = cli.command else {
            panic!("{:?}", cli.command)
        };
        assert_eq!(pipeline, ["task", "review"]);
    }

    #[test]
    fn no_help_line_runs_past_one_hundred_columns() {
        let help = Cli::command().render_help().to_string();
        let wide: Vec<&str> = help.lines().filter(|l| l.chars().count() > 100).collect();
        assert!(wide.is_empty(), "past 100 columns: {wide:#?}");
        let lines = help.lines().count();
        assert!(lines <= 25, "--help is {lines} lines");
    }

    #[test]
    fn a_uses_line_does_not_say_what_a_job_runs() {
        let workflow = "\
jobs:
  vendored:
    name: vendored
    runs-on: ubuntu-latest
    steps:
      - uses: acme/vendored-action@0000000000000000000000000000000000000000 # v1
        with:
          name: vendored
      # vendored
      - run: echo hello
  named:
    runs-on: ubuntu-latest
    steps:
      - name: the vendored pin is checked
        run: echo hello
";
        let jobs = job_steps(workflow);
        assert_eq!(jobs.len(), 2, "jobs found: {jobs:?}");
        assert!(
            !jobs[0].1.contains("vendored"),
            "a uses: path, a with: value or a comment satisfied the id"
        );
        assert!(jobs[1].1.contains("vendored"), "a step name: was dropped");
    }

    #[test]
    fn every_ci_job_id_names_what_it_runs() {
        let mut jobs = job_steps(CI);
        jobs.extend(job_steps(RELEASE));
        assert_eq!(jobs.len(), 8, "jobs found: {:?}", jobs);
        for (id, steps) in &jobs {
            assert!(!steps.is_empty(), "{id} has no named step");
            for word in id.split('-') {
                assert!(says_word(steps, word), "{id}: no step says `{word}`");
            }
        }
    }
}
