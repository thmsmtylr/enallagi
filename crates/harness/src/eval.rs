//! The eval runner and the three-condition admission gate for a candidate rule.

use crate::agent;
use crate::config;
use crate::fixture::Repo;
use crate::git;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("the fixture could not be built")]
    Fixture,
}

fn install_fixture(root: &Path) -> Result<(), EvalError> {
    crate::init::install(root, &crate::init::InitOpts::default())
        .map(|_| ())
        .map_err(|_| EvalError::Fixture)
}

// exit classes: 0 pass, 3 fixture error, 4 agent error, anything else a fail
enum Outcome {
    Pass,
    Fail,
    FixtureError,
    AgentError,
}

impl Outcome {
    fn rc(&self) -> i32 {
        match self {
            Outcome::Pass => 0,
            Outcome::Fail => 1,
            Outcome::FixtureError => 3,
            Outcome::AgentError => 4,
        }
    }
}

fn eval_dir(pkg: &Path, name: &str) -> PathBuf {
    pkg.join("evals").join(name)
}

fn list_evals(pkg: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(pkg.join("evals"))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn resolve_agent(pkg: &Path) -> Option<Vec<String>> {
    if let Ok(env) = std::env::var("EVAL_AGENT") {
        let words: Vec<String> = env.split_whitespace().map(String::from).collect();
        if !words.is_empty() {
            return Some(words);
        }
    }
    let cfg = config::load(pkg).ok()?;
    let presets = agent::presets();
    agent::resolve(&cfg.agent, "default", &presets)
        .ok()
        .map(|r| r.argv)
}

fn run_script(script: &Path, cwd: &Path, pkg: Option<&Path>) -> bool {
    let mut cmd = Command::new("bash");
    cmd.arg(script).current_dir(cwd);
    // the fixture was installed by this binary, so a script's `enallagi` must be this binary too, not an older one on PATH
    if let Some(bin) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let dirs = std::iter::once(bin).chain(std::env::split_paths(&path));
        if let Ok(joined) = std::env::join_paths(dirs) {
            cmd.env("PATH", joined);
        }
    }
    if let Some(pkg) = pkg {
        cmd.env("EVAL_PKG", pkg);
    }
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

fn run_agent(argv: &[String], prompt: &str, cwd: &Path) -> bool {
    let argv = match config::load(cwd) {
        Ok(cfg) => agent::fill_layout(argv, &cfg.layout),
        Err(_) => return false,
    };
    let words: Vec<String> = argv
        .iter()
        .map(|w| w.replace("{prompt}", prompt).replace("{turns}", "40"))
        .collect();
    let Some((program, rest)) = words.split_first() else {
        return false;
    };
    Command::new(program)
        .args(rest)
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_one(pkg: &Path, name: &str, agent_argv: &[String], ablate: bool) -> Outcome {
    let dir = eval_dir(pkg, name);

    let repo = match Repo::try_new() {
        Ok(r) => r,
        Err(_) => return Outcome::FixtureError,
    };
    if install_fixture(&repo.root).is_err() {
        return Outcome::FixtureError;
    }

    // ablation runs before setup: fixture is identical either way, only the rule's presence differs
    if ablate {
        let ablate_sh = dir.join("ablate.sh");
        if !ablate_sh.is_file() {
            return Outcome::FixtureError;
        }
        if !run_script(&ablate_sh, &repo.root, Some(pkg)) {
            return Outcome::FixtureError;
        }
    }

    // committed before setup.sh, so a setup that leaves work uncommitted on purpose is what the agent sees
    let _ = git::commit_paths(&repo.root, &["."], "fixture");

    if !run_script(&dir.join("setup.sh"), &repo.root, Some(pkg)) {
        return Outcome::FixtureError;
    }

    let prompt = match std::fs::read_to_string(dir.join("prompt.txt")) {
        Ok(p) => p,
        Err(_) => return Outcome::FixtureError,
    };
    // An agent that did not run is not a role that disobeyed.
    if !run_agent(agent_argv, &prompt, &repo.root) {
        return Outcome::AgentError;
    }

    if run_script(&dir.join("assert.sh"), &repo.root, None) {
        Outcome::Pass
    } else {
        Outcome::Fail
    }
}

fn report(name: &str, outcome: &Outcome) -> bool {
    match outcome {
        Outcome::Pass => {
            println!("EVAL {name} PASS");
            true
        }
        Outcome::FixtureError => {
            println!("EVAL {name} ERROR (the fixture could not be built — nothing was measured)");
            false
        }
        Outcome::AgentError => {
            println!("EVAL {name} ERROR (the agent exited non-zero — nothing was measured)");
            false
        }
        Outcome::Fail => {
            println!("EVAL {name} FAIL");
            false
        }
    }
}

fn refuse(lines: &[&str]) -> anyhow::Error {
    for line in lines {
        eprintln!("{line}");
    }
    anyhow::anyhow!("{}", lines.first().copied().unwrap_or("refused"))
}

const NO_AGENT: [&str; 2] = [
    "evals: no agent configured. Set EVAL_AGENT, or put an agentCommand in harness.json.",
    "evals: refusing to report a result for something that was never run.",
];

pub fn run(pkg: &Path, names: &[String], agent: Option<Vec<String>>) -> anyhow::Result<bool> {
    let agent_argv = match agent.or_else(|| resolve_agent(pkg)) {
        Some(a) => a,
        None => return Err(refuse(&NO_AGENT)),
    };

    let names: Vec<String> = if names.is_empty() {
        list_evals(pkg)
    } else {
        names.to_vec()
    };
    if names.is_empty() {
        let msg = format!(
            "evals: nothing under {} to run. An eval is a directory with setup.sh, prompt.txt and assert.sh.",
            pkg.join("evals").display()
        );
        return Err(refuse(&[
            &msg,
            "evals: refusing to report a result for something that was never run.",
        ]));
    }

    let mut all_pass = true;
    for name in &names {
        if !eval_dir(pkg, name).join("assert.sh").is_file() {
            println!("EVAL {name} ERROR (no such eval)");
            all_pass = false;
            continue;
        }
        let outcome = run_one(pkg, name, &agent_argv, false);
        if !report(name, &outcome) {
            all_pass = false;
        }
    }
    Ok(all_pass)
}

pub fn gate(pkg: &Path, name: &str, agent: Option<Vec<String>>) -> anyhow::Result<bool> {
    let agent_argv = match agent.or_else(|| resolve_agent(pkg)) {
        Some(a) => a,
        None => return Err(refuse(&NO_AGENT)),
    };

    if !eval_dir(pkg, name).join("ablate.sh").is_file() {
        return Err(refuse(&[&format!(
            "GATE {name} REJECT no ablate.sh: without one, nothing can tell a rule that works from a rule that is never consulted"
        )]));
    }

    let with = run_one(pkg, name, &agent_argv, false);
    if matches!(with, Outcome::FixtureError | Outcome::AgentError) {
        println!(
            "GATE {name} REJECT the fixture could not be built or the agent did not run ({}), so nothing was measured",
            with.rc()
        );
        return Ok(false);
    }
    if !matches!(with, Outcome::Pass) {
        println!(
            "GATE {name} REJECT the rule does not fix the case it came from (its eval fails with the rule in place)"
        );
        return Ok(false);
    }

    let without = run_one(pkg, name, &agent_argv, true);
    if matches!(without, Outcome::FixtureError | Outcome::AgentError) {
        println!(
            "GATE {name} REJECT the ablated arm could not be built or its agent did not run ({}), so nothing was measured",
            without.rc()
        );
        return Ok(false);
    }
    if matches!(without, Outcome::Pass) {
        println!(
            "GATE {name} REJECT the case passes with the rule ablated, so the rule changed no outcome"
        );
        return Ok(false);
    }

    let mut regressed = String::new();
    for other in list_evals(pkg) {
        if other == name {
            continue;
        }
        if !matches!(run_one(pkg, &other, &agent_argv, false), Outcome::Pass) {
            regressed.push(' ');
            regressed.push_str(&other);
        }
    }
    if !regressed.is_empty() {
        println!("GATE {name} REJECT it regresses evals that were passing:{regressed}");
        return Ok(false);
    }

    println!("GATE {name} ACCEPT fixes its case, fails without itself, regresses nothing");
    Ok(true)
}
