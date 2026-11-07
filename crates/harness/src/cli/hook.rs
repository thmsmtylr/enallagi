//! `enallagi hook <name>` -- the agent's payload on stdin, exiting with the named hook's code

use std::io::Read;
use std::path::PathBuf;

use crate::hooks;

pub struct Args {
    pub name: String,
}

fn root() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("CLAUDE_PROJECT_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    super::root()
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let root = root()?;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    let (code, message) = match args.name.as_str() {
        "immutable" => hooks::immutable(
            &root,
            &input,
            std::env::var("ENALLAGI_TASK").ok().as_deref(),
        ),
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
