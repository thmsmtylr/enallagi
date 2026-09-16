//! Removes what `enallagi init` put into a repository: the harness directory, the entry points it excluded, and the exclude block.

use crate::{config, git};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// init.rs writes this block; the eject tests round-trip init through eject, so the two cannot drift silently
const EXCLUDE_OPEN: &str = "# >>> harness";
const EXCLUDE_CLOSE: &str = "# <<< harness";

#[derive(Debug, Default)]
pub struct EjectOpts {
    pub dry_run: bool,
    pub keep_record: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct EjectReport {
    /// Paths relative to the root, removed or, under `--dry-run`, to be removed.
    pub removed: Vec<String>,
    /// Excluded paths the product tracks since init wrote them, left alone.
    pub kept: Vec<String>,
    /// The git path of the exclude file, when it held the block.
    pub exclude: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum EjectError {
    #[error("eject refused, nothing was removed:\n  {}", .0.join("\n  "))]
    Refused(Vec<String>),
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error(transparent)]
    Git(#[from] git::GitError),
}

fn io(path: impl std::fmt::Display) -> impl FnOnce(std::io::Error) -> EjectError {
    let path = path.to_string();
    move |source| EjectError::Io { path, source }
}

pub fn eject(root: &Path, opts: &EjectOpts) -> Result<EjectReport, EjectError> {
    let dir = config::harness_dir(root);
    let mut refusals = lanes(root, &dir)?;
    if let Some(pid) = loop_pid(root, &dir) {
        refusals.push(format!("a loop is live: pid {pid} in {dir}/loop.pid"));
    }
    if let Some(record) = &opts.keep_record {
        refusals.extend(record_refusal(root, record));
    }
    if !refusals.is_empty() {
        return Err(EjectError::Refused(refusals));
    }

    let exclude = git::git(root, &["rev-parse", "--git-path", "info/exclude"])?;
    let text = fs::read_to_string(root.join(&exclude)).unwrap_or_default();
    let (outside, entries) = split_block(&text);

    let mut report = EjectReport::default();
    let candidates = std::iter::once(dir.clone()).chain(entries);
    for path in candidates {
        let exists = root.join(&path).symlink_metadata().is_ok();
        let seen = report.removed.contains(&path) || report.kept.contains(&path);
        if path.is_empty() || !exists || seen {
            continue;
        }
        if git::git(root, &["ls-files", "--", &path]).is_ok_and(|out| !out.is_empty()) {
            report.kept.push(path);
        } else {
            report.removed.push(path);
        }
    }
    if text.lines().any(|l| l == EXCLUDE_OPEN) {
        report.exclude = Some(exclude.clone());
    }
    if opts.dry_run {
        return Ok(report);
    }

    for path in &report.removed {
        let at = root.join(path);
        match &opts.keep_record {
            Some(record) if *path == dir => move_dir(&at, record)?,
            _ if at.is_dir() => fs::remove_dir_all(&at).map_err(io(at.display()))?,
            _ => fs::remove_file(&at).map_err(io(at.display()))?,
        }
        let mut parent = at.parent();
        while let Some(emptied) = parent.filter(|d| *d != root && fs::remove_dir(d).is_ok()) {
            parent = emptied.parent();
        }
    }
    if report.exclude.is_some() {
        fs::write(root.join(&exclude), outside).map_err(io(&exclude))?;
    }
    Ok(report)
}

// every registered worktree under the harness directory is a lane, left or running
fn lanes(root: &Path, dir: &str) -> Result<Vec<String>, EjectError> {
    let Ok(under) = fs::canonicalize(root.join(dir).join("worktrees")) else {
        return Ok(Vec::new());
    };
    let list = git::git(root, &["worktree", "list", "--porcelain"])?;
    Ok(list
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .filter(|wt| Path::new(wt).starts_with(&under))
        .map(|wt| format!("a lane worktree exists: {wt}"))
        .collect())
}

// not archive::loop_live, which exempts the loop's own descendants: a lane inside the loop must not delete it
fn loop_pid(root: &Path, dir: &str) -> Option<u32> {
    let pid: u32 = fs::read_to_string(root.join(dir).join("loop.pid"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|s| s.success())
        .then_some(pid)
}

fn record_refusal(root: &Path, record: &Path) -> Option<String> {
    if record.symlink_metadata().is_ok() {
        return Some(format!(
            "{} already exists; nothing is overwritten",
            record.display()
        ));
    }
    let parent = record.parent().and_then(|p| fs::canonicalize(p).ok());
    let inside = match (parent, fs::canonicalize(root)) {
        (Some(parent), Ok(root)) => parent.starts_with(root),
        _ => return Some(format!("{}: its parent does not exist", record.display())),
    };
    inside.then(|| {
        format!(
            "{} is inside the repository; the record goes outside it",
            record.display()
        )
    })
}

// rename cannot cross filesystems, and a record usually leaves the temp or project volume
fn move_dir(from: &Path, to: &Path) -> Result<(), EjectError> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let status = Command::new("mv")
        .arg(from)
        .arg(to)
        .status()
        .map_err(io(from.display()))?;
    if !status.success() {
        return Err(EjectError::Io {
            path: from.display().to_string(),
            source: std::io::Error::other(format!("mv exited with {status}")),
        });
    }
    Ok(())
}

fn split_block(text: &str) -> (String, Vec<String>) {
    let mut outside: Vec<&str> = Vec::new();
    let mut entries = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        match line {
            EXCLUDE_OPEN => inside = true,
            EXCLUDE_CLOSE => inside = false,
            _ if inside => entries.push(line.trim_matches('/').to_string()),
            _ => outside.push(line),
        }
    }
    let mut out = outside.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    (out, entries)
}
