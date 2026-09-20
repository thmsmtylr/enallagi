//! What a checkout can establish about the branch its remote calls default, and what a host tool says about it.

use super::common;
use super::{Finding, ProbeCtx, ProbeResult};
use crate::git;
use std::path::Path;
use std::process::Command;

const REMOTE: &str = "origin";

// one tool per host; a remote on any other host gets the host-free leg alone
const TOOLS: [(&str, &str); 1] = [("github", "gh")];

const JQ: &str = "[.[].type] | join(\", \")";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    ProbeResult::Count(find(ctx.root))
}

// advisory: every leg that cannot run reports nothing, so no exit code of the loop can depend on it
fn find(root: &Path) -> Vec<Finding> {
    let Some(host) = remote_host(root) else {
        return Vec::new();
    };
    let Some(default) = default_branch(root) else {
        return Vec::new();
    };
    let mut found = direct_commits(root, &default);
    found.extend(host_answer(root, &host, &default));
    found
}

fn remote_host(root: &Path) -> Option<String> {
    host_of(&git::git(root, &["remote", "get-url", REMOTE]).ok()?)
}

// a URL with no host names a path on this machine, where nothing stands between a branch and a push
fn host_of(url: &str) -> Option<String> {
    let Some((_, rest)) = url.split_once("://") else {
        return scp_host(url);
    };
    let authority = rest.split('/').next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split_once(':').map_or(host, |(h, _)| h);
    (!host.is_empty()).then(|| host.to_string())
}

fn scp_host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once('@')?;
    let (host, path) = rest.split_once(':')?;
    (!host.is_empty() && !host.contains('/') && !path.is_empty()).then(|| host.to_string())
}

fn default_branch(root: &Path) -> Option<String> {
    let head = format!("refs/remotes/{REMOTE}/HEAD");
    let full = git::git(root, &["symbolic-ref", "--short", &head]).ok()?;
    full.strip_prefix(&format!("{REMOTE}/")).map(String::from)
}

fn direct_commits(root: &Path, default: &str) -> Vec<Finding> {
    let branch = format!("{REMOTE}/{default}");
    let args = [
        "log",
        "--first-parent",
        "--no-merges",
        "--format=%h",
        &branch,
    ];
    let Ok(log) = git::git(root, &args) else {
        return Vec::new();
    };
    let count = log.lines().filter(|line| !line.is_empty()).count();
    if count == 0 {
        return Vec::new();
    }
    vec![common::finding(
        ".git/config",
        0,
        format!("`git {} | wc -l` -> {count}", args.join(" ")),
    )]
}

fn host_answer(root: &Path, host: &str, default: &str) -> Vec<Finding> {
    let Some((_, tool)) = TOOLS.iter().find(|(named, _)| host.contains(named)) else {
        return Vec::new();
    };
    let path = format!("repos/{{owner}}/{{repo}}/rules/branches/{default}");
    let Ok(out) = Command::new(tool)
        .current_dir(root)
        .args(["api", &path, "--jq", JQ])
        .output()
    else {
        return Vec::new();
    };
    let said = if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if stdout.is_empty() {
            "\"\"".to_string()
        } else {
            stdout
        }
    } else {
        // a failed call is neither a protected nor an unprotected branch
        format!(
            "unknown, exit {}: {}",
            out.status
                .code()
                .map_or_else(|| "signal".to_string(), |c| c.to_string()),
            common::cut(String::from_utf8_lossy(&out.stderr).trim(), 120)
        )
    };
    vec![common::finding(
        ".git/config",
        0,
        format!("`{tool} api {path} --jq '{JQ}'` -> {said}"),
    )]
}
