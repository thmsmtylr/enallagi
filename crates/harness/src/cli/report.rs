//! `enallagi report` -- prints the document, or with --skill writes it as a SKILL.md another agent can load.

use crate::{agent, config, git, report, skills};

pub struct Args {
    pub skill: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => std::path::PathBuf::from(top),
        Err(_) => cwd,
    };
    let cfg = config::load(&root)?;
    let doc = match report::document(&root, &cfg) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!("{err}");
            return Ok(1);
        }
    };
    if !args.skill {
        print!("{doc}");
        return Ok(0);
    }
    let presets = agent::presets();
    let rel = skills::skills_dir(&cfg, presets.get(&cfg.agent.preset))
        .join(report::SKILL_ID)
        .join("SKILL.md");
    let path = root.join(&rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, report::skill(&doc))?;
    println!("report: wrote {}", rel.display());
    Ok(0)
}
