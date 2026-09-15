//! A friction recorded twice in PROGRESS.md that neither a LEARNINGS.md rule nor a dated kill line covers: matched by token overlap, not exact text.

use super::common::{self, Res};
use super::ponytail_ceiling;
use super::{Finding, ProbeCtx, ProbeResult};
use std::collections::BTreeSet;

// measured against real friction-line duplicates in this repo's own history, not picked
const FRICTION_OVERLAP: f64 = 0.5;

struct Group {
    first: BTreeSet<String>,
    hits: Vec<(usize, String)>,
}

fn words(text: &str) -> BTreeSet<String> {
    common::normal(text)
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

fn overlap(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    let shared = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        shared / union
    }
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

// ponytail: each group compares only to its first member, so an A-B-C chain whose ends don't overlap stays two groups
fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let mut groups: Vec<Group> = Vec::new();
    for (index, line) in common::lines_of(ctx.root, "PROGRESS.md")?
        .iter()
        .enumerate()
    {
        let Some(rest) = line.strip_prefix("friction:") else {
            continue;
        };
        let text = rest.trim().to_string();
        let key = common::normal(&text);
        if key.is_empty() || key == "none" || key == "na" || key == "nothing" {
            continue;
        }
        let tokens: BTreeSet<String> = key.split(' ').map(String::from).collect();
        match groups
            .iter_mut()
            .find(|group| overlap(&group.first, &tokens) >= FRICTION_OVERLAP)
        {
            Some(group) => group.hits.push((index + 1, text)),
            None => groups.push(Group {
                first: tokens,
                hits: vec![(index + 1, text)],
            }),
        }
    }

    // ponytail: coverage is per-line word-set containment; a rule or kill line that paraphrases every word escapes it
    let mut coverage: Vec<BTreeSet<String>> = if common::exists(ctx.root, "LEARNINGS.md") {
        common::lines_of(ctx.root, "LEARNINGS.md")?
            .iter()
            .filter(|l| l.starts_with("- "))
            .map(|l| words(l))
            .collect()
    } else {
        vec![]
    };
    // a rule the ablation gate refuses closes as a dated refutation instead; `PROGRESS.md` keeps a
    // kill line about anything else from counting
    coverage.extend(
        ponytail_ceiling::kill_lines(ctx)?
            .iter()
            .filter(|line| line.contains("PROGRESS.md"))
            .map(|line| words(line)),
    );
    let covered = |first: &BTreeSet<String>| {
        coverage.iter().any(|rule| {
            !rule.is_empty()
                && first.intersection(rule).count() as f64 / first.len() as f64 >= FRICTION_OVERLAP
        })
    };

    Ok(groups
        .iter()
        .filter(|group| group.hits.len() > 1 && !covered(&group.first))
        .filter_map(|group| {
            let last = group.hits.last()?;
            Some(common::finding(
                "PROGRESS.md",
                last.0,
                format!(
                    "the same friction is recorded {} times and no LEARNINGS.md rule or dated kill line covers it: {}",
                    group.hits.len(),
                    common::cut(&last.1, 90)
                ),
            ))
        })
        .collect())
}
