//! issue: one GitHub issue, read with gh, becomes one `proposed` block. No model writes it.

use crate::queue::{self, QueueError};
use std::process::Command;

const FIELDS: &str = "number,title,body,url,labels";

#[derive(Debug, serde::Deserialize)]
pub struct Issue {
    pub title: String,
    pub body: String,
    pub url: String,
    pub labels: Vec<Label>,
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
fn carries(line: &str, url: &str) -> bool {
    line.match_indices(url).any(|(at, _)| {
        !line[at + url.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

// the body is quoted line by line, so a fence or a heading in it cannot end the block
pub fn render(issue: &Issue, id: &str) -> String {
    let labels: Vec<&str> = issue.labels.iter().map(|l| l.name.as_str()).collect();
    let mut lines = vec![
        queue::heading(id, issue.title.trim()),
        "scope: src/thing.ts, src/thing.test.ts".to_string(),
        "blockedBy:".to_string(),
        "status: proposed".to_string(),
        "rows: none — harness".to_string(),
        "criteria:".to_string(),
        "  - <objective, and naming the command whose output changes when it is done>".to_string(),
        format!("notes: {}", issue.url),
        format!("  labels: {}", labels.join(", ")),
    ];
    for line in issue.body.lines() {
        lines.push(format!("  > {line}").trim_end().to_string());
    }
    lines.join("\n")
}

// returns the block and the queue with it appended
pub fn append(text: &str, issue: &Issue) -> Result<(String, String), IssueError> {
    let blocks = queue::parse(text)?;
    if let Some(b) = blocks
        .iter()
        .find(|b| b.body.iter().any(|(_, l)| carries(l, &issue.url)))
    {
        return Err(IssueError::Queued {
            task: b.id.clone(),
            url: issue.url.clone(),
        });
    }
    let block = render(issue, &queue::next_id(&blocks));
    let mut next = text.trim_end_matches('\n').to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(&block);
    next.push('\n');
    Ok((block, next))
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
    fn a_longer_issue_number_is_not_a_duplicate() {
        let q = format!(
            "{}\nnotes: https://x/issues/123\n",
            queue::heading("T-001", "a")
        );
        let (block, _) = append(&q, &issue("https://x/issues/12", "")).unwrap();
        assert!(
            block.starts_with(&queue::heading("T-002", "a title")),
            "{block}"
        );
        assert!(matches!(
            append(&q, &issue("https://x/issues/123", "")),
            Err(IssueError::Queued { task, .. }) if task == "T-001"
        ));
    }

    #[test]
    fn a_body_fence_leaves_the_queue_parseable() {
        let body = format!("```\n{}", queue::heading("T-9", "x"));
        let (_, next) = append("", &issue("https://x/issues/1", &body)).unwrap();
        let blocks = queue::parse(&next).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, "T-001");
    }
}
