//! The round-end question, enforced. First occurrence is evidence and stays in
//! PROGRESS.md; the SECOND becomes a rule in LEARNINGS.md, or the loop is
//! paying for it every round. Exact text match missed five reworded sightings
//! of one friction, which is why this is an overlap.
//!
//! ponytail: greedy, and each group is compared against its FIRST member only,
//! so an A-B-C chain whose ends do not overlap stays two groups -- n is the
//! friction lines in one PROGRESS.md window, so nothing here needs a real
//! clustering.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use std::collections::BTreeSet;

/// Jaccard over the normalised tokens of the `friction:` line, and the number
/// is measured rather than picked. Against the five sightings of "a check
/// firing on the prose that documents it" in this package's own record
/// (PROGRESS.archive.md:213, :312, :428 and :477, at 2026-09-06) every pair of
/// them scores 0.500 or higher -- 0.875 for :428/:477, 0.500 for the widest
/// pair :213/:477 -- and the closest pair that is NOT a repeat, two entries
/// opening "none new. One thing worth the next lane's time" at :1253 and :1351,
/// scores 0.476. So 0.5 separates the whole of the evidence, by 0.024 at its
/// narrowest. The command that produced those numbers is in TASKS.md [T-045]
/// notes.
const FRICTION_OVERLAP: f64 = 0.5;

/// One group: the tokens of its first member, and every sighting in it.
struct Group {
    first: BTreeSet<String>,
    hits: Vec<(usize, String)>,
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

    // A rule covers a friction when most of the friction's words appear in one
    // rule line. The grouping above is overlap, and a verbatim-substring test
    // here meant a rule written in LEARNINGS.md's own format never silenced the
    // probe it was written for. ponytail: containment in the rule's word set,
    // one line at a time; a rule that paraphrases every word escapes it.
    let learned: Vec<BTreeSet<String>> = if common::exists(ctx.root, "LEARNINGS.md") {
        common::lines_of(ctx.root, "LEARNINGS.md")?
            .iter()
            .filter(|l| l.starts_with("- "))
            .map(|l| {
                common::normal(l)
                    .split(' ')
                    .filter(|w| !w.is_empty())
                    .map(String::from)
                    .collect()
            })
            .collect()
    } else {
        vec![]
    };
    let covered = |first: &BTreeSet<String>| {
        learned.iter().any(|rule| {
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
                    "the same friction is recorded {} times and LEARNINGS.md carries no rule for it: {}",
                    group.hits.len(),
                    common::cut(&last.1, 90)
                ),
            ))
        })
        .collect())
}
