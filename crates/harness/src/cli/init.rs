//! `enallagi init` -- installs in one pass: answers, files, adapter, skills, the state commit and a probe; or moves instance files into the harness directory, or prunes the defaulted keys out of enallagi.toml.

use crate::config;
use crate::init::{self, Answers, InitOpts};
use std::io::IsTerminal;

pub struct Args {
    pub adapter: Option<String>,
    pub dry_run: bool,
    pub move_files: bool,
    pub prune_defaults: bool,
    pub yes: bool,
    pub answers: Answers,
    pub sync: bool,
    pub frozen: bool,
    pub issue: Option<String>,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match crate::git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => std::path::PathBuf::from(top),
        Err(_) => cwd,
    };
    let opts = InitOpts {
        adapter: args.adapter.clone(),
        dry_run: args.dry_run,
        answers: args.answers.clone(),
        yes: args.yes,
    };
    if args.prune_defaults {
        let verb = if args.dry_run {
            "would drop"
        } else {
            "dropped"
        };
        let path = config::config_path(&root);
        if !path.is_file() {
            eprintln!("{} not found, nothing pruned", path.display());
            return Ok(1);
        }
        let dropped = init::prune(&root, args.dry_run)?;
        if dropped.is_empty() {
            println!("  no key equals its default, nothing dropped");
        }
        for key in dropped {
            println!("  {verb}: {key}");
        }
        println!("\nNext: re-run `enallagi init`.");
        return Ok(0);
    }
    println!(
        "installing the harness into {}{}",
        root.display(),
        if args.dry_run { "  (dry run)" } else { "" }
    );
    if args.move_files {
        let moves = init::moves(&root)?;
        if !args.dry_run {
            init::relocate(&root, &moves)?;
        }
        let verb = if args.dry_run { "would move" } else { "moved" };
        for (old, new) in &moves {
            println!("  {verb}: {old} -> {new}");
        }
        println!("\nNext: git add the old and new paths, then re-run `enallagi init`.");
        return Ok(0);
    }
    let report = init::install(&root, &opts)?;

    for (old, new) in &report.moves {
        println!("  would move: {old} -> {new}");
    }
    if !report.moves.is_empty() {
        println!("  nothing was moved; `enallagi init --move` moves them");
    }

    let verb = if args.dry_run { "would write" } else { "wrote" };
    for path in &report.wrote {
        println!("  {verb}: {path}");
    }
    for path in &report.kept {
        println!("  kept: {path}");
    }
    for key in &report.migrated_keys {
        println!("  migrated: {key}");
    }
    for note in &report.notes {
        println!("  {note}");
    }

    if args.dry_run {
        println!("\nNext: enallagi init");
        return Ok(0);
    }

    let cfg = config::load(&root)?;
    let dir = config::harness_dir(&root);
    let mut failed = false;

    // the skills a lane loads, vendored now rather than at the first stage; a stdin that is not a
    // terminal is a fixture or a pipeline, where a fetch mid-install is a surprise, so it needs --sync
    let fetch = !args.frozen
        && !crate::skills::frozen_from_env(std::env::var_os("CI").as_deref())
        && (args.sync || std::io::stdin().is_terminal());
    if fetch {
        match vendor(&root, &cfg) {
            Ok(lines) => {
                for line in lines {
                    println!("  vendored: {line}");
                }
            }
            Err(err) => {
                failed = true;
                println!("  skills: {err}; `enallagi skills sync` once the source is reachable");
            }
        }
    }

    // the verdict gate wants every instance edit committed; the operator should not have to know that
    let nested = crate::git::state_root(&root, &dir) != root;
    match crate::git::commit_instance(
        &root,
        &dir,
        &[],
        &format!("init: enallagi {}", env!("CARGO_PKG_VERSION")),
    ) {
        Ok(true) => println!("  committed: {dir}"),
        Ok(false) => {}
        Err(err) => {
            failed = true;
            println!("  {dir}: the state was not committed: {err}");
        }
    }
    // a harness directory the product tracks is the product's to commit, so the paths are named
    if !nested && !report.track.is_empty() {
        println!("  git add {}", report.track.join(" "));
    }

    if let Some(reference) = &args.issue {
        match queue_issue(&root, &dir, reference) {
            Ok(id) => println!("  proposed: {id}"),
            Err(err) => {
                failed = true;
                println!("  issue: {err}");
            }
        }
    }

    // what the tree says about itself, printed as `enallagi probe` prints it; a finding is a report,
    // not a failed install, since a re-run over a repository with standing findings is still installed.
    // The check is not run here: it is the repository's own suite and takes as long as it takes, so
    // `check-red`, the one probe that reads it, is left to `enallagi probe`
    let unrun = crate::probes::CheckOutcome {
        ran: false,
        red: false,
        output: "init does not run the check".to_string(),
    };
    let ctx = crate::probes::ProbeCtx {
        root: &root,
        cfg: &cfg,
        check: Some(&unrun),
        driver: false,
    };
    let names: Vec<String> = crate::probes::NAMES
        .iter()
        .filter(|name| **name != "check-red")
        .map(|name| name.to_string())
        .collect();
    let results = crate::probes::run_all(&ctx, &names);
    let rendered = crate::probes::render(&results);
    let findings = rendered
        .lines()
        .filter(|l| l.starts_with("FINDING "))
        .count();
    for line in rendered.lines().filter(|l| l.starts_with("FINDING ")) {
        println!("  {line}");
    }
    println!("  probe: {findings} finding(s)");

    println!("\nNext: enallagi run --pr-per-task");
    Ok(if failed { 1 } else { 0 })
}

fn vendor(root: &std::path::Path, cfg: &config::Config) -> anyhow::Result<Vec<String>> {
    let presets = crate::agent::presets();
    let preset = presets.get(&cfg.agent.preset);
    let ids: Vec<String> = cfg.skill.iter().map(|s| s.id.clone()).collect();
    let opts = crate::skills::ResolveOpts {
        frozen: false,
        ..crate::skills::ResolveOpts::default()
    };
    let mut events = crate::events::Writer::new(crate::events::Log::open(
        &root.join(&cfg.layout.harness_dir),
    ));
    let resolved = crate::skills::resolve(root, cfg, preset, &ids, &opts, &mut events)?;
    Ok(resolved
        .into_iter()
        .map(|r| format!("{}  {}", r.id, r.result))
        .collect())
}

// the same append `enallagi issue` makes, then the state commit that carries it
fn queue_issue(root: &std::path::Path, dir: &str, reference: &str) -> anyhow::Result<String> {
    let queue = crate::queue::Queue {
        path: config::instance_path(root, dir, "TASKS.md"),
    };
    let decisions = std::fs::read_to_string(config::instance_path(root, dir, "DECISIONS.md"))
        .unwrap_or_default();
    let found = crate::issue::read(reference)?;
    let (block, next) = crate::issue::append(&queue.read()?, &decisions, &found)?;
    queue.write(&next)?;
    let id = block.lines().next().unwrap_or_default().to_string();
    crate::git::commit_instance(root, dir, &["TASKS.md"], &format!("queue: {id}"))?;
    Ok(id)
}
