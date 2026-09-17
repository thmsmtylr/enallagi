//! Moves `done` and expired task blocks out of TASKS.md into DECISIONS.md, and rolls PROGRESS.md over past a size cap.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config::{self, Config};
use crate::git;
use crate::queue;

const PROGRESS_MAX_DEFAULT: usize = 2000;
const PROGRESS_KEEP_DEFAULT: usize = 200;
const EXPIRED: &str = "## Expired findings";
const DECISIONS_HEADER: &str = "# DECISIONS\n\nCompleted task blocks, verbatim, moved out of \
     TASKS.md once `done`.\nThe queue stays small; the audit trail stays whole. Each block is \
     the implementer's and\nthe verifier's own words, never summarised on the way in.\n";

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive: TASKS.md has uncommitted changes — commit the verdict first")]
    UncommittedTasks,
    #[error(transparent)]
    Queue(#[from] queue::QueueError),
    #[error(transparent)]
    Git(#[from] git::GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct ArchiveReport {
    pub moved: Vec<String>,
    pub expired: Vec<String>,
    pub progress_rolled: usize,
    pub refused: Option<String>,
}

// a block's age in state commits, counted from the one that first carried its heading
pub(crate) fn ages(root: &Path, cfg: &Config, ids: &[String]) -> BTreeMap<String, usize> {
    let dir = &cfg.layout.harness_dir;
    let tasks = config::instance_rel(root, dir, "TASKS.md");
    let (repo, inner, _) = git::locate(root, dir, &tasks);
    let commits: Vec<String> = git::git(&repo, &["log", "--format=%H"])
        .map(|out| out.lines().map(String::from).collect())
        .unwrap_or_default();
    ids.iter()
        .map(|id| (id.clone(), age_of(&repo, &inner, &commits, id)))
        .collect()
}

// an id no commit carries is one a hand edit just wrote, which is as new as a block gets
fn age_of(repo: &Path, inner: &str, commits: &[String], id: &str) -> usize {
    let block = queue::Block {
        id: id.to_string(),
        title: String::new(),
        line: 0,
        body: Vec::new(),
    };
    let heading = format!("^{}", regex::escape(queue::block_text(&block).trim_end()));
    git::git(
        repo,
        &[
            "log",
            "--reverse",
            "--format=%H",
            "--pickaxe-regex",
            "-S",
            &heading,
            "--",
            inner,
        ],
    )
    .ok()
    .and_then(|found| found.lines().next().map(String::from))
    .and_then(|sha| commits.iter().position(|c| *c == sha))
    .unwrap_or(0)
}

// rewriting TASKS.md from outside a running loop's own lane races it: one checkout, one writer
pub fn loop_live(root: &Path, harness_dir: &str) -> Option<u32> {
    let pidfile = root.join(harness_dir).join("loop.pid");
    let content = fs::read_to_string(pidfile).ok()?;
    let pid: u32 = content.trim().parse().ok()?;

    let alive = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !alive {
        return None;
    }

    let mut p = std::process::id();
    while p > 1 {
        if p == pid {
            return None;
        }
        p = match ppid_of(p) {
            Some(parent) => parent,
            None => break,
        };
    }
    Some(pid)
}

fn ppid_of(pid: u32) -> Option<u32> {
    let out = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn archive_done(
    root: &Path,
    cfg: &Config,
    dry_run: bool,
) -> Result<ArchiveReport, ArchiveError> {
    archive_done_with(
        root,
        cfg,
        dry_run,
        env_usize("PROGRESS_MAX", PROGRESS_MAX_DEFAULT),
        env_usize("PROGRESS_KEEP", PROGRESS_KEEP_DEFAULT),
    )
}

// split out so tests can pin PROGRESS_MAX/PROGRESS_KEEP as arguments instead of process env vars, which parallel tests would race on
pub fn archive_done_with(
    root: &Path,
    cfg: &Config,
    dry_run: bool,
    progress_max: usize,
    progress_keep: usize,
) -> Result<ArchiveReport, ArchiveError> {
    if let Some(_pid) = loop_live(root, &cfg.layout.harness_dir) {
        return Ok(ArchiveReport {
            moved: Vec::new(),
            expired: Vec::new(),
            progress_rolled: 0,
            refused: Some(
                "archive: an agent is running — TASKS.md is its bus, not touching it".to_string(),
            ),
        });
    }

    // An uncommitted verdict folded into an archive commit loses its author and its message.
    let dir = &cfg.layout.harness_dir;
    let tasks = config::instance_rel(root, dir, "TASKS.md");
    let (repo, inner, _) = git::locate(root, dir, &tasks);
    if !dry_run && !git::git_ok(&repo, &["diff", "--quiet", "--", &inner]) {
        return Err(ArchiveError::UncommittedTasks);
    }

    let moved = archive_tasks(root, dir, dry_run)?;
    let expired = expire_proposed(root, cfg, dry_run)?;
    let progress_rolled = roll_progress(root, dir, dry_run, progress_max, progress_keep)?;

    if dry_run {
        return Ok(ArchiveReport {
            moved,
            expired,
            progress_rolled,
            refused: None,
        });
    }

    git::commit_instance(
        root,
        dir,
        &[
            "TASKS.md",
            "DECISIONS.md",
            "PROGRESS.md",
            "PROGRESS.archive.md",
        ],
        "chore(archive): finished blocks to DECISIONS.md, old entries to PROGRESS.archive.md",
    )?;

    Ok(ArchiveReport {
        moved,
        expired,
        progress_rolled,
        refused: None,
    })
}

fn archive_tasks(root: &Path, dir: &str, dry_run: bool) -> Result<Vec<String>, ArchiveError> {
    let tasks = config::instance_rel(root, dir, "TASKS.md");
    let tasks_path = root.join(&tasks);
    let src = fs::read_to_string(&tasks_path)?;
    let blocks = queue::parse(&src)?;

    let mut archived: Vec<(String, String)> = Vec::new();
    let moved: Vec<String>;

    {
        let (repo, inner, nested) = git::locate(root, dir, &tasks);
        let sha = git::git(&repo, &["rev-parse", "--short", "HEAD"])?;
        let show = if nested {
            format!("git -C {dir} show {sha}:{inner}")
        } else {
            format!("git show {sha}:{tasks}")
        };
        let lines: Vec<&str> = src.split('\n').collect();
        let mut out: Vec<String> = Vec::new();
        let mut cursor = 0usize;

        for b in &blocks {
            let start = b.line - 1;
            let stop = start + b.body.len() + 1;
            out.extend(lines[cursor..start].iter().map(|s| s.to_string()));
            let body = &lines[start..stop];
            cursor = stop;

            let is_done = queue::field(b, "status").as_deref() == Some("done");
            let already_archived = queue::field(b, "archived").is_some();
            if !is_done || already_archived {
                out.extend(body.iter().map(|s| s.to_string()));
                continue;
            }

            // keep the FIRST of each key, the one field() reads
            let mut keep: Vec<String> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for l in &body[1..] {
                let key = l.split(':').next().unwrap_or("");
                if matches!(key, "blockedBy" | "scope" | "attended") && seen.insert(key.to_string())
                {
                    keep.push((*l).to_string());
                }
            }
            out.push(body[0].to_string());
            out.extend(keep);
            out.push("status: done".to_string());
            out.push(format!("archived: DECISIONS.md — full block at `{show}`"));
            out.push(String::new());

            let block_text = body.join("\n");
            archived.push((b.id.clone(), format!("{}\n", block_text.trim_end())));
        }
        out.extend(lines[cursor..].iter().map(|s| s.to_string()));

        moved = archived.iter().map(|(id, _)| id.clone()).collect();

        if !dry_run && !archived.is_empty() {
            let decisions_path = config::instance_path(root, dir, "DECISIONS.md");
            let dec = fs::read_to_string(&decisions_path)
                .unwrap_or_else(|_| DECISIONS_HEADER.to_string());
            let joined = archived
                .iter()
                .map(|(_, b)| b.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            fs::write(&decisions_path, with_archived(&dec, &joined))?;
            fs::write(&tasks_path, out.join("\n"))?;
        }
    }

    Ok(moved)
}

// the expired section is the file's last, so a `done` block always has somewhere above it to land
fn split_at_expired(decisions: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = decisions.split('\n').collect();
    match lines.iter().position(|l| l.trim_end() == EXPIRED) {
        Some(i) => (lines[..i].join("\n"), Some(lines[i..].join("\n"))),
        None => (decisions.to_string(), None),
    }
}

fn with_archived(decisions: &str, joined: &str) -> String {
    match split_at_expired(decisions) {
        (head, Some(tail)) => format!(
            "{}\n\n{}\n\n{}\n",
            head.trim_end(),
            joined.trim_end(),
            tail.trim_end()
        ),
        (_, None) => format!("{}\n\n{}", decisions.trim_end(), joined),
    }
}

fn with_expired(decisions: &str, blocks: &[String]) -> String {
    let joined = blocks.join("\n\n");
    let dec = match decisions.trim_end() {
        "" => DECISIONS_HEADER.trim_end().to_string(),
        text => text.to_string(),
    };
    if split_at_expired(&dec).1.is_some() {
        return format!("{dec}\n\n{}\n", joined.trim_end());
    }
    format!(
        "{dec}\n\n{EXPIRED}\n\nProposals no adjudicator decided in time, verbatim, moved out \
         of TASKS.md.\n\n{}\n",
        joined.trim_end()
    )
}

// a proposal nobody decided stands in the way of every later one, so the queue drops the head
fn expire_proposed(root: &Path, cfg: &Config, dry_run: bool) -> Result<Vec<String>, ArchiveError> {
    let dir = &cfg.layout.harness_dir;
    let tasks = config::instance_rel(root, dir, "TASKS.md");
    let tasks_path = root.join(&tasks);
    let src = fs::read_to_string(&tasks_path)?;
    let blocks = queue::parse(&src)?;
    let proposed = queue::ids_at(&blocks, "proposed");
    if proposed.is_empty() {
        return Ok(Vec::new());
    }
    let ages = ages(root, cfg, &proposed);
    let stale: Vec<String> = proposed
        .into_iter()
        .filter(|id| ages.get(id).copied().unwrap_or(0) >= cfg.queue.proposed_rounds)
        .collect();
    if stale.is_empty() {
        return Ok(Vec::new());
    }

    let lines: Vec<&str> = src.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut moved: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    for b in &blocks {
        let start = b.line - 1;
        let stop = start + b.body.len() + 1;
        out.extend(lines[cursor..start].iter().map(|s| s.to_string()));
        let body = &lines[start..stop];
        cursor = stop;
        if stale.contains(&b.id) {
            moved.push(format!("{}\n", body.join("\n").trim_end()));
        } else {
            out.extend(body.iter().map(|s| s.to_string()));
        }
    }
    out.extend(lines[cursor..].iter().map(|s| s.to_string()));

    if !dry_run {
        let decisions_path = config::instance_path(root, dir, "DECISIONS.md");
        let dec = fs::read_to_string(&decisions_path).unwrap_or_default();
        fs::write(&decisions_path, with_expired(&dec, &moved))?;
        fs::write(&tasks_path, out.join("\n"))?;
    }
    Ok(stale)
}

// split point snaps to the nearest entry heading so no entry is cut in half
fn roll_progress(
    root: &Path,
    dir: &str,
    dry_run: bool,
    max: usize,
    keep: usize,
) -> Result<usize, ArchiveError> {
    let progress_path = config::instance_path(root, dir, "PROGRESS.md");
    let text = match fs::read_to_string(&progress_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() <= max {
        return Ok(0);
    }

    let rule = lines
        .iter()
        .position(|l| l.trim() == "---")
        .map(|i| i + 1)
        .unwrap_or(1);
    let (head, body) = lines.split_at(rule);
    let start = body.len().saturating_sub(keep);
    let split = body
        .iter()
        .enumerate()
        .find(|(i, l)| l.starts_with("## ") && *i >= start)
        .map(|(i, _)| i)
        .unwrap_or(start);
    let (moved, kept) = body.split_at(split);
    if moved.is_empty() {
        return Ok(0);
    }
    if dry_run {
        return Ok(moved.len());
    }

    let note = "<!-- Entries before this point are in PROGRESS.archive.md. Nothing reads it; it \
                is the record. -->";
    let archive_path = config::instance_path(root, dir, "PROGRESS.archive.md");
    let archive_prefix = match fs::read_to_string(&archive_path) {
        Ok(existing) => format!("{}\n\n", existing.trim_end()),
        Err(_) => "# PROGRESS (archive)\n\nEntries rolled out of PROGRESS.md by \
                   `enallagi run`, oldest first.\nThe loop does not read this file. It \
                   exists so the record stays whole.\n\n"
            .to_string(),
    };
    let moved_text = moved.join("\n");
    fs::write(
        &archive_path,
        format!("{}{}\n", archive_prefix, moved_text.trim()),
    )?;

    let mut new_progress: Vec<&str> = head.to_vec();
    new_progress.push("");
    new_progress.push(note);
    new_progress.push("");
    new_progress.extend(kept.iter());
    fs::write(&progress_path, new_progress.join("\n"))?;

    Ok(moved.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;

    fn cfg(root: &Path) -> Config {
        crate::config::load(root).expect("load default config")
    }

    fn read(r: &Repo, name: &str) -> String {
        fs::read_to_string(config::instance_path(
            &r.root,
            &cfg(&r.root).layout.harness_dir,
            name,
        ))
        .unwrap()
    }

    #[test]
    fn a_done_block_moves_to_decisions_and_leaves_a_stub() {
        let r = Repo::new();
        r.write(
            "TASKS.md",
            "# TASKS\n\n\
             ## [T-001] Some finished work\n\
             status: done\n\
             notes: line one\n\
             notes: line two\n\
             notes: line three\n\
             \n\
             ## [T-002] Still open\n\
             status: ready\n",
        );
        r.write("DECISIONS.md", "# DECISIONS\n");
        r.commit_all("seed tasks");

        let cfg = cfg(&r.root);
        let report = archive_done(&r.root, &cfg, false).expect("archive_done");

        assert!(report.refused.is_none());
        assert_eq!(report.moved, vec!["T-001".to_string()]);

        let decisions = read(&r, "DECISIONS.md");
        assert!(decisions.contains("## [T-001] Some finished work"));
        assert!(decisions.contains("status: done"));
        assert!(decisions.contains("notes: line one"));
        assert!(decisions.contains("notes: line two"));
        assert!(decisions.contains("notes: line three"));

        let tasks = read(&r, "TASKS.md");
        assert!(tasks.contains("## [T-001] Some finished work"));
        assert!(!tasks.contains("notes: line one"));
        assert!(tasks.contains("status: done"));
        assert!(tasks.contains("archived: DECISIONS.md"));
        assert!(tasks.contains("## [T-002] Still open"));
        assert!(tasks.contains("status: ready"));
    }

    #[test]
    fn a_quoted_block_is_not_archived() {
        let r = Repo::new();
        r.write(
            "TASKS.md",
            "# TASKS\n\n\
             ## [T-001] done, and its notes quote a block\n\
             status: done\n\
             notes: |\n\
             ```\n\
             ## [T-999] the quoted block, not a task\n\
             status: done\n\
             ```\n\
             \n\
             ## [T-002] the next real task\n\
             status: ready\n",
        );
        r.commit_all("seed tasks with a quote");

        let cfg = cfg(&r.root);
        let report = archive_done(&r.root, &cfg, false).expect("archive_done");

        assert!(report.refused.is_none());
        assert_eq!(report.moved, vec!["T-001".to_string()]);
        assert!(!report.moved.contains(&"T-999".to_string()));

        let tasks = read(&r, "TASKS.md");
        let blocks = queue::parse(&tasks).unwrap();
        assert_eq!(queue::ids_at(&blocks, "ready"), vec!["T-002".to_string()]);
        assert!(!blocks.iter().any(|b| b.id == "T-999"));
    }

    #[test]
    fn progress_rolls_and_keeps_its_header() {
        let r = Repo::new();
        let header = "# PROGRESS\n\n\
             The loop's own record, one entry per iteration, newest last.\n\n\
             ## Entry format\n\n\
             ```\n\
             ## <date> — <task id> — <landed | BLOCKED | rejected>\n\
             next: <what the following iteration inherits>\n\
             ```\n\n\
             ---";
        let mut progress = header.to_string();
        for n in 1..=6 {
            progress.push_str(&format!(
                "\n\n## fixture entry {n}\nfriction: none\nnext: nothing\n"
            ));
        }
        r.write("PROGRESS.md", &progress);
        r.write("TASKS.md", "# TASKS\n");
        r.commit_all("seed progress");

        let cfg = cfg(&r.root);
        let report = archive_done_with(&r.root, &cfg, false, 12, 4).expect("archive_done");

        assert!(report.refused.is_none());
        assert!(report.progress_rolled > 0);

        let archive = read(&r, "PROGRESS.archive.md");
        assert!(archive.contains("fixture entry 1"));
        let pos1 = archive.find("fixture entry 1").unwrap();
        let pos2 = archive.find("fixture entry 2").unwrap();
        assert!(pos1 < pos2, "oldest entry comes first in the archive");

        let rolled = read(&r, "PROGRESS.md");
        assert!(!rolled.contains("fixture entry 1"));
        assert!(rolled.contains("fixture entry 6"));
        assert!(rolled.contains("## Entry format"));

        let after_note = rolled
            .split("Nothing reads it; it is the record. -->")
            .nth(1)
            .unwrap();
        let next_heading = after_note
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap()
            .trim();
        assert_eq!(next_heading, "## fixture entry 6");
    }

    #[test]
    fn a_proposed_block_expires_after_its_rounds() {
        let r = Repo::new();
        let first = "# TASKS\n\n\
             ## [T-001] a finding no one decided\n\
             status: proposed\n\
             notes: the whole block\n";
        r.write("TASKS.md", first);
        r.write("DECISIONS.md", "# DECISIONS\n");
        r.commit_all("file the proposal");
        // T-002 arrives two rounds in, so one bound covers both the expiring block and a standing one
        for n in 1..=6 {
            if n == 2 {
                r.write(
                    "TASKS.md",
                    &format!("{first}\n## [T-002] a finding filed later\nstatus: proposed\n"),
                );
            }
            r.write("PROGRESS.md", &format!("round {n}\n"));
            r.commit_all(&format!("round {n}"));
        }

        let cfg = cfg(&r.root);
        let report = archive_done(&r.root, &cfg, false).expect("archive_done");

        assert_eq!(report.expired, vec!["T-001".to_string()]);

        let tasks = read(&r, "TASKS.md");
        assert!(!tasks.contains("T-001"), "{tasks}");
        assert!(
            tasks.contains("## [T-002] a finding filed later"),
            "{tasks}"
        );

        let decisions = read(&r, "DECISIONS.md");
        let at = decisions
            .find("## Expired findings")
            .unwrap_or_else(|| panic!("{decisions}"));
        assert!(decisions[..at].contains("# DECISIONS"), "{decisions}");
        assert!(
            decisions[at..].contains("## [T-001] a finding no one decided"),
            "{decisions}"
        );
        assert!(
            decisions[at..].contains("notes: the whole block"),
            "{decisions}"
        );
    }

    #[test]
    fn an_archived_block_lands_above_the_expired() {
        let r = Repo::new();
        r.write("TASKS.md", "# TASKS\n\n## [T-003] finished\nstatus: done\n");
        r.write(
            "DECISIONS.md",
            "# DECISIONS\n\n## Expired findings\n\n## [T-001] expired earlier\nstatus: proposed\n",
        );
        r.commit_all("seed tasks");

        let cfg = cfg(&r.root);
        archive_done(&r.root, &cfg, false).expect("archive_done");

        let decisions = read(&r, "DECISIONS.md");
        let done = decisions.find("## [T-003] finished").expect(&decisions);
        let expired = decisions.find("## Expired findings").expect(&decisions);
        assert!(done < expired, "{decisions}");
    }

    #[test]
    fn refuses_to_rewrite_under_another_loop() {
        let r = Repo::new();
        r.write("TASKS.md", "# TASKS\n\n## [T-001] x\nstatus: done\n");
        r.commit_all("seed tasks");

        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        r.write(".enallagi/loop.pid", &child.id().to_string());

        let cfg = cfg(&r.root);
        let before = read(&r, "TASKS.md");
        let report = archive_done(&r.root, &cfg, false).expect("archive_done");
        let after = read(&r, "TASKS.md");

        assert!(report.refused.is_some());
        assert_eq!(before, after);

        let _ = child.kill();
        let _ = child.wait();
    }
}
