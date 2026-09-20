//! review: a pull request's open review comments, read with gh, become proposed blocks. No model writes them.

use crate::issue;
use crate::queue::{self, QueueError};
use std::process::Command;

// ponytail: first:100 on each list; a longer pull request is reported short rather than read, and
// `hasNextPage` is what says so. Follow the cursors when one repository needs it.
const QUERY: &str = concat!(
    "query($owner:String!,$repo:String!,$number:Int!){",
    "repository(owner:$owner,name:$repo){pullRequest(number:$number){",
    "reviews(first:100){pageInfo{hasNextPage} nodes{body}}",
    "reviewThreads(first:100){pageInfo{hasNextPage} nodes{isResolved isOutdated ",
    "comments(first:100){pageInfo{hasNextPage} nodes{path body url}}}}",
    "}}}",
);

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("enallagi review: {0} is neither a pull request URL nor owner/repo#n")]
    BadRef(String),
    #[error("enallagi review: `{command}` failed: {reason}")]
    Gh { command: String, reason: String },
    #[error(transparent)]
    Queue(#[from] QueueError),
}

#[derive(Debug, serde::Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
    #[serde(rename = "pageInfo", default)]
    page: Option<PageInfo>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
}

impl<T> Nodes<T> {
    fn short(&self) -> bool {
        self.page.as_ref().is_some_and(|p| p.has_next_page)
    }
}

#[derive(Debug, serde::Deserialize)]
struct Summary {
    body: String,
}

#[derive(Debug, serde::Deserialize)]
struct Remark {
    path: Option<String>,
    body: String,
    url: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Thread {
    is_resolved: bool,
    is_outdated: bool,
    comments: Nodes<Remark>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequest {
    reviews: Nodes<Summary>,
    review_threads: Nodes<Thread>,
}

#[derive(Debug, serde::Deserialize)]
struct Repository {
    #[serde(rename = "pullRequest")]
    pull_request: PullRequest,
}

#[derive(Debug, serde::Deserialize)]
struct Data {
    repository: Repository,
}

#[derive(Debug, serde::Deserialize)]
struct Response {
    data: Data,
}

// what a block needs from one comment: its anchor, its words and its permalink
#[derive(Debug)]
pub struct Anchored {
    pub path: String,
    pub body: String,
    pub url: String,
}

#[derive(Debug, Default)]
pub struct Found {
    pub anchored: Vec<Anchored>,
    pub unanchored: usize,
    pub settled: usize,
    // a list came back at its page limit, so what follows was never read
    pub short: bool,
}

#[derive(Debug, Default)]
pub struct Appended {
    pub blocks: Vec<String>,
    pub carried: Vec<(String, String)>,
    pub queue: String,
}

fn valid<'a>(
    owner: &'a str,
    repo: &'a str,
    number: &'a str,
) -> Option<(&'a str, &'a str, &'a str)> {
    let ok = !owner.is_empty()
        && !repo.is_empty()
        && !repo.contains('/')
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit());
    ok.then_some((owner, repo, number))
}

fn parts(reference: &str) -> Option<(&str, &str, &str)> {
    if let Some(rest) = reference.strip_prefix("https://") {
        let mut path = rest.split('/').skip(1);
        let owner = path.next()?;
        let repo = path.next()?;
        if path.next()? != "pull" {
            return None;
        }
        return valid(owner, repo, path.next()?);
    }
    let (repo, number) = reference.split_once('#')?;
    let (owner, repo) = repo.split_once('/')?;
    valid(owner, repo, number)
}

pub fn gh_args(reference: &str) -> Result<Vec<String>, ReviewError> {
    let (owner, repo, number) =
        parts(reference).ok_or_else(|| ReviewError::BadRef(reference.to_string()))?;
    Ok(vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={QUERY}"),
        "-f".to_string(),
        format!("owner={owner}"),
        "-f".to_string(),
        format!("repo={repo}"),
        "-F".to_string(),
        format!("number={number}"),
    ])
}

fn collect(response: Response) -> Found {
    let pull = response.data.repository.pull_request;
    // a review's own words carry no anchor, so they are a finding nobody can scope
    let unanchored = pull
        .reviews
        .nodes
        .iter()
        .filter(|r| !r.body.trim().is_empty())
        .count();
    let mut found = Found {
        unanchored,
        short: pull.reviews.short() || pull.review_threads.short(),
        ..Found::default()
    };
    for thread in pull.review_threads.nodes {
        found.short |= thread.comments.short();
        if thread.is_resolved || thread.is_outdated {
            found.settled += thread.comments.nodes.len();
            continue;
        }
        for remark in thread.comments.nodes {
            match remark.path {
                Some(path) => found.anchored.push(Anchored {
                    path,
                    body: remark.body,
                    url: remark.url,
                }),
                None => found.unanchored += 1,
            }
        }
    }
    found
}

pub fn read(reference: &str) -> Result<Found, ReviewError> {
    let args = gh_args(reference)?;
    let failed = |reason: String| ReviewError::Gh {
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
    let response: Response =
        serde_json::from_slice(&out.stdout).map_err(|e| failed(e.to_string()))?;
    Ok(collect(response))
}

// a comment has no title, so the block takes its first line without the marker that opens it
fn title(comment: &Anchored) -> String {
    comment
        .body
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '>']).trim())
        .find(|l| !l.is_empty())
        .map_or_else(|| comment.path.clone(), str::to_string)
}

pub fn render(comment: &Anchored, id: &str) -> String {
    let mut lines = issue::scaffold(id, &title(comment), &comment.path);
    lines.push(format!("notes: {}", comment.url));
    issue::quote(&comment.body, &mut lines);
    lines.join("\n")
}

pub fn append(text: &str, decisions: &str, found: &Found) -> Result<Appended, ReviewError> {
    let expired = issue::expired_at(decisions);
    let archived = queue::parse(decisions)?;
    let mut blocks = queue::parse(text)?;
    let mut out = Appended::default();
    let mut next = text.trim_end_matches('\n').to_string();
    for comment in &found.anchored {
        let carrier = issue::carrier(blocks.iter(), &comment.url)
            .or_else(|| issue::carrier(archived.iter().filter(|b| b.line < expired), &comment.url));
        if let Some(task) = carrier {
            out.carried.push((task, comment.url.clone()));
            continue;
        }
        let block = render(comment, &queue::next_id(&blocks, &archived));
        if !next.is_empty() {
            next.push_str("\n\n");
        }
        next.push_str(&block);
        blocks = queue::parse(&next)?;
        out.blocks.push(block);
    }
    out.queue = if out.blocks.is_empty() {
        text.to_string()
    } else {
        format!("{next}\n")
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchored(path: &str, body: &str, url: &str) -> Found {
        Found {
            anchored: vec![Anchored {
                path: path.to_string(),
                body: body.to_string(),
                url: url.to_string(),
            }],
            ..Found::default()
        }
    }

    #[test]
    fn a_reference_is_a_url_or_owner_repo_number() {
        assert_eq!(parts("a/b#7"), Some(("a", "b", "7")));
        assert_eq!(
            parts("https://github.com/a/b/pull/7"),
            Some(("a", "b", "7"))
        );
        for bad in [
            "a/b",
            "a#7",
            "a/b#",
            "a/b#x",
            "/b#7",
            "a/b/c#7",
            "https://github.com/a/b/issues/7",
            "https://github.com/a/b",
        ] {
            assert!(matches!(gh_args(bad), Err(ReviewError::BadRef(_))), "{bad}");
        }
    }

    #[test]
    fn a_settled_thread_yields_no_comment() {
        let json = include_str!("../tests/fixtures/reviews/settled.json");
        let found = collect(serde_json::from_str(json).expect("parse"));
        assert_eq!(found.settled, 2);
        assert_eq!(found.anchored.len(), 1);
        assert_eq!(found.anchored[0].path, "src/cli.rs");
    }

    #[test]
    fn a_body_fence_leaves_the_queue_parseable() {
        let body = format!("```\n{}", queue::heading("T-9", "x"));
        let found = anchored("src/a.rs", &body, "https://x/pull/1#discussion_r1");
        let out = append("", "", &found).unwrap();
        let blocks = queue::parse(&out.queue).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, "T-001");
    }

    #[test]
    fn a_comment_already_queued_is_carried() {
        let found = anchored("src/a.rs", "x", "https://x/pull/1#discussion_r12");
        let queued = format!(
            "{}\nnotes: https://x/pull/1#discussion_r123\n",
            queue::heading("T-001", "a")
        );
        let out = append(&queued, "", &found).unwrap();
        assert_eq!(out.blocks.len(), 1, "a longer id read as the same comment");
        let out = append(&out.queue, "", &found).unwrap();
        assert!(out.blocks.is_empty());
        assert_eq!(out.carried[0].0, "T-002");
    }
}
