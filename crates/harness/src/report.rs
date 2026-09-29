//! report: one markdown document about the repository the loop worked on, read from DECISIONS.md, the event log and six probes. No model writes it.

use crate::cli::events::{summarize, TaskRow};
use crate::config::Config;
use crate::events::{self, task_of, Kind, Log};
use crate::probes::{self, CheckOutcome, ProbeCtx, ProbeResult};
use crate::{git, queue};
use regex::Regex;
use std::path::Path;

pub const STALENESS: &str = "## Staleness";
pub const REPOSITORY: &str = "## This repository";
pub const HARNESS: &str = "## The harness";
const HARNESS_ROWS: &str = "none — harness";
pub const SKILL_ID: &str = "enallagi-report";

// the probes that detect a behaviour repeating; none of them runs the check
const PROBES: [&str; 6] = [
    "friction-repeat",
    "verdict-flip",
    "rejection-repeat",
    "stage-outlier",
    "turns-exhausted",
    "limit-repeat",
];

pub fn document(root: &Path, cfg: &Config) -> anyhow::Result<String> {
    let head = git::git(root, &["rev-parse", "HEAD"]).map_err(anyhow::Error::msg)?;
    let decisions_rel = format!("{}/DECISIONS.md", cfg.layout.harness_dir);
    let decisions = match std::fs::read_to_string(root.join(&decisions_rel)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => anyhow::bail!("{decisions_rel}: {e}"),
    };
    let log = Log::open(&root.join(&cfg.layout.harness_dir));
    let events = log.read()?;

    let mut runs: Vec<&str> = Vec::new();
    for e in &events {
        if matches!(e.kind, Kind::RunStart { .. }) && !runs.contains(&e.run.as_str()) {
            runs.push(&e.run);
        }
    }
    let rows = summarize(events.iter());
    let sha_of = |task: &str| {
        events
            .iter()
            .rev()
            .filter(|e| task_of(&e.kind) == Some(task))
            .find_map(|e| e.sha.as_deref())
    };

    let binary = events::binary();
    let mut doc = format!(
        "# enallagi report\n\n{STALENESS}\n\n\
         - product HEAD {head} — `git rev-parse HEAD`\n\
         - binary {} at {} — `enallagi --version`\n\
         - runs read: {} — `enallagi events --json`\n",
        binary.version,
        binary.commit,
        if runs.is_empty() {
            "none".to_string()
        } else {
            runs.join(", ")
        }
    );

    let mut product = Vec::new();
    let mut harness = Vec::new();
    for block in queue::parse(&decisions)? {
        let rows_line = block
            .body
            .iter()
            .find_map(|(_, l)| l.strip_prefix("rows:"))
            .unwrap_or("");
        let mut line = format!(
            "- {} {} — `{decisions_rel}:{}`",
            block.id, block.title, block.line
        );
        if let Some(row) = rows.iter().find(|r| r.task == block.id) {
            line.push_str(&metrics(row, sha_of(&block.id)));
        }
        if rows_line.trim() == HARNESS_ROWS {
            harness.push(line);
        } else {
            product.push(line);
        }
    }

    // the check is never run: these six read PROGRESS.md, the queue and the log
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
    let names: Vec<String> = PROBES.iter().map(|n| n.to_string()).collect();
    let prefix = format!("{}/", root.display());
    for (name, result) in probes::run_all(&ctx, &names) {
        match result {
            ProbeResult::Count(found) if found.is_empty() => {
                harness.push(format!("- {name}: nothing found — `enallagi probe {name}`"));
            }
            ProbeResult::Count(found) => harness.extend(found.into_iter().map(|f| {
                let path = f.path.strip_prefix(&prefix).unwrap_or(&f.path);
                format!(
                    "- {name}: {} — `{path}:{}`",
                    f.message.replace('\n', " "),
                    f.line
                )
            })),
            ProbeResult::Error(e) | ProbeResult::Off(e) => harness.push(format!(
                "- {name}: not read, {} — `enallagi probe {name}`",
                e.replace('\n', " ")
            )),
        }
    }

    for (heading, lines) in [(REPOSITORY, product), (HARNESS, harness)] {
        doc.push_str(&format!("\n{heading}\n\n"));
        if lines.is_empty() {
            doc.push_str(&format!(
                "- nothing archived — `{decisions_rel}:{}`\n",
                decisions.lines().count().max(1)
            ));
        }
        for line in lines {
            doc.push_str(&line);
            doc.push('\n');
        }
    }

    let bare = unevidenced(&doc);
    if !bare.is_empty() {
        anyhow::bail!(
            "enallagi report: a claim carries no evidence:\n{}",
            bare.join("\n")
        );
    }
    Ok(doc)
}

fn metrics(row: &TaskRow, sha: Option<&str>) -> String {
    let mut out = format!(
        "; stages {}, verify rounds {}, rejections {}, ${:.2}",
        row.stages, row.verify_rounds, row.rejections, row.cost
    );
    match sha {
        Some(sha) => out.push_str(&format!(" at {}", &sha[..sha.len().min(7)])),
        None => out.push_str(&format!(
            " — `enallagi events --summary --task {}`",
            row.task
        )),
    }
    out
}

// every line that is not a heading is a claim, and carries a `file:line`, a command, or a sha
pub fn unevidenced(doc: &str) -> Vec<String> {
    let evidence =
        Regex::new(r"`[^`\s]+:\d+`|`(enallagi|git) [^`]+`|\bat [0-9a-f]{7,40}\b").expect("pattern");
    doc.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter(|l| !evidence.is_match(l))
        .map(str::to_string)
        .collect()
}

pub fn skill(doc: &str) -> String {
    format!(
        "---\nname: {SKILL_ID}\ndescription: What the loop recorded about this repository, each claim with its evidence. Written by `enallagi report --skill`.\n---\n\n{doc}"
    )
}
