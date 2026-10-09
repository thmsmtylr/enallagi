//! audit: review findings, frictions and rejections the repository already holds, handed to the `auditor` role, which names each class that recurs. The binary writes a learning only when every citation it carries resolves.

use crate::config::{self, Config};
use crate::git;
use crate::probes::{self, CheckOutcome, ProbeCtx, ProbeResult};
use crate::queue::{self, QueueError};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::Duration;

pub const PROPOSED: &str = "## Proposed learnings";
const EARNED: &str = "## Earned rules";
const REJECTED: &str = "## Rejected findings";
const EXPIRED: &str = "## Expired findings";
const PROMOTED: &str = "promoted:";
const REVISED: &str = "revised:";
// a rule revised this many times that recurs again is removed, never handed back
const REVISIONS: usize = 2;
// a class with fewer instances than this is one occurrence, which PROGRESS.md keeps as evidence
const RECURS: usize = 2;
const BEGIN: &str = "BEGIN ENALLAGI LEARNING";
const END: &str = "END ENALLAGI LEARNING";
const TURNS: u32 = 20;
const TIMEOUT: Duration = Duration::from_secs(600);
// function words two unrelated learnings share, so they would count toward an overlap
const STOP: [&str; 40] = [
    "a", "an", "and", "are", "as", "at", "be", "before", "by", "for", "from", "has", "in", "into",
    "is", "it", "its", "no", "not", "of", "on", "one", "or", "so", "than", "that", "the", "their",
    "then", "there", "this", "to", "was", "what", "when", "where", "which", "with", "without",
    "never",
];

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error("enallagi audit: {0}")]
    Io(#[from] std::io::Error),
    #[error("enallagi audit: {0}")]
    Read(String),
    #[error("enallagi audit: {0}")]
    Agent(String),
}

#[derive(Debug)]
pub struct Proposal {
    // the class in the auditor's words
    pub class: String,
    pub instances: Vec<String>,
    // what went wrong, then the rule instead
    pub learning: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub proposed: Vec<Proposal>,
    pub merged: Vec<String>,
    pub refused: Vec<String>,
    pub revised: Vec<String>,
    pub removed: Vec<String>,
    pub decisions: String,
}

fn read(root: &Path, rel: &str) -> Result<String, AuditError> {
    match fs::read_to_string(root.join(rel)) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(AuditError::Read(format!("{rel}: {e}"))),
    }
}

fn section<'a>(text: &'a str, heading: &str) -> Vec<(usize, &'a str)> {
    text.split('\n')
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .skip_while(|(_, l)| l.trim_end() != heading)
        .skip(1)
        .take_while(|(_, l)| !l.starts_with("## "))
        .collect()
}

// `- ` opens an entry, an indented line continues it
fn entries(lines: &[(usize, &str)]) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for (at, line) in lines {
        if line.starts_with("- ") {
            out.push((*at, line.to_string()));
        } else if line.starts_with("  ") && !line.trim().is_empty() {
            if let Some(last) = out.last_mut() {
                last.1.push('\n');
                last.1.push_str(line);
            }
        }
    }
    out
}

struct Signal {
    at: String,
    text: String,
}

fn frictions(root: &Path, cfg: &Config, rel: &str) -> Result<Vec<Signal>, AuditError> {
    let mut out: Vec<Signal> = read(root, rel)?
        .split('\n')
        .enumerate()
        .filter_map(|(i, l)| {
            let text = l.strip_prefix("friction:")?.trim();
            // `none` and the template's `<...>` placeholder are an entry saying nothing went wrong
            (!text.is_empty() && text != "none" && !text.starts_with('<')).then(|| Signal {
                at: format!("{rel}:{}", i + 1),
                text: text.to_string(),
            })
        })
        .collect();
    // the check is never run: friction-repeat reads PROGRESS.md and nothing else
    let unrun = CheckOutcome {
        ran: false,
        red: false,
        output: String::new(),
    };
    let ctx = ProbeCtx {
        root,
        cfg,
        check: Some(&unrun),
        driver: false,
    };
    for (name, result) in probes::run_all(&ctx, &["friction-repeat".to_string()]) {
        match result {
            // an ineffective rule is handed back on its own, with the instances that recurred
            ProbeResult::Count(found) => {
                out.extend(found.into_iter().filter(|f| f.path == rel).map(|f| Signal {
                    at: format!("{}:{}", f.path, f.line),
                    text: f.message,
                }))
            }
            ProbeResult::Error(e) => return Err(AuditError::Read(format!("{name}: {e}"))),
            ProbeResult::Off(_) => {}
        }
    }
    Ok(out)
}

// a block `enallagi review` queued carries its comment's permalink
fn reviewed(text: &str, rel: &str) -> Result<Vec<Signal>, AuditError> {
    let permalink = Regex::new(r"/pull/\d+#(discussion_r|pullrequestreview)").expect("pattern");
    Ok(queue::parse(text)?
        .into_iter()
        .filter(|b| b.body.iter().any(|(_, l)| permalink.is_match(l)))
        .map(|b| Signal {
            at: format!("{rel}:{}", b.line),
            text: std::iter::once(b.title)
                .chain(b.body.into_iter().map(|(_, l)| l))
                .collect::<Vec<_>>()
                .join("\n"),
        })
        .collect())
}

fn rejections(text: &str, rel: &str) -> Result<Vec<Signal>, AuditError> {
    let verdict = Regex::new(r"^\s*(VERIFIER[^:]*:\s*)?REJECTED\b").expect("pattern");
    let mut out: Vec<Signal> = entries(&section(text, REJECTED))
        .into_iter()
        .map(|(at, text)| Signal {
            at: format!("{rel}:{at}"),
            text,
        })
        .collect();
    for block in queue::parse(text)? {
        out.extend(
            block
                .body
                .into_iter()
                .filter(|(_, l)| verdict.is_match(l))
                .map(|(at, text)| Signal {
                    at: format!("{rel}:{at}"),
                    text,
                }),
        );
    }
    Ok(out)
}

struct Learning {
    class: String,
    text: String,
    instances: Vec<String>,
    revises: Option<String>,
}

// a JSON event line is read for the strings it carries, so a preset that streams events still answers
fn answers(output: &str) -> Vec<String> {
    fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) if text.contains(BEGIN) => out.push(text.clone()),
            serde_json::Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
            serde_json::Value::Object(map) => map.values().for_each(|v| strings(v, out)),
            _ => {}
        }
    }
    let mut texts = vec![String::new()];
    for line in output.lines() {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) if value.is_object() || value.is_array() => strings(&value, &mut texts),
            _ => {
                texts[0].push_str(line);
                texts[0].push('\n');
            }
        }
    }
    let mut out: Vec<String> = Vec::new();
    for text in &texts {
        for part in text.split(BEGIN).skip(1) {
            if let Some((body, _)) = part.split_once(END) {
                let body = body.trim().to_string();
                if !body.is_empty() && !out.contains(&body) {
                    out.push(body);
                }
            }
        }
    }
    out
}

fn parse(body: &str) -> Learning {
    let cite = Regex::new(r"`([^`]+)`").expect("pattern");
    let mut learning = Learning {
        class: String::new(),
        text: String::new(),
        instances: Vec::new(),
        revises: None,
    };
    let mut text: Vec<&str> = Vec::new();
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(class) = line.strip_prefix("class:") {
            learning.class = class.trim().trim_matches('`').to_string();
        } else if let Some(rule) = line.strip_prefix("revises:") {
            learning.revises = Some(rule.trim().trim_matches('`').to_string());
        } else if let Some(cites) = line.strip_prefix("instances:") {
            for c in cite.captures_iter(cites) {
                if !learning.instances.contains(&c[1].to_string()) {
                    learning.instances.push(c[1].to_string());
                }
            }
        } else {
            text.push(line.strip_prefix("- ").unwrap_or(line));
        }
    }
    learning.text = text.join(" ");
    learning
}

// a sha git knows, or a `file:line` whose file has that line
fn resolves(root: &Path, cite: &str) -> bool {
    let sha = Regex::new(r"^[0-9a-f]{7,40}$").expect("pattern");
    if sha.is_match(cite) {
        return git::git_ok(root, &["cat-file", "-e", cite]);
    }
    let Some((file, line)) = cite.rsplit_once(':') else {
        return false;
    };
    match (line.parse::<usize>(), fs::read_to_string(root.join(file))) {
        (Ok(line), Ok(text)) => line >= 1 && text.lines().count() >= line,
        _ => false,
    }
}

fn words(text: &str) -> BTreeSet<String> {
    probes::common::normal(text)
        .split_whitespace()
        .filter(|w| !STOP.contains(w))
        .map(String::from)
        .collect()
}

/// Two texts overlap when they share two or more words and those make half of the smaller word set.
pub fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (words(a), words(b));
    let shared = a.intersection(&b).count();
    shared >= 2 && shared * 2 >= a.len().min(b.len())
}

struct Entry {
    // index into `Texts`: 0 is LEARNINGS.md, 1 is DECISIONS.md
    file: usize,
    // 0-based line the entry opens on
    line: usize,
    text: String,
    killed: Option<String>,
}

// what an entry says, without its marker, its instances or its kill
fn said(entry: &str) -> String {
    let mut lines = entry.lines();
    let first = lines.next().unwrap_or_default();
    let first = first.strip_prefix("- ").unwrap_or(first);
    let first = match first.strip_prefix('[') {
        Some(rest) => rest.split_once("] ").map_or(first, |(_, r)| r),
        None => first,
    };
    std::iter::once(first)
        .chain(lines.map(str::trim).filter(|l| {
            !["instances:", "killed:", PROMOTED, REVISED]
                .iter()
                .any(|k| l.starts_with(k))
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

fn standing(learnings: &str, decisions: &str) -> Vec<Entry> {
    let killed = Regex::new(r"^\s+killed: (\d{4}-\d{2}-\d{2})\b").expect("pattern");
    let all: Vec<(usize, &str)> = learnings
        .split('\n')
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .collect();
    let mut out: Vec<Entry> = entries(&all)
        .into_iter()
        .map(|(at, text)| Entry {
            file: 0,
            line: at - 1,
            text: said(&text),
            killed: None,
        })
        .collect();
    for heading in [EARNED, PROPOSED] {
        out.extend(
            entries(&section(decisions, heading))
                .into_iter()
                .map(|(at, text)| Entry {
                    file: 1,
                    line: at - 1,
                    killed: text
                        .lines()
                        .find_map(|l| killed.captures(l))
                        .map(|c| c[1].to_string()),
                    text: said(&text),
                }),
        );
    }
    out
}

// the line after the entry opening at 0-based `line`
fn end(lines: &[String], line: usize) -> usize {
    line + 1
        + lines[line + 1..]
            .iter()
            .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
            .count()
}

// adds each cite the entry lacks to its `instances:` line, or opens one; returns how many were new
fn merge(text: &str, line: usize, cites: &[String]) -> (String, usize) {
    let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
    let end = end(&lines, line);
    let at = (line + 1..end).find(|&i| lines[i].trim_start().starts_with("instances:"));
    let held = at.map(|i| lines[i].clone()).unwrap_or_default();
    let new: Vec<String> = cites
        .iter()
        .map(|c| format!("`{c}`"))
        .filter(|c| !held.contains(c.as_str()))
        .collect();
    if new.is_empty() {
        return (text.to_string(), 0);
    }
    match at {
        Some(i) => lines[i] = format!("{}, {}", lines[i].trim_end(), new.join(", ")),
        None => lines.insert(end, format!("  instances: {}", new.join(", "))),
    }
    (lines.join("\n"), new.len())
}

pub fn render(proposal: &Proposal) -> String {
    let cites: Vec<String> = proposal
        .instances
        .iter()
        .map(|i| format!("`{i}`"))
        .collect();
    format!(
        "- [proposed] `{}`: {}\n  instances: {}",
        proposal.class,
        proposal.learning,
        cites.join(", ")
    )
}

fn prompt(role: &str, signal: &[Signal], rel: &str, handed: &[Ineffective]) -> String {
    let cited: Vec<String> = signal
        .iter()
        .map(|s| {
            let text: Vec<String> = s
                .text
                .lines()
                .map(|l| format!("    {}", l.trim()))
                .collect();
            format!("- `{}`\n{}", s.at, text.join("\n"))
        })
        .collect();
    let mut out = format!(
        "{role}\n\nThe {} findings, each with the text read at its citation:\n\n{}\n",
        signal.len(),
        cited.join("\n")
    );
    if !handed.is_empty() {
        let rules: Vec<String> = handed
            .iter()
            .map(|r| {
                let new: Vec<String> = r.recurred.iter().map(|c| format!("`{c}`")).collect();
                format!(
                    "- `{rel}:{}` promoted at {}: {}\n    recurred: {}",
                    r.line,
                    r.promoted,
                    r.rule,
                    new.join(", ")
                )
            })
            .collect();
        out.push_str(&format!(
            "\nThe {} earned rules below recurred after their promotion, so each did not work. Revise each one: write one learning for it, cite the instances that recurred, and add the line `revises: <its citation>` above `END ENALLAGI LEARNING`.\n\n{}\n",
            handed.len(),
            rules.join("\n")
        ));
    }
    out
}

// an installed role file overrides the shipped one, so an empty one leaves the agent no role
fn role(root: &Path, cfg: &Config) -> String {
    let installed = root
        .join(&cfg.layout.harness_dir)
        .join("roles")
        .join("auditor.md");
    let text = fs::read_to_string(installed)
        .unwrap_or_else(|_| include_str!("../../../roles/auditor.md").to_string());
    config::subst(body(&text), cfg)
}

// an agent CLI reads a prompt that starts with `-` as an option, and the frontmatter starts with `---`
fn body(role: &str) -> &str {
    role.strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(role, |(_, body)| body)
        .trim_start()
}

fn ask(root: &Path, cfg: &Config, prompt: String) -> Result<String, AuditError> {
    let presets = crate::agent::presets();
    let resolved = crate::agent::resolve(&cfg.agent, "auditor", &presets)
        .map_err(|e| AuditError::Agent(e.to_string()))?;
    let dir = &cfg.layout.harness_dir;
    let spawn = crate::agent::StageSpawn {
        argv: crate::agent::fill_layout(&resolved.argv, &cfg.layout),
        env: BTreeMap::new(),
        cwd: root,
        timeout: Some(TIMEOUT),
        prompt,
        turns: TURNS,
        stage: "auditor".to_string(),
        task: None,
        preset: resolved.preset,
    };
    let rate_limit = Regex::new(&format!("(?i){}", cfg.agent.rate_limit_pattern))
        .map_err(|e| AuditError::Agent(e.to_string()))?;
    let mut events = crate::events::Writer::new(crate::events::Log::open(&root.join(dir)));
    let stop_file = config::instance_path(root, dir, "STOP");
    let result = crate::agent::spawn(&spawn, &mut events, &stop_file, &rate_limit)
        .map_err(|e| AuditError::Agent(e.to_string()))?;
    // a failed agent's output can quote the role's own template, which parses as a learning
    if result.exit != 0 {
        let first = result.output.lines().next().unwrap_or_default();
        return Err(AuditError::Agent(format!(
            "the auditor exited {}: {first}",
            result.exit
        )));
    }
    Ok(result.output)
}

pub fn with_proposed(decisions: &str, rendered: &[String]) -> String {
    let lines: Vec<&str> = decisions.lines().collect();
    let (head, body, tail) = match lines.iter().position(|l| l.trim_end() == PROPOSED) {
        Some(at) => {
            let rest = &lines[at + 1..];
            let end = rest
                .iter()
                .position(|l| l.starts_with("## "))
                .unwrap_or(rest.len());
            (&lines[..at], &rest[..end], &rest[end..])
        }
        None => {
            // after the rejected findings, so the earned-rules range a lane reads stops short of a proposal
            let heading = [REJECTED, EARNED]
                .into_iter()
                .find_map(|h| lines.iter().position(|l| l.trim_end() == h));
            let anchor = match heading {
                Some(at) => lines[at + 1..]
                    .iter()
                    .position(|l| l.starts_with("## "))
                    .map_or(lines.len(), |i| at + 1 + i),
                None => lines
                    .iter()
                    .position(|l| l.trim_end() == EXPIRED)
                    .unwrap_or(lines.len()),
            };
            (&lines[..anchor], &[][..], &lines[anchor..])
        }
    };
    let blank = |l: &&str| l.trim().is_empty();
    let trimmed = |part: &[&str]| -> Vec<String> {
        let from = part.iter().position(|l| !blank(l)).unwrap_or(part.len());
        let to = part.iter().rposition(|l| !blank(l)).map_or(from, |i| i + 1);
        part[from..to].iter().map(|l| l.to_string()).collect()
    };
    let mut out = trimmed(head);
    if !out.is_empty() {
        out.push(String::new());
    }
    out.push(PROPOSED.to_string());
    out.push(String::new());
    out.extend(trimmed(body));
    for entry in rendered {
        out.extend(entry.split('\n').map(String::from));
    }
    if !tail.is_empty() {
        out.push(String::new());
        out.extend(tail.iter().map(|l| l.to_string()));
    }
    out.join("\n") + "\n"
}

#[derive(Debug)]
pub struct Waiting {
    pub class: String,
    // 1-based line in DECISIONS.md
    pub line: usize,
    // state commits since the entry was first committed
    pub age: usize,
}

fn class_of(entry: &str) -> Option<String> {
    let rest = entry.strip_prefix("- [proposed] `")?;
    rest.split_once('`').map(|(class, _)| class.to_string())
}

// an entry no commit carries is one just written, which is as new as an entry gets
fn age(root: &Path, cfg: &Config, rel: &str, needle: &str) -> usize {
    let (repo, inner, _) = git::locate(root, &cfg.layout.harness_dir, rel);
    let commits = git::git(&repo, &["log", "--format=%H"]).unwrap_or_default();
    git::git(
        &repo,
        &[
            "log",
            "--reverse",
            "--format=%H",
            "-S",
            needle,
            "--",
            &inner,
        ],
    )
    .ok()
    .and_then(|found| found.lines().next().map(String::from))
    .and_then(|sha| commits.lines().position(|c| c == sha))
    .unwrap_or(0)
}

/// Each proposed learning no `killed:` line has decided, with its age in rounds.
pub fn waiting(root: &Path, cfg: &Config) -> Result<Vec<Waiting>, AuditError> {
    let rel = config::instance_rel(root, &cfg.layout.harness_dir, "DECISIONS.md");
    let text = read(root, &rel)?;
    Ok(entries(&section(&text, PROPOSED))
        .into_iter()
        .filter(|(_, entry)| !entry.lines().any(|l| l.trim_start().starts_with("killed:")))
        .filter_map(|(line, entry)| {
            let class = class_of(&entry)?;
            let age = age(root, cfg, &rel, &format!("- [proposed] `{class}`:"));
            Some(Waiting { class, line, age })
        })
        .collect())
}

/// Of the classes that were waiting, those since moved under `## Earned rules`, and those since killed.
pub fn decided(root: &Path, cfg: &Config, before: &[String]) -> (Vec<String>, Vec<String>) {
    let rel = config::instance_rel(root, &cfg.layout.harness_dir, "DECISIONS.md");
    let text = read(root, &rel).unwrap_or_default();
    let still: Vec<String> = waiting(root, cfg)
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.class)
        .collect();
    let earned: Vec<String> = section(&text, EARNED)
        .into_iter()
        .map(|(_, l)| l.to_string())
        .collect();
    let (mut promoted, mut killed) = (Vec::new(), Vec::new());
    for class in before.iter().filter(|c| !still.contains(c)) {
        let named = format!("`{class}`");
        if earned.iter().any(|l| l.contains(&named)) {
            promoted.push(class.clone());
        } else if entries(&section(&text, PROPOSED))
            .iter()
            .any(|(_, e)| class_of(e).as_ref() == Some(class) && e.contains("killed:"))
        {
            killed.push(class.clone());
        }
    }
    (promoted, killed)
}

/// Moves each learning waiting `queue.proposed_rounds` rounds or more out of `## Proposed learnings`, as a dated line under `## Expired findings`.
pub fn expire(root: &Path, cfg: &Config, today: &str) -> Result<Vec<String>, AuditError> {
    let stale: Vec<Waiting> = waiting(root, cfg)?
        .into_iter()
        .filter(|w| w.age >= cfg.queue.proposed_rounds)
        .collect();
    if stale.is_empty() {
        return Ok(Vec::new());
    }
    let rel = config::instance_rel(root, &cfg.layout.harness_dir, "DECISIONS.md");
    let text = read(root, &rel)?;
    let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
    let mut dated = Vec::new();
    // from the bottom, so removing an entry leaves every earlier line number true
    for w in stale.iter().rev() {
        let at = w.line - 1;
        let end = end(&lines, at);
        let entry: Vec<String> = lines.drain(at..end).collect();
        dated.push(format!(
            "- [{today}] `{}` expired after {} rounds undecided: {}",
            w.class,
            w.age,
            said(&entry.join("\n"))
        ));
    }
    dated.reverse();
    fs::write(root.join(&rel), with_expired(&lines.join("\n"), &dated))?;
    Ok(stale.into_iter().map(|w| w.class).collect())
}

fn with_expired(text: &str, dated: &[String]) -> String {
    let mut out = text.trim_end().to_string();
    if !out.lines().any(|l| l.trim_end() == EXPIRED) {
        out.push_str(&format!("\n\n{EXPIRED}\n"));
    }
    out.push('\n');
    out.push_str(&dated.join("\n"));
    out.push('\n');
    out
}

// the trimmed value of each indented `key` line under an entry
fn values<'a>(entry: &'a str, key: &str) -> Vec<&'a str> {
    entry
        .lines()
        .skip(1)
        .filter_map(|l| l.trim_start().strip_prefix(key))
        .map(str::trim)
        .collect()
}

fn cites(value: &str) -> Vec<String> {
    let cite = Regex::new(r"`([^`]+)`").expect("pattern");
    cite.captures_iter(value)
        .map(|c| c[1].to_string())
        .collect()
}

#[derive(Debug)]
pub struct Ineffective {
    // 1-based line in DECISIONS.md
    pub line: usize,
    pub rule: String,
    pub promoted: String,
    // the instances whose cited line the promotion commit did not hold
    pub recurred: Vec<String>,
    // the class each revision was proposed under
    pub revised: Vec<String>,
}

// ponytail: a sha cite never reads as recurred; judging one needs commit order across both repositories
fn recurred(root: &Path, cfg: &Config, sha: &str, cite: &str) -> bool {
    let Some((file, line)) = cite.rsplit_once(':') else {
        return false;
    };
    let Ok(line) = line.parse::<usize>() else {
        return false;
    };
    let now = fs::read_to_string(root.join(file))
        .ok()
        .and_then(|t| {
            t.lines()
                .nth(line.wrapping_sub(1))
                .map(|l| l.trim().to_string())
        })
        .unwrap_or_default();
    let (repo, inner, _) = git::locate(root, &cfg.layout.harness_dir, file);
    if now.is_empty() || !git::git_ok(&repo, &["cat-file", "-e", &format!("{sha}^{{commit}}")]) {
        return false;
    }
    let then = git::git(&repo, &["show", &format!("{sha}:{inner}")]).unwrap_or_default();
    !then.lines().any(|l| l.trim() == now)
}

fn recurring(root: &Path, cfg: &Config, text: &str) -> Vec<Ineffective> {
    entries(&section(text, EARNED))
        .into_iter()
        .filter_map(|(line, entry)| {
            let promoted = values(&entry, PROMOTED).first()?.to_string();
            let recurred: Vec<String> = values(&entry, "instances:")
                .iter()
                .flat_map(|v| cites(v))
                .filter(|c| recurred(root, cfg, &promoted, c))
                .collect();
            (!recurred.is_empty()).then(|| Ineffective {
                line,
                rule: said(&entry),
                promoted,
                recurred,
                revised: values(&entry, REVISED)
                    .iter()
                    .filter_map(|v| cites(v).pop())
                    .collect(),
            })
        })
        .collect()
}

/// Each earned rule an instance recurred against after the commit its `promoted:` line names.
pub fn ineffective(root: &Path, cfg: &Config) -> Result<Vec<Ineffective>, AuditError> {
    let rel = config::instance_rel(root, &cfg.layout.harness_dir, "DECISIONS.md");
    Ok(recurring(root, cfg, &read(root, &rel)?))
}

// the commit that first carried the rule's opening line, or HEAD while it is uncommitted
fn stamp(root: &Path, cfg: &Config, rel: &str, text: &str) -> String {
    let (repo, inner, _) = git::locate(root, &cfg.layout.harness_dir, rel);
    let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
    for (at, entry) in entries(&section(text, EARNED)).into_iter().rev() {
        if !values(&entry, PROMOTED).is_empty() {
            continue;
        }
        let first = entry.lines().next().unwrap_or_default();
        let sha = git::git(
            &repo,
            &["log", "--reverse", "--format=%h", "-S", first, "--", &inner],
        )
        .ok()
        .and_then(|found| found.lines().next().map(String::from))
        .or_else(|| git::git(&repo, &["rev-parse", "--short", "HEAD"]).ok());
        if let Some(sha) = sha {
            let end = end(&lines, at - 1);
            lines.insert(end, format!("  {PROMOTED} {sha}"));
        }
    }
    lines.join("\n")
}

// a rule whose last revision was promoted under `## Earned rules` gives way to it, which carries its revisions on
fn supersede(text: &str) -> String {
    let mut text = text.to_string();
    'again: loop {
        let earned = entries(&section(&text, EARNED));
        for (i, (at, entry)) in earned.iter().enumerate() {
            let revised = values(entry, REVISED);
            let Some(class) = revised.last().and_then(|v| cites(v).pop()) else {
                continue;
            };
            let named = format!("`{class}`");
            let Some((later, _)) = earned[i + 1..]
                .iter()
                .find(|(_, e)| e.lines().next().is_some_and(|l| l.contains(&named)))
            else {
                continue;
            };
            let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
            let to = end(&lines, later - 1);
            let carried: Vec<String> = revised.iter().map(|v| format!("  {REVISED} {v}")).collect();
            lines.splice(to..to, carried);
            let to = end(&lines, at - 1);
            lines.drain(at - 1..to);
            text = lines.join("\n");
            continue 'again;
        }
        return text;
    }
}

fn refusal(root: &Path, body: &str, learning: &Learning) -> Option<String> {
    let class = format!("`{}`", learning.class);
    if learning.class.is_empty() {
        return Some(format!(
            "a learning refused: it names no `class:` line: {body}"
        ));
    }
    if let Some(cite) = learning.instances.iter().find(|c| !resolves(root, c)) {
        return Some(format!("{class} refused: `{cite}` does not resolve"));
    }
    (learning.instances.len() < RECURS).then(|| {
        format!(
            "{class} refused: {} instance, a learning needs {RECURS}",
            learning.instances.len()
        )
    })
}

/// Hands the signal to the auditor role, and writes each learning whose citations resolve, merged into an entry it overlaps.
pub fn run(root: &Path, cfg: &Config) -> Result<Report, AuditError> {
    let dir = &cfg.layout.harness_dir;
    let decisions_rel = config::instance_rel(root, dir, "DECISIONS.md");
    let learnings_rel = config::instance_rel(root, dir, "LEARNINGS.md");
    let tasks_rel = config::instance_rel(root, dir, "TASKS.md");
    let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
    let mut report = Report {
        decisions: decisions_rel.clone(),
        ..Report::default()
    };

    let held = read(root, &decisions_rel)?;
    let mut lines: Vec<String> = stamp(root, cfg, &decisions_rel, &supersede(&held))
        .split('\n')
        .map(String::from)
        .collect();
    let waiting: Vec<String> = waiting(root, cfg)?.into_iter().map(|w| w.class).collect();
    let mut handed = Vec::new();
    let mut removed = Vec::new();
    // from the bottom, so removing a rule leaves every earlier line number true
    for rule in recurring(root, cfg, &lines.join("\n")).into_iter().rev() {
        if rule.revised.last().is_some_and(|c| waiting.contains(c)) {
            continue;
        }
        if rule.revised.len() < REVISIONS {
            handed.push(rule);
            continue;
        }
        let to = end(&lines, rule.line - 1);
        lines.drain(rule.line - 1..to);
        let cited: Vec<String> = rule.recurred.iter().map(|c| format!("`{c}`")).collect();
        removed.push(format!(
            "- [{today}] `{}` removed: {} recurred after {} revisions since {}: {}",
            rule.revised.last().map_or("", String::as_str),
            cited.join(", "),
            rule.revised.len(),
            rule.promoted,
            rule.rule
        ));
        report
            .removed
            .push(format!("{decisions_rel}:{}", rule.line));
    }
    handed.reverse();
    removed.reverse();
    let mut kept = lines.join("\n");
    if !removed.is_empty() {
        kept = with_expired(&kept, &removed);
    }
    if kept != held {
        fs::write(root.join(&decisions_rel), &kept)?;
    }
    let decisions = kept;

    let mut signal = frictions(root, cfg, &config::instance_rel(root, dir, "PROGRESS.md"))?;
    signal.extend(reviewed(&read(root, &tasks_rel)?, &tasks_rel)?);
    signal.extend(reviewed(&decisions, &decisions_rel)?);
    signal.extend(rejections(&decisions, &decisions_rel)?);
    let mut seen = BTreeSet::new();
    signal.retain(|s| seen.insert(s.at.clone()));

    if signal.len() < RECURS && handed.is_empty() {
        return Ok(report);
    }
    let output = ask(
        root,
        cfg,
        prompt(&role(root, cfg), &signal, &decisions_rel, &handed),
    )?;
    let bodies = answers(&output);
    if bodies.is_empty() {
        report
            .refused
            .push(format!("the auditor printed no `{BEGIN}` block"));
    }

    let rels = [learnings_rel, decisions_rel.clone()];
    let original = [read(root, &rels[0])?, decisions];
    let mut texts = original.clone();
    let parsed: Vec<(&String, Learning)> = bodies.iter().map(|b| (b, parse(b))).collect();
    let target = |l: &Learning| {
        handed
            .iter()
            .map(|h| h.line)
            .find(|line| l.revises.as_deref() == Some(format!("{decisions_rel}:{line}").as_str()))
    };
    let mut revisions: Vec<(usize, &String, &Learning)> = parsed
        .iter()
        .filter_map(|(b, l)| Some((target(l)?, *b, l)))
        .collect();
    // from the bottom, so a `revised:` line leaves every earlier rule's line true
    revisions.sort_by_key(|(line, _, _)| std::cmp::Reverse(*line));
    let mut rendered = Vec::new();
    for (line, body, learning) in revisions {
        if let Some(refused) = refusal(root, body, learning) {
            report.refused.push(refused);
            continue;
        }
        let mut lines: Vec<String> = texts[1].split('\n').map(String::from).collect();
        let to = end(&lines, line - 1);
        lines.insert(to, format!("  {REVISED} {today} `{}`", learning.class));
        texts[1] = lines.join("\n");
        report.revised.push(format!(
            "`{}` revises {decisions_rel}:{line}",
            learning.class
        ));
        let proposal = Proposal {
            class: learning.class.clone(),
            instances: learning.instances.clone(),
            learning: learning.text.clone(),
        };
        rendered.push(render(&proposal));
        report.proposed.push(proposal);
    }
    if !rendered.is_empty() {
        texts[1] = with_proposed(&texts[1], &rendered);
    }
    for (body, learning) in parsed.into_iter().filter(|(_, l)| target(l).is_none()) {
        let class = format!("`{}`", learning.class);
        if let Some(refused) = refusal(root, body, &learning) {
            report.refused.push(refused);
            continue;
        }
        let said = format!("{} {}", learning.class, learning.text);
        let entries = standing(&texts[0], &texts[1]);
        match entries.iter().find(|e| overlaps(&said, &e.text)) {
            Some(Entry {
                killed: Some(date), ..
            }) => report.refused.push(format!("{class} killed {date}")),
            Some(entry) => {
                let (text, new) = merge(&texts[entry.file], entry.line, &learning.instances);
                texts[entry.file] = text;
                report.merged.push(format!(
                    "{class} merged into {}:{}, {new} new instances",
                    rels[entry.file],
                    entry.line + 1
                ));
            }
            None => {
                let proposal = Proposal {
                    class: learning.class,
                    instances: learning.instances,
                    learning: learning.text,
                };
                texts[1] = with_proposed(&texts[1], &[render(&proposal)]);
                report.proposed.push(proposal);
            }
        }
    }
    for i in 0..2 {
        if texts[i] != original[i] {
            fs::write(root.join(&rels[i]), &texts[i])?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_shared_words_of_a_short_text_overlap() {
        assert!(overlaps(
            "a dropped result → assert on the caller",
            "the result the caller receives is dropped by spawn, discarding its accounting"
        ));
        assert!(overlaps(
            "the result the caller receives is dropped by spawn, discarding its accounting",
            "a dropped result → assert on the caller"
        ));
    }

    #[test]
    fn one_shared_word_or_a_small_share_is_no_overlap() {
        assert!(!overlaps("a vacuous test", "the test runner timed out"));
        assert!(!overlaps(
            "a test passes with its logic deleted → watch it fail first",
            "a result is dropped before the caller → assert on what the caller receives"
        ));
        assert!(!overlaps(
            "a dropped result → assert on the caller",
            "the result of a slow disk, a timed out runner and a stale lock held by another lane"
        ));
    }

    #[test]
    fn two_shared_words_below_half_is_no_overlap() {
        let (a, b) = (
            "dropped result caller merge review lane",
            "dropped result retried slow runner stale lock",
        );
        assert_eq!(words(a).intersection(&words(b)).count(), 2);
        assert!(!overlaps(a, b));
        assert!(!overlaps(b, a));
        assert!(overlaps("dropped result caller merge", b));
    }

    #[test]
    fn a_cite_opens_or_extends_the_instances_line() {
        let text = "- [proposed] `x`: a → b\n  instances: `a:1`\n- next";
        let (merged, new) = merge(text, 0, &["a:1".to_string(), "b:2".to_string()]);
        assert_eq!(new, 1);
        assert_eq!(
            merged,
            "- [proposed] `x`: a → b\n  instances: `a:1`, `b:2`\n- next"
        );
        let (opened, new) = merge("- [seed] a rule\n  more\n", 0, &["b:2".to_string()]);
        assert_eq!(new, 1);
        assert_eq!(opened, "- [seed] a rule\n  more\n  instances: `b:2`\n");
    }

    #[test]
    fn a_new_section_lands_after_rejected_findings() {
        let text = "# D\n\n## Earned rules\n\n- [2026-01-01] a\n\n## Rejected findings\n\n- x\n\n## [T-001] done\n";
        let once = with_proposed(text, &["- one\n  instances: `a:1`".to_string()]);
        assert_eq!(
            once,
            "# D\n\n## Earned rules\n\n- [2026-01-01] a\n\n## Rejected findings\n\n- x\n\n## Proposed learnings\n\n- one\n  instances: `a:1`\n\n## [T-001] done\n"
        );
        let twice = with_proposed(&once, &["- two".to_string()]);
        assert!(
            twice.contains("  instances: `a:1`\n- two\n\n## [T-001] done"),
            "{twice}"
        );
        assert_eq!(
            with_proposed("", &["- one".to_string()]),
            "## Proposed learnings\n\n- one\n"
        );
    }
}
