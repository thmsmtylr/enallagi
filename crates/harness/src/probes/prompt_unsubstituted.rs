//! A rendered role prompt still carrying a `__TOKEN__`, or naming a test pathspec that matches no tracked file, sends a lane a command that reads nothing.
//!
//! ponytail: the verifier is the only role naming `__TEST_GLOB__`; read init's role list once a second role does.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::git;

const VERIFIER: &str = include_str!("../../../../roles/verifier.md");

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let dir = &ctx.cfg.layout.harness_dir;
    let token = common::re(r"__[A-Z_]+__")?;
    let glob_unmatched = unmatched(ctx)?;
    let glob_line = VERIFIER
        .lines()
        .position(|l| l.contains("__TEST_GLOB__"))
        .map_or(0, |i| i + 1);
    let mut found = Vec::new();
    for sub in ["roles", "run/roles"] {
        let rel_dir = format!("{dir}/{sub}");
        let Ok(entries) = std::fs::read_dir(ctx.root.join(&rel_dir)) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".md"))
            .collect();
        names.sort();
        for name in names {
            let rel = format!("{rel_dir}/{name}");
            for (index, line) in common::lines_of(ctx.root, &rel)?.iter().enumerate() {
                for m in token.find_iter(line) {
                    found.push(common::finding(
                        &rel,
                        index + 1,
                        format!(
                            "carries {}, which nothing substituted; set the key it names, then run `enallagi init`",
                            m.as_str()
                        ),
                    ));
                }
            }
            if name == "verifier.md" {
                if let Some(message) = &glob_unmatched {
                    found.push(common::finding(&rel, glob_line, message.clone()));
                }
            }
        }
    }
    Ok(found)
}

fn unmatched(ctx: &ProbeCtx) -> Res<Option<String>> {
    let specs = &ctx.cfg.layout.test_glob;
    if specs.is_empty() {
        return Ok(Some(
            "layout.test_glob is unset, so the verifier's test diff names no pathspec; set it in enallagi.toml, then run `enallagi init`".to_string(),
        ));
    }
    let mut args = vec!["ls-files", "--"];
    args.extend(specs.iter().map(String::as_str));
    let out = git::git(ctx.root, &args).map_err(|e| e.to_string())?;
    if !out.trim().is_empty() {
        return Ok(None);
    }
    let quoted: Vec<String> = specs.iter().map(|s| format!("'{s}'")).collect();
    Ok(Some(format!(
        "layout.test_glob ({}) matches no tracked file, so the verifier's test diff reads nothing",
        quoted.join(" ")
    )))
}
