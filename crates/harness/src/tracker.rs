//! The queue behind one interface: one file per ticket under `specs/<slug>/issues/` in Pocock's local-tracker format, handed back as `block::Block`s in the `key: value` form.

use crate::block::{self, Block, QueueError};
use crate::git;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The instance directory the queue lives under.
pub const SPECS: &str = "specs";

// `__TRACKER__`: stands in for what `/setup-matt-pocock-skills` would write into the client repo
pub const ABOUT: &str = "The tracker is local files, already set up: never run `/setup-matt-pocock-skills`. Each spec is `__ENALLAGI_DIR__/specs/<spec>/spec.md` and each of its tickets is `__ENALLAGI_DIR__/specs/<spec>/issues/T-NNN-<slug>.md`; a `T-NNN` id is global across specs and the launcher assigns it. A ticket's `**Status:**` is one of `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human` and `wontfix`, then `review` and `done`. Notes go under its `## Comments`; the fields the launcher reads go under its `## Harness`.";

const STATUS: &str = "**Status:**";
const BLOCKED_BY: &str = "**Blocked by:**";
const CRITERIA: &str = "## Acceptance criteria";
const COMMENTS: &str = "## Comments";
const HARNESS: &str = "## Harness";

/// The specs directory.
pub struct Tracker(pub PathBuf);

pub fn state(b: &Block) -> Option<String> {
    block::field(b, "status")
}

/// A ticket ready for anyone.
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
            block::blockers(b)
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

/// One ticket file read as a block: `**Status:**`, `**Blocked by:**`, the criteria list, `## Comments` and the `## Harness` trailer become `key: value` fields.
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

/// A block as the ticket file it becomes. Every line the gates read survives: the fields with a Pocock home move there, everything else stays verbatim in `## Harness`.
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
    let status = first("status").map(|v| v[0].clone()).unwrap_or_default();
    let blockers = block::blockers(b);
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
        if moved.contains(&k) && !seen.contains(&k) {
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

/// A ticket's `verdict:` line in `## Harness`.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Verified,
    Rejected(String),
    Answered,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Verdict::Verified => write!(f, "verified by verifier"),
            Verdict::Rejected(reason) => write!(f, "rejected by verifier: {reason}"),
            Verdict::Answered => write!(f, "answered by implementer"),
        }
    }
}

/// Every form a `verdict:` line takes; the role prompts name each.
pub const VERDICT_FORMS: [&str; 3] = [
    "verified by verifier",
    "rejected by verifier: <reason>",
    "answered by implementer",
];

/// A `verdict:` line, the `spec:`/`standards:` review axes and `ran:`/`exit:` pairs directly under it.
#[derive(Debug, PartialEq)]
pub struct Ruling {
    pub verdict: Verdict,
    pub spec: Option<String>,
    pub standards: Option<String>,
    pub runs: Vec<(String, i32)>,
}

/// The ticket's ruling. A verdict value outside `VERDICT_FORMS` reads as none.
pub fn verdict(b: &Block) -> Option<Ruling> {
    // `## Comments` comes last, opening at `notes:`; a verdict quoted there is prose
    let lines: Vec<&str> = b
        .body
        .iter()
        .map(|(_, l)| l.as_str())
        .take_while(|l| !l.starts_with("notes:"))
        .collect();
    let at = lines.iter().position(|l| l.starts_with("verdict:"))?;
    let value = lines[at]["verdict:".len()..].trim();
    let verdict = match value {
        "verified by verifier" => Verdict::Verified,
        "answered by implementer" => Verdict::Answered,
        _ => {
            let rest = value.strip_prefix("rejected by verifier")?;
            if !(rest.is_empty() || rest.starts_with(':')) {
                return None;
            }
            Verdict::Rejected(rest[rest.len().min(1)..].trim().to_string())
        }
    };
    let mut r = Ruling {
        verdict,
        spec: None,
        standards: None,
        runs: Vec::new(),
    };
    let mut rest = &lines[at + 1..];
    while let Some((line, tail)) = rest.split_first() {
        let axis = |key: &str| {
            line.strip_prefix(key)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(String::from)
        };
        if line.starts_with("spec:") {
            r.spec = axis("spec:");
        } else if line.starts_with("standards:") {
            r.standards = axis("standards:");
        } else {
            break;
        }
        rest = tail;
    }
    for pair in rest.chunks(2) {
        let (Some(cmd), Some(exit)) = (
            pair[0].strip_prefix("ran:"),
            pair.get(1).and_then(|l| l.strip_prefix("exit:")),
        ) else {
            break;
        };
        if let Ok(exit) = exit.trim().trim_matches('`').parse() {
            r.runs
                .push((cmd.trim().trim_matches('`').to_string(), exit));
        }
    }
    Some(r)
}

/// The spec a new ticket lands in: the product repository's directory name.
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

/// The default spec, relative to the root: `layout.spec` when set, else the first `specs/<slug>/spec.md` holding `layout.rows_heading`, else the one a new install seeds.
pub fn spec_rel(root: &Path, cfg: &crate::config::Config) -> String {
    let dir = &cfg.layout.harness_dir;
    if !cfg.layout.spec.is_empty() {
        return crate::config::instance_rel(root, dir, &cfg.layout.spec);
    }
    let seed =
        crate::config::instance_rel(root, dir, &format!("{SPECS}/{}/spec.md", repo_slug(root)));
    let found = spec_paths(root, dir);
    let heading = format!("\n{}", cfg.layout.rows_heading);
    found
        .iter()
        .find(|p| std::fs::read_to_string(p).is_ok_and(|t| t.contains(&heading)))
        .or(found.first())
        .and_then(|p| p.strip_prefix(root).ok())
        .map(|p| p.display().to_string())
        .unwrap_or(seed)
}

/// Every spec the exit criteria are read from, relative to the root: `layout.spec` first, then each other `specs/<slug>/spec.md`.
pub fn spec_files(root: &Path, cfg: &crate::config::Config) -> Vec<String> {
    let mut out = vec![cfg.layout.spec.clone()];
    for p in spec_paths(root, &cfg.layout.harness_dir) {
        let Ok(rel) = p.strip_prefix(root) else {
            continue;
        };
        let rel = rel.display().to_string();
        if !out.contains(&rel) {
            out.push(rel);
        }
    }
    out
}

fn spec_paths(root: &Path, dir: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(Tracker::open(root, dir).0)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("spec.md"))
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found
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
        Tracker(crate::config::instance_path(root, harness_dir, SPECS))
    }

    // id order, which matches a queue that only appends
    pub fn blocks(&self) -> Result<Vec<Block>, QueueError> {
        let mut found = Vec::new();
        for path in ticket_files(&self.0) {
            let text = std::fs::read_to_string(&path)?;
            let b = parse_ticket(&text).map_err(|e| bad(&path, &e))?;
            found.push((b, path.display().to_string()));
        }
        sorted(found)
    }

    pub fn get(&self, id: &str) -> Result<Option<Block>, QueueError> {
        Ok(self.blocks()?.into_iter().find(|b| b.id == id))
    }

    pub fn path_of(&self, id: &str) -> Option<PathBuf> {
        ticket_files(&self.0).into_iter().find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{id}-")))
        })
    }

    pub fn set_status(&self, id: &str, status: &str, reason: &str) -> Result<(), QueueError> {
        let path = self
            .path_of(id)
            .ok_or_else(|| QueueError::NoSuchTask(id.to_string()))?;
        let text = std::fs::read_to_string(&path)?;
        let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with(STATUS))
            .ok_or_else(|| bad(&path, "no **Status:** line"))?;
        lines[at] = format!("{STATUS} {status}");
        // the newest gate goes first, because field() reads the first
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

    /// Appends a block as a ticket file under `specs/<spec>/issues/`, and returns the path written.
    pub fn append(&self, spec: &str, text: &str) -> Result<PathBuf, QueueError> {
        let b = block::parse(text)?
            .into_iter()
            .next()
            .ok_or_else(|| bad(&self.0, "the appended text holds no block"))?;
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
        let dir = self.0.join(spec).join("issues");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(file_name(&b.id, &b.title));
        std::fs::write(&path, ticket_text(&b))?;
        Ok(path)
    }

    pub fn next_id(&self, archived: &[Block]) -> Result<String, QueueError> {
        Ok(block::next_id(&self.blocks()?, archived))
    }
}

/// Every ticket as the instance repository held it at `rev`.
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
    let specs = crate::config::instance_rel(root, harness_dir, SPECS);
    let (repo, inner, _) = git::locate(root, harness_dir, &specs);
    let names = git::git(&repo, &["ls-tree", "-r", "--name-only", rev, "--", &inner]).ok()?;
    let mine = |n: &&str| id.is_none_or(|id| n.contains(&format!("/issues/{id}-")));
    // ponytail: one `git show` per ticket; a `cat-file --batch` read if a 1000-ticket queue makes gates slow
    let mut found = Vec::new();
    for name in names
        .lines()
        .filter(|n| n.contains("/issues/") && n.ends_with(".md"))
        .filter(mine)
    {
        let text = git::git(&repo, &["show", &format!("{rev}:{name}")]).ok()?;
        found.push((parse_ticket(&text).ok()?, name.to_string()));
    }
    sorted(found).ok()
}

/// The commits in `range` that touched `id`'s ticket, oldest first.
pub fn ticket_revs(root: &Path, harness_dir: &str, id: &str, range: &str) -> Option<Vec<String>> {
    let specs = crate::config::instance_rel(root, harness_dir, SPECS);
    let (repo, specs, _) = git::locate(root, harness_dir, &specs);
    let ticket = format!("{specs}/*/issues/{id}-*.md");
    let revs = git::git(
        &repo,
        &["log", "--reverse", "--format=%H", range, "--", &ticket],
    )
    .ok()?;
    Some(revs.lines().map(String::from).collect())
}

/// The commit that queued a ticket: the one that added its file. None when none did, which a caller may never pass to `git show`: an empty revision reads the index.
pub fn queued_commit(root: &Path, harness_dir: &str, id: &str) -> Result<Option<String>, String> {
    let specs = crate::config::instance_rel(root, harness_dir, SPECS);
    let (repo, inner, _) = git::locate(root, harness_dir, &specs);
    let added = git::git(
        &repo,
        &[
            "log",
            "--diff-filter=A",
            "--format=%H",
            "--",
            &format!("{inner}/*/issues/{id}-*.md"),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(added.lines().last().map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;

    fn head(r: &Repo) -> String {
        git::git(&r.root, &["rev-parse", "HEAD"]).unwrap()
    }

    const BLOCKS: &str = "## [T-001] first done\nscope: a.rs\nblockedBy:\nstatus: done\n\n## [T-002] second, waits on first\nscope: b.rs, c.rs\nblockedBy: T-001\nstatus: ready-for-agent\nrows: none — harness\ncriteria:\n  - `cargo test` passes\n    and keeps passing\n  - the check passes\nnotes: Operator, 2026-11-06. Look at b.rs.\n  second line of notes\n\n## [T-003] needs a person\nscope: d.rs\nblockedBy: none\nstatus: ready-for-human\n\n## [T-004] filed by a probe\nscope: e.rs\nstatus: needs-triage\nprobe: title-length\n\n## [T-005] parked\nscope: f.rs\nstatus: wontfix -- out of scope\nwidened: g.rs\n";

    fn files(r: &Repo) -> Tracker {
        let t = Tracker::open(&r.root, ".enallagi");
        for b in block::parse(BLOCKS).unwrap() {
            t.append("enallagi", &block::block_text(&b)).unwrap();
        }
        t
    }

    #[test]
    fn takes_lowest_ready_unblocked_id() {
        let r = Repo::new();
        let t = files(&r);
        assert_eq!(takeable(&t.blocks().unwrap()).as_deref(), Some("T-002"));
        t.set_status("T-001", "review", "").unwrap();
        assert_eq!(takeable(&t.blocks().unwrap()), None);
    }

    #[test]
    fn fields_survive() {
        let old = block::parse(BLOCKS).unwrap();
        let r = Repo::new();
        let new = files(&r).blocks().unwrap();
        assert_eq!(old.len(), new.len());
        for (o, n) in old.iter().zip(&new) {
            assert_eq!((&o.id, &o.title), (&n.id, &n.title));
            for key in ["status", "scope", "rows", "probe", "widened", "notes"] {
                assert_eq!(block::field(o, key), block::field(n, key), "{} {key}", o.id);
            }
            assert_eq!(block::blockers(o), block::blockers(n), "{}", o.id);
        }
        let second = block::block_text(&new[1]);
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
        assert!(block::block_text(&new[4]).contains("status: wontfix -- out of scope"));
    }

    #[test]
    fn ticket_shape() {
        let b = &block::parse(BLOCKS).unwrap()[1];
        assert_eq!(file_name(&b.id, &b.title), "T-002-second-waits-on-first.md");
        assert_eq!(
            ticket_text(b),
            "# T-002: second, waits on first\n\n**Status:** ready-for-agent\n**Blocked by:** T-001\n\n## Acceptance criteria\n\n- [ ] `cargo test` passes\n    and keeps passing\n- [ ] the check passes\n\n## Comments\n\nOperator, 2026-11-06. Look at b.rs.\n  second line of notes\n\n## Harness\n\nscope: b.rs, c.rs\nrows: none — harness\n"
        );
    }

    #[test]
    fn set_status_and_append() {
        let r = Repo::new();
        let t = files(&r);
        t.set_status("T-002", "ready-for-agent", "verdict: a criterion is red")
            .unwrap();
        let b = t.get("T-002").unwrap().unwrap();
        assert_eq!(state(&b).as_deref(), Some("ready-for-agent"));
        assert_eq!(
            block::field(&b, "gate").as_deref(),
            Some("verdict: a criterion is red")
        );
        let next = t.next_id(&[]).unwrap();
        assert_eq!(next, "T-006");
        let path = t
            .append(
                "enallagi",
                &format!("## [{next}] a new finding\nscope: h.rs\nstatus: needs-triage\n"),
            )
            .unwrap();
        assert!(path.ends_with("specs/enallagi/issues/T-006-a-new-finding.md"));
        assert_eq!(
            state(&t.get("T-006").unwrap().unwrap()).as_deref(),
            Some("needs-triage")
        );
        assert!(t
            .append("enallagi", "## [T-006] again\nstatus: needs-triage\n")
            .is_err());
    }

    #[test]
    fn verdict_forms() {
        let read = |harness: &str| {
            let text = format!("# T-001: t\n\n**Status:** review\n\n## Comments\n\nThe verifier wrote:\nverdict: verified by verifier\n\n## Harness\n\n{harness}\n");
            verdict(&parse_ticket(&text).unwrap())
        };
        let bare = |verdict| Ruling {
            verdict,
            spec: None,
            standards: None,
            runs: vec![],
        };
        assert_eq!(
            read("gate: x\nverdict: verified by verifier\nstandards: none\nspec: `a` untested\nran: `cargo test`\nexit: 0\nran: true\nexit: 1\nscope: a.rs\nran: late\nexit: 0"),
            Some(Ruling {
                spec: Some("`a` untested".to_string()),
                standards: Some("none".to_string()),
                runs: vec![("cargo test".to_string(), 0), ("true".to_string(), 1)],
                ..bare(Verdict::Verified)
            })
        );
        assert_eq!(
            read("verdict: rejected by verifier: red on `a`\nran: false"),
            Some(bare(Verdict::Rejected("red on `a`".to_string())))
        );
        assert_eq!(
            read("verdict: rejected by verifier"),
            Some(bare(Verdict::Rejected(String::new())))
        );
        assert_eq!(
            read("verdict: answered by implementer\nspec:\nran: x"),
            Some(bare(Verdict::Answered))
        );
        for bad in [
            "verdict: VERIFIED",
            "verdict: rejected by verifiers: x",
            "scope: a.rs",
        ] {
            assert_eq!(read(bad), None, "{bad}");
        }
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
        t.append("enallagi", "## [T-006] later\nstatus: needs-triage\n")
            .unwrap();
        r.commit_all("queue: T-006");
        let second = head(&r);
        t.set_status("T-006", "ready-for-agent", "").unwrap();
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
}
