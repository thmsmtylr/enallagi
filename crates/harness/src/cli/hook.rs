use std::io::Read;
use std::path::PathBuf;

use crate::{git, hooks};

pub struct Args {
    pub name: String,
}

fn root() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("CLAUDE_PROJECT_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let cwd = std::env::current_dir()?;
    Ok(match git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => PathBuf::from(top),
        Err(_) => cwd,
    })
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = root()?;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    let (code, message) = match args.name.as_str() {
        "immutable" => hooks::immutable(&root, &input),
        "one-writer" => hooks::one_writer(&root, &input),
        "verify-done" => hooks::verify_done(&root, &input),
        "skills" => hooks::skills_contract(&root),
        other => (2, format!("enallagi hook: unknown hook `{other}`")),
    };
    if !message.is_empty() {
        eprintln!("{message}");
    }
    Ok(code)
}
