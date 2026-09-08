//! `harness skills check|sync|list` -- the CLI surface over `skills`.

use crate::agent;
use crate::config;
use crate::events::{Log, Writer};
use crate::skills::{self, ResolveOpts, SkillError};
use std::path::Path;

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
    let root = Path::new(".");
    let cfg = config::load(root)?;
    let presets = agent::presets();
    let preset = presets
        .get(&cfg.agent.preset)
        .ok_or_else(|| anyhow::anyhow!("harness: unknown agent preset {}", cfg.agent.preset))?;
    let ids: Vec<String> = cfg.skill.iter().map(|s| s.id.clone()).collect();

    if let SkillsCmd::List = args.cmd {
        let lock = skills::read_lock(root)?;
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
            root,
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

    if unresolved.is_empty() {
        return Ok(0);
    }
    for line in &unresolved {
        eprintln!("harness skills: {line}");
    }
    Ok(2)
}
