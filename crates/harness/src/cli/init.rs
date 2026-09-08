use crate::config;
use crate::init::{self, InitOpts};

pub struct Args {
    pub adapter: Option<String>,
    pub dry_run: bool,
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
    println!(
        "installing the harness into {}{}",
        root.display(),
        if args.dry_run { "  (dry run)" } else { "" }
    );
    let report = init::install(&root, &opts)?;

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
    println!(
        "
Next, in {root}:
  1. Edit harness.toml — 'check.command', 'layout.spec' and 'agent.preset' are the three that matter.
  2. Re-run `harness init`. Substitution is idempotent.
  3. Write your exit criteria into {spec} under the heading harness.toml names.
  4. git add harness.toml{track}
     # everything this run wrote. gate_verdict counts an untracked path as work off the branch,
     # so a document left untracked here fails the first verdict.
  5. harness probe     # what the tree says about itself
  6. harness run --iterations 1     # one iteration, attended, watch it work",
        root = root.display(),
        // named unconditionally: on a re-run it's neither written nor kept, so leaving it off would leave the answers untracked
        track = report
            .track
            .iter()
            .filter(|path| *path != "harness.toml")
            .fold(String::new(), |line, path| line + " " + path),
    );
    Ok(0)
}
