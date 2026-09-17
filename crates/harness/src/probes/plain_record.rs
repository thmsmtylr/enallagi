//! A commit subject, a task note or a printed line that comments on the change instead of recording it.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::git;
use regex::Regex;
use std::path::Path;

// the window the `one-row` rail already has a lane re-read at the start of an iteration
const WINDOW: &str = "-20";

struct Rules {
    prefix: Regex,
    clause: Regex,
    count: Regex,
    meaning: Regex,
    aside: Regex,
    print: Regex,
    literal: Regex,
}

impl Rules {
    fn new() -> Res<Rules> {
        Ok(Rules {
            prefix: common::re(r"^[a-z]+(\([^)]*\))?:\s+")?,
            clause: common::re(r",\s|;\s| \u{2014} | \u{2013} | -- ")?,
            count: common::re(
                r"(?i)^(one|two|three|four|five|six|seven|eight|nine|ten|\d+) [a-z]+s$",
            )?,
            meaning: common::re(
                r"(?i)which is why|the point is|it matters|what it meant|never asked for",
            )?,
            aside: common::re(r" \u{2014} | \u{2013} ")?,
            print: common::re(
                r"(println!|eprintln!|print!|eprint!|console\.(log|error|warn))\s*\(",
            )?,
            literal: common::re(r#""(?:[^"\\]|\\.)*""#)?,
        })
    }

    // the first clause is the record of what changed; a later one that only counts it is commentary
    fn commentary(&self, text: &str) -> Option<String> {
        let body = self.prefix.replace(text, "");
        for (index, clause) in self.clause.split(&body).enumerate() {
            let clause = clause.trim();
            if index > 0 && self.count.is_match(clause) {
                return Some(format!("`{clause}` restates the count, not what changed"));
            }
            if self.meaning.is_match(clause) {
                return Some(format!(
                    "`{}` says what the work meant",
                    common::cut(clause, 80)
                ));
            }
        }
        self.aside.split(&body).nth(1).map(|aside| {
            format!(
                "`{}` is an aside set off by an em dash",
                common::cut(aside.trim(), 80)
            )
        })
    }
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let rules = Rules::new()?;
    let mut found = Vec::new();
    subjects(ctx, &rules, &mut found)?;
    notes(ctx, &rules, &mut found)?;
    printed(ctx, &rules, &mut found)?;
    Ok(found)
}

// a repository with no commit yet is empty, not an error; any other git failure is reported as one
fn log(root: &Path) -> Res<Vec<(String, String)>> {
    if git::head(root).is_none() {
        return Ok(Vec::new());
    }
    let out = git::git(root, &["log", WINDOW, "--format=%h%x00%s"]).map_err(|e| e.to_string())?;
    Ok(out
        .lines()
        .filter_map(|line| line.split_once('\0'))
        .map(|(sha, subject)| (sha.to_string(), subject.to_string()))
        .collect())
}

fn subjects(ctx: &ProbeCtx, rules: &Rules, found: &mut Vec<Finding>) -> Res<()> {
    let state = git::state_root(ctx.root, &ctx.cfg.layout.harness_dir);
    let mut roots = vec![ctx.root.to_path_buf()];
    if state != ctx.root {
        roots.push(state);
    }
    for root in roots {
        for (sha, subject) in log(&root)? {
            if let Some(clause) = rules.commentary(&subject) {
                found.push(common::finding(
                    &sha,
                    0,
                    format!("the subject comments on the change: {clause}"),
                ));
            }
        }
    }
    Ok(())
}

fn notes(ctx: &ProbeCtx, rules: &Rules, found: &mut Vec<Finding>) -> Res<()> {
    let tasks = common::instance(ctx, "TASKS.md");
    let fence = common::re(r"^\s*```")?;
    let quoted = common::re("`[^`]*`")?;
    let verdict = common::re(r"^\s*REJECTED\b")?;
    // a notes: field carries no blank line, so the verifier's paragraph ends where the next author signs
    let author = common::re(r"^\s*(IMPLEMENTER|VERIFIED|VERIFIER|OPERATOR)\b")?;
    for block in common::task_blocks(ctx)? {
        let Some((start, _)) = common::field(&block, "notes") else {
            continue;
        };
        let mut fenced = false;
        let mut rejected = false;
        for (at, text) in block.body.iter().filter(|(at, _)| *at >= start) {
            if fence.is_match(text) {
                fenced = !fenced;
                continue;
            }
            // a note is required to paste its command and its output, so only the prose around them is read
            if fenced {
                continue;
            }
            if verdict.is_match(text) {
                rejected = true;
            } else if author.is_match(text) || text.trim().is_empty() {
                rejected = false;
            }
            // the verifier wrote the rejection and the paragraph under it; an implementer cannot clear someone else's words
            if rejected {
                continue;
            }
            if let Some(clause) = rules.commentary(&quoted.replace_all(text, " ")) {
                found.push(common::finding(
                    &tasks,
                    *at,
                    format!("the note comments on the change: {clause}"),
                ));
            }
        }
    }
    Ok(())
}

fn printed(ctx: &ProbeCtx, rules: &Rules, found: &mut Vec<Finding>) -> Res<()> {
    let ext = &ctx.cfg.layout.source_ext;
    for path in common::tracked(ctx.root)? {
        if !ext.iter().any(|e| path.ends_with(e.as_str())) || !common::is_file(ctx.root, &path) {
            continue;
        }
        for (index, line) in common::lines_of(ctx.root, &path)?.iter().enumerate() {
            // ponytail: the macro and its literal have to share a line; a wrapped call is missed, and a parser is the upgrade
            if !rules.print.is_match(line) {
                continue;
            }
            for m in rules.literal.find_iter(line) {
                if let Some(clause) = rules.commentary(m.as_str().trim_matches('"')) {
                    found.push(common::finding(
                        &path,
                        index + 1,
                        format!("the printed line comments on the change: {clause}"),
                    ));
                }
            }
        }
    }
    Ok(())
}
