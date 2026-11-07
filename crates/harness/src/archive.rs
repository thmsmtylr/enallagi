//! Expires untriaged tickets and rolls PROGRESS.md over past a size cap.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::block;
use crate::config::{self, Config};
use crate::git;
use crate::tracker::{self, Tracker};

const PROGRESS_MAX_DEFAULT: usize = 2000;
const PROGRESS_KEEP_DEFAULT: usize = 200;
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive: the queue has uncommitted changes — commit the verdict first")]
    UncommittedTasks,
    #[error(transparent)]
    Queue(#[from] block::QueueError),
    #[error(transparent)]
    Git(#[from] git::GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct ArchiveReport {
    pub expired: Vec<String>,
    pub progress_rolled: usize,
    pub refused: Option<String>,
}

// newest first, each with its round: a loop commit's `Iteration:` trailer, else the commit itself
fn state_commits(root: &Path, dir: &str) -> (PathBuf, Vec<(String, String)>) {
    let rel = config::instance_rel(root, dir, tracker::SPECS);
    let (repo, _, _) = git::locate(root, dir, &rel);
    let format = "--format=%H %(trailers:key=Iteration,valueonly,separator=)";
    // a landing merge is no round; topo order lists the queuing commit after every later one
    let commits = git::git(&repo, &["log", "--no-merges", "--topo-order", format])
        .map(|out| {
            out.lines()
                .map(|l| l.split_once(' ').unwrap_or((l, "")))
                .map(|(sha, it)| (sha.to_string(), it.trim().to_string()))
                .collect()
        })
        .unwrap_or_default();
    (repo, commits)
}

// rounds since the commit that queued the ticket, one per loop iteration or hand commit, and
// whether a root commit queued it; an id no commit carries is one a hand edit just wrote
fn aged(root: &Path, cfg: &Config, ids: &[String]) -> BTreeMap<String, (usize, bool)> {
    let dir = &cfg.layout.harness_dir;
    let (repo, commits) = state_commits(root, dir);
    let roots = git::git(&repo, &["rev-list", "--max-parents=0", "HEAD"]).unwrap_or_default();
    ids.iter()
        .map(|id| {
            let sha = tracker::queued_commit(root, dir, id).ok().flatten();
            let at = sha
                .as_ref()
                .and_then(|sha| commits.iter().position(|(c, _)| c == sha))
                .unwrap_or(0);
            let rounds: HashSet<&str> = commits[..at]
                .iter()
                .map(|(c, it)| if it.is_empty() { c } else { it })
                .map(String::as_str)
                .collect();
            let root = sha.is_some_and(|sha| roots.lines().any(|r| r == sha));
            (id.clone(), (rounds.len(), root))
        })
        .collect()
}

pub(crate) fn ages(root: &Path, cfg: &Config, ids: &[String]) -> BTreeMap<String, usize> {
    aged(root, cfg, ids)
        .into_iter()
        .map(|(id, (age, _))| (id, age))
        .collect()
}

// rewriting the queue from outside a running loop's own lane races it: one checkout, one writer
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
            expired: Vec::new(),
            progress_rolled: 0,
            refused: Some(
                "archive: an agent is running — the queue is its bus, not touching it".to_string(),
            ),
        });
    }

    // An uncommitted verdict folded into an archive commit loses its author and its message.
    let dir = &cfg.layout.harness_dir;
    let (repo, inner, _) = git::locate(root, dir, &config::instance_rel(root, dir, tracker::SPECS));
    if !dry_run && !git::git_ok(&repo, &["diff", "--quiet", "--", &inner]) {
        return Err(ArchiveError::UncommittedTasks);
    }

    let expired = expire_proposed(root, cfg, dry_run)?;
    let progress_rolled = roll_progress(root, dir, dry_run, progress_max, progress_keep)?;

    if dry_run {
        return Ok(ArchiveReport {
            expired,
            progress_rolled,
            refused: None,
        });
    }

    git::commit_instance(
        root,
        dir,
        &[tracker::SPECS, "PROGRESS.md", "PROGRESS.archive.md"],
        "chore(archive): expired tickets, old entries to PROGRESS.archive.md",
    )?;

    Ok(ArchiveReport {
        expired,
        progress_rolled,
        refused: None,
    })
}

// a proposal nobody decided stands in the way of every later one, so the queue drops the head
fn expire_proposed(root: &Path, cfg: &Config, dry_run: bool) -> Result<Vec<String>, ArchiveError> {
    let dir = &cfg.layout.harness_dir;
    let t = Tracker::open(root, dir);
    let proposed = tracker::ids_in(&t.blocks()?, &["needs-triage"]);
    if proposed.is_empty() {
        return Ok(Vec::new());
    }
    let aged = aged(root, cfg, &proposed);
    let age = |id: &String| aged.get(id).map_or(0, |a| a.0);
    // a root commit installed the queue whole, so a ticket it carries records no round it was filed in
    let mut stale: Vec<String> = proposed
        .into_iter()
        .filter(|id| age(id) >= cfg.queue.proposed_rounds && !aged.get(id).is_some_and(|a| a.1))
        .collect();
    stale.sort_by_key(|id| std::cmp::Reverse(age(id)));
    if stale.is_empty() || dry_run {
        return Ok(stale);
    }

    let reason = format!("expired after {} rounds", cfg.queue.proposed_rounds);
    for id in &stale {
        t.set_status(id, "wontfix", &reason)?;
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
    fn progress_rolls_keeps_header() {
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
        r.write(".enallagi/PROGRESS.md", &progress);
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
    fn a_standing_ticket_expires_to_wontfix() {
        let r = Repo::new();
        r.queue(
            ".enallagi",
            "## [T-001] a finding no one triaged\nstatus: needs-triage\n",
        );
        r.commit_all("file the finding");
        let t = Tracker::open(&r.root, ".enallagi");
        for n in 1..=6 {
            if n == 2 {
                t.append(
                    "repo",
                    "## [T-002] a finding filed later\nstatus: needs-triage\n",
                )
                .unwrap();
            }
            r.write(".enallagi/PROGRESS.md", &format!("round {n}\n"));
            r.commit_all(&format!("round {n}"));
        }

        let cfg = cfg(&r.root);
        let report = archive_done(&r.root, &cfg, false).expect("archive_done");

        assert_eq!(report.expired, vec!["T-001".to_string()]);
        let expired = t.get("T-001").unwrap().unwrap();
        assert_eq!(tracker::state(&expired).as_deref(), Some("wontfix"));
        assert_eq!(
            block::field(&expired, "gate"),
            Some(format!(
                "expired after {} rounds",
                cfg.queue.proposed_rounds
            ))
        );
        let standing = t.get("T-002").unwrap().unwrap();
        assert_eq!(tracker::state(&standing).as_deref(), Some("needs-triage"));
        assert!(!config::instance_path(&r.root, ".enallagi", "DECISIONS.md").exists());
    }

    #[test]
    fn one_iteration_one_round() {
        let r = Repo::new();
        r.queue(
            ".enallagi",
            "## [T-001] a finding one iteration filed\nstatus: needs-triage\n",
        );
        r.commit_all("file the finding");
        for n in 1..=8 {
            r.write(".enallagi/PROGRESS.md", &format!("commit {n}\n"));
            r.commit_all(&format!("commit {n}\n\nIteration: run-a/{}", n / 3));
        }

        let cfg = cfg(&r.root);
        let aged = ages(&r.root, &cfg, &["T-001".to_string()]);
        assert_eq!(aged["T-001"], 3, "{aged:?}");
        let report = archive_done(&r.root, &cfg, false).expect("archive_done");
        assert!(report.expired.is_empty(), "{:?}", report.expired);
    }

    #[test]
    fn landing_merge_uncounted() {
        let r = Repo::new();
        r.queue(".enallagi", "## [T-001] a\nstatus: needs-triage\n");
        r.commit_all("file the finding");
        let main = git::git(&r.root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        for n in 1..=2 {
            git::git(&r.root, &["checkout", "-q", "-b", &format!("lane{n}")]).unwrap();
            r.write(".enallagi/PROGRESS.md", &format!("lane {n}\n"));
            r.commit_all(&format!("lane {n}\n\nIteration: run-{n}/1"));
            git::git(&r.root, &["checkout", "-q", &main]).unwrap();
            let lane = format!("lane{n}");
            let merge = [
                "-c",
                "commit.gpgsign=false",
                "merge",
                "-q",
                "--no-ff",
                "-m",
                "land",
                &lane,
            ];
            git::git(&r.root, &merge).unwrap();
        }
        let aged = ages(&r.root, &cfg(&r.root), &["T-001".to_string()]);
        assert_eq!(aged["T-001"], 2, "{aged:?}");
    }

    #[test]
    fn root_carried_ticket_stands() {
        let r = Repo::new();
        r.queue(
            ".enallagi",
            "## [T-001] a finding the reinstall carried\nstatus: needs-triage\n",
        );
        let t = Tracker::open(&r.root, ".enallagi");
        git::git(&r.root, &["add", "-A"]).unwrap();
        git::git(
            &r.root,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--amend",
                "--no-edit",
            ],
        )
        .unwrap();
        // T-002 arrives late enough to stand, so an empty `expired` speaks for T-001 alone
        for n in 1..=10 {
            if n == 8 {
                t.append(
                    "repo",
                    "## [T-002] a finding filed later\nstatus: needs-triage\n",
                )
                .unwrap();
            }
            r.write(".enallagi/PROGRESS.md", &format!("round {n}\n"));
            r.commit_all(&format!("round {n}"));
        }

        let cfg = cfg(&r.root);
        let ids = vec!["T-001".to_string(), "T-002".to_string()];
        let aged = ages(&r.root, &cfg, &ids);
        assert!(aged["T-001"] > aged["T-002"], "{aged:?}");

        let report = archive_done(&r.root, &cfg, false).expect("archive_done");
        assert!(report.expired.is_empty(), "{:?}", report.expired);
        let carried = t.get("T-001").unwrap().unwrap();
        assert_eq!(tracker::state(&carried).as_deref(), Some("needs-triage"));
    }

    #[test]
    fn refuses_under_another_loop() {
        let r = Repo::new();
        r.queue(".enallagi", "## [T-001] x\nstatus: needs-triage\n");
        r.commit_all("seed tasks");

        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        r.write(".enallagi/loop.pid", &child.id().to_string());

        let cfg = cfg(&r.root);
        let ticket = Tracker::open(&r.root, ".enallagi")
            .path_of("T-001")
            .unwrap();
        let before = fs::read_to_string(&ticket).unwrap();
        let report = archive_done(&r.root, &cfg, false).expect("archive_done");
        let after = fs::read_to_string(&ticket).unwrap();

        assert!(report.refused.is_some());
        assert_eq!(before, after);

        let _ = child.kill();
        let _ = child.wait();
    }
}
