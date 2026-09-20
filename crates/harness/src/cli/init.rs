//! `enallagi init` -- installs, or moves instance files into the harness directory, or prunes the defaulted keys out of enallagi.toml.

use crate::config;
use crate::init::{self, InitOpts};

pub struct Args {
    pub adapter: Option<String>,
    pub dry_run: bool,
    pub move_files: bool,
    pub prune_defaults: bool,
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

    let spec = config::load(&root)
        .map(|cfg| cfg.layout.spec)
        .unwrap_or_else(|_| "SPEC.md".to_string());
    let spec = config::instance_rel(&root, &config::harness_dir(&root), &spec);
    let toml = config::config_path(&root);
    let toml = toml
        .strip_prefix(&root)
        .unwrap_or(&toml)
        .display()
        .to_string();
    // the product ignores a nested harness repository, and `git add` exits 1 on an ignored path
    let add = if crate::git::locate(&root, &config::harness_dir(&root), &toml).2 {
        String::new()
    } else {
        format!(" {toml}")
    };
    println!(
        "
Next, in {root}:
  1. Edit {toml}: check.command, layout.spec, agent.preset
  2. enallagi init
  3. Write exit criteria into {spec}
  4. git add{add}{track}
  5. enallagi probe
  6. enallagi run --iterations 1",
        root = root.display(),
        // named unconditionally: on a re-run it's neither written nor kept, so leaving it off would leave the answers untracked
        track = report
            .track
            .iter()
            .filter(|path| *path != "enallagi.toml")
            .fold(String::new(), |line, path| line + " " + path),
    );
    Ok(0)
}
