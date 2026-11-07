//! twin: a `gh` served from one JSON state file that pushes, replies and merges mutate (ADR 0010).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub const ENV: &str = "ENALLAGI_TWIN";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub repo: String,
    pub origin: PathBuf,
    pub default_branch: String,
    pub pulls: Vec<Pull>,
    pub script: Vec<Round>,
    pub responses: Vec<Response>,
    pub calls: Vec<Vec<String>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Pull {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub head_sha: String,
    pub state: String,
    pub round: u32,
    // statusCheckRollup's recorded shape, served verbatim
    pub checks: Vec<Value>,
    pub reviews: Vec<Note>,
    pub comments: Vec<Note>,
    pub threads: Vec<Thread>,
}

// a review summary or an issue comment
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    pub author: String,
    pub typename: String,
    pub body: String,
    pub url: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Thread {
    pub id: String,
    pub resolved: bool,
    pub outdated: bool,
    pub comments: Vec<Remark>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Remark {
    pub author: String,
    pub typename: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub start_line: Option<u64>,
    pub body: String,
    pub url: String,
}

// applied when a pull whose head matches (`*` for any) reaches `round`
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Round {
    pub head: String,
    pub round: u32,
    pub checks: Vec<Value>,
}

// a canned answer for a call with no state, matched on exact argv
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Response {
    pub args: Vec<String>,
    pub stdout: String,
    pub stderr: String,
    pub exit: i32,
}

impl Response {
    fn ok(stdout: String) -> Response {
        Response {
            stdout,
            ..Response::default()
        }
    }

    fn err(stderr: String) -> Response {
        Response {
            stderr,
            exit: 1,
            ..Response::default()
        }
    }
}

pub fn load(path: &Path) -> State {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("twin: read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("twin: parse {}: {e}", path.display()))
}

// renamed into place, so a reader never sees half a file
pub fn save(path: &Path, state: &State) {
    let tmp = path.with_extension(format!("{}", std::process::id()));
    let text = serde_json::to_string_pretty(state).expect("twin: serialize");
    fs::write(&tmp, text).unwrap_or_else(|e| panic!("twin: write {}: {e}", tmp.display()));
    fs::rename(&tmp, path).unwrap_or_else(|e| panic!("twin: place {}: {e}", path.display()));
}

/// Writes `state` and a `gh` shim execing `bin twin gh` into `dir`; returns the `PATH` to use and the state path.
pub fn install(dir: &Path, bin: &str, state: &State) -> (OsString, PathBuf) {
    fs::create_dir_all(dir).expect("twin: create the shim dir");
    let path = dir.join("twin.json");
    save(&path, state);
    let shim = dir.join("gh");
    fs::write(
        &shim,
        format!(
            "#!/bin/sh\nexec /usr/bin/env {ENV}='{}' '{bin}' twin gh \"$@\"\n",
            path.display()
        ),
    )
    .expect("twin: write the shim");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).expect("twin: chmod");
    }
    let mut dirs = vec![dir.to_path_buf()];
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    (std::env::join_paths(dirs).expect("twin: PATH"), path)
}

/// `enallagi twin gh <args>`: one call against the state file `ENALLAGI_TWIN` names.
pub fn gh(args: &[String]) -> i32 {
    let Some(path) = std::env::var_os(ENV) else {
        eprintln!("twin: {ENV} is not set");
        return 1;
    };
    let path = PathBuf::from(path);
    // ponytail: no lock; two gh calls at once lose one's update. Lock the file if the launcher ever calls gh in parallel.
    let mut state = load(&path);
    state.calls.push(args.to_vec());
    pushes(&mut state);
    let reply = state
        .responses
        .iter()
        .find(|r| r.args == args)
        .cloned()
        .or_else(|| serve(&mut state, args))
        .unwrap_or_else(|| Response::err(format!("twin: no response for gh {}\n", args.join(" "))));
    save(&path, &state);
    print!("{}", reply.stdout);
    eprint!("{}", reply.stderr);
    reply.exit
}

fn origin_sha(state: &State, branch: &str) -> Option<String> {
    let rev = format!("refs/heads/{branch}");
    crate::git::git(&state.origin, &["rev-parse", "--verify", "-q", &rev]).ok()
}

// a head that moved on the origin is a push: a new round, and that round's script
fn pushes(state: &mut State) {
    for i in 0..state.pulls.len() {
        let pull = &state.pulls[i];
        if pull.state != "OPEN" {
            continue;
        }
        let Some(sha) = origin_sha(state, &pull.head) else {
            continue;
        };
        if sha != pull.head_sha {
            state.pulls[i].head_sha = sha;
            state.pulls[i].round += 1;
            apply(state, i);
        }
    }
}

fn apply(state: &mut State, i: usize) {
    let pull = &state.pulls[i];
    let due: Vec<Round> = state
        .script
        .iter()
        .filter(|r| (r.head == "*" || r.head == pull.head) && r.round == pull.round)
        .cloned()
        .collect();
    for round in due {
        state.pulls[i].checks = round.checks;
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].as_str())
}

fn serve(state: &mut State, args: &[String]) -> Option<Response> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    Some(match words.as_slice() {
        ["pr", "create", ..] => create(state, args),
        ["pr", "list", ..] => list(state, args),
        ["pr", "view", ..] => view(state, args),
        ["pr", "comment", ..] => comment(state, args),
        ["api", "graphql", ..] => return graphql(state, args),
        _ => return None,
    })
}

// a url, a number or a head branch, as gh takes them
fn find(state: &State, selector: Option<&String>) -> Option<usize> {
    let s = selector.filter(|s| !s.starts_with('-'))?;
    state
        .pulls
        .iter()
        .position(|p| &p.url == s || p.number.to_string() == *s || &p.head == s)
}

fn create(state: &mut State, args: &[String]) -> Response {
    let Some(head) = flag(args, "--head") else {
        return Response::err("twin: pr create needs --head\n".to_string());
    };
    let base = flag(args, "--base")
        .unwrap_or(&state.default_branch)
        .to_string();
    if let Some(p) = state
        .pulls
        .iter()
        .find(|p| p.head == head && p.base == base && p.state == "OPEN")
    {
        return Response::err(format!(
            "a pull request for branch \"{head}\" into branch \"{base}\" already exists:\n{}\n",
            p.url
        ));
    }
    let Some(head_sha) = origin_sha(state, head) else {
        return Response::err(format!(
            "pull request create failed: GraphQL: Head sha can't be blank, No commits between {base} and {head} (createPullRequest)\n"
        ));
    };
    let body = match (flag(args, "--body"), flag(args, "--body-file")) {
        (Some(b), _) => b.to_string(),
        (None, Some(f)) => fs::read_to_string(f).unwrap_or_default(),
        _ => String::new(),
    };
    let number = state.pulls.iter().map(|p| p.number).max().unwrap_or(0) + 1;
    let url = format!("https://github.com/{}/pull/{number}", state.repo);
    state.pulls.push(Pull {
        number,
        url: url.clone(),
        title: flag(args, "--title").unwrap_or_default().to_string(),
        body,
        head: head.to_string(),
        base,
        head_sha,
        state: "OPEN".to_string(),
        ..Pull::default()
    });
    apply(state, state.pulls.len() - 1);
    Response::ok(format!("{url}\n"))
}

fn author(login: &str, typename: &str) -> Value {
    let typename = if typename.is_empty() {
        "User"
    } else {
        typename
    };
    json!({"login": login, "__typename": typename})
}

fn notes(notes: &[Note]) -> Value {
    notes
        .iter()
        .map(|n| json!({"author": author(&n.author, &n.typename), "body": n.body, "url": n.url}))
        .collect()
}

fn field(pull: &Pull, name: &str) -> Option<Value> {
    Some(match name {
        "number" => json!(pull.number),
        "url" => json!(pull.url),
        "title" => json!(pull.title),
        "body" => json!(pull.body),
        "state" => json!(pull.state),
        "headRefName" => json!(pull.head),
        "baseRefName" => json!(pull.base),
        "headRefOid" => json!(pull.head_sha),
        "statusCheckRollup" => json!(pull.checks),
        "reviews" => notes(&pull.reviews),
        "comments" => notes(&pull.comments),
        _ => return None,
    })
}

fn project(pull: &Pull, fields: &str) -> Result<Value, Response> {
    let mut out = Map::new();
    for name in fields.split(',') {
        let value = field(pull, name)
            .ok_or_else(|| Response::err(format!("Unknown JSON field: \"{name}\"\n")))?;
        out.insert(name.to_string(), value);
    }
    Ok(Value::Object(out))
}

fn list(state: &State, args: &[String]) -> Response {
    let fields = flag(args, "--json").unwrap_or("number,url,title,headRefName");
    let want = flag(args, "--state").unwrap_or("open").to_uppercase();
    let mut out = Vec::new();
    for pull in &state.pulls {
        if want != "ALL" && pull.state != want {
            continue;
        }
        match project(pull, fields) {
            Ok(v) => out.push(v),
            Err(e) => return e,
        }
    }
    Response::ok(format!("{}\n", Value::Array(out)))
}

fn view(state: &State, args: &[String]) -> Response {
    let Some(i) = find(state, args.get(2)) else {
        return Response::err(format!(
            "no pull requests found for {}\n",
            args.get(2).map_or("", String::as_str)
        ));
    };
    let Some(fields) = flag(args, "--json") else {
        return Response::err("twin: pr view is served with --json only\n".to_string());
    };
    match project(&state.pulls[i], fields) {
        Ok(v) => Response::ok(format!("{v}\n")),
        Err(e) => e,
    }
}

fn comment(state: &mut State, args: &[String]) -> Response {
    let Some(i) = find(state, args.get(2)) else {
        return Response::err("twin: pr comment names no pull request\n".to_string());
    };
    let pull = &mut state.pulls[i];
    let url = format!("{}#issuecomment-{}", pull.url, pull.comments.len() + 1);
    pull.comments.push(Note {
        author: "twin".to_string(),
        body: flag(args, "--body").unwrap_or_default().to_string(),
        url: url.clone(),
        ..Note::default()
    });
    Response::ok(format!("{url}\n"))
}

// the operation a document names: `query Name(...)` or `mutation Name(...)`
fn operation(query: &str) -> Option<&str> {
    let rest = query
        .trim_start()
        .strip_prefix("query")
        .or_else(|| query.trim_start().strip_prefix("mutation"))?;
    let name = rest.trim_start();
    let end = name
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(name.len());
    (end > 0).then(|| &name[..end])
}

fn graphql(state: &mut State, args: &[String]) -> Option<Response> {
    let mut vars = Map::new();
    for w in args.windows(2) {
        if matches!(w[0].as_str(), "-f" | "-F" | "--raw-field" | "--field") {
            if let Some((k, v)) = w[1].split_once('=') {
                vars.insert(k.to_string(), json!(v));
            }
        }
    }
    let var = |k: &str| vars.get(k).and_then(Value::as_str).unwrap_or_default();
    Some(match operation(var("query"))? {
        "ReviewThreads" => {
            let repo = format!("{}/{}", var("owner"), var("repo"));
            let pull = state
                .pulls
                .iter()
                .find(|p| repo == state.repo && p.number.to_string() == var("number"));
            match pull {
                Some(p) => Response::ok(format!("{}\n", threads(p))),
                None => Response::err(format!(
                    "GraphQL: Could not resolve to a PullRequest with the number of {}. (repository.pullRequest)\n",
                    var("number")
                )),
            }
        }
        "SettleThread" => settle(state, var("thread"), var("body")),
        _ => return None,
    })
}

fn page(nodes: Vec<Value>) -> Value {
    json!({"pageInfo": {"hasNextPage": false}, "nodes": nodes})
}

fn threads(pull: &Pull) -> Value {
    let reviews = pull
        .reviews
        .iter()
        .map(|r| json!({"author": author(&r.author, &r.typename), "body": r.body}))
        .collect();
    let threads = pull
        .threads
        .iter()
        .map(|t| {
            let comments = t
                .comments
                .iter()
                .map(|c| {
                    json!({
                        "author": author(&c.author, &c.typename),
                        "path": c.path,
                        "line": c.line,
                        "startLine": c.start_line,
                        "body": c.body,
                        "url": c.url,
                    })
                })
                .collect();
            json!({"id": t.id, "isResolved": t.resolved, "isOutdated": t.outdated, "comments": page(comments)})
        })
        .collect();
    json!({"data": {"repository": {"pullRequest": {"reviews": page(reviews), "reviewThreads": page(threads)}}}})
}

fn settle(state: &mut State, id: &str, body: &str) -> Response {
    let found = state.pulls.iter_mut().find_map(|p| {
        let url = p.url.clone();
        p.threads.iter_mut().find(|t| t.id == id).map(|t| (url, t))
    });
    let Some((url, thread)) = found else {
        return Response::err(format!(
            "GraphQL: Could not resolve to a node with the global id of '{id}'. (addPullRequestReviewThreadReply)\n"
        ));
    };
    let first = thread.comments.first().cloned().unwrap_or_default();
    let url = format!("{url}#discussion_{id}_{}", thread.comments.len() + 1);
    thread.comments.push(Remark {
        author: "twin".to_string(),
        path: first.path,
        line: first.line,
        start_line: first.start_line,
        body: body.to_string(),
        url: url.clone(),
        ..Remark::default()
    });
    thread.resolved = true;
    Response::ok(format!(
        "{}\n",
        json!({"data": {
            "addPullRequestReviewThreadReply": {"comment": {"url": url}},
            "resolveReviewThread": {"thread": {"isResolved": true}},
        }})
    ))
}
