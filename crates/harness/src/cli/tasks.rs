//! `enallagi tasks <verb>` -- the queue read and written from a shell, one verb per question a role asks of it.

use crate::queue::{self, Queue, QueueError};
use crate::tracker::{self, Tracker};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct Args {
    pub cmd: String,
    pub args: Vec<String>,
}

const USAGE: &str = "enallagi tasks: usage: enallagi tasks <list|ready|ready-unattended|ids-at|block|field|set-status|unblock|rejections|archive|migrate|landed [--built]> [args] [file]";

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let a = &args.args;
    match args.cmd.as_str() {
        "list" => {
            let blocks = match parsed_blocks(a, 0) {
                Ok(b) => b,
                Err(code) => return Ok(code),
            };
            let out = queue::list(&blocks);
            if !out.is_empty() {
                println!("{out}");
            }
            Ok(0)
        }
        "ready" | "ready-unattended" => {
            let blocks = match parsed_blocks(a, 0) {
                Ok(b) => b,
                Err(code) => return Ok(code),
            };
            if let Some(id) = tracker::takeable(&blocks) {
                println!("{id}");
            }
            Ok(0)
        }
        "ids-at" => {
            let status = a.first().cloned().unwrap_or_default();
            let blocks = match parsed_blocks(a, 1) {
                Ok(b) => b,
                Err(code) => return Ok(code),
            };
            // `ready` names both states `attended: true` split it into
            let states = match status.as_str() {
                "ready" => tracker::READY.to_vec(),
                s => vec![tracker::canonical(s)],
            };
            for id in tracker::ids_in(&blocks, &states) {
                println!("{id}");
            }
            Ok(0)
        }
        "block" => {
            let id = a.first().cloned().unwrap_or_default();
            let blocks = match parsed_blocks(a, 1) {
                Ok(b) => b,
                Err(code) => return Ok(code),
            };
            match blocks.iter().find(|b| b.id == id) {
                Some(b) => {
                    println!("{}", queue::block_text(b));
                    Ok(0)
                }
                None => {
                    eprintln!("{}", QueueError::NoSuchTask(id));
                    Ok(1)
                }
            }
        }
        "field" => {
            let id = a.first().cloned().unwrap_or_default();
            let key = a.get(1).cloned().unwrap_or_default();
            let blocks = match parsed_blocks(a, 2) {
                Ok(b) => b,
                Err(code) => return Ok(code),
            };
            match blocks.iter().find(|b| b.id == id) {
                Some(b) => {
                    if let Some(value) = queue::field(b, &key) {
                        println!("{value}");
                    }
                    Ok(0)
                }
                None => {
                    eprintln!("{}", QueueError::NoSuchTask(id));
                    Ok(1)
                }
            }
        }
        "set-status" => {
            let id = a.first().cloned().unwrap_or_default();
            let status = a.get(1).cloned().unwrap_or_default();
            let reason = a.get(2).cloned().unwrap_or_default();
            let t = tracker_arg(a, 3);
            match t.get(&id) {
                Ok(Some(b)) if queue::field(&b, "status").is_some() => {}
                Ok(_) => {
                    eprintln!("enallagi tasks: no block {id}, nothing written");
                    return Ok(1);
                }
                Err(e) => return Ok(fail(&e)),
            }
            match t.set_status(&id, &status, &reason) {
                Ok(()) => Ok(0),
                Err(e) => Ok(fail(&e)),
            }
        }
        "unblock" => match tracker_arg(a, 0).unblock() {
            Ok(()) => Ok(0),
            Err(e) => Ok(fail(&e)),
        },
        "archive" => {
            let root = toplevel()?;
            let cfg = crate::config::load(&root)?;
            let report = crate::archive::archive_done(&root, &cfg, false)?;
            if let Some(refused) = report.refused {
                eprintln!("{refused}");
                return Ok(1);
            }
            if report.moved.is_empty() {
                println!("nothing to archive");
            }
            for id in &report.moved {
                println!("{id}");
            }
            Ok(0)
        }
        "migrate" => {
            let root = toplevel()?;
            migrate(&root, &crate::config::harness_dir(&root))
        }
        "landed" => {
            let built = a.iter().any(|x| x == "--built");
            let rows = crate::pr::landed(&std::env::current_dir()?)?;
            for row in rows {
                if built && !row.state.starts_with("built ") {
                    continue;
                }
                let commits = match row.commits.is_empty() {
                    true => "-".to_string(),
                    false => row.commits.join(","),
                };
                println!("{}\t{}\t{}\t{}", row.id, row.title, commits, row.state);
            }
            Ok(0)
        }
        "rejections" => {
            let path = file_arg(a, 0, "DECISIONS.md");
            if !path.exists() {
                return Ok(0);
            }
            let text = match read_text(&path) {
                Ok(t) => t,
                Err(code) => return Ok(code),
            };
            for line in queue::rejections(&text) {
                println!("{line}");
            }
            Ok(0)
        }
        _ => {
            eprintln!("{USAGE}");
            Ok(2)
        }
    }
}

fn toplevel() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(
        match crate::git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
            Ok(top) => PathBuf::from(top),
            Err(_) => cwd,
        },
    )
}

fn migrate(root: &Path, dir: &str) -> anyhow::Result<i32> {
    let tasks = match Tracker::open(root, dir) {
        Tracker::Legacy(path) if path.is_file() => path,
        _ => {
            println!("nothing to migrate");
            return Ok(0);
        }
    };
    let rel = tasks
        .strip_prefix(root)
        .unwrap_or(&tasks)
        .to_string_lossy()
        .to_string();
    let (repo, inner, _) = crate::git::locate(root, dir, &rel);
    let home = match Path::new(&inner).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().to_string(),
        _ => ".".to_string(),
    };
    // a staged path outside the instance would ride along in a shared repository's commit
    let dirty = crate::git::git(&repo, &["status", "--porcelain", "--", &home])?;
    if !dirty.is_empty() || !crate::git::git_ok(&repo, &["diff", "--cached", "--quiet"]) {
        eprintln!("enallagi tasks: the instance tree is dirty, commit or stash it first:\n{dirty}");
        return Ok(1);
    }
    let text = match read_text(&tasks) {
        Ok(t) => t,
        Err(code) => return Ok(code),
    };
    let base = tasks.parent().unwrap_or(root);
    let spec = crate::tracker::repo_slug(root);
    let specs = base.join("specs");
    let n = match crate::tracker::write_converted(&specs, &spec, &text) {
        Ok(n) => n,
        Err(e) => return Ok(fail(&e)),
    };
    if base.join("SPEC.md").is_file() {
        std::fs::rename(base.join("SPEC.md"), specs.join(&spec).join("spec.md"))?;
    }
    std::fs::remove_file(&tasks)?;
    // commit_instance adds only paths that exist, so the deletions are staged here
    crate::git::git(&repo, &["add", "-A", "--", &home])?;
    crate::git::commit_instance(root, dir, &["specs"], crate::tracker::MIGRATE_SUBJECT)?;
    println!("{n} tickets moved to {}", specs.join(&spec).display());
    Ok(0)
}

fn fail(e: &QueueError) -> i32 {
    eprintln!("{e}");
    2
}

fn file_arg(a: &[String], pos: usize, default: &str) -> PathBuf {
    a.get(pos).map(PathBuf::from).unwrap_or_else(|| {
        let root = Path::new(".");
        crate::config::instance_path(root, &crate::config::harness_dir(root), default)
    })
}

fn read_text(path: &Path) -> Result<String, i32> {
    (Queue {
        path: path.to_path_buf(),
    })
    .read()
    .map_err(|e| fail(&e))
}

// a directory argument is a specs tree, a file a legacy queue
fn tracker_arg(a: &[String], pos: usize) -> Tracker {
    match a.get(pos).map(PathBuf::from) {
        Some(p) if p.is_dir() => Tracker::Files(p),
        Some(p) => Tracker::Legacy(p),
        None => {
            let root = Path::new(".");
            Tracker::open(root, &crate::config::harness_dir(root))
        }
    }
}

fn parsed_blocks(a: &[String], pos: usize) -> Result<Vec<queue::Block>, i32> {
    tracker_arg(a, pos).blocks().map_err(|e| fail(&e))
}
