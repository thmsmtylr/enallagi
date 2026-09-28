//! audit: review findings, frictions and rejections the repository already holds, grouped by rule into classes; the `auditor` role words each class as a proposed learning.

use crate::config::{self, Config};
use crate::probes::{self, CheckOutcome, ProbeCtx, ProbeResult};
use crate::queue::{self, QueueError};
use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

pub const PROPOSED: &str = "## Proposed learnings";
const EARNED: &str = "## Earned rules";
const REJECTED: &str = "## Rejected findings";
const EXPIRED: &str = "## Expired findings";
// a class with fewer instances than this is one occurrence, which PROGRESS.md keeps as evidence
const RECURS: usize = 2;
const BEGIN: &str = "BEGIN ENALLAGI LEARNING";
const END: &str = "END ENALLAGI LEARNING";
const TURNS: u32 = 20;
const TIMEOUT: Duration = Duration::from_secs(600);

pub struct Class {
    pub name: &'static str,
    pub shape: &'static str,
    pub rule: &'static str,
    // matched against `probes::common::normal` text: lowercase words, punctuation as spaces
    pattern: &'static str,
}

// ponytail: two classes in a fixed table, so a class outside it is never proposed; read `[[audit.class]]` from enallagi.toml when a third recurs
pub const CLASSES: [Class; 2] = [
    Class {
        name: "lost-result",
        shape: "a result that does not survive the boundary to its caller",
        rule: "assert on what the caller receives, not on what the callee computed",
        pattern: r"\b(discard\w*|swallow\w*|(drop|ignor|los)\w* (the |its |a )?(\w+ )?(error|result|exit|status|failure|accounting)|(report|return)\w* \w+ (true |ok )?(on|after|despite) a fail\w*)\b",
    },
    Class {
        name: "vacuous-test",
        shape: "a test that passes with the logic deleted",
        rule:
            "delete the branch under test, run the test by name, and watch it fail before it lands",
        pattern: r"\b(pass\w* with (the )?(\w+ )?(\w+ )?(deleted|removed|reverted|gone)|mutants? (survived|survives|lived)|surviving mutants?|vacuous\w*|selects? (0|no|zero) tests?|0 passed|never fails?)\b",
    },
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
    pub class: &'static str,
    pub instances: Vec<String>,
    // the auditor role's learning, as it wrote it between the markers
    pub learning: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub proposed: Vec<Proposal>,
    // a class with enough signal that a kill, an acceptance or an earlier proposal already answers
    pub settled: Vec<String>,
    pub decisions: String,
}

struct Signal {
    at: String,
    text: String,
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

fn frictions(root: &Path, cfg: &Config, rel: &str) -> Result<Vec<Signal>, AuditError> {
    let mut out: Vec<Signal> = read(root, rel)?
        .split('\n')
        .enumerate()
        .filter_map(|(i, l)| {
            Some(Signal {
                at: format!("{rel}:{}", i + 1),
                text: l.strip_prefix("friction:")?.to_string(),
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
            ProbeResult::Count(found) => out.extend(found.into_iter().map(|f| Signal {
                at: format!("{}:{}", f.path, f.line),
                text: f.message,
            })),
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

enum State {
    Proposed,
    Killed(String),
    Accepted,
}

fn state(decisions: &str, class: &str) -> Option<State> {
    let token = format!("`{class}`");
    if section(decisions, EARNED)
        .iter()
        .any(|(_, l)| l.contains(&token))
    {
        return Some(State::Accepted);
    }
    let killed = Regex::new(r"^\s+killed: (\d{4}-\d{2}-\d{2})\b").expect("pattern");
    let entry = entries(&section(decisions, PROPOSED))
        .into_iter()
        .find(|(_, e)| e.lines().next().is_some_and(|l| l.contains(&token)))?;
    Some(match entry.1.lines().find_map(|l| killed.captures(l)) {
        Some(c) => State::Killed(c[1].to_string()),
        None => State::Proposed,
    })
}

pub fn render(proposal: &Proposal) -> String {
    let mut lines = proposal.learning.lines().map(str::trim);
    let first = lines.next().unwrap_or_default();
    let first = first.strip_prefix("- ").unwrap_or(first);
    std::iter::once(format!("- [proposed] `{}`: {first}", proposal.class))
        .chain(lines.filter(|l| !l.is_empty()).map(|l| format!("  {l}")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn prompt(role: &str, class: &Class, instances: &[&Signal]) -> String {
    let cited: Vec<String> = instances
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
    format!(
        "{role}\n\nThe class is `{}`: {}. The step's rule for it: {}.\n\nIts {} instances, each with the text the step read there:\n\n{}\n",
        class.name,
        class.shape,
        class.rule,
        instances.len(),
        cited.join("\n")
    )
}

// a JSON event line is read for the strings it carries, so a preset that streams events still answers
fn answer(output: &str) -> Option<String> {
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
    texts
        .iter()
        .flat_map(|text| {
            text.split(BEGIN)
                .skip(1)
                .filter_map(|part| part.split_once(END))
                .map(|(body, _)| body.trim().to_string())
                .collect::<Vec<_>>()
        })
        .rfind(|body| !body.is_empty())
}

// an installed role file overrides the shipped one, so an empty one leaves the agent no role
fn role(root: &Path, cfg: &Config) -> String {
    let installed = root
        .join(&cfg.layout.harness_dir)
        .join("roles")
        .join("auditor.md");
    let text = fs::read_to_string(installed)
        .unwrap_or_else(|_| include_str!("../../../roles/auditor.md").to_string());
    config::subst(&text, cfg)
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
    crate::agent::spawn(&spawn, &mut events, &stop_file, &rate_limit)
        .map(|result| result.output)
        .map_err(|e| AuditError::Agent(e.to_string()))
}

// the section sits after the earned rules, so an agent reading those does not take a proposal for one
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
            let anchor = match lines.iter().position(|l| l.trim_end() == EARNED) {
                Some(at) => lines[at + 1..]
                    .iter()
                    .position(|l| l.starts_with("## "))
                    .map_or(lines.len(), |i| at + 1 + i),
                None => lines
                    .iter()
                    .position(|l| {
                        let l = l.trim_end();
                        l == REJECTED || l == EXPIRED
                    })
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

/// Reads the signal, proposes a learning per class that recurs and nothing answers yet, and writes them to DECISIONS.md.
pub fn run(root: &Path, cfg: &Config) -> Result<Report, AuditError> {
    let dir = &cfg.layout.harness_dir;
    let decisions_rel = config::instance_rel(root, dir, "DECISIONS.md");
    let tasks_rel = config::instance_rel(root, dir, "TASKS.md");
    let decisions = read(root, &decisions_rel)?;

    let mut signal = frictions(root, cfg, &config::instance_rel(root, dir, "PROGRESS.md"))?;
    signal.extend(reviewed(&read(root, &tasks_rel)?, &tasks_rel)?);
    signal.extend(reviewed(&decisions, &decisions_rel)?);
    signal.extend(rejections(&decisions, &decisions_rel)?);

    let mut report = Report {
        decisions: decisions_rel.clone(),
        ..Report::default()
    };
    let role = role(root, cfg);
    for class in &CLASSES {
        let pattern = Regex::new(class.pattern).expect("pattern");
        let mut matched: Vec<&Signal> = Vec::new();
        for s in &signal {
            if pattern.is_match(&probes::common::normal(&s.text))
                && !matched.iter().any(|m| m.at == s.at)
            {
                matched.push(s);
            }
        }
        if matched.len() < RECURS {
            continue;
        }
        let instances: Vec<String> = matched.iter().map(|s| s.at.clone()).collect();
        match state(&decisions, class.name) {
            None => {
                let output = ask(root, cfg, prompt(&role, class, &matched))?;
                match answer(&output) {
                    Some(learning)
                        if instances
                            .iter()
                            .any(|i| learning.contains(&format!("`{i}`"))) =>
                    {
                        report.proposed.push(Proposal {
                            class: class.name,
                            instances,
                            learning,
                        })
                    }
                    Some(_) => report.settled.push(format!(
                        "`{}` refused: the auditor's learning names no instance of its class",
                        class.name
                    )),
                    None => report.settled.push(format!(
                        "`{}` refused: the auditor printed no `{BEGIN}` block",
                        class.name
                    )),
                }
            }
            Some(State::Proposed) => report
                .settled
                .push(format!("`{}` already proposed", class.name)),
            Some(State::Killed(date)) => report
                .settled
                .push(format!("`{}` killed {date}", class.name)),
            Some(State::Accepted) => report
                .settled
                .push(format!("`{}` accepted under {EARNED}", class.name)),
        }
    }
    if !report.proposed.is_empty() {
        let rendered: Vec<String> = report.proposed.iter().map(render).collect();
        fs::write(
            root.join(&decisions_rel),
            with_proposed(&decisions, &rendered),
        )?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(name: &str) -> Regex {
        let c = CLASSES.iter().find(|c| c.name == name).expect("class");
        Regex::new(c.pattern).expect("pattern")
    }

    #[test]
    fn each_class_reads_its_shape_and_not_the_other() {
        let lost = class("lost-result");
        let vacuous = class("vacuous-test");
        for text in [
            "`enallagi worktree` reporting `merged: true` on a failed fetch",
            "`spawn` returning `Err` after a full `StageResult`, discarding stage accounting",
            "the hook swallowed the exit status",
        ] {
            let text = probes::common::normal(text);
            assert!(lost.is_match(&text) && !vacuous.is_match(&text), "{text}");
        }
        for text in [
            "the test passed with the filter deleted",
            "cargo test printed 0 passed; 0 failed",
            "a mutant survived the citation floor",
        ] {
            let text = probes::common::normal(text);
            assert!(vacuous.is_match(&text) && !lost.is_match(&text), "{text}");
        }
        let noise = probes::common::normal("the lane waited on a slow disk");
        assert!(!lost.is_match(&noise) && !vacuous.is_match(&noise));
    }

    #[test]
    fn a_new_section_lands_before_rejected_findings() {
        let text = "# D\n\n## Earned rules\n\n- [2026-01-01] a\n\n## Rejected findings\n\n- x\n";
        let once = with_proposed(text, &["- one\n  instances: `a:1`".to_string()]);
        assert_eq!(
            once,
            "# D\n\n## Earned rules\n\n- [2026-01-01] a\n\n## Proposed learnings\n\n- one\n  instances: `a:1`\n\n## Rejected findings\n\n- x\n"
        );
        let twice = with_proposed(&once, &["- two".to_string()]);
        assert!(
            twice.contains("  instances: `a:1`\n- two\n\n## Rejected findings"),
            "{twice}"
        );
        assert_eq!(
            with_proposed("", &["- one".to_string()]),
            "## Proposed learnings\n\n- one\n"
        );
    }
}
