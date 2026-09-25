use crate::queue::{self, Queue, QueueError};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct Args {
    pub cmd: String,
    pub args: Vec<String>,
}

const USAGE: &str = "enallagi tasks: usage: enallagi tasks <list|ready|ready-unattended|ids-at|block|field|set-status|unblock|rejections|archive> [args] [file]";

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
            if let Some(id) = queue::ready_unattended(&blocks) {
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
            for id in queue::ids_at(&blocks, &status) {
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
            let path = file_arg(a, 3, "TASKS.md");
            let text = match read_text(&path) {
                Ok(t) => t,
                Err(code) => return Ok(code),
            };
            let out = match queue::set_status(&text, &id, &status, &reason) {
                Ok(o) => o,
                Err(e) => return Ok(fail(&e)),
            };
            if out == text {
                eprintln!("enallagi tasks: no block {id}, nothing written");
                return Ok(1);
            }
            match (Queue { path }).write(&out) {
                Ok(()) => Ok(0),
                Err(e) => Ok(fail(&e)),
            }
        }
        "unblock" => {
            let path = file_arg(a, 0, "TASKS.md");
            let text = match read_text(&path) {
                Ok(t) => t,
                Err(code) => return Ok(code),
            };
            let out = match queue::unblock(&text) {
                Ok(o) => o,
                Err(e) => return Ok(fail(&e)),
            };
            if out != text {
                if let Err(e) = (Queue { path }).write(&out) {
                    return Ok(fail(&e));
                }
            }
            Ok(0)
        }
        "archive" => {
            let cwd = std::env::current_dir()?;
            let root = match crate::git::git(&cwd, &["rev-parse", "--show-toplevel"]) {
                Ok(top) => PathBuf::from(top),
                Err(_) => cwd,
            };
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

fn parsed_blocks(a: &[String], pos: usize) -> Result<Vec<queue::Block>, i32> {
    let text = read_text(&file_arg(a, pos, "TASKS.md"))?;
    queue::parse(&text).map_err(|e| fail(&e))
}
