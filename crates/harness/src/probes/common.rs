//! Helpers the text probes share: table parsers, the queue/learnings readers, and tree lookups.

use super::{Finding, ProbeCtx, ProbeResult};
use crate::config::Config;
use crate::git;
use globset::GlobBuilder;
use regex::Regex;
use std::path::Path;

pub type Res<T> = Result<T, String>;

pub fn result(found: Res<Vec<Finding>>) -> ProbeResult {
    match found {
        Ok(found) => ProbeResult::Count(found),
        Err(reason) => ProbeResult::Error(reason),
    }
}

pub fn finding(path: impl Into<String>, line: usize, message: impl Into<String>) -> Finding {
    Finding {
        path: path.into(),
        line,
        message: message.into(),
    }
}

pub fn cut(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

pub fn re(pattern: &str) -> Res<Regex> {
    Regex::new(pattern).map_err(|e| e.to_string())
}

pub fn instance(ctx: &ProbeCtx, name: &str) -> String {
    crate::config::instance_rel(ctx.root, &ctx.cfg.layout.harness_dir, name)
}

pub fn read(root: &Path, rel: &str) -> Res<String> {
    std::fs::read(root.join(rel))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|e| format!("{rel}: {e}"))
}

pub fn lines_of(root: &Path, rel: &str) -> Res<Vec<String>> {
    Ok(read(root, rel)?.split('\n').map(String::from).collect())
}

pub fn exists(root: &Path, rel: &str) -> bool {
    root.join(rel).exists()
}

pub fn is_file(root: &Path, rel: &str) -> bool {
    root.join(rel).is_file()
}

pub fn is_dir(root: &Path, rel: &str) -> bool {
    root.join(rel).is_dir()
}

// a harness directory that is its own repository is outside the product's index; list what the launcher's `git add -A` there commits
pub fn tracked(root: &Path) -> Res<Vec<String>> {
    let list = |repo: &Path, args: &[&str], prefix: &str| -> Res<Vec<String>> {
        let out = git::git(repo, args).map_err(|e| e.to_string())?;
        Ok(out
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| format!("{prefix}{l}"))
            .collect())
    };
    let mut paths = list(root, &["ls-files"], "")?;
    let dir = crate::config::harness_dir(root);
    let state = git::state_root(root, &dir);
    if state != root {
        let args = ["ls-files", "--cached", "--others", "--exclude-standard"];
        paths.extend(list(&state, &args, &format!("{dir}/"))?);
    }
    Ok(paths)
}

pub fn walk(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    descend(root, "", &mut out);
    out.sort();
    out
}

fn descend(root: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root.join(prefix)) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name == ".git" {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        out.push(rel.clone());
        if dir {
            descend(root, &rel, out);
        }
    }
}

// `*` stops at a `/`, `**` crosses it
pub fn matches(paths: &[String], pattern: &str) -> Res<Vec<String>> {
    let pattern = pattern.strip_suffix('/').unwrap_or(pattern);
    let glob = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| e.to_string())?
        .compile_matcher();
    Ok(paths
        .iter()
        .filter(|p| glob.is_match(p.as_str()))
        .cloned()
        .collect())
}

pub fn declares(root: &Path, rel: &str, name: &str, patterns: &[String]) -> Res<bool> {
    let wanted: Vec<String> = patterns.iter().map(|p| p.replace("{name}", name)).collect();
    Ok(lines_of(root, rel)?.iter().any(|line| {
        let code = line.trim_start();
        wanted.iter().any(|w| code.starts_with(w.as_str()))
    }))
}

pub fn backticked(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        rest = &rest[open + 1..];
        match rest.find('`') {
            Some(close) => {
                out.push(rest[..close].to_string());
                rest = &rest[close + 1..];
            }
            None => break,
        }
    }
    out
}

pub struct SpecRow {
    pub name: String,
    pub test: String,
    pub line: usize,
}

const SEPARATOR: &str = r"^\|[\s:|-]+\|\s*$";

// must slice and read cells the same way the floor's own row parser does, or the two disagree
pub fn spec_rows(ctx: &ProbeCtx) -> Res<Vec<SpecRow>> {
    let cfg: &Config = ctx.cfg;
    let spec = &instance(ctx, &cfg.layout.spec);
    let text = read(ctx.root, spec)?;
    let heading = &cfg.layout.rows_heading;
    let start = text
        .find(&format!("\n{heading}"))
        .ok_or_else(|| format!("{spec}: no \"{heading}\" heading"))?;
    let after = text[start..]
        .find(&format!("\n{}", cfg.layout.rows_end_heading))
        .map(|at| start + at);
    let section = &text[start..after.unwrap_or(text.len())];
    let base = text[..start].matches('\n').count() + 1;

    let row = re(&format!(
        r"(?m)^\|[^|]+\|\s*`([\w./-]+{})::([^`]+)`\s*\|",
        cfg.layout.test_file_suffix_re
    ))?;
    let rows: Vec<SpecRow> = row
        .captures_iter(section)
        .filter_map(|m| {
            let at = m.get(0)?.start();
            Some(SpecRow {
                name: m.get(1)?.as_str().to_string(),
                test: m.get(2)?.as_str().to_string(),
                line: base + section[..at].matches('\n').count(),
            })
        })
        .collect();

    let separator = re(SEPARATOR)?;
    let mut shaped = 0usize;
    for line in section.split('\n') {
        if !line.starts_with('|') || separator.is_match(line) {
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        if cells.len() > 2 && cells[cells.len() - 2].trim().to_lowercase() != "test" {
            shaped += 1;
        }
    }
    if shaped != rows.len() {
        return Err(format!(
            "the exit-criteria section has {shaped} row-shaped lines but {} parsed as file::test — a table went unread",
            rows.len()
        ));
    }
    Ok(rows)
}

pub const RAILS_HEADER: &str = "| Rail | What it means | Enforced by |";

pub fn rails_file(cfg: &Config) -> String {
    format!("{}/RAILS.md", cfg.layout.harness_dir)
}

pub struct RailRow {
    pub line: usize,
    pub cells: Vec<String>,
}

pub fn rail_rows(ctx: &ProbeCtx) -> Res<Vec<RailRow>> {
    let file = rails_file(ctx.cfg);
    if !exists(ctx.root, &file) {
        return Err(format!(
            "{file} does not exist -- the rails are unwritten, so nothing here is enforced"
        ));
    }
    let separator = re(SEPARATOR)?;
    let mut rows = Vec::new();
    let (mut inside, mut seen) = (false, false);
    for (index, line) in lines_of(ctx.root, &file)?.iter().enumerate() {
        if line.trim() == RAILS_HEADER {
            inside = true;
            seen = true;
            continue;
        }
        if inside && !line.starts_with('|') {
            inside = false;
        }
        if !inside || separator.is_match(line) {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 2 {
            let cells: Vec<String> = parts[1..parts.len() - 1]
                .iter()
                .map(|c| c.to_string())
                .collect();
            if cells.len() >= 3 {
                rows.push(RailRow {
                    line: index + 1,
                    cells,
                });
            }
        }
    }
    if !seen {
        return Err(format!(
            "{file} has no table headed `{RAILS_HEADER}`, so no rail can be read"
        ));
    }
    Ok(rows)
}

pub fn learning_entries(ctx: &ProbeCtx) -> Res<Vec<(usize, String)>> {
    let lines = lines_of(ctx.root, &instance(ctx, "LEARNINGS.md"))?;
    Ok(entries_of(numbered(&lines)))
}

// a rule the loop earned sits under `## Earned rules` in DECISIONS.md, and `## Rejected findings` below it is not one
pub fn earned_rules(ctx: &ProbeCtx) -> Res<Vec<(usize, String)>> {
    let decisions = instance(ctx, "DECISIONS.md");
    if !is_file(ctx.root, &decisions) {
        return Ok(Vec::new());
    }
    let lines = lines_of(ctx.root, &decisions)?;
    Ok(entries_of(
        numbered(&lines)
            .skip_while(|(_, line)| line.trim() != "## Earned rules")
            .skip(1)
            .take_while(|(_, line)| !line.starts_with("## ")),
    ))
}

fn numbered(lines: &[String]) -> impl Iterator<Item = (usize, &String)> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| (index + 1, line))
}

fn entries_of<'a>(lines: impl Iterator<Item = (usize, &'a String)>) -> Vec<(usize, String)> {
    let mut entries: Vec<(usize, String)> = Vec::new();
    let mut open_at: Option<usize> = None;
    for (line_no, line) in lines {
        if line.starts_with("- ") {
            entries.push((line_no, line.clone()));
            open_at = Some(entries.len() - 1);
        } else if let Some(at) = open_at {
            if line.starts_with("  ") && !line.trim().is_empty() {
                entries[at].1.push(' ');
                entries[at].1.push_str(line.trim());
            } else if line.trim().is_empty() {
                open_at = None;
            }
        } else if line.trim().is_empty() {
            open_at = None;
        }
    }
    entries
}

pub struct TaskBlock {
    pub id: String,
    pub line: usize,
    pub body: Vec<(usize, String)>,
}

pub fn task_blocks(ctx: &ProbeCtx) -> Res<Vec<TaskBlock>> {
    let heading = re(r"^## \[(T-\d+)\]")?;
    let mut blocks: Vec<TaskBlock> = Vec::new();
    let mut open = false;
    for (index, line) in lines_of(ctx.root, &instance(ctx, "TASKS.md"))?
        .iter()
        .enumerate()
    {
        if let Some(m) = heading.captures(line) {
            let id = m.get(1).map(|g| g.as_str()).unwrap_or_default().to_string();
            blocks.push(TaskBlock {
                id,
                line: index + 1,
                body: Vec::new(),
            });
            open = true;
            continue;
        }
        if line.starts_with("## ") {
            open = false;
            continue;
        }
        if open {
            if let Some(block) = blocks.last_mut() {
                block.body.push((index + 1, line.clone()));
            }
        }
    }
    Ok(blocks)
}

pub fn field(block: &TaskBlock, key: &str) -> Option<(usize, String)> {
    let prefix = format!("{key}:");
    block.body.iter().find_map(|(line, text)| {
        text.strip_prefix(&prefix)
            .map(|value| (*line, value.trim().to_string()))
    })
}

pub fn normal(text: &str) -> String {
    let mapped: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
}
