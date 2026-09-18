//! A sentence in the target's contribution guide that refuses or conditions generated contributions, read before `enallagi pr --push`.
//!
//! ponytail: two word lists over prose catch the explicit refusal only; `--policy-read` is the override for anything subtler.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use std::path::Path;

pub const FILES: [&str; 4] = [
    "CONTRIBUTING.md",
    ".github/CONTRIBUTING.md",
    "AI_POLICY.md",
    ".github/PULL_REQUEST_TEMPLATE.md",
];

// `AI` is case-sensitive so that no word containing "ai" reads as the term
const GENERATED: &str = r"\bAI\b|(?i:\bLLMs?\b|language models?|chatgpt|copilot|machine[- ]generated|\bai[- ](?:generated|assisted|written|produced|authored|tools?)\b|generated (?:contributions?|pull requests?|PRs?|patches|changes|submissions?))";
const REFUSES: &str = r"(?i)\b(?:not|never|no|don't|won't|cannot|can't|must|only|unless|disclose[ds]?|disclosure|reject(?:ed|s)?|refuse[ds]?|prohibit(?:ed|s)?|forbid(?:den|s)?|ban(?:ned|s)?|close[ds]?|decline[ds]?|require[ds]?)\b";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx.root))
}

pub fn find(root: &Path) -> Res<Vec<Finding>> {
    let generated = common::re(GENERATED)?;
    let refuses = common::re(REFUSES)?;
    let mut found = Vec::new();
    for file in FILES {
        if !common::is_file(root, file) {
            continue;
        }
        for (line, sentence) in sentences(&common::read(root, file)?) {
            if generated.is_match(&sentence) && refuses.is_match(&sentence) {
                found.push(common::finding(file, line, format!("\"{sentence}\"")));
            }
        }
    }
    Ok(found)
}

// a sentence wrapped across lines is one sentence, numbered by the line it starts on
fn sentences(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut start = 0;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            if !current.is_empty() {
                out.push((start, std::mem::take(&mut current)));
            }
            continue;
        }
        let mut rest = line;
        while !rest.is_empty() {
            if current.is_empty() {
                start = index + 1;
            } else {
                current.push(' ');
            }
            let end = rest
                .match_indices(['.', '!', '?'])
                .map(|(at, _)| at + 1)
                .find(|&at| rest[at..].is_empty() || rest[at..].starts_with(' '));
            match end {
                Some(at) => {
                    current.push_str(&rest[..at]);
                    out.push((start, std::mem::take(&mut current)));
                    rest = rest[at..].trim_start();
                }
                None => {
                    current.push_str(rest);
                    rest = "";
                }
            }
        }
    }
    if !current.is_empty() {
        out.push((start, current));
    }
    out
}
