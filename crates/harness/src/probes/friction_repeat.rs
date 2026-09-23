//! A friction recorded twice in PROGRESS.md that neither a LEARNINGS.md rule nor a dated kill line covers: matched by token overlap, not exact text.

use super::common::{self, Res};
use super::ponytail_ceiling;
use super::{Finding, ProbeCtx, ProbeResult};
use std::collections::{BTreeSet, HashMap};

// FRICTION_OVERLAP on FRICTION_FIXTURE (tests/probes.rs): reworded pair 0.8750, near-miss pair 0.4762, by `overlap` over `common::normal` words
const FRICTION_OVERLAP: f64 = 0.5;
// RARE_SHARED: PROGRESS.md:136 and :199 share 4 rare long words, no other pair of its 71 frictions more than 2, FRICTION_FIXTURE's near-miss 2
const RARE_SHARED: usize = 3;
// shorter words are the connective prose that a near-miss pair shares, `lane` and `time` in FRICTION_FIXTURE
const RARE_WORD_LEN: usize = 5;
// PROSE_WORDS: closed-class words of RARE_WORD_LEN or more, dropped before RARE_SHARED counts; PROSE_SHARE_FIXTURE's pair falls 4 -> 1, LONG_REPEAT_FIXTURE's 7 -> 6, this repo's PROGRESS.md:136 and :199 hold at 3
const PROSE_WORDS: [&str; 54] = [
    "about",
    "after",
    "again",
    "against",
    "already",
    "although",
    "always",
    "another",
    "anything",
    "because",
    "before",
    "being",
    "below",
    "between",
    "cannot",
    "could",
    "during",
    "either",
    "enough",
    "every",
    "everything",
    "except",
    "further",
    "however",
    "instead",
    "itself",
    "might",
    "neither",
    "nobody",
    "nothing",
    "other",
    "others",
    "rather",
    "really",
    "should",
    "since",
    "something",
    "still",
    "their",
    "there",
    "these",
    "those",
    "through",
    "under",
    "until",
    "where",
    "whether",
    "which",
    "while",
    "whose",
    "within",
    "without",
    "would",
    "yourself",
];

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

// a long word only these two frictions carry: counted, not divided, so a longer rewrite of one friction still matches
fn rare_shared(
    a: &BTreeSet<String>,
    b: &BTreeSet<String>,
    spread: &HashMap<String, usize>,
) -> usize {
    a.intersection(b)
        .filter(|w| {
            w.len() >= RARE_WORD_LEN
                && spread.get(*w) == Some(&2)
                && !PROSE_WORDS.contains(&w.as_str())
        })
        .count()
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

// ponytail: each group compares only to its first member, so an A-B-C chain whose ends don't overlap stays two groups
fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let progress = common::instance(ctx, "PROGRESS.md");
    let learnings = common::instance(ctx, "LEARNINGS.md");
    let mut frictions: Vec<(usize, String, BTreeSet<String>)> = Vec::new();
    for (index, line) in common::lines_of(ctx.root, &progress)?.iter().enumerate() {
        let Some(rest) = line.strip_prefix("friction:") else {
            continue;
        };
        let text = rest.trim().to_string();
        let key = common::normal(&text);
        if key.is_empty() || key == "none" || key == "na" || key == "nothing" {
            continue;
        }
        let tokens: BTreeSet<String> = key.split(' ').map(String::from).collect();
        frictions.push((index + 1, text, tokens));
    }
    let mut spread: HashMap<String, usize> = HashMap::new();
    for word in frictions.iter().flat_map(|(_, _, tokens)| tokens) {
        *spread.entry(word.clone()).or_default() += 1;
    }

    let mut groups: Vec<Group> = Vec::new();
    // ponytail: a rare word is one exactly two frictions carry, so a third occurrence joins only by overlap; count spread outside the group to lift it
    for (line, text, tokens) in frictions {
        match groups.iter_mut().find(|group| {
            overlap(&group.first, &tokens) >= FRICTION_OVERLAP
                || rare_shared(&group.first, &tokens, &spread) >= RARE_SHARED
        }) {
            Some(group) => group.hits.push((line, text)),
            None => groups.push(Group {
                first: tokens,
                hits: vec![(line, text)],
            }),
        }
    }

    // ponytail: coverage is per-line word-set containment; a rule or kill line that paraphrases every word escapes it
    let mut coverage: Vec<BTreeSet<String>> = if common::exists(ctx.root, &learnings) {
        common::lines_of(ctx.root, &learnings)?
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
                &progress,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn counted(words: &[&str]) -> usize {
        let pair: BTreeSet<String> = words.iter().map(|w| w.to_string()).collect();
        let spread: HashMap<String, usize> = words.iter().map(|w| (w.to_string(), 2)).collect();
        rare_shared(&pair, &pair, &spread)
    }

    #[test]
    fn each_prose_word_is_dropped_from_rare_shared() {
        for word in PROSE_WORDS {
            // an entry under RARE_WORD_LEN reads as cover the length filter already gives
            assert!(word.len() >= RARE_WORD_LEN, "{word}");
            assert_eq!(counted(&[word]), 0, "{word}");
        }
        assert_eq!(counted(&["clippy"]), 1);
    }

    #[test]
    fn prose_words_never_carry_a_pair_to_rare_shared() {
        let content = ["clippy", "fixture", "unused"];
        let mut shared = PROSE_WORDS.to_vec();
        shared.extend(&content[..RARE_SHARED - 1]);
        assert_eq!(counted(&shared), RARE_SHARED - 1);
        shared.push(content[RARE_SHARED - 1]);
        assert_eq!(counted(&shared), RARE_SHARED);
    }
}
