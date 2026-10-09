//! `enallagi skills check|sync|list|--cost` -- the CLI surface over `skills`.

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
    pub cmd: Option<SkillsCmd>,
    pub frozen: bool,
    pub cost: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let root = match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => PathBuf::from(top),
        Err(_) => cwd,
    };
    let cfg = config::load(&root)?;
    let presets = agent::presets();
    // resolving skills runs no check, so a repository that has not set one still syncs and lists
    if let Err(errs) = config::validate(&cfg, &presets, &|role| role_source(&root, &cfg, role)) {
        let errs: Vec<String> = errs
            .iter()
            .filter(|e| !matches!(e, config::ConfigError::EmptyCheck))
            .map(|e| e.to_string())
            .collect();
        if !errs.is_empty() {
            anyhow::bail!(errs.join("\n"));
        }
    }
    // a custom preset has no directory of its own; layout.skills_dir or <harness_dir>/skills answers
    let preset = presets.get(&cfg.agent.preset);
    let ids: Vec<String> = cfg.skill.iter().map(|s| s.id.clone()).collect();

    if args.cost {
        let dir = root.join(skills::skills_dir(&cfg, preset));
        let sized: Vec<(String, Option<u64>)> = ids
            .iter()
            .map(|id| (id.clone(), skills::dir_bytes(&dir.join(id))))
            .collect();
        let loads = |role: &str| {
            role_source(&root, &cfg, role)
                .map(|source| skills::required_ids(&config::subst(&source, &cfg)))
                .unwrap_or_default()
        };
        let events = Log::open(&root.join(&cfg.layout.harness_dir)).read()?;
        print!("{}", skills::cost_report(&sized, &loads, &events));
        return Ok(0);
    }
    // clap requires a subcommand whenever --cost is absent
    let Some(cmd) = &args.cmd else {
        return Ok(2);
    };

    if let SkillsCmd::List = cmd {
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

    let frozen = args.frozen || matches!(cmd, SkillsCmd::Check);
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
                    if let SkillsCmd::Sync = cmd {
                        println!("{}  {}", r.id, r.result);
                    }
                }
            }
            Err(SkillError::Unresolved { id, why }) => unresolved.push(format!("{id}: {why}")),
            Err(e) => return Err(e.into()),
        }
    }

    if let SkillsCmd::Sync = cmd {
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
