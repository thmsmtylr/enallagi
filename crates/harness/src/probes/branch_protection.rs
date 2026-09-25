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

// how long the host tool gets to answer before the leg reports unknown; a stuck tool would
// otherwise hold a scout stage, an init and a pipeline predicate open with no ceiling
const HOST_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

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
    let Ok(out) = bounded(root, tool, &["api", &path, "--jq", JQ]) else {
        return Vec::new();
    };
    let Some(out) = out else {
        return vec![common::finding(
            ".git/config",
            0,
            format!(
                "`{tool} api {path} --jq '{JQ}'` -> unknown, no answer in {}s",
                HOST_WAIT.as_secs()
            ),
        )];
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

// the tool runs in its own process group and is given HOST_WAIT; `None` is a tool that never
// answered, killed with everything it spawned, so the line the leg prints is the same on every run
fn bounded(
    root: &Path,
    tool: &str,
    args: &[&str],
) -> std::io::Result<Option<std::process::Output>> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    let mut child = Command::new(tool)
        .current_dir(root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let pgid = child.id();
    let drain = |pipe: Option<_>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut pipe) = pipe {
                let _: std::io::Result<usize> = std::io::Read::read_to_end(&mut pipe, &mut buf);
            }
            buf
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = std::time::Instant::now() + HOST_WAIT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if std::time::Instant::now() >= deadline {
            let _ = crate::gates::kill_group(&mut child, pgid);
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(status.map(|status| std::process::Output {
        status,
        stdout,
        stderr,
    }))
}
