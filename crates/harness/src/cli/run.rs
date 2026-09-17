//! `enallagi run [n]` -- the launcher, in the TUI when there is one to draw in.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::events::{self, Event, Kind};
use crate::pipeline::{self, RunOpts};
use crate::{agent, git, skills, tui};

pub struct Args {
    pub iterations: u32,
    pub pipelines: Vec<String>,
    pub budget_usd: Option<f64>,
    pub budget_seconds: Option<u64>,
    pub budget_tokens: Option<u64>,
    pub no_tui: bool,
    pub dry_run: bool,
    pub frozen: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    // a stop is the operator's, so it ends the run through the digest rather than orphaning a lane
    agent::catch_stop_signals();
    let cwd = std::env::current_dir()?;
    let root = match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => std::path::PathBuf::from(top),
        Err(_) => cwd,
    };
    if !args.dry_run {
        if let Err(err) = pipeline::preflight(&root) {
            eprintln!("enallagi run: {err}");
            return Ok(2);
        }
    }
    let tui = !args.no_tui && !args.dry_run && std::io::stdout().is_terminal();
    // flags set here win: with_env_budgets only fills a field still None
    let opts = RunOpts {
        max_iter: args.iterations,
        pipelines: args.pipelines.clone(),
        dry_run: args.dry_run,
        // CI is frozen whether or not anyone passed the flag: fetching a skill mid-run breaks the lock
        frozen: args.frozen || skills::frozen_from_env(std::env::var_os("CI").as_deref()),
        tui,
        budget_usd: args.budget_usd,
        budget_seconds: args.budget_seconds,
        budget_tokens: args.budget_tokens,
    }
    .with_env_budgets();

    // exit code is read off the events, not threaded back through every return path
    let failed = Arc::new(AtomicBool::new(false));
    let dir = crate::config::harness_dir(&root);
    let stop = crate::config::instance_path(&root, &dir, "STOP");
    let outcome = if tui {
        let (tx, rx) = mpsc::channel::<Event>();
        let thread_root = root.clone();
        let flag = Arc::clone(&failed);
        let worker = std::thread::spawn(move || {
            pipeline::run(
                &thread_root,
                &opts,
                Box::new(move |e| {
                    if counts_as_failure(e) {
                        flag.store(true, Ordering::Relaxed);
                    }
                    let _ = tx.send(e.clone());
                }),
            )
        });
        let drawn = tui::run_live(
            rx,
            &crate::config::instance_path(&root, &dir, "TASKS.md"),
            &stop,
        );
        if drawn.is_err() {
            let _ = std::fs::write(&stop, b"");
        }
        let ran = match worker.join() {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        };
        drawn?;
        // the TUI path never printed one; the no-TUI path gets its from pipeline::run itself
        if let Ok(digest) = &ran {
            print!("{}", pipeline::digest_text(digest));
        }
        ran
    } else {
        let flag = Arc::clone(&failed);
        pipeline::run(
            &root,
            &opts,
            Box::new(move |e| {
                if counts_as_failure(e) {
                    flag.store(true, Ordering::Relaxed);
                }
                println!("{}", events::render_line(e));
            }),
        )
    };

    match outcome {
        Ok(digest) => Ok(i32::from(
            failed.load(Ordering::Relaxed) || !digest.halts.is_empty(),
        )),
        Err(err) if err.downcast_ref::<pipeline::Refused>().is_some() => {
            eprintln!("enallagi run: {err}");
            Ok(2)
        }
        Err(err) => Err(err),
    }
}

// dry-round's rejection is a routing decision, not a verdict: a round with nothing takeable is how a run ends well
fn counts_as_failure(e: &Event) -> bool {
    match &e.kind {
        Kind::Halt { .. } => true,
        Kind::Gate { gate, pass, .. } => !pass && gate != "dry-round",
        Kind::StageEnd { exit, .. } => *exit != 0,
        _ => false,
    }
}
