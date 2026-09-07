//! `harness run [n]` -- the launcher, in the TUI when there is one to draw in.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::events::{self, Event, Kind};
use crate::pipeline::{self, RunOpts};
use crate::{git, tui};

pub struct Args {
    pub n: Option<u32>,
    pub no_tui: bool,
    pub dry_run: bool,
    pub frozen: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => std::path::PathBuf::from(top),
        Err(_) => cwd,
    };
    let tui = !args.no_tui && !args.dry_run && std::io::stdout().is_terminal();
    let opts = RunOpts {
        max_iter: args.n.unwrap_or(3),
        dry_run: args.dry_run,
        frozen: args.frozen,
        tui,
        ..RunOpts::default()
    }
    .with_env_budgets();

    // A halt or a rejected gate is exit 1, whatever the digest says: both are
    // visible in the stream, so the code is read off the events rather than
    // threaded back through every return path.
    let failed = Arc::new(AtomicBool::new(false));
    let outcome = if tui {
        let (tx, rx) = mpsc::channel::<Event>();
        let thread_root = root.clone();
        let flag = Arc::clone(&failed);
        let worker = std::thread::spawn(move || {
            pipeline::run(&thread_root, &opts, &mut |e| {
                if counts_as_failure(e) {
                    flag.store(true, Ordering::Relaxed);
                }
                let _ = tx.send(e.clone());
            })
        });
        tui::run_live(rx, &root.join("TASKS.md"), &root.join("STOP"))?;
        match worker.join() {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    } else {
        pipeline::run(&root, &opts, &mut |e| {
            if counts_as_failure(e) {
                failed.store(true, Ordering::Relaxed);
            }
            println!("{}", events::render_line(e));
        })
    };

    match outcome {
        Ok(digest) => Ok(i32::from(
            failed.load(Ordering::Relaxed) || !digest.halts.is_empty(),
        )),
        Err(err) if err.downcast_ref::<pipeline::Refused>().is_some() => {
            eprintln!("harness run: {err}");
            Ok(2)
        }
        Err(err) => Err(err),
    }
}

/// `dry-round` is the one gate whose rejection is a routing decision rather
/// than a verdict: a round that left nothing takeable is how a run ends well.
fn counts_as_failure(e: &Event) -> bool {
    match &e.kind {
        Kind::Halt { .. } => true,
        Kind::Gate { gate, pass, .. } => !pass && gate != "dry-round",
        Kind::StageEnd { exit, .. } => *exit != 0,
        _ => false,
    }
}
