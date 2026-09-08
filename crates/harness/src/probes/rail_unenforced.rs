//! A rail whose `Enforced by` column names something that does not exist, or that exists and nothing runs.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use std::collections::BTreeSet;

fn subcommands() -> Vec<String> {
    use clap::CommandFactory;
    crate::cli::Cli::command()
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect()
}

pub fn is_path(token: &str) -> Res<bool> {
    Ok(common::re(r"\.(sh|ts|tsx|js|json|toml|md|lock)$")?.is_match(token))
}

pub fn resolve(ctx: &ProbeCtx, tracked: &[String], token: &str) -> Option<String> {
    if common::exists(ctx.root, token) {
        return Some(token.to_string());
    }
    if token.contains('/') {
        return None;
    }
    // the context file names most enforcement by basename; a tracked file of that name is the enforcement
    tracked
        .iter()
        .find(|p| p.rsplit('/').next() == Some(token))
        .cloned()
}

fn harness_text(ctx: &ProbeCtx) -> Res<String> {
    let layout = &ctx.cfg.layout;
    let dir = &layout.harness_dir;
    let mut files: Vec<String> = layout
        .harness_files
        .iter()
        .filter(|f| common::exists(ctx.root, f))
        .cloned()
        .collect();
    let tree = common::walk(ctx.root);
    for pattern in &layout.harness_globs {
        files.extend(common::matches(&tree, pattern)?);
    }
    // enforcement is: the check runs it, the tool's settings wire it, or a hash covers it
    let wired = [
        "test-hashes.json".to_string(),
        format!("{dir}/loop.sh"),
        format!("{dir}/tasks.py"),
    ];
    files.extend(wired);
    for pattern in [
        format!("{dir}/lib/*.sh"),
        ".*/settings.json".to_string(),
        ".*/settings.local.json".to_string(),
        format!("{dir}/roles/*.md"),
    ] {
        files.extend(common::matches(&tree, &pattern)?);
    }

    let mut text = String::new();
    for file in &files {
        if common::is_file(ctx.root, file) {
            text.push_str(&common::read(ctx.root, file)?);
            text.push('\n');
        }
    }
    // one hop: check runs a script that runs a script, and both are enforcement
    let hop = common::re(r"[\w./-]+\.(?:sh|ts)")?;
    let hops: BTreeSet<String> = hop
        .find_iter(&text)
        .map(|m| {
            let mut path = m.as_str();
            while let Some(rest) = path.strip_prefix("../") {
                path = rest;
            }
            path.trim_start_matches('/').to_string()
        })
        .collect();
    let mut extra = String::new();
    for path in hops {
        if common::is_file(ctx.root, &path) {
            extra.push('\n');
            extra.push_str(&common::read(ctx.root, &path)?);
        }
    }
    text.push_str(&extra);
    Ok(text)
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let rails = common::rails_file(ctx.cfg);
    let tracked = common::tracked(ctx.root)?;
    let harness = harness_text(ctx)?;
    let mut found = Vec::new();

    for row in common::rail_rows(ctx)? {
        for token in common::backticked(&row.cells[2]) {
            if let Some((name, test)) = token.split_once("::") {
                match resolve(ctx, &tracked, name) {
                    None => found.push(common::finding(
                        &rails,
                        row.line,
                        format!("enforced by {token}, and {name} does not exist"),
                    )),
                    Some(path) => {
                        if !common::declares(
                            ctx.root,
                            &path,
                            test,
                            &ctx.cfg.layout.test_decl_patterns,
                        )? {
                            found.push(common::finding(
                                &rails,
                                row.line,
                                format!("enforced by {token}, and {path} declares no such test"),
                            ));
                        }
                    }
                }
                continue;
            }
            if let Some(rest) = token.strip_prefix("harness ") {
                // a rail naming a subcommand the binary doesn't have is the same failure as a missing script
                let sub = rest.split_whitespace().next().unwrap_or("");
                if !subcommands().iter().any(|s| s == sub) {
                    found.push(common::finding(
                        &rails,
                        row.line,
                        format!("enforced by `{token}`, and the binary has no {sub} subcommand"),
                    ));
                }
                continue;
            }
            if !token.contains('/') && !token.starts_with('.') && !is_path(&token)? {
                continue;
            }
            let Some(path) = resolve(ctx, &tracked, token.trim_end_matches('/')) else {
                found.push(common::finding(
                    &rails,
                    row.line,
                    format!("enforced by {token}, which does not exist"),
                ));
                continue;
            };
            if path.ends_with(".test.ts") || !(path.ends_with(".sh") || path.ends_with(".ts")) {
                continue;
            }
            let base = path.rsplit('/').next().unwrap_or(&path);
            if !harness.contains(base) {
                found.push(common::finding(
                    &rails,
                    row.line,
                    format!("{token} exists and the check does not run it"),
                ));
            }
        }
    }
    Ok(found)
}
