//! A rejection nothing has answered: the last verdict in a block's `notes:` is REJECTED and the status is not `ready`. A block parked at `needs-spec` is reported too.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

// every verdict and answer is written into notes: in turn, so only the last one is still open
const VERDICTS: [&str; 5] = ["REJECTED", "VERIFIED", "VERIFIER", "IMPLEMENTER", "Passed"];

// a later paragraph quotes an earlier verdict as `REJECTED:`, so only bare text is read
fn outside_code(notes: &str) -> String {
    notes.split('`').step_by(2).collect::<Vec<_>>().join(" ")
}

fn opening_verdict(text: &str) -> Option<&'static str> {
    VERDICTS
        .iter()
        .find(|word| text.starts_with(**word))
        .copied()
}

fn is_date(word: &str) -> bool {
    word.len() == 10 && word.chars().all(|c| c.is_ascii_digit() || c == '-')
}

// `VERIFIER 2026-09-18:` and `2026-09-18 verifier:` both label a note, and the verdict is what follows.
// The date is what makes the prefix a label: a bare `REJECTED:` opens that rejection's own reasons.
fn after_label(sentence: &str) -> Option<&str> {
    let (label, rest) = sentence.split_once(':')?;
    let words: Vec<&str> = label.split_whitespace().collect();
    let labelled = words.iter().any(|word| is_date(word))
        && words
            .iter()
            .all(|word| is_date(word) || VERDICTS.iter().any(|v| v.eq_ignore_ascii_case(word)));
    labelled.then(|| rest.trim_start())
}

// a verdict opens its sentence: `Only three criteria Passed` inside a rejection is not an answer
fn latest_verdict(notes: &str) -> Option<&'static str> {
    let text = outside_code(notes);
    text.split('\n')
        .flat_map(|line| line.split(". "))
        .filter_map(|sentence| {
            let opening = sentence.trim_start();
            let opening = opening
                .strip_prefix("notes:")
                .unwrap_or(opening)
                .trim_start();
            after_label(opening)
                .and_then(opening_verdict)
                .or_else(|| opening_verdict(opening))
        })
        .last()
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let mut found = Vec::new();
    let tasks = common::instance(ctx, "TASKS.md");
    for block in common::task_blocks(ctx)? {
        let Some((line, status)) = common::field(&block, "status") else {
            continue;
        };
        let notes_at = common::field(&block, "notes").map(|(at, _)| at);
        let notes: String = block
            .body
            .iter()
            .filter(|(at, _)| notes_at.is_some_and(|start| *at >= start))
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if latest_verdict(&notes) == Some("REJECTED") && status != "ready" {
            found.push(common::finding(
                &tasks,
                line,
                format!(
                    "{} carries an unanswered REJECTED while status is {status}",
                    block.id
                ),
            ));
        }
        if status == "needs-spec" {
            found.push(common::finding(
                &tasks,
                line,
                format!("{} is parked at needs-spec", block.id),
            ));
        }
    }
    Ok(found)
}
