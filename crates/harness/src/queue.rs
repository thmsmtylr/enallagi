//! TASKS.md, parsed once. A heading inside a fenced code block is documentation, not a task; an unterminated fence or duplicate id is refused, not silently resolved.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Block {
    pub id: String,
    pub title: String,
    pub line: usize,
    pub body: Vec<(usize, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error(
        "harness tasks: duplicate id {id} at lines {first} and {second} -- the queue is \
         ambiguous and nothing here will guess"
    )]
    DuplicateId {
        id: String,
        first: usize,
        second: usize,
    },
    #[error(
        "harness tasks: the code fence opened at line {opened} never closes, so every block \
         after it would be invisible. Close it."
    )]
    UnterminatedFence { opened: usize },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("harness tasks: no such task {0}")]
    NoSuchTask(String),
}

fn match_heading(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("## [")?;
    let close = rest.find(']')?;
    let id = &rest[..close];
    let digits = id.strip_prefix("T-")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let title = rest[close + 1..].trim().to_string();
    Some((id.to_string(), title))
}

fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

pub fn parse(text: &str) -> Result<Vec<Block>, QueueError> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut current: Option<usize> = None;
    let mut fenced = false;
    let mut opened = 0usize;
    let mut seen: HashMap<String, usize> = HashMap::new();

    for (index, line) in text.split('\n').enumerate() {
        if is_fence(line) {
            fenced = !fenced;
            opened = index + 1;
        }
        if !fenced {
            if let Some((id, title)) = match_heading(line) {
                if let Some(&first) = seen.get(&id) {
                    return Err(QueueError::DuplicateId {
                        id,
                        first,
                        second: index + 1,
                    });
                }
                seen.insert(id.clone(), index + 1);
                blocks.push(Block {
                    id,
                    title,
                    line: index + 1,
                    body: Vec::new(),
                });
                current = Some(blocks.len() - 1);
                continue;
            }
            if line.starts_with("## ") || line.starts_with("# ") {
                current = None;
                continue;
            }
        }
        if let Some(i) = current {
            blocks[i].body.push((index + 1, line.to_string()));
        }
    }
    if fenced {
        return Err(QueueError::UnterminatedFence { opened });
    }
    Ok(blocks)
}

// status is its first word: everything after it is the reason (`deferred -- out of scope`)
pub fn field(b: &Block, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for (_, line) in &b.body {
        if let Some(rest) = line.strip_prefix(&prefix) {
            let value = rest.trim().to_string();
            if key == "status" && !value.is_empty() {
                return value.split_whitespace().next().map(str::to_string);
            }
            return Some(value);
        }
    }
    None
}

pub fn blockers(b: &Block) -> Vec<String> {
    let raw = field(b, "blockedBy").unwrap_or_default().replace(' ', "");
    if raw.is_empty() || raw == "none" {
        return Vec::new();
    }
    raw.split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn ready_unattended(blocks: &[Block]) -> Option<String> {
    let status: HashMap<&str, Option<String>> = blocks
        .iter()
        .map(|b| (b.id.as_str(), field(b, "status")))
        .collect();
    for b in blocks {
        if field(b, "status").as_deref() != Some("ready")
            || field(b, "attended").as_deref() == Some("true")
        {
            continue;
        }
        if blockers(b)
            .iter()
            .all(|id| status.get(id.as_str()).and_then(|s| s.as_deref()) == Some("done"))
        {
            return Some(b.id.clone());
        }
    }
    None
}

pub fn ids_at(blocks: &[Block], status: &str) -> Vec<String> {
    blocks
        .iter()
        .filter(|b| field(b, "status").as_deref() == Some(status))
        .map(|b| b.id.clone())
        .collect()
}

pub fn block_text(b: &Block) -> String {
    let mut lines = vec![format!("## [{}] {}", b.id, b.title)];
    lines.extend(b.body.iter().map(|(_, l)| l.clone()));
    lines.join("\n")
}

pub fn set_status(
    text: &str,
    task: &str,
    status: &str,
    reason: &str,
) -> Result<String, QueueError> {
    let blocks = parse(text)?;
    for b in &blocks {
        if b.id != task {
            continue;
        }
        for (at, line) in &b.body {
            if !line.starts_with("status:") {
                continue;
            }
            let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
            lines[*at - 1] = format!("status: {status}");
            if !reason.is_empty() {
                lines.insert(*at, format!("gate: {reason}"));
            }
            return Ok(lines.join("\n"));
        }
    }
    Ok(text.to_string())
}

pub fn unblock(text: &str) -> Result<String, QueueError> {
    let blocks = parse(text)?;
    let status: HashMap<&str, Option<String>> = blocks
        .iter()
        .map(|b| (b.id.as_str(), field(b, "status")))
        .collect();
    let release: HashSet<&str> = blocks
        .iter()
        .filter(|b| {
            field(b, "status").as_deref() == Some("blocked")
                && !blockers(b).is_empty()
                && blockers(b)
                    .iter()
                    .all(|id| status.get(id.as_str()).and_then(|s| s.as_deref()) == Some("done"))
        })
        .map(|b| b.id.as_str())
        .collect();
    if release.is_empty() {
        return Ok(text.to_string());
    }
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    for b in &blocks {
        if !release.contains(b.id.as_str()) {
            continue;
        }
        for (at, line) in &b.body {
            if line.starts_with("status:") {
                lines[*at - 1] = "status: ready".to_string();
                break;
            }
        }
    }
    Ok(lines.join("\n"))
}

pub fn rejections(decisions: &str) -> Vec<String> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in decisions.split('\n') {
        if line.starts_with("## Rejected findings") {
            inside = true;
            continue;
        }
        if inside && match_heading(line).is_some() {
            break;
        }
        if inside && line.starts_with("- [") {
            out.push(line.to_string());
        }
    }
    out
}

// file order is queue priority: ready_unattended takes the first ready block in file order
pub fn list(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|b| {
            let status = field(b, "status")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "?".to_string());
            let head = format!("{}  {:<44.44}", b.id, b.title);
            format!("{}  \u{2192} {}", head.trim_end(), status)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub struct Queue {
    pub path: PathBuf,
}

impl Queue {
    // a missing file reads as an empty queue: a fresh repo with no TASKS.md yet is not an error
    pub fn read(&self) -> Result<String, QueueError> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(QueueError::Io(e)),
        }
    }

    pub fn write(&self, text: &str) -> Result<(), QueueError> {
        std::fs::write(&self.path, text).map_err(QueueError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: &str = "## [T-001] first\nscope: src/a.ts\nblockedBy: none\nstatus: ready\ncriteria:\n  - x\n\n## [T-002] second\nscope: src/b.ts\nblockedBy: T-001\nstatus: blocked\n\n```\n## [T-999] not a task\nstatus: ready\n```\n\n## [T-003] attended\nscope: src/c.ts\nblockedBy:\nstatus: ready\nattended: true\n";

    #[test]
    fn fenced_heading_is_not_a_task() {
        let b = parse(Q).unwrap();
        assert_eq!(
            b.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            ["T-001", "T-002", "T-003"]
        );
    }

    #[test]
    fn duplicate_id_is_refused() {
        assert!(matches!(
            parse("## [T-1] a\n## [T-1] b\n"),
            Err(QueueError::DuplicateId { .. })
        ));
    }

    #[test]
    fn unterminated_fence_is_refused() {
        assert!(matches!(
            parse("## [T-1] a\n```\n## [T-2] b\n"),
            Err(QueueError::UnterminatedFence { opened: 2 })
        ));
    }

    #[test]
    fn status_is_its_first_word() {
        let b = parse("## [T-1] a\nstatus: deferred -- later\n").unwrap();
        assert_eq!(field(&b[0], "status").as_deref(), Some("deferred"));
    }

    #[test]
    fn ready_unattended_skips_attended_and_blocked() {
        let b = parse(Q).unwrap();
        assert_eq!(ready_unattended(&b).as_deref(), Some("T-001"));
    }

    #[test]
    fn unblock_promotes_when_blockers_done() {
        let t = Q.replace("status: ready\ncriteria", "status: done\ncriteria");
        let out = unblock(&t).unwrap();
        let b = parse(&out).unwrap();
        assert_eq!(field(&b[1], "status").as_deref(), Some("ready"));
    }

    #[test]
    fn set_status_writes_reason_and_keeps_rest() {
        let out = set_status(Q, "T-001", "ready", "gate said so").unwrap();
        assert!(out.contains("status: ready\ngate: gate said so"));
        assert!(out.contains("## [T-002] second"));
    }

    #[test]
    fn list_cuts_title_at_44() {
        let b =
            parse("## [T-1] 0123456789012345678901234567890123456789012345678\nstatus: ready\n")
                .unwrap();
        let l = list(&b);
        assert!(l.starts_with("T-1  01234567890123456789012345678901234567890123  \u{2192} ready"));
    }

    #[test]
    fn heading_title_trims_both_ends() {
        let b = parse("## [T-1]   a title   \nstatus: ready\n").unwrap();
        assert_eq!(b[0].title, "a title");
    }

    #[test]
    fn list_shows_question_mark_for_empty_status_value() {
        let b = parse("## [T-1] a\nstatus:\n").unwrap();
        assert_eq!(list(&b), "T-1  a  \u{2192} ?");
    }

    #[test]
    fn rejections_reads_the_kill_lines() {
        let d = "# DECISIONS\n\n## Rejected findings\n- [2026-09-01] a claim \u{2014} refuted by `x`: y\n\n## [T-001] done\n";
        assert_eq!(
            rejections(d),
            vec!["- [2026-09-01] a claim \u{2014} refuted by `x`: y"]
        );
    }

    const FIXTURE: &str = "# TASKS\n\n## Block format\n\n```\n## [T-042] the example in the documentation, which is not a task\nblockedBy:\nstatus: ready\n```\n\n---\n\n## [T-001] a blocker that is done\nblockedBy:\nstatus: done\n\n## [T-002] ready, and its blocker is not\nblockedBy: T-009\nstatus: ready\n\n## [T-003] ready, attended, not a lane's\nblockedBy: T-001\nstatus: ready\nattended: true\n\n## [T-004] the one a lane may take\nblockedBy: T-001\nstatus: ready\nrows: none \u{2014} harness\n\n## [T-005] blocked, and every blocker is done\nblockedBy: T-001\nstatus: blocked\n\n## [T-006] blockedBy spelled none\nblockedBy: none\nstatus: review\n\n## [T-007] blocked, and its blocker is not done\nblockedBy: T-009\nstatus: blocked\n\n## [T-009] the blocker T-002 waits on\nblockedBy:\nstatus: ready\n";

    #[test]
    fn a_title_is_read_off_the_heading() {
        let blocks = parse(FIXTURE).unwrap();
        let t004 = blocks.iter().find(|b| b.id == "T-004").unwrap();
        assert_eq!(t004.title, "the one a lane may take");
    }

    #[test]
    fn a_heading_inside_a_code_fence_is_not_a_task() {
        let blocks = parse(FIXTURE).unwrap();
        assert!(!blocks.iter().any(|b| b.id == "T-042"));
    }

    #[test]
    fn every_real_block_is_parsed() {
        let blocks = parse(FIXTURE).unwrap();
        assert_eq!(blocks.len(), 8);
    }

    #[test]
    fn a_ready_task_whose_blocker_is_not_done_is_not_selected() {
        let blocks = parse(FIXTURE).unwrap();
        assert_ne!(ready_unattended(&blocks).as_deref(), Some("T-002"));
    }

    #[test]
    fn an_attended_ready_task_is_skipped() {
        let blocks = parse(FIXTURE).unwrap();
        assert_ne!(ready_unattended(&blocks).as_deref(), Some("T-003"));
    }

    #[test]
    fn the_first_takeable_task_is_selected() {
        let blocks = parse(FIXTURE).unwrap();
        assert_eq!(ready_unattended(&blocks).as_deref(), Some("T-004"));
    }

    #[test]
    fn blocked_by_none_is_no_blocker() {
        let blocks = parse(FIXTURE).unwrap();
        let t006 = blocks.iter().find(|b| b.id == "T-006").unwrap();
        assert_eq!(blockers(t006), Vec::<String>::new());
    }

    #[test]
    fn a_field_is_read_off_its_own_block() {
        let blocks = parse(FIXTURE).unwrap();
        let t004 = blocks.iter().find(|b| b.id == "T-004").unwrap();
        assert_eq!(
            field(t004, "rows").as_deref(),
            Some("none \u{2014} harness")
        );
    }

    #[test]
    fn a_missing_field_is_none() {
        let blocks = parse(FIXTURE).unwrap();
        assert_eq!(field(&blocks[0], "rows"), None);
    }

    #[test]
    fn a_blocked_task_whose_blockers_are_done_is_released() {
        let released = unblock(FIXTURE).unwrap();
        assert!(released.contains(
            "## [T-005] blocked, and every blocker is done\nblockedBy: T-001\nstatus: ready"
        ));
    }

    #[test]
    fn unblock_leaves_every_other_status_alone() {
        let released = unblock(FIXTURE).unwrap();
        assert_eq!(
            FIXTURE.matches("status:").count(),
            released.matches("status:").count()
        );
    }

    #[test]
    fn unblock_never_releases_a_task_whose_blocker_is_not_done() {
        let released = unblock(FIXTURE).unwrap();
        let blocks = parse(&released).unwrap();
        let t007 = blocks.iter().find(|b| b.id == "T-007").unwrap();
        assert_eq!(field(t007, "status").as_deref(), Some("blocked"));
    }

    #[test]
    fn set_status_rewrites_the_named_block() {
        let changed = set_status(FIXTURE, "T-004", "ready", "the gate was red").unwrap();
        assert!(changed.contains(
            "## [T-004] the one a lane may take\nblockedBy: T-001\nstatus: ready\ngate: the gate was red"
        ));
    }

    #[test]
    fn set_status_rewrites_exactly_one_status_line() {
        let changed = set_status(FIXTURE, "T-004", "ready", "the gate was red").unwrap();
        assert_eq!(changed.matches("gate: ").count(), 1);
    }

    #[test]
    fn set_status_leaves_the_file_otherwise_intact() {
        let changed = set_status(FIXTURE, "T-004", "ready", "the gate was red").unwrap();
        assert!(changed.contains("## [T-009]"));
    }

    #[test]
    fn set_status_on_an_unknown_id_changes_nothing() {
        assert_eq!(
            set_status(FIXTURE, "T-999", "done", "").unwrap(),
            FIXTURE.to_string()
        );
    }

    #[test]
    fn a_status_is_its_first_word_the_rest_is_the_reason() {
        let suffixed = FIXTURE.replace(
            "status: done",
            "status: done \u{2014} VERIFIED, the check was green",
        );
        let blocks = parse(&suffixed).unwrap();
        let t001 = blocks.iter().find(|b| b.id == "T-001").unwrap();
        assert_eq!(field(t001, "status").as_deref(), Some("done"));
    }

    #[test]
    fn a_duplicate_id_is_refused_not_resolved() {
        let text = FIXTURE.replace("## [T-002]", "## [T-001]");
        assert!(matches!(
            parse(&text),
            Err(QueueError::DuplicateId {
                first: 13,
                second: 17,
                ..
            })
        ));
    }

    #[test]
    fn an_unterminated_fence_is_refused_not_silently_swallowed() {
        let text = FIXTURE.replace("## [T-004]", "```\n## [T-004]");
        assert!(matches!(
            parse(&text),
            Err(QueueError::UnterminatedFence { opened: 26 })
        ));
    }
}
