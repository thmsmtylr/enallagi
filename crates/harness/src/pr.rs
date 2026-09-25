//! Builds a pull-request branch off the upstream default branch from the product commits whose subject names a landed task.

use crate::probes::{contribution_policy, Finding};
use crate::{config, gates, git, queue};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Default)]
pub struct PrOpts {
    pub push: bool,
    pub policy_read: bool,
    /// Tasks whose branch this run already built, in build order; an unmerged blocker among them is the base, not a refusal.
    pub stack_on: Vec<String>,
}

#[derive(Debug)]
pub struct PrReport {
    pub branch: String,
    /// The branch the pull request merges into: the default branch, or a blocker's `task/` branch.
    pub base: String,
    pub description: PathBuf,
    /// The pull request's URL, or why none was opened, when `--push` ran.
    pub opened: Option<String>,
    pub policy: Vec<Finding>,
}

#[derive(Debug, thiserror::Error)]
pub enum PrError {
    #[error("enallagi pr refused, nothing was built:\n  {}", .0.join("\n  "))]
    Refused(Vec<String>),
    #[error("the diff does not apply to {base}, a three-way merge conflict in:\n  {}", .files.join("\n  "))]
    Conflict { base: String, files: Vec<String> },
    #[error("the check failed in the task worktree, nothing was pushed:\n{0}")]
    Check(String),
    #[error("enallagi pr --push refused, the contribution guide conditions generated changes; nothing was pushed:\n  {}\nread the guide, then pass --policy-read to push. The refusal is recorded in {}", .sentences.join("\n  "), .description.display())]
    Policy {
        sentences: Vec<String>,
        description: PathBuf,
    },
    #[error("the branch is pushed, and gh pr create failed: {0}")]
    Gh(String),
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error(transparent)]
    Git(#[from] git::GitError),
}

struct Task {
    id: String,
    title: String,
    block: queue::Block,
}

pub fn build(root: &Path, ids: &[String], opts: &PrOpts) -> Result<PrReport, PrError> {
    let cfg = config::load(root).map_err(|e| PrError::Refused(vec![e.to_string()]))?;
    let dir = cfg.layout.harness_dir.clone();
    let read = |name: &str| fs::read_to_string(config::instance_path(root, &dir, name));
    let tasks_md = read("TASKS.md").unwrap_or_default();
    let decisions = read("DECISIONS.md").unwrap_or_default();
    let queued = parse(&tasks_md)?;
    let archived = parse(&decisions)?;

    let mut refusals = Vec::new();
    let mut tasks = Vec::new();
    for id in ids {
        // the stub in TASKS.md carries the live status, the archive the full block
        let found = queued.iter().find(|b| b.id == *id);
        let Some(full) = archived.iter().find(|b| b.id == *id).or(found) else {
            refusals.push(format!("{id} is in neither TASKS.md nor DECISIONS.md"));
            continue;
        };
        let status = queue::field(found.unwrap_or(full), "status").unwrap_or_default();
        if status != "done" {
            refusals.push(format!("{id} is {status:?}, not done"));
            continue;
        }
        tasks.push(Task {
            id: id.clone(),
            title: full.title.clone(),
            block: full.clone(),
        });
    }
    if !refusals.is_empty() {
        return Err(PrError::Refused(refusals));
    }

    let default = default_branch(root)?;
    git::git(root, &["fetch", "-q", "origin", &default])?;
    let base = format!("origin/{default}");
    // a checkout behind its upstream makes every task's replay conflict on files that are not the task's
    let range = format!("HEAD..{base}");
    let ahead = git::git(root, &["rev-list", "--count", &range])?;
    if ahead != "0" {
        return Err(PrError::Refused(vec![format!(
            "`git rev-list --count {range}` -> {ahead}. Reconcile the two histories before any task builds: `git merge {base}`."
        )]));
    }
    let range = format!("{base}..HEAD");
    let landed = git::git(root, &["log", "--format=%B", &base])?;
    let mut stacked: Option<usize> = None;
    let mut on: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for task in &tasks {
        for blocker in queue::blockers(&task.block) {
            if ids.contains(&blocker)
                || commits(root, &range, std::slice::from_ref(&blocker))?.is_empty()
            {
                continue;
            }
            if gates::names_task(&landed, &blocker) {
                continue;
            }
            // the blocker built last is the base; that it carries the others is checked below
            if let Some(at) = opts.stack_on.iter().position(|id| *id == blocker) {
                on.insert(at);
                stacked = stacked.max(Some(at));
            } else {
                refusals.push(format!(
                    "{} is blocked by {blocker}, whose product change is not on {base}",
                    task.id
                ));
            }
        }
    }
    // a branch has one base, so two blockers stack only when the later one already carries the earlier
    if let Some(at) = stacked {
        let branch = format!("task/{}", opts.stack_on[at]);
        let history = git::git(root, &["log", "--format=%B", &branch]).unwrap_or_default();
        for other in on.iter().filter(|o| **o != at) {
            let id = &opts.stack_on[*other];
            if !gates::names_task(&history, id) {
                refusals.push(format!(
                    "{id} and {} are both unmerged blockers, and {branch} does not carry {id}",
                    opts.stack_on[at]
                ));
            }
        }
    }
    if !refusals.is_empty() {
        return Err(PrError::Refused(refusals));
    }
    let (onto, base) = match stacked {
        Some(at) => {
            let branch = format!("task/{}", opts.stack_on[at]);
            (branch.clone(), branch)
        }
        None => (default.clone(), base),
    };

    let picked = commits(root, &range, ids)?;
    for id in ids {
        if !picked
            .iter()
            .any(|(_, subject)| gates::names_task(subject, id))
        {
            refusals.push(format!(
                "{id}: no commit on {range} names it, so it has no product change"
            ));
        }
    }
    if !refusals.is_empty() {
        return Err(PrError::Refused(refusals));
    }
    let branch = format!("task/{}", ids.join("-"));
    let scratch = tempfile::TempDir::new().map_err(io("a temporary directory"))?;
    let wt = scratch.path().join("worktree");
    let wt_arg = wt.display().to_string();
    git::git(
        root,
        &["worktree", "add", "-q", "-b", &branch, &wt_arg, &base],
    )?;
    let built = apply_and_commit(root, &wt, &base, &cfg, &tasks, &picked);
    git::git(root, &["worktree", "remove", "--force", &wt_arg])?;
    if let Err(err) = built {
        git::git(root, &["branch", "-D", &branch])?;
        return Err(err);
    }

    let description = root
        .join(&dir)
        .join("pr")
        .join(format!("{}.md", ids.join("-")));
    let stat = git::git(root, &["diff", "--stat", &base, &branch])?;
    let policy = contribution_policy::find(root).map_err(|e| PrError::Refused(vec![e]))?;
    let refused = opts.push && !opts.policy_read && !policy.is_empty();
    let mut text = describe(root, &dir, &tasks, &picked, &stat)?;
    text.push_str(&policy_section(&policy, opts, refused));
    if let Some(parent) = description.parent() {
        fs::create_dir_all(parent).map_err(io(parent.display()))?;
    }
    fs::write(&description, text).map_err(io(description.display()))?;
    // the description is the only record of the contribution-policy decision, so it is committed rather than left for the next tidy-up
    let record = format!("pr/{}.md", ids.join("-"));
    // the record alone: an operator's uncommitted queue edits are not this command's to commit
    git::commit_instance_only(root, &dir, &[&record], &format!("pr {}", ids.join("-")))?;

    if refused {
        git::git(root, &["branch", "-D", &branch])?;
        return Err(PrError::Policy {
            sentences: policy.iter().map(cite).collect(),
            description,
        });
    }

    let mut report = PrReport {
        branch,
        base: onto,
        description,
        opened: None,
        policy,
    };
    if opts.push {
        git::git(root, &["push", "-q", "origin", &report.branch])?;
        report.opened = Some(open(root, &report)?);
    }
    Ok(report)
}

pub fn cite(f: &Finding) -> String {
    format!("{}:{} {}", f.path, f.line, f.message)
}

fn policy_section(policy: &[Finding], opts: &PrOpts, refused: bool) -> String {
    if policy.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n## Contribution policy\n\n");
    for f in policy {
        out.push_str(&format!("- {}\n", cite(f)));
    }
    out.push_str(if refused {
        "\nrefused: `enallagi pr --push` without `--policy-read`, nothing was pushed.\n"
    } else if opts.push {
        "\npushed with `--policy-read`: the operator read the sentences above.\n"
    } else {
        "\nnot pushed: `enallagi pr` ran without `--push`.\n"
    });
    out
}

fn parse(text: &str) -> Result<Vec<queue::Block>, PrError> {
    queue::parse(text).map_err(|e| PrError::Refused(vec![e.to_string()]))
}

fn io(path: impl std::fmt::Display) -> impl FnOnce(std::io::Error) -> PrError {
    let path = path.to_string();
    move |source| PrError::Io { path, source }
}

// origin/HEAD is only set by a clone, so ask the remote
fn default_branch(root: &Path) -> Result<String, PrError> {
    let out = git::git(root, &["ls-remote", "--symref", "origin", "HEAD"])?;
    out.lines()
        .find_map(|l| l.strip_prefix("ref: refs/heads/"))
        .and_then(|l| l.split_whitespace().next())
        .map(String::from)
        .ok_or_else(|| PrError::Refused(vec!["origin names no default branch".to_string()]))
}

fn commits(root: &Path, range: &str, ids: &[String]) -> Result<Vec<(String, String)>, PrError> {
    let log = git::git(
        root,
        &["log", "--reverse", "--no-merges", "--format=%h %s", range],
    )?;
    Ok(log
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(_, subject)| ids.iter().any(|id| gates::names_task(subject, id)))
        .map(|(sha, subject)| (sha.to_string(), subject.to_string()))
        .collect())
}

fn apply_and_commit(
    root: &Path,
    wt: &Path,
    base: &str,
    cfg: &config::Config,
    tasks: &[Task],
    picked: &[(String, String)],
) -> Result<(), PrError> {
    for (sha, _) in picked {
        let patch = git::git(root, &["format-patch", "-1", "--stdout", "--binary", sha])?;
        let mut apply = Command::new("git")
            .current_dir(wt)
            .args(["apply", "--3way", "--index"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(io("git apply"))?;
        if let Some(mut stdin) = apply.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(format!("{patch}\n").as_bytes())
                .map_err(io("git apply"))?;
        }
        if !apply.wait().map_err(io("git apply"))?.success() {
            let unmerged = git::git(wt, &["diff", "--name-only", "--diff-filter=U"])?;
            let mut files: Vec<String> = unmerged.lines().map(String::from).collect();
            if files.is_empty() {
                files = git::git(root, &["show", "--format=", "--name-only", sha])?
                    .lines()
                    .map(String::from)
                    .collect();
            }
            return Err(PrError::Conflict {
                base: base.to_string(),
                files,
            });
        }
    }

    let report = gates::check_delta(wt, cfg, false);
    if report.exit != 0 {
        return Err(PrError::Check(report.output));
    }

    let round = git::git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let head = git::head(root).unwrap_or_default();
    let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    let msg = format!(
        "{}\n\n{}, from {round} at {head}.",
        subject(tasks, picked),
        ids.join(", ")
    );
    git::git(wt, &["add", "-A"])?;
    git::git(
        wt,
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", &msg],
    )?;
    Ok(())
}

// the task's last commit subject with its id dropped: `feat(scope): T-### x` -> `feat(scope): x`.
// A block title is the finding in the finding's own words and runs long; a pull request wants the
// one line the commit already wrote. The block's title is the body's heading, not the subject.
const SUBJECT_MAX: usize = 72;

fn subject(tasks: &[Task], picked: &[(String, String)]) -> String {
    let written = regex::Regex::new(r"^([a-z]+(?:\([^)]*\))?!?):\s*T-\d+\s+(.+)$").expect("regex");
    let line = picked
        .iter()
        .rev()
        .find_map(|(_, s)| written.captures(s))
        .map(|c| format!("{}: {}", &c[1], &c[2]))
        .unwrap_or_else(|| tasks[0].title.clone());
    match line.char_indices().nth(SUBJECT_MAX) {
        None => line,
        Some((at, _)) => format!("{}...", line[..at].trim_end()),
    }
}

fn describe(
    root: &Path,
    dir: &str,
    tasks: &[Task],
    picked: &[(String, String)],
    stat: &str,
) -> Result<String, PrError> {
    let mut out = format!(
        "# {}\n\n## What changed\n\n```\n{stat}\n```\n",
        subject(tasks, picked)
    );
    let state = git::state_root(root, dir);
    let recorded = if state == root {
        String::new()
    } else {
        git::git(&state, &["log", "--reverse", "--format=%h %s"]).unwrap_or_default()
    };
    for task in tasks {
        let body = queue::block_text(&task.block);
        let criteria: Vec<&str> = body
            .lines()
            .skip_while(|l| !l.starts_with("criteria:"))
            .skip(1)
            .take_while(|l| l.starts_with(' '))
            .collect();
        let notes: Vec<&str> = body
            .lines()
            .skip_while(|l| !l.starts_with("notes:"))
            .collect();
        let verdict = notes
            .iter()
            .rposition(|l| {
                let l = l.trim_start().trim_start_matches("notes:").trim_start();
                l.to_lowercase().starts_with("verifier")
            })
            .map(|at| notes[at..].join("\n"))
            .unwrap_or_default();
        out.push_str(&format!(
            "\n## {} {}\n\n{}\n",
            task.id,
            task.title,
            criteria.join("\n")
        ));
        out.push_str("\n### Iterations\n\n");
        let product = picked
            .iter()
            .filter(|(_, s)| gates::names_task(s, &task.id));
        let state_log = recorded
            .lines()
            .filter_map(|l| l.split_once(' '))
            .filter(|(_, s)| gates::names_task(s, &task.id));
        for (sha, subject) in product {
            out.push_str(&format!("- {sha} {subject}\n"));
        }
        for (sha, subject) in state_log {
            out.push_str(&format!("- {dir} {sha} {subject}\n"));
        }
        out.push_str(&format!("\n### Verification\n\n{verdict}\n"));
    }
    Ok(out)
}

// gh missing is not a failure: the branch is pushed and the description is on disk
fn open(root: &Path, report: &PrReport) -> Result<String, PrError> {
    let subject = git::git(root, &["log", "-1", "--format=%s", &report.branch])?;
    let spawned = Command::new("gh")
        .current_dir(root)
        .args([
            "pr",
            "create",
            "--base",
            &report.base,
            "--head",
            &report.branch,
            "--title",
        ])
        .arg(subject)
        .arg("--body-file")
        .arg(&report.description)
        .output();
    match spawned {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok("gh is not installed; open the pull request from the pushed branch".to_string())
        }
        Err(e) => Err(PrError::Io {
            path: "gh".to_string(),
            source: e,
        }),
        Ok(out) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
        Ok(out) => Err(PrError::Gh(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        )),
    }
}
