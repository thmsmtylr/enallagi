//! review: a pull request's review threads, review bodies, comments and failed checks, read with gh, become findings in one shape. No model writes them.

use crate::block::{self, QueueError};
use crate::issue;
use std::path::Path;
use std::process::Command;

// ponytail: first:100 on each list; a longer pull request is reported short rather than read, and
// `hasNextPage` is what says so. Follow the cursors when one repository needs it.
// `{who}` is the bot table's account selection, the one place that names it
const QUERY: &str = concat!(
    "query ReviewThreads($owner:String!,$repo:String!,$number:Int!){",
    "repository(owner:$owner,name:$repo){pullRequest(number:$number){headRefOid ",
    "comments(first:100){pageInfo{hasNextPage} nodes{{who} body url}} ",
    "reviews(first:100){pageInfo{hasNextPage} nodes{{who} body url}} ",
    "reviewThreads(first:100){pageInfo{hasNextPage} nodes{id isResolved isOutdated ",
    "comments(first:100){pageInfo{hasNextPage} nodes{{who} path line startLine body url}}}} ",
    "commits(last:1){nodes{commit{statusCheckRollup{contexts(first:100){nodes{__typename ",
    "... on CheckRun{name conclusion detailsUrl} ... on StatusContext{context state targetUrl}}}}}}}",
    "}}}",
);

// the loop's own replies carry this, so a read never takes them for findings
pub const OWN: &str = "<!-- enallagi:";

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

impl<T> Default for Nodes<T> {
    fn default() -> Self {
        Nodes {
            nodes: Vec::new(),
            page: None,
        }
    }
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

// a review body or a plain comment
#[derive(Debug, serde::Deserialize)]
struct Note {
    #[serde(flatten)]
    who: bots::Who,
    body: String,
    #[serde(default)]
    url: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Remark {
    #[serde(flatten)]
    who: bots::Who,
    path: Option<String>,
    #[serde(default)]
    line: Option<u64>,
    #[serde(default)]
    start_line: Option<u64>,
    body: String,
    url: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Thread {
    #[serde(default)]
    id: Option<String>,
    is_resolved: bool,
    is_outdated: bool,
    comments: Nodes<Remark>,
}

// a CheckRun or a StatusContext, as the rollup's contexts carry them
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Check {
    name: Option<String>,
    conclusion: Option<String>,
    details_url: Option<String>,
    context: Option<String>,
    state: Option<String>,
    target_url: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Rollup {
    contexts: Nodes<Check>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Commit {
    status_check_rollup: Option<Rollup>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Head {
    commit: Commit,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequest {
    #[serde(default)]
    head_ref_oid: String,
    #[serde(default)]
    comments: Nodes<Note>,
    reviews: Nodes<Note>,
    review_threads: Nodes<Thread>,
    #[serde(default)]
    commits: Nodes<Head>,
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

/// One finding, whatever raised it.
#[derive(Debug, Clone, Default)]
pub struct Finding {
    /// `F-<n>`, in read order.
    pub id: String,
    /// `thread`, `review`, `comment` or `check`.
    pub source: &'static str,
    /// The vendor for a bot in the table, else the account's own name.
    pub by: String,
    /// What makes two findings the same: the thread id, a summary's marker, else `by`, path and plain body.
    pub key: String,
    pub url: String,
    pub path: Option<String>,
    lines: (Option<u64>, Option<u64>),
    /// The host's id for a resolvable thread.
    pub thread: Option<String>,
    /// The count a bot's summary carries.
    pub severity: Option<u64>,
    pub body: String,
    /// Later comments in an open thread, quoted under its block.
    pub replies: Vec<String>,
    /// The same finding raised again on the same lines: its permalinks.
    pub also: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Found {
    pub findings: Vec<Finding>,
    /// The head the read saw.
    pub head: String,
    pub settled: usize,
    pub folded: usize,
    // a list came back at its page limit, so what follows was never read
    pub short: bool,
}

#[derive(Debug, Default)]
pub struct Appended {
    pub blocks: Vec<String>,
    pub carried: Vec<(String, String)>,
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
        format!("query={}", QUERY.replace("{who}", bots::WHO)),
        "-f".to_string(),
        format!("owner={owner}"),
        "-f".to_string(),
        format!("repo={repo}"),
        "-F".to_string(),
        format!("number={number}"),
    ])
}

// the poll's skip key in one small read: state, head, newest comment and review, check rollup
const LIGHT: &str = concat!(
    "query PullState($owner:String!,$repo:String!,$number:Int!){",
    "repository(owner:$owner,name:$repo){pullRequest(number:$number){",
    "state headRefOid comments(last:1){nodes{url}} reviews(last:1){nodes{url}} ",
    "commits(last:1){nodes{commit{statusCheckRollup{state}}}}",
    "}}}",
);

#[derive(Debug, PartialEq)]
pub struct Light {
    /// `OPEN`, `MERGED` or `CLOSED`.
    pub state: String,
    /// What `polled` holds: equal keys mean nothing new to read.
    pub key: String,
}

pub fn light(reference: &str) -> Result<Light, ReviewError> {
    let mut args = gh_args(reference)?;
    args[3] = format!("query={LIGHT}");
    let v: serde_json::Value = call(&args)?;
    let pull = &v["data"]["repository"]["pullRequest"];
    // `polled` is read back trimmed, so an empty part is written `-`
    let word = |v: &serde_json::Value| match v.as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => "-".to_string(),
    };
    let key = [
        word(&pull["headRefOid"]),
        word(&pull["comments"]["nodes"][0]["url"]),
        word(&pull["reviews"]["nodes"][0]["url"]),
        word(&pull["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["state"]),
    ];
    Ok(Light {
        state: pull["state"].as_str().unwrap_or_default().to_string(),
        key: key.join(" "),
    })
}

fn call<T: serde::de::DeserializeOwned>(args: &[String]) -> Result<T, ReviewError> {
    let failed = |reason: String| ReviewError::Gh {
        command: format!("gh {}", args.join(" ")),
        reason,
    };
    let out = Command::new("gh")
        .args(args)
        .output()
        .map_err(|e| failed(e.to_string()))?;
    if !out.status.success() {
        return Err(failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| failed(e.to_string()))
}

const FAILED: [&str; 6] = [
    "FAILURE",
    "ERROR",
    "TIMED_OUT",
    "CANCELLED",
    "ACTION_REQUIRED",
    "STARTUP_FAILURE",
];

fn collect(response: Response) -> Found {
    let pull = response.data.repository.pull_request;
    let mut found = Found {
        head: pull.head_ref_oid,
        short: pull.reviews.short() || pull.review_threads.short() || pull.comments.short(),
        ..Found::default()
    };
    // a bot that left a thread at this head restates it in its summary
    let mut threaded: Vec<String> = Vec::new();
    for thread in pull.review_threads.nodes {
        found.short |= thread.comments.short();
        if thread.is_resolved || thread.is_outdated {
            found.settled += thread.comments.nodes.len();
            continue;
        }
        // the response ranks no comment, so the first anchored one names the thread
        let mut remarks = thread.comments.nodes;
        let at = remarks.iter().position(|r| r.path.is_some()).unwrap_or(0);
        if remarks.is_empty() || remarks[at].body.contains(OWN) {
            continue;
        }
        let first = remarks.remove(at);
        let by = first.who.name();
        threaded.push(by.clone());
        let replies: Vec<String> = remarks.into_iter().skip(at).map(|r| r.body).collect();
        found.folded += replies.len();
        let finding = Finding {
            source: "thread",
            key: thread.id.clone().unwrap_or_else(|| {
                format!(
                    "{by} {} {}",
                    first.path.as_deref().unwrap_or("-"),
                    plain(&first.body)
                )
            }),
            by,
            url: first.url,
            path: first.path,
            lines: (first.start_line, first.line),
            thread: thread.id,
            body: first.body,
            replies,
            ..Finding::default()
        };
        let same = found.findings.iter_mut().find(|a| {
            a.path.is_some()
                && a.path == finding.path
                && a.lines == finding.lines
                && plain(&a.body) == plain(&finding.body)
        });
        match same {
            Some(a) => a.also.push(finding.url),
            None => found.findings.push(finding),
        }
    }
    let notes = pull
        .reviews
        .nodes
        .into_iter()
        .map(|n| ("review", n))
        .chain(pull.comments.nodes.into_iter().map(|n| ("comment", n)));
    for (source, note) in notes {
        if note.body.trim().is_empty() || note.body.contains(OWN) {
            continue;
        }
        let by = note.who.name();
        let summary = bots::summary(&by, &note.body);
        if let Some((_, count)) = &summary {
            if threaded.contains(&by) || *count == Some(0) {
                continue;
            }
        }
        let key = match &summary {
            Some((marker, _)) => marker.to_string(),
            None => format!("{by} - {}", plain(&note.body)),
        };
        let finding = Finding {
            source,
            by,
            key,
            url: note.url,
            severity: summary.and_then(|(_, count)| count),
            body: note.body,
            ..Finding::default()
        };
        // a summary edited in place, or posted again, is one finding: the newest words win
        match found.findings.iter_mut().find(|f| f.key == finding.key) {
            Some(f) => *f = finding,
            None => found.findings.push(finding),
        }
    }
    let checks = pull
        .commits
        .nodes
        .into_iter()
        .next_back()
        .and_then(|h| h.commit.status_check_rollup)
        .map(|r| r.contexts.nodes)
        .unwrap_or_default();
    for check in checks {
        let name = check.name.or(check.context).unwrap_or_default();
        let conclusion = check.conclusion.or(check.state).unwrap_or_default();
        if !FAILED.contains(&conclusion.as_str()) {
            continue;
        }
        let url = check.details_url.or(check.target_url).unwrap_or_default();
        found.findings.push(Finding {
            source: "check",
            by: name.clone(),
            key: format!("check {name}"),
            body: format!("{name}: {conclusion} {url}"),
            url,
            ..Finding::default()
        });
    }
    for (n, f) in found.findings.iter_mut().enumerate() {
        f.id = format!("F-{}", n + 1);
    }
    found
}

// case, whitespace and the markdown a review tool wraps a finding in are not the finding
fn plain(body: &str) -> String {
    body.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn read(reference: &str) -> Result<Found, ReviewError> {
    Ok(collect(call(&gh_args(reference)?)?))
}

// a comment has no title, so the block takes its first line without the marker that opens it
fn title(f: &Finding) -> String {
    f.body
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '>']).trim())
        .find(|l| !l.is_empty() && !l.starts_with("<!--"))
        .map_or_else(|| f.url.clone(), str::to_string)
}

pub fn render(f: &Finding, id: &str) -> String {
    let scope = f.path.as_deref().unwrap_or(block::SCOPE_PLACEHOLDER);
    let mut lines = issue::scaffold(id, &title(f), scope);
    lines.push(format!("notes: {}", f.url));
    if let Some(thread) = &f.thread {
        lines.push(format!("  thread: {thread}"));
    }
    if !f.also.is_empty() {
        for url in std::iter::once(&f.url).chain(&f.also) {
            lines.push(format!("  > {url}"));
        }
    }
    issue::quote(&f.body, &mut lines);
    for reply in &f.replies {
        lines.push("  >".to_string());
        issue::quote(reply, &mut lines);
    }
    lines.join("\n")
}

/// A finding as its ticket carries it under its harness trailer: a `finding:` line, its fields indented under it, its body quoted.
pub fn entry(f: &Finding, id: &str, round: u64) -> String {
    let mut lines = vec![
        format!("finding: {id} {} round {round}", f.source),
        format!("  by: {}", f.by),
        format!("  key: {}", f.key),
        format!("  url: {}", f.url),
    ];
    if let Some(path) = &f.path {
        let at = match f.lines {
            (Some(a), Some(b)) if a != b => format!(":{a}-{b}"),
            (_, Some(l)) | (Some(l), None) => format!(":{l}"),
            _ => String::new(),
        };
        lines.push(format!("  anchor: {path}{at}"));
    }
    // `thread-id`, not `thread`: `thread_of` reads the first `thread:` as a block's origin
    if let Some(thread) = &f.thread {
        lines.push(format!("  thread-id: {thread}"));
    }
    if let Some(n) = f.severity {
        lines.push(format!("  severity: {n}"));
    }
    issue::quote(&f.body, &mut lines);
    lines.join("\n")
}

/// The thread id a block carries from the moment `append` wrote it, or none for a block with no origin thread.
pub fn thread_of(block: &block::Block) -> Option<String> {
    block
        .body
        .iter()
        .find_map(|(_, l)| l.trim_start().strip_prefix("thread:"))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

pub const SETTLE: &str = concat!(
    "mutation SettleThread($thread:ID!,$body:String!){",
    "addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$thread,body:$body}){comment{url}} ",
    "resolveReviewThread(input:{threadId:$thread}){thread{isResolved}}",
    "}",
);

/// Replies `body` on the thread and resolves it, through the same gh the read uses.
pub fn settle(root: &Path, thread: &str, body: &str) -> Result<(), ReviewError> {
    let args = [
        "api",
        "graphql",
        "-f",
        &format!("query={SETTLE}"),
        "-f",
        &format!("thread={thread}"),
        "-f",
        &format!("body={body}"),
    ];
    let failed = |reason: String| ReviewError::Gh {
        command: format!("gh api graphql -f thread={thread}"),
        reason,
    };
    let out = Command::new("gh")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| failed(e.to_string()))?;
    if !out.status.success() {
        return Err(failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

/// The blocks the findings become, given the queue's tickets; the caller appends them through the tracker.
pub fn append(
    blocks: &[block::Block],
    decisions: &str,
    found: &Found,
) -> Result<Appended, ReviewError> {
    let expired = issue::expired_at(decisions);
    let archived = block::parse(decisions)?;
    let mut blocks = blocks.to_vec();
    let mut out = Appended::default();
    for f in &found.findings {
        let carrier = issue::carrier(blocks.iter(), &f.url)
            .or_else(|| issue::carrier(archived.iter().filter(|b| b.line < expired), &f.url));
        if let Some(task) = carrier {
            out.carried.push((task, f.url.clone()));
            continue;
        }
        let block = render(f, &block::next_id(&blocks, &archived));
        blocks.extend(block::parse(&block)?);
        out.blocks.push(block);
    }
    Ok(out)
}

// The bot table: the crate's only vendor-specific code. Nothing outside this module names a
// vendor or an account field; a bot not in it is read under its own name.
mod bots {
    pub(super) const WHO: &str = "author{login __typename}";

    struct Bot {
        // GraphQL logins; REST's carry `[bot]`, which `name` drops
        logins: &'static [&'static str],
        vendor: &'static str,
        // the marker of the one summary comment it edits in place, or ""
        summary: &'static str,
        // where the summary states its count of findings
        count: Option<&'static str>,
    }

    const BOTS: &[Bot] = &[
        Bot {
            logins: &["kilo-code-bot"],
            vendor: "Kilo",
            summary: "<!-- kilo-review -->",
            // `[*]`, not `\*`: the floor scan reads a ticket marker in an escaped one
            count: Some(r"[*]{2}Status:[*]{2} (\d+) Issues? Found"),
        },
        // `greptile_confidence_score:N` is a confidence, not a count
        Bot {
            logins: &["greptile-apps"],
            vendor: "Greptile",
            summary: "<!-- greptile_summary -->",
            count: None,
        },
        Bot {
            logins: &["copilot-pull-request-reviewer"],
            vendor: "Copilot",
            summary: "",
            count: None,
        },
        Bot {
            logins: &["cursor"],
            vendor: "Cursor Bugbot",
            summary: "",
            count: None,
        },
        Bot {
            logins: &["coderabbitai"],
            vendor: "CodeRabbit",
            summary: "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->",
            count: None,
        },
        Bot {
            logins: &["claude"],
            vendor: "Claude",
            summary: "<!-- bhfn:1:",
            count: None,
        },
    ];

    #[derive(Debug, Default, serde::Deserialize)]
    pub(super) struct Who {
        #[serde(default)]
        author: Option<Account>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct Account {
        login: String,
        #[serde(rename = "__typename", default)]
        kind: String,
    }

    impl Who {
        /// The vendor for a bot the table holds, else the login with `[bot]` dropped; a deleted account is GitHub's `ghost`.
        pub(super) fn name(&self) -> String {
            let Some(a) = &self.author else {
                return "ghost".to_string();
            };
            let stripped = a.login.strip_suffix("[bot]");
            let bot = a.kind == "Bot" || stripped.is_some();
            let login = stripped.unwrap_or(&a.login);
            BOTS.iter()
                .find(|b| bot && b.logins.contains(&login))
                .map_or_else(|| login.to_string(), |b| b.vendor.to_string())
        }
    }

    /// A summary's marker and count, when `by` is a table bot and `body` carries its marker.
    pub(super) fn summary(by: &str, body: &str) -> Option<(&'static str, Option<u64>)> {
        let b = BOTS
            .iter()
            .find(|b| b.vendor == by && !b.summary.is_empty() && body.contains(b.summary))?;
        let count = b.count.and_then(|re| {
            regex::Regex::new(re)
                .ok()?
                .captures(body)?
                .get(1)?
                .as_str()
                .parse()
                .ok()
        });
        Some((b.summary, count))
    }
}
// end of the bot table

#[cfg(test)]
mod tests {
    use super::*;

    fn anchored(path: &str, body: &str, url: &str) -> Found {
        Found {
            findings: vec![Finding {
                path: Some(path.to_string()),
                body: body.to_string(),
                url: url.to_string(),
                ..Finding::default()
            }],
            ..Found::default()
        }
    }

    use crate::fixture::twin;

    // the read of a twin pull, so every shape is the one the twin serves from the recordings
    fn served(pull: &twin::Pull) -> Found {
        collect(serde_json::from_value(twin::threads(pull)).expect("parse"))
    }

    fn note(login: &str, kind: &str, body: &str, n: u32) -> twin::Note {
        twin::Note {
            author: login.to_string(),
            typename: kind.to_string(),
            body: body.to_string(),
            url: format!("https://github.com/o/r/pull/1#issuecomment-{n}"),
        }
    }

    fn bot_pull(bot: &str, texts: &[&str]) -> twin::Pull {
        let mut pull = twin::Pull {
            number: 1,
            url: "https://github.com/o/r/pull/1".to_string(),
            ..twin::Pull::default()
        };
        let rec = twin::recordings()
            .into_iter()
            .find(|r| r.bot == bot)
            .expect("recording");
        let found: Vec<twin::Finding> = texts
            .iter()
            .map(|t| twin::Finding {
                bot: bot.to_string(),
                path: "src/a.rs".to_string(),
                line: 1,
                text: t.to_string(),
            })
            .collect();
        twin::post(&mut pull, &rec, &found.iter().collect::<Vec<_>>());
        pull
    }

    #[test]
    fn plain_comment_finding() {
        let mut pull = bot_pull("kilo", &[]);
        pull.comments
            .push(note("pat", "User", "please rename this", 1));
        pull.reviews
            .push(note("sam", "User", "looks risky overall", 2));
        let found = served(&pull);
        let got: Vec<(&str, &str, &str)> = found
            .findings
            .iter()
            .map(|f| (f.id.as_str(), f.source, f.by.as_str()))
            .collect();
        assert_eq!(got, [("F-1", "review", "sam"), ("F-2", "comment", "pat")]);
        assert_eq!(found.findings[1].body, "please rename this");
        assert!(found.findings[1].url.ends_with("#issuecomment-1"));
    }

    #[test]
    fn login_maps_vendor() {
        let mut pull = bot_pull("kilo", &[]);
        for (n, (login, kind)) in [
            ("kilo-code-bot[bot]", "Bot"),
            ("copilot-pull-request-reviewer", "Bot"),
            ("someone-else[bot]", "Bot"),
            ("kilo-code-bot", "User"),
        ]
        .into_iter()
        .enumerate()
        {
            pull.comments
                .push(note(login, kind, &format!("remark {n}"), n as u32));
        }
        let by: Vec<String> = served(&pull).findings.into_iter().map(|f| f.by).collect();
        assert_eq!(by, ["Kilo", "Copilot", "someone-else", "kilo-code-bot"]);
    }

    #[test]
    fn summary_dedups_by_marker() {
        let mut pull = bot_pull("kilo", &["first"]);
        pull.threads[0].resolved = true;
        let edited = pull.comments[0].body.replace("1 Issues", "2 Issues");
        pull.comments.push(note("kilo-code-bot", "Bot", &edited, 9));
        let found = served(&pull);
        assert_eq!(found.findings.len(), 1, "{:?}", found.findings);
        let f = &found.findings[0];
        assert_eq!(
            (f.source, f.by.as_str(), f.key.as_str()),
            ("comment", "Kilo", "<!-- kilo-review -->")
        );
        assert_eq!(f.severity, Some(2));
        assert!(f.url.ends_with("#issuecomment-9"));
    }

    #[test]
    fn summary_beside_threads() {
        let found = served(&bot_pull("greptile", &["drops the offset"]));
        let sources: Vec<&str> = found.findings.iter().map(|f| f.source).collect();
        assert_eq!(sources, ["thread"]);
        assert_eq!(found.findings[0].by, "Greptile");

        let mut pull = bot_pull("kilo", &["x"]);
        pull.threads.clear();
        pull.comments[0].body = pull.comments[0].body.replace("1 Issues", "0 Issues");
        assert!(served(&pull).findings.is_empty());

        pull.comments[0].body = pull.comments[0].body.replace("0 Issues", "1 Issues");
        assert_eq!(served(&pull).findings.len(), 1);
    }

    #[test]
    fn failed_check_finding() {
        let mut pull = bot_pull("kilo", &[]);
        pull.checks = vec![
            serde_json::json!({"__typename": "CheckRun", "name": "test", "status": "COMPLETED", "conclusion": "FAILURE", "detailsUrl": "https://github.com/o/r/actions/runs/7/job/8"}),
            serde_json::json!({"__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "SUCCESS", "detailsUrl": "https://x/2"}),
            serde_json::json!({"__typename": "StatusContext", "context": "ci/legacy", "state": "ERROR", "targetUrl": "https://x/3"}),
        ];
        let found = served(&pull);
        let got: Vec<(&str, &str, &str)> = found
            .findings
            .iter()
            .map(|f| (f.source, f.key.as_str(), f.body.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "check",
                    "check test",
                    "test: FAILURE https://github.com/o/r/actions/runs/7/job/8"
                ),
                ("check", "check ci/legacy", "ci/legacy: ERROR https://x/3"),
            ]
        );
    }

    #[test]
    fn own_reply_ignored() {
        let mut pull = bot_pull("kilo", &[]);
        pull.comments.push(note(
            "pat",
            "User",
            "<!-- enallagi:F-1 -->\nAnswered in abc1234.",
            1,
        ));
        pull.comments
            .push(note("pat", "User", "and this one is mine", 2));
        let found = served(&pull);
        assert_eq!(found.findings.len(), 1, "{:?}", found.findings);
        assert_eq!(found.findings[0].body, "and this one is mine");
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
        assert_eq!(found.findings.len(), 1);
        assert_eq!(found.findings[0].path.as_deref(), Some("src/cli.rs"));
    }

    #[test]
    fn body_fence_parseable() {
        let body = format!("```\n{}", block::heading("T-9", "x"));
        let found = anchored("src/a.rs", &body, "https://x/pull/1#discussion_r1");
        let out = append(&[], "", &found).unwrap();
        let blocks = block::parse(&out.blocks[0]).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, "T-001");
    }

    #[test]
    fn block_carries_thread() {
        let json = include_str!("../tests/fixtures/reviews/thread.json");
        let found = collect(serde_json::from_str(json).expect("parse"));
        let out = append(&[], "", &found).unwrap();
        let blocks = block::parse(&out.blocks[0]).unwrap();
        assert_eq!(thread_of(&blocks[0]).as_deref(), Some("PRRT_kwDOA1"));
        let bare = anchored("src/a.rs", "x", "https://x/pull/1#discussion_r1");
        let out = append(&[], "", &bare).unwrap();
        assert_eq!(thread_of(&block::parse(&out.blocks[0]).unwrap()[0]), None);
    }

    #[test]
    fn queued_comment_carried() {
        let found = anchored("src/a.rs", "x", "https://x/pull/1#discussion_r12");
        let queued = format!(
            "{}\nnotes: https://x/pull/1#discussion_r123\n",
            block::heading("T-001", "a")
        );
        let mut queued = block::parse(&queued).unwrap();
        let out = append(&queued, "", &found).unwrap();
        assert_eq!(out.blocks.len(), 1, "a longer id read as the same comment");
        queued.extend(block::parse(&out.blocks[0]).unwrap());
        let out = append(&queued, "", &found).unwrap();
        assert!(out.blocks.is_empty());
        assert_eq!(out.carried[0].0, "T-002");
    }
}
