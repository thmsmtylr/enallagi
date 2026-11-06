//! issue: one GitHub issue, read with gh, becomes one `proposed` block. No model writes it.

use crate::block::{self, QueueError};
use std::process::Command;

const FIELDS: &str = "number,title,body,url,labels,state";

#[derive(Debug, serde::Deserialize)]
pub struct Issue {
    pub title: String,
    pub body: String,
    pub url: String,
    pub labels: Vec<Label>,
    #[serde(default = "open")]
    pub state: String,
}

fn open() -> String {
    "OPEN".to_string()
}

#[derive(Debug, serde::Deserialize)]
pub struct Label {
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum IssueError {
    #[error("enallagi issue: {0} is neither an issue URL nor owner/repo#n")]
    BadRef(String),
    #[error("enallagi issue: `{command}` failed: {reason}")]
    Gh { command: String, reason: String },
    #[error("enallagi issue: {task} already carries {url}")]
    Queued { task: String, url: String },
    #[error(transparent)]
    Queue(#[from] QueueError),
}

pub fn gh_args(reference: &str) -> Result<Vec<String>, IssueError> {
    let mut args = vec!["issue".to_string(), "view".to_string()];
    if reference.starts_with("https://") && reference.contains("/issues/") {
        args.push(reference.to_string());
    } else {
        let (repo, number) = reference
            .split_once('#')
            .filter(|(repo, n)| {
                repo.split('/').filter(|p| !p.is_empty()).count() == 2
                    && !n.is_empty()
                    && n.bytes().all(|b| b.is_ascii_digit())
            })
            .ok_or_else(|| IssueError::BadRef(reference.to_string()))?;
        args.extend([number.to_string(), "--repo".to_string(), repo.to_string()]);
    }
    args.extend(["--json".to_string(), FIELDS.to_string()]);
    Ok(args)
}

pub fn read(reference: &str) -> Result<Issue, IssueError> {
    let args = gh_args(reference)?;
    let failed = |reason: String| IssueError::Gh {
        command: format!("gh {}", args.join(" ")),
        reason,
    };
    let out = Command::new("gh")
        .args(&args)
        .output()
        .map_err(|e| failed(e.to_string()))?;
    if !out.status.success() {
        return Err(failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| failed(e.to_string()))
}

// `.../issues/12` is a prefix of `.../issues/123`, so a match followed by a digit is another issue
pub(crate) fn carries(line: &str, url: &str) -> bool {
    line.match_indices(url).any(|(at, _)| {
        !line[at + url.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

// the fields a proposed block carries before anyone writes what done means
pub(crate) fn scaffold(id: &str, title: &str, scope: &str) -> Vec<String> {
    vec![
        block::heading(id, title),
        format!("scope: {scope}"),
        "blockedBy:".to_string(),
        "status: needs-triage".to_string(),
        "rows: none — harness".to_string(),
        "criteria:".to_string(),
        format!("  - {}", block::CRITERIA_PLACEHOLDER),
    ]
}

// quoted line by line, so a fence or a heading in the text cannot end the block
pub(crate) fn quote(text: &str, lines: &mut Vec<String>) {
    for line in text.lines() {
        lines.push(format!("  > {line}").trim_end().to_string());
    }
}

pub fn render(issue: &Issue, id: &str) -> String {
    let labels: Vec<&str> = issue.labels.iter().map(|l| l.name.as_str()).collect();
    let mut lines = scaffold(id, issue.title.trim(), block::SCOPE_PLACEHOLDER);
    // the adjudicator kills a block with no probe, command and output unread; an issue's are its state
    lines.splice(
        4..4,
        [
            "probe: issue".to_string(),
            format!(
                "command: `gh issue view {} --json state,title --jq '.state + \": \" + .title'`",
                issue.url
            ),
            format!("output: {}: {}", issue.state, issue.title.trim()),
        ],
    );
    lines.push(format!("notes: {}", issue.url));
    lines.push(format!("  labels: {}", labels.join(", ")));
    quote(&issue.body, &mut lines);
    lines.join("\n")
}

// an archived stub carries no notes:, so a landed finding is found in DECISIONS.md; an expired one may return
pub(crate) fn expired_at(decisions: &str) -> usize {
    decisions
        .split('\n')
        .position(|l| l.trim_end() == "## Expired findings")
        .map_or(usize::MAX, |i| i + 1)
}

// an expired ticket stays in its file under the files layout, and may return like one in `## Expired findings`
pub(crate) fn carrier<'a>(
    blocks: impl Iterator<Item = &'a block::Block>,
    url: &str,
) -> Option<String> {
    blocks
        .filter(|b| {
            !b.body
                .iter()
                .any(|(_, l)| l.starts_with("gate: expired after"))
        })
        .find(|b| b.body.iter().any(|(_, l)| carries(l, url)))
        .map(|b| b.id.clone())
}

/// The block an issue becomes, given the queue's tickets; the caller appends it through the tracker.
pub fn append(
    blocks: &[block::Block],
    decisions: &str,
    issue: &Issue,
) -> Result<String, IssueError> {
    let expired = expired_at(decisions);
    let archived = block::parse(decisions)?;
    let queued = carrier(blocks.iter(), &issue.url)
        .or_else(|| carrier(archived.iter().filter(|b| b.line < expired), &issue.url));
    if let Some(task) = queued {
        return Err(IssueError::Queued {
            task,
            url: issue.url.clone(),
        });
    }
    Ok(render(issue, &block::next_id(blocks, &archived)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(url: &str, body: &str) -> Issue {
        Issue {
            title: "a title".to_string(),
            body: body.to_string(),
            url: url.to_string(),
            labels: Vec::new(),
            state: "OPEN".to_string(),
        }
    }

    #[test]
    fn a_reference_is_a_url_or_owner_repo_number() {
        assert_eq!(
            gh_args("a/b#7").unwrap(),
            ["issue", "view", "7", "--repo", "a/b", "--json", FIELDS]
        );
        for bad in ["a/b", "a#7", "a/b#", "a/b#x", "/b#7", "https://x/pull/1"] {
            assert!(matches!(gh_args(bad), Err(IssueError::BadRef(_))), "{bad}");
        }
    }

    #[test]
    fn longer_issue_number_distinct() {
        let q = format!(
            "{}\nnotes: https://x/issues/123\n",
            block::heading("T-001", "a")
        );
        let q = block::parse(&q).unwrap();
        let block = append(&q, "", &issue("https://x/issues/12", "")).unwrap();
        assert!(
            block.starts_with(&block::heading("T-002", "a title")),
            "{block}"
        );
        assert!(matches!(
            append(&q, "", &issue("https://x/issues/123", "")),
            Err(IssueError::Queued { task, .. }) if task == "T-001"
        ));
    }

    #[test]
    fn longer_archived_number_distinct() {
        let decisions = format!(
            "{}\nnotes: https://x/issues/123\n",
            block::heading("T-001", "a")
        );
        assert!(append(&[], &decisions, &issue("https://x/issues/12", "")).is_ok());
        assert!(matches!(
            append(&[], &decisions, &issue("https://x/issues/123", "")),
            Err(IssueError::Queued { task, .. }) if task == "T-001"
        ));
    }

    #[test]
    fn expired_ticket_returns() {
        let q = format!(
            "{}\nstatus: wontfix\ngate: expired after 5 rounds\nnotes: https://x/issues/12\n",
            block::heading("T-001", "a")
        );
        let q = block::parse(&q).unwrap();
        let block = append(&q, "", &issue("https://x/issues/12", "")).unwrap();
        assert!(block.starts_with(&block::heading("T-002", "a title")));
    }

    #[test]
    fn body_fence_parseable() {
        let body = format!("```\n{}", block::heading("T-9", "x"));
        let block = append(&[], "", &issue("https://x/issues/1", &body)).unwrap();
        let blocks = block::parse(&block).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, "T-001");
    }
}
