//! The queue behind one interface: the legacy single TASKS.md, or one file per ticket under `specs/<slug>/issues/` in Pocock's local-tracker format. Every adapter hands back `queue::Block`s whose body is in the legacy `key: value` form, so a caller reads a field the same way whichever layout it is under.

use crate::git;
use crate::queue::{self, Block, QueueError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The subject of the commit `enallagi tasks migrate` makes; a ticket file it added was queued before it.
pub const MIGRATE_SUBJECT: &str = "tasks: migrate to the specs layout";

/// The instance names the queue lives under, legacy first; a commit by name passes both, and the missing one is skipped.
pub const QUEUE: [&str; 2] = ["TASKS.md", "specs"];

const STATUS: &str = "**Status:**";
const BLOCKED_BY: &str = "**Blocked by:**";
const CRITERIA: &str = "## Acceptance criteria";
const COMMENTS: &str = "## Comments";
const HARNESS: &str = "## Harness";

pub enum Tracker {
    Legacy(PathBuf),
    Files(PathBuf),
}

/// A ticket state in Pocock's names; a legacy name reads as the one it became.
pub fn canonical(status: &str) -> &str {
    match status {
        "proposed" => "needs-triage",
        "ready" | "blocked" => "ready-for-agent",
        "needs-spec" => "needs-info",
        "attended" => "ready-for-human",
        "deferred" => "wontfix",
        other => other,
    }
}

fn legacy_name(status: &str) -> &str {
    match status {
        "needs-triage" => "proposed",
        "ready-for-agent" | "ready-for-human" => "ready",
        "needs-info" => "needs-spec",
        "wontfix" => "deferred",
        other => other,
    }
}

pub fn state(b: &Block) -> Option<String> {
    let raw = queue::field(b, "status")?;
    let name = canonical(&raw);
    if name == "ready-for-agent" && queue::field(b, "attended").as_deref() == Some("true") {
        return Some("ready-for-human".to_string());
    }
    Some(name.to_string())
}

/// A ticket ready for anyone: the legacy `ready`, which `attended: true` splits in two.
pub const READY: [&str; 2] = ["ready-for-agent", "ready-for-human"];

/// The ids whose state is one of `states`, in queue order.
pub fn ids_in(blocks: &[Block], states: &[&str]) -> Vec<String> {
    blocks
        .iter()
        .filter(|b| state(b).is_some_and(|s| states.contains(&s.as_str())))
        .map(|b| b.id.clone())
        .collect()
}

/// The first `ready-for-agent` ticket, in the order given, whose blockers are all `done`.
pub fn takeable(blocks: &[Block]) -> Option<String> {
    let states: HashMap<&str, Option<String>> =
        blocks.iter().map(|b| (b.id.as_str(), state(b))).collect();
    blocks
        .iter()
        .filter(|b| state(b).as_deref() == Some("ready-for-agent"))
        .find(|b| {
            queue::blockers(b)
                .iter()
                .all(|id| states.get(id.as_str()).and_then(|s| s.as_deref()) == Some("done"))
        })
        .map(|b| b.id.clone())
}

fn number(id: &str) -> u64 {
    id.strip_prefix("T-")
        .and_then(|d| d.parse().ok())
        .unwrap_or(u64::MAX)
}

fn slug(title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .map(str::to_ascii_lowercase)
        .collect();
    let s = words.join("-");
    s.chars()
        .take(60)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

pub fn file_name(id: &str, title: &str) -> String {
    format!("{id}-{}.md", slug(title))
}

/// One ticket file read as a block: `**Status:**`, `**Blocked by:**`, the criteria list, `## Comments` and the `## Harness` trailer become the legacy fields.
pub fn parse_ticket(text: &str) -> Result<Block, String> {
    let mut lines = text.split('\n').enumerate();
    let (id, title) = lines
        .by_ref()
        .find(|(_, l)| !l.trim().is_empty())
        .and_then(|(_, l)| {
            let (id, title) = l.strip_prefix("# ")?.split_once(':')?;
            (number(id) != u64::MAX).then(|| (id.to_string(), title.trim().to_string()))
        })
        .ok_or("the first line is not `# T-NNN: title`")?;
    let mut body: Vec<(usize, String)> = Vec::new();
    // comments go last, so a reader slicing `notes:` to the block's end stops at the ticket's end
    let mut comments: Vec<(usize, String)> = Vec::new();
    let mut section = "";
    let mut criteria = false;
    let mut notes = false;
    let mut fresh = true;
    for (i, line) in lines {
        let at = i + 1;
        if line.starts_with("## ") {
            for v in [&mut body, &mut comments] {
                while v.last().is_some_and(|(_, l)| l.trim().is_empty()) {
                    v.pop();
                }
            }
            fresh = true;
            section = match line.trim_end() {
                CRITERIA => CRITERIA,
                COMMENTS => COMMENTS,
                HARNESS => HARNESS,
                _ => "",
            };
            continue;
        }
        if fresh && line.trim().is_empty() {
            continue;
        }
        fresh = false;
        match section {
            "" => {
                if let Some(v) = line.strip_prefix(STATUS) {
                    body.push((at, format!("status: {}", v.trim())));
                } else if let Some(v) = line.strip_prefix(BLOCKED_BY) {
                    let ids: Vec<&str> = v
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .filter(|w| number(w) != u64::MAX)
                        .collect();
                    body.push((at, format!("blockedBy: {}", ids.join(", "))));
                }
            }
            CRITERIA => {
                let item = line
                    .strip_prefix("- [ ] ")
                    .or_else(|| line.strip_prefix("- [x] "));
                match item {
                    Some(item) => {
                        if !criteria {
                            body.push((at, "criteria:".to_string()));
                            criteria = true;
                        }
                        body.push((at, format!("  - {item}")));
                    }
                    None if criteria && !line.trim().is_empty() => {
                        body.push((at, line.to_string()))
                    }
                    None => {}
                }
            }
            COMMENTS => {
                if !notes {
                    if !line.trim().is_empty() {
                        comments.push((at, format!("notes: {line}")));
                        notes = true;
                    }
                } else {
                    comments.push((at, line.to_string()));
                }
            }
            _ => body.push((at, line.to_string())),
        }
    }
    body.extend(comments);
    while body.last().is_some_and(|(_, l)| l.trim().is_empty()) {
        body.pop();
    }
    Ok(Block {
        id,
        title,
        line: 1,
        body,
    })
}

fn field_start(line: &str) -> Option<(&str, &str)> {
    let (key, rest) = line.split_once(':')?;
    let ok = !key.is_empty()
        && key.chars().next()?.is_ascii_alphabetic()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then_some((key, rest))
}

/// A legacy block as the ticket file it becomes. Every line the gates read survives: the fields with a Pocock home move there, everything else stays verbatim in `## Harness`.
pub fn ticket_text(b: &Block) -> String {
    let mut fields: Vec<(String, Vec<String>)> = Vec::new();
    for (_, line) in &b.body {
        match field_start(line) {
            Some((key, rest)) => fields.push((key.to_string(), vec![rest.trim().to_string()])),
            None => match fields.last_mut() {
                Some((_, v)) => v.push(line.clone()),
                None => fields.push((String::new(), vec![line.clone()])),
            },
        }
    }
    let first = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let raw = first("status").map(|v| v[0].clone()).unwrap_or_default();
    let (name, reason) = raw.split_once(char::is_whitespace).unwrap_or((&raw, ""));
    let mut status = canonical(name).to_string();
    // attended survives as a field unless the state now carries it
    let attended = status == "ready-for-agent" && first("attended").is_some_and(|v| v[0] == "true");
    if attended {
        status = "ready-for-human".to_string();
    }
    let status = format!("{status} {}", reason.trim()).trim().to_string();
    let blockers = queue::blockers(b);
    let mut out = vec![
        format!("# {}: {}", b.id, b.title),
        String::new(),
        format!("{STATUS} {status}"),
        format!(
            "{BLOCKED_BY} {}",
            if blockers.is_empty() {
                "None".to_string()
            } else {
                blockers.join(", ")
            }
        ),
    ];
    let trim = |v: &mut Vec<String>| {
        while v
            .last()
            .is_some_and(|l| l.trim().is_empty() || l.trim() == "---")
        {
            v.pop();
        }
    };
    if let Some(mut v) = first("criteria") {
        trim(&mut v);
        out.extend([String::new(), CRITERIA.to_string(), String::new()]);
        if !v[0].is_empty() {
            out.push(format!("- [ ] {}", v[0]));
        }
        for line in &v[1..] {
            match line.trim_start().strip_prefix("- ") {
                Some(item) if line.starts_with("  - ") || line.starts_with("- ") => {
                    out.push(format!("- [ ] {item}"))
                }
                _ => out.push(line.clone()),
            }
        }
    }
    if let Some(mut v) = first("notes") {
        trim(&mut v);
        out.extend([String::new(), COMMENTS.to_string(), String::new()]);
        out.extend(v);
    }
    let moved = ["status", "blockedBy", "criteria", "notes"];
    let mut rest: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for (key, v) in &fields {
        let k = key.as_str();
        if (moved.contains(&k) && !seen.contains(&k)) || (k == "attended" && attended) {
            seen.push(k);
            continue;
        }
        match k {
            "" => rest.extend(v.iter().cloned()),
            _ => {
                rest.push(format!("{k}: {}", v[0]).trim_end().to_string());
                rest.extend(v[1..].iter().cloned());
            }
        }
    }
    trim(&mut rest);
    while rest.first().is_some_and(|l| l.trim().is_empty()) {
        rest.remove(0);
    }
    if !rest.is_empty() {
        out.extend([String::new(), HARNESS.to_string(), String::new()]);
        out.extend(rest);
    }
    out.push(String::new());
    out.join("\n")
}

/// Every block of a legacy queue as `(file name, ticket text)`.
pub fn convert(text: &str) -> Result<Vec<(String, String)>, QueueError> {
    Ok(queue::parse(text)?
        .iter()
        .map(|b| (file_name(&b.id, &b.title), ticket_text(b)))
        .collect())
}

/// The spec a migrated queue lands in: the product repository's directory name.
pub fn repo_slug(root: &Path) -> String {
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    match slug(name) {
        s if s.is_empty() => "repo".to_string(),
        s => s,
    }
}

/// Writes a legacy queue's blocks as ticket files under `specs/<spec>/issues/`.
pub fn write_converted(specs: &Path, spec: &str, text: &str) -> Result<usize, QueueError> {
    let dir = specs.join(spec).join("issues");
    std::fs::create_dir_all(&dir)?;
    let tickets = convert(text)?;
    for (name, ticket) in &tickets {
        std::fs::write(dir.join(name), ticket)?;
    }
    Ok(tickets.len())
}

fn ticket_files(specs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir(specs) else {
        return out;
    };
    for dir in dirs.flatten() {
        if let Ok(files) = std::fs::read_dir(dir.path().join("issues")) {
            out.extend(
                files
                    .flatten()
                    .map(|f| f.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "md")),
            );
        }
    }
    out
}

fn bad(path: &Path, why: &str) -> QueueError {
    QueueError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{}: {why}", path.display()),
    ))
}

// the duplicate check reads ids, not paths: two files can carry one id after a hand copy
fn sorted(mut found: Vec<(Block, String)>) -> Result<Vec<Block>, QueueError> {
    found.sort_by_key(|(b, _)| number(&b.id));
    for pair in found.windows(2) {
        if pair[0].0.id == pair[1].0.id {
            return Err(QueueError::DuplicateTicket {
                id: pair[0].0.id.clone(),
                first: pair[0].1.clone(),
                second: pair[1].1.clone(),
            });
        }
    }
    Ok(found.into_iter().map(|(b, _)| b).collect())
}

impl Tracker {
    pub fn open(root: &Path, harness_dir: &str) -> Tracker {
        let specs = crate::config::instance_path(root, harness_dir, "specs");
        if specs.is_dir() {
            Tracker::Files(specs)
        } else {
            Tracker::Legacy(crate::config::instance_path(root, harness_dir, QUEUE[0]))
        }
    }

    // legacy order is file order; the files layout has none, so it is id order, which matches a queue that only appends
    pub fn blocks(&self) -> Result<Vec<Block>, QueueError> {
        match self {
            Tracker::Legacy(path) => queue::parse(&queue::Queue { path: path.clone() }.read()?),
            Tracker::Files(specs) => {
                let mut found = Vec::new();
                for path in ticket_files(specs) {
                    let text = std::fs::read_to_string(&path)?;
                    let b = parse_ticket(&text).map_err(|e| bad(&path, &e))?;
                    found.push((b, path.display().to_string()));
                }
                sorted(found)
            }
        }
    }

    pub fn get(&self, id: &str) -> Result<Option<Block>, QueueError> {
        Ok(self.blocks()?.into_iter().find(|b| b.id == id))
    }

    pub fn path_of(&self, id: &str) -> Option<PathBuf> {
        match self {
            Tracker::Legacy(path) => Some(path.clone()),
            Tracker::Files(specs) => ticket_files(specs).into_iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&format!("{id}-")))
            }),
        }
    }

    pub fn set_status(&self, id: &str, status: &str, reason: &str) -> Result<(), QueueError> {
        match self {
            Tracker::Legacy(path) => {
                let q = queue::Queue { path: path.clone() };
                q.write(&queue::set_status(
                    &q.read()?,
                    id,
                    legacy_name(status),
                    reason,
                )?)
            }
            Tracker::Files(_) => {
                let path = self
                    .path_of(id)
                    .ok_or_else(|| QueueError::NoSuchTask(id.to_string()))?;
                let text = std::fs::read_to_string(&path)?;
                let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
                let at = lines
                    .iter()
                    .position(|l| l.starts_with(STATUS))
                    .ok_or_else(|| bad(&path, "no **Status:** line"))?;
                lines[at] = format!("{STATUS} {}", canonical(status));
                // the newest gate goes first, as the legacy insert under `status:` put it, because field() reads the first
                if !reason.is_empty() {
                    let gate = format!("gate: {reason}");
                    match lines.iter().position(|l| l.trim_end() == HARNESS) {
                        Some(h) => {
                            let blank = lines.get(h + 1).is_some_and(|l| l.is_empty());
                            lines.insert(h + 1 + usize::from(blank), gate)
                        }
                        None => {
                            while lines.last().is_some_and(|l| l.is_empty()) {
                                lines.pop();
                            }
                            lines.extend([String::new(), HARNESS.to_string(), String::new(), gate]);
                            lines.push(String::new());
                        }
                    }
                }
                std::fs::write(&path, lines.join("\n"))?;
                Ok(())
            }
        }
    }

    /// Appends a legacy-form block, under `spec` in the files layout, and returns the path written.
    pub fn append(&self, spec: &str, block: &str) -> Result<PathBuf, QueueError> {
        match self {
            Tracker::Legacy(path) => {
                let q = queue::Queue { path: path.clone() };
                let text = q.read()?;
                let sep = if text.is_empty() || text.ends_with("\n\n") {
                    ""
                } else if text.ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                q.write(&format!("{text}{sep}{}\n", block.trim_end()))?;
                Ok(path.clone())
            }
            Tracker::Files(specs) => {
                let b = queue::parse(block)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| bad(specs, "the appended text holds no block"))?;
                if self.get(&b.id)?.is_some() {
                    return Err(QueueError::DuplicateTicket {
                        id: b.id.clone(),
                        first: self
                            .path_of(&b.id)
                            .unwrap_or_default()
                            .display()
                            .to_string(),
                        second: "the appended block".to_string(),
                    });
                }
                let dir = specs.join(spec).join("issues");
                std::fs::create_dir_all(&dir)?;
                let path = dir.join(file_name(&b.id, &b.title));
                std::fs::write(&path, ticket_text(&b))?;
                Ok(path)
            }
        }
    }

    /// Releases every legacy `blocked` block whose blockers are done; the files layout derives blocking from `Blocked by:`, so there is nothing to release.
    pub fn unblock(&self) -> Result<(), QueueError> {
        match self {
            Tracker::Legacy(path) => {
                let q = queue::Queue { path: path.clone() };
                let text = q.read()?;
                let out = queue::unblock(&text)?;
                if out != text {
                    q.write(&out)?;
                }
                Ok(())
            }
            Tracker::Files(_) => Ok(()),
        }
    }

    pub fn next_id(&self, archived: &[Block]) -> Result<String, QueueError> {
        Ok(queue::next_id(&self.blocks()?, archived))
    }
}

/// Every ticket as the instance repository held it at `rev`; a revision from before the migration reads TASKS.md.
pub fn blocks_at(root: &Path, harness_dir: &str, rev: &str) -> Option<Vec<Block>> {
    read_at(root, harness_dir, rev, None)
}

/// One ticket at `rev`, reading only its own file.
pub fn get_at(root: &Path, harness_dir: &str, id: &str, rev: &str) -> Option<Block> {
    read_at(root, harness_dir, rev, Some(id))?
        .into_iter()
        .find(|b| b.id == id)
}

fn read_at(root: &Path, harness_dir: &str, rev: &str, id: Option<&str>) -> Option<Vec<Block>> {
    let specs = crate::config::instance_rel(root, harness_dir, "specs");
    let (repo, inner, _) = git::locate(root, harness_dir, &specs);
    let names = git::git(&repo, &["ls-tree", "-r", "--name-only", rev, "--", &inner]).ok()?;
    let files: Vec<&str> = names
        .lines()
        .filter(|n| n.contains("/issues/") && n.ends_with(".md"))
        .collect();
    if files.is_empty() {
        let tasks = crate::config::instance_rel(root, harness_dir, QUEUE[0]);
        let (repo, inner, _) = git::locate(root, harness_dir, &tasks);
        let text = git::git(&repo, &["show", &format!("{rev}:{inner}")]).ok()?;
        return queue::parse(&text).ok();
    }
    let mine = |n: &&str| id.is_none_or(|id| n.contains(&format!("/issues/{id}-")));
    // ponytail: one `git show` per ticket; a `cat-file --batch` read if a 1000-ticket queue makes gates slow
    let mut found = Vec::new();
    for name in files.into_iter().filter(mine) {
        let text = git::git(&repo, &["show", &format!("{rev}:{name}")]).ok()?;
        found.push((parse_ticket(&text).ok()?, name.to_string()));
    }
    sorted(found).ok()
}

/// The commits in `range` that touched `id`'s ticket, oldest first; before the migration, every commit that touched TASKS.md.
pub fn ticket_revs(root: &Path, harness_dir: &str, id: &str, range: &str) -> Option<Vec<String>> {
    let specs = crate::config::instance_rel(root, harness_dir, "specs");
    let tasks = crate::config::instance_rel(root, harness_dir, QUEUE[0]);
    let (repo, specs, _) = git::locate(root, harness_dir, &specs);
    let (_, tasks, _) = git::locate(root, harness_dir, &tasks);
    let ticket = format!("{specs}/*/issues/{id}-*.md");
    let revs = git::git(
        &repo,
        &[
            "log",
            "--reverse",
            "--format=%H",
            range,
            "--",
            &ticket,
            &tasks,
        ],
    )
    .ok()?;
    Some(revs.lines().map(String::from).collect())
}

/// The commit that queued a ticket: the one that added its file, or, for a ticket the migration moved, the one that added its block to TASKS.md.
pub fn queued_commit(root: &Path, harness_dir: &str, id: &str) -> Result<Option<String>, String> {
    let specs = crate::config::instance_rel(root, harness_dir, "specs");
    let (repo, inner, _) = git::locate(root, harness_dir, &specs);
    let added = git::git(
        &repo,
        &[
            "log",
            "--diff-filter=A",
            "--format=%H %s",
            "--",
            &format!("{inner}/*/issues/{id}-*.md"),
        ],
    )
    .map_err(|e| e.to_string())?;
    match added.lines().last().and_then(|l| l.split_once(' ')) {
        Some((sha, subject)) if subject.starts_with(MIGRATE_SUBJECT) => {
            legacy_queued(root, harness_dir, id, Some(&format!("{sha}^")))
        }
        Some((sha, _)) => Ok(Some(sha.to_string())),
        None => legacy_queued(root, harness_dir, id, None),
    }
}

// None when the pickaxe finds none, which a caller may never pass to `git show` as a revision: an
// empty one reads the index instead. `until` bounds the search: past the migration TASKS.md is
// gone, and its deletion is the newest commit that changed the heading's count
fn legacy_queued(
    root: &Path,
    harness_dir: &str,
    id: &str,
    until: Option<&str>,
) -> Result<Option<String>, String> {
    let tasks = crate::config::instance_rel(root, harness_dir, QUEUE[0]);
    let (repo, inner, nested) = git::locate(root, harness_dir, &tasks);
    let heading = queue::heading(id, "").trim_end().to_string();
    // the nested layout anchors the pickaxe, because a notes: line quoting the heading is not the
    // commit that added the block; the root layout keeps the pickaxe the verifier ran before
    let anchored = format!("^{}", regex::escape(&heading));
    let mut args = match nested {
        true => vec![
            "log",
            "--reverse",
            "--format=%H",
            "--pickaxe-regex",
            "-S",
            &anchored,
        ],
        false => vec!["log", "-1", "--format=%H", "-S", &heading],
    };
    args.extend(until);
    args.extend(["--", &inner]);
    let found = git::git(&repo, &args).map_err(|e| e.to_string())?;
    Ok(found.lines().next().map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;

    fn head(r: &Repo) -> String {
        git::git(&r.root, &["rev-parse", "HEAD"]).unwrap()
    }

    const LEGACY: &str = "# TASKS\n\nprose\n\n## [T-001] first done\nscope: a.rs\nblockedBy:\nstatus: done\n\n## [T-002] second, waits on first\nscope: b.rs, c.rs\nblockedBy: T-001\nstatus: ready\nrows: none — harness\ncriteria:\n  - `cargo test` passes\n    and keeps passing\n  - the check passes\nnotes: Operator, 2026-11-06. Look at b.rs.\n  second line of notes\n\n## [T-003] needs a person\nscope: d.rs\nblockedBy: none\nstatus: ready\nattended: true\n\n## [T-004] proposed by a probe\nscope: e.rs\nstatus: proposed\nprobe: title-length\n\n## [T-005] parked\nscope: f.rs\nstatus: deferred -- out of scope\nwidened: g.rs\n";

    fn files(r: &Repo) -> Tracker {
        for (name, text) in convert(LEGACY).unwrap() {
            r.write(&format!(".enallagi/specs/enallagi/issues/{name}"), &text);
        }
        Tracker::open(&r.root, ".enallagi")
    }

    fn legacy(r: &Repo) -> Tracker {
        r.write(".enallagi/TASKS.md", LEGACY);
        Tracker::open(&r.root, ".enallagi")
    }

    #[test]
    fn takes_lowest_ready_unblocked_id() {
        for make in [legacy, files] {
            let r = Repo::new();
            let t = make(&r);
            let blocks = t.blocks().unwrap();
            assert_eq!(takeable(&blocks).as_deref(), Some("T-002"));
            t.set_status("T-001", "review", "").unwrap();
            assert_eq!(takeable(&t.blocks().unwrap()), None);
        }
    }

    #[test]
    fn legacy_state_names_read_as_pocock_names() {
        let r = Repo::new();
        let states: Vec<Option<String>> = legacy(&r).blocks().unwrap().iter().map(state).collect();
        let want = [
            "done",
            "ready-for-agent",
            "ready-for-human",
            "needs-triage",
            "wontfix",
        ];
        assert_eq!(states, want.map(|s| Some(s.to_string())));
        let r = Repo::new();
        let states: Vec<Option<String>> = files(&r).blocks().unwrap().iter().map(state).collect();
        assert_eq!(states, want.map(|s| Some(s.to_string())));
    }

    #[test]
    fn every_field_survives_conversion() {
        let old = queue::parse(LEGACY).unwrap();
        let r = Repo::new();
        let new = files(&r).blocks().unwrap();
        assert_eq!(old.len(), new.len());
        for (o, n) in old.iter().zip(&new) {
            assert_eq!((&o.id, &o.title), (&n.id, &n.title));
            for key in ["scope", "rows", "probe", "widened", "notes"] {
                assert_eq!(queue::field(o, key), queue::field(n, key), "{} {key}", o.id);
            }
            assert_eq!(queue::blockers(o), queue::blockers(n), "{}", o.id);
        }
        let second = queue::block_text(&new[1]);
        assert!(
            second.contains(
                "criteria:\n  - `cargo test` passes\n    and keeps passing\n  - the check passes"
            ),
            "{second}"
        );
        assert!(
            second.ends_with("notes: Operator, 2026-11-06. Look at b.rs.\n  second line of notes"),
            "{second}"
        );
        assert_eq!(queue::field(&new[4], "status").as_deref(), Some("wontfix"));
        assert!(queue::block_text(&new[4]).contains("status: wontfix -- out of scope"));
    }

    #[test]
    fn a_ticket_file_is_pococks_shape() {
        let text = &convert(LEGACY).unwrap()[1];
        assert_eq!(text.0, "T-002-second-waits-on-first.md");
        assert_eq!(
            text.1,
            "# T-002: second, waits on first\n\n**Status:** ready-for-agent\n**Blocked by:** T-001\n\n## Acceptance criteria\n\n- [ ] `cargo test` passes\n    and keeps passing\n- [ ] the check passes\n\n## Comments\n\nOperator, 2026-11-06. Look at b.rs.\n  second line of notes\n\n## Harness\n\nscope: b.rs, c.rs\nrows: none — harness\n"
        );
    }

    #[test]
    fn set_status_and_append_write_the_ticket_file() {
        let r = Repo::new();
        let t = files(&r);
        t.set_status("T-002", "ready", "verdict: a criterion is red")
            .unwrap();
        let b = t.get("T-002").unwrap().unwrap();
        assert_eq!(state(&b).as_deref(), Some("ready-for-agent"));
        assert_eq!(
            queue::field(&b, "gate").as_deref(),
            Some("verdict: a criterion is red")
        );
        let next = t.next_id(&[]).unwrap();
        assert_eq!(next, "T-006");
        let path = t
            .append(
                "enallagi",
                &format!("## [{next}] a new finding\nscope: h.rs\nstatus: proposed\n"),
            )
            .unwrap();
        assert!(path.ends_with("specs/enallagi/issues/T-006-a-new-finding.md"));
        assert_eq!(
            state(&t.get("T-006").unwrap().unwrap()).as_deref(),
            Some("needs-triage")
        );
        assert!(t
            .append("enallagi", "## [T-006] again\nstatus: proposed\n")
            .is_err());
    }

    #[test]
    fn a_duplicate_id_across_files_is_refused() {
        let r = Repo::new();
        let t = files(&r);
        r.write(
            ".enallagi/specs/other/issues/T-002-copy.md",
            "# T-002: copy\n\n**Status:** done\n",
        );
        assert!(matches!(
            t.blocks(),
            Err(QueueError::DuplicateTicket { .. })
        ));
    }

    #[test]
    fn queued_commit_is_the_file_add() {
        let r = Repo::new();
        let t = files(&r);
        r.commit_all("queue: the first five");
        let first = head(&r);
        t.append("enallagi", "## [T-006] later\nstatus: proposed\n")
            .unwrap();
        r.commit_all("queue: T-006");
        let second = head(&r);
        t.set_status("T-006", "ready", "").unwrap();
        r.commit_all("promote T-006");
        assert_eq!(
            queued_commit(&r.root, ".enallagi", "T-002").unwrap(),
            Some(first)
        );
        assert_eq!(
            queued_commit(&r.root, ".enallagi", "T-006").unwrap(),
            Some(second)
        );
        assert_eq!(queued_commit(&r.root, ".enallagi", "T-099").unwrap(), None);
    }

    #[test]
    fn revision_read_crosses_the_migration() {
        let r = Repo::new();
        r.write(".enallagi/TASKS.md", LEGACY);
        r.commit_all("queue: the first five");
        let before = head(&r);
        for (name, text) in convert(LEGACY).unwrap() {
            r.write(&format!(".enallagi/specs/enallagi/issues/{name}"), &text);
        }
        std::fs::remove_file(r.root.join(".enallagi/TASKS.md")).unwrap();
        r.commit_all(&format!("{MIGRATE_SUBJECT} at abcdef1"));
        let after = head(&r);
        let old = blocks_at(&r.root, ".enallagi", &before).unwrap();
        let new = blocks_at(&r.root, ".enallagi", &after).unwrap();
        assert_eq!(
            old.iter().map(state).collect::<Vec<_>>(),
            new.iter().map(state).collect::<Vec<_>>()
        );
        assert_eq!(
            queued_commit(&r.root, ".enallagi", "T-002").unwrap(),
            Some(before)
        );
    }
}
