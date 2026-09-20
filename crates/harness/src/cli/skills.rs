//! `enallagi skills check|sync|list` -- the CLI surface over `skills`.

use crate::agent;
use crate::config;
use crate::events::{Log, Writer};
use crate::git;
use crate::pipeline::role_source;
use crate::skills::{self, ResolveOpts, SkillError};
use std::path::PathBuf;

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SkillsCmd {
    Check,
    Sync,
    List,
}

pub struct Args {
    pub cmd: SkillsCmd,
    pub frozen: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => PathBuf::from(top),
        Err(_) => cwd,
    };
    let cfg = config::load(&root)?;
    let presets = agent::presets();
    config::validate(&cfg, &presets, &|role| role_source(&root, &cfg, role)).map_err(|errs| {
        anyhow::anyhow!(errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n"))
    })?;
    // a custom preset has no directory of its own; layout.skills_dir or <harness_dir>/skills answers
    let preset = presets.get(&cfg.agent.preset);
    let ids: Vec<String> = cfg.skill.iter().map(|s| s.id.clone()).collect();

    if let SkillsCmd::List = args.cmd {
        let lock = skills::read_lock(&root, &cfg.layout.harness_dir)?;
        for decl in &cfg.skill {
            let source = match &decl.rev {
                Some(rev) => format!("{}@{rev}", decl.source),
                None => decl.source.clone(),
            };
            // a path source has no commit to pin, so its lock line is the content hash; "unlocked" means the lock omits it entirely
            let state = lock
                .skill
                .iter()
                .find(|e| e.id == decl.id)
                .map(|e| {
                    e.commit.clone().unwrap_or_else(|| {
                        format!("sha256:{}", e.sha256.chars().take(12).collect::<String>())
                    })
                })
                .unwrap_or_else(|| "unlocked".to_string());
            println!("{}  {source}  {state}", decl.id);
        }
        return Ok(0);
    }

    let frozen = args.frozen || matches!(args.cmd, SkillsCmd::Check);
    let opts = ResolveOpts {
        frozen,
        ..ResolveOpts::default()
    };
    let mut events = Writer::new(Log::open(&root.join(&cfg.layout.harness_dir)));

    let mut unresolved = Vec::new();
    for id in &ids {
        match skills::resolve(
            &root,
            &cfg,
            preset,
            std::slice::from_ref(id),
            &opts,
            &mut events,
        ) {
            Ok(resolved) => {
                for r in resolved {
                    if let SkillsCmd::Sync = args.cmd {
                        println!("{}  {}", r.id, r.result);
                    }
                }
            }
            Err(SkillError::Unresolved { id, why }) => unresolved.push(format!("{id}: {why}")),
            Err(e) => return Err(e.into()),
        }
    }

    if let SkillsCmd::Sync = args.cmd {
        if !frozen {
            for id in skills::prune(&root, &cfg, preset)? {
                println!("{id}  removed");
            }
        }
    }

    if unresolved.is_empty() {
        return Ok(0);
    }
    for line in &unresolved {
        eprintln!("enallagi skills: {line}");
    }
    Ok(2)
}
