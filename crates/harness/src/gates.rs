//! The named checks the loop runs after a stage exits.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::events::{Kind, Writer};
use crate::git::{self, git, head, porcelain};
use crate::queue::{self, Queue};
use crate::skills::LockEntry;

pub(crate) const BOOKKEEPING: &[&str] = &[
    "TASKS.md",
    "PROGRESS.md",
    "PROGRESS.archive.md",
    "DECISIONS.md",
    "LEARNINGS.md",
];

pub struct GateCtx<'a> {
    pub root: &'a Path,
    pub cfg: &'a Config,
    pub task: Option<String>,
    pub iter_base: Option<String>,
    // the harness directory repository's base; the same as iter_base while instance files share the product's
    pub state_base: Option<String>,
    pub stage_output: String,
    pub events: &'a mut Writer,
    pub dry_run: bool,
    pub warnings: &'a mut Vec<String>,
    pub halts: &'a mut Vec<String>,
    pub dry_rounds: &'a mut u32,
}

#[derive(Debug, Clone, Default)]
pub struct GateOutcome {
    pub pass: bool,
    pub reason: String,
    pub skip_rest: bool,
    pub halt: bool,
}

fn pass(reason: impl Into<String>) -> GateOutcome {
    GateOutcome {
        pass: true,
        reason: reason.into(),
        ..GateOutcome::default()
    }
}

fn fail(reason: impl Into<String>) -> GateOutcome {
    GateOutcome {
        pass: false,
        reason: reason.into(),
        ..GateOutcome::default()
    }
}

pub fn run(name: &str, ctx: &mut GateCtx) -> GateOutcome {
    let outcome = match name {
        "implementer-not-done" => implementer_not_done(ctx),
        "commit-verdict" => commit_verdict(ctx),
        "verdict" => verdict(ctx),
        "scope" => scope(ctx),
        "queue-intact" => queue_intact(ctx),
        "check-delta" => check_gate(ctx),
        "commit-round" => commit_round(ctx),
        "adjudicator-halt" => adjudicator_halt(ctx),
        "dry-round" => dry_round(ctx),
        other => fail(format!("no such gate: {other}")),
    };
    let task = ctx.task.clone().unwrap_or_default();
    ctx.events.emit(Kind::Gate {
        gate: name.to_string(),
        task,
        pass: outcome.pass,
        reason: outcome.reason.clone(),
    });
    outcome
}

fn rel(ctx: &GateCtx, name: &str) -> String {
    crate::config::instance_rel(ctx.root, &ctx.cfg.layout.harness_dir, name)
}

// the repository an instance file's history is in, the path inside it, and that repository's base
fn at_base(ctx: &GateCtx, path: &str) -> Option<(PathBuf, String, String)> {
    let (repo, inner, nested) = git::locate(ctx.root, &ctx.cfg.layout.harness_dir, path);
    let base = if nested {
        &ctx.state_base
    } else {
        &ctx.iter_base
    };
    Some((repo, inner, base.clone().filter(|b| !b.is_empty())?))
}

fn show_at(ctx: &GateCtx, path: &str, rev: Option<&str>) -> Option<String> {
    let (repo, inner, base) = at_base(ctx, path)?;
    let rev = rev.unwrap_or(&base);
    git(&repo, &["show", &format!("{rev}:{inner}")]).ok()
}

fn diff_since_base(ctx: &GateCtx, path: &str) -> Option<String> {
    let (repo, inner, base) = at_base(ctx, path)?;
    git(&repo, &["diff", &base, "HEAD", "--", &inner]).ok()
}

fn commit_instance(ctx: &GateCtx, names: &[&str], msg: &str) -> Result<bool, git::GitError> {
    git::commit_instance(ctx.root, &ctx.cfg.layout.harness_dir, names, msg)
}

fn tasks_file(root: &Path, cfg: &Config) -> Queue {
    Queue {
        path: crate::config::instance_path(root, &cfg.layout.harness_dir, "TASKS.md"),
    }
}

fn status_of(ctx: &GateCtx, task: &str) -> Result<Option<String>, queue::QueueError> {
    let text = tasks_file(ctx.root, ctx.cfg).read()?;
    let blocks = queue::parse(&text)?;
    Ok(blocks
        .iter()
        .find(|b| b.id == task)
        .and_then(|b| queue::field(b, "status")))
}

fn field_of(ctx: &GateCtx, task: &str, key: &str) -> String {
    let Ok(text) = tasks_file(ctx.root, ctx.cfg).read() else {
        return String::new();
    };
    let Ok(blocks) = queue::parse(&text) else {
        return String::new();
    };
    blocks
        .iter()
        .find(|b| b.id == task)
        .and_then(|b| queue::field(b, key))
        .unwrap_or_default()
}

fn unreadable(ctx: &mut GateCtx, task: &str) -> GateOutcome {
    ctx.warnings.push(format!(
        "{task}: TASKS.md could not be read; the gate did not run."
    ));
    fail("unreadable")
}

fn force_back(
    ctx: &mut GateCtx,
    gate: &str,
    task: &str,
    to: &str,
    reason: &str,
    commit_msg: &str,
    warning: &str,
) -> GateOutcome {
    ctx.warnings.push(warning.to_string());
    let from = status_of(ctx, task)
        .ok()
        .flatten()
        .unwrap_or_else(|| "done".to_string());
    if !ctx.dry_run {
        let q = tasks_file(ctx.root, ctx.cfg);
        let written = q
            .read()
            .and_then(|text| queue::set_status(&text, task, to, reason))
            .and_then(|text| q.write(&text));
        match written {
            Ok(()) => {
                if let Err(err) = commit_instance(ctx, &["TASKS.md"], commit_msg) {
                    ctx.warnings
                        .push(format!("{task}: the queue was not committed: {err}"));
                }
            }
            // the queue still says done, so a task.status event here isn't a change the file carries
            Err(err) => {
                ctx.warnings
                    .push(format!("{task}: TASKS.md could not be rewritten: {err}"));
                return fail(reason);
            }
        }
    }
    ctx.events.emit(Kind::TaskStatus {
        task: task.to_string(),
        from,
        to: to.to_string(),
        reason: reason.to_string(),
        by: gate.to_string(),
    });
    fail(reason)
}

// a done written by the implementer itself (not the verifier) is forced back to ready; anything short
// of review has nothing to verify either, so the rest of the iteration is skipped in both cases
fn implementer_not_done(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => unreadable(ctx, &task),
        Ok(status) if status.as_deref() == Some("review") => {
            pass("the implementer left it at review")
        }
        Ok(status) if status.as_deref() != Some("done") => GateOutcome {
            skip_rest: true,
            ..pass(format!(
                "the implementer left it at {}",
                status.as_deref().unwrap_or("no status")
            ))
        },
        Ok(_) => {
            let mut out = force_back(
                ctx,
                "implementer-not-done",
                &task,
                "ready",
                "the implementer set done; only the verifier may, and the launcher gates that",
                &format!("chore({task}): the implementer marked its own task done"),
                &format!("{task} was forced back to ready: the implementer marked it done itself."),
            );
            out.skip_rest = true;
            out
        }
    }
}

// nothing else commits the verifier's verdict; an uncommitted rejection could otherwise sit unnoticed
fn commit_verdict(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    if let Some(phrase) = deferred_finding(ctx, &task) {
        let reason = format!(
            "the verdict's notes say \"{phrase}\" and it added no status: proposed block; write that finding as a proposed block with the command that shows it"
        );
        let mut out = force_back(
            ctx,
            "commit-verdict",
            &task,
            "review",
            &reason,
            &format!("verify: {task} verdict refused, a finding left in notes"),
            &format!("{task} went back to review: its verdict deferred a finding (\"{phrase}\") without proposing it."),
        );
        out.skip_rest = true;
        return out;
    }
    commit(ctx, &["TASKS.md"], &format!("verify: {task} verdict"))
}

// a finding left only in notes is archived with its block and never reaches the queue
fn deferred_finding(ctx: &GateCtx, task: &str) -> Option<String> {
    at_base(ctx, &rel(ctx, "TASKS.md"))?;
    let before = show_at(ctx, &rel(ctx, "TASKS.md"), None).unwrap_or_default();
    let before = queue::parse(&before).unwrap_or_default();
    let now = queue::parse(&tasks_file(ctx.root, ctx.cfg).read().ok()?).ok()?;
    let proposed_before = queue::ids_at(&before, "proposed");
    if queue::ids_at(&now, "proposed")
        .iter()
        .any(|id| !proposed_before.contains(id))
    {
        return None;
    }
    let old: Vec<&str> = before
        .iter()
        .filter(|b| b.id == task)
        .flat_map(|b| b.body.iter().map(|(_, l)| l.as_str()))
        .collect();
    let re = regex::Regex::new(
        r"(?i)\b(not a reason for rejection|not a rejection reason|minor|for later)\b",
    )
    .ok()?;
    // "no minor issues" says there is nothing to propose
    let negated =
        regex::Regex::new(r"(?i)\b(no|nothing)\s+minor\b|\bminor\s+or\s+otherwise\b").ok()?;
    now.iter()
        .filter(|b| b.id == task)
        .flat_map(|b| b.body.iter().map(|(_, l)| l.as_str()))
        .filter(|l| !old.contains(l))
        .find_map(|l| {
            re.find(&negated.replace_all(l, ""))
                .map(|m| m.as_str().to_lowercase())
        })
}

fn commit_round(ctx: &mut GateCtx) -> GateOutcome {
    let iter = ctx.events.iter;
    commit(
        ctx,
        &["TASKS.md", "DECISIONS.md"],
        &format!("queue: scout and adjudicator round (iteration {iter})"),
    )
}

fn commit(ctx: &mut GateCtx, names: &[&str], msg: &str) -> GateOutcome {
    if ctx.dry_run {
        return pass(format!("dry run: would commit {msg}"));
    }
    match commit_instance(ctx, names, msg) {
        Ok(true) => pass(format!("committed {msg}")),
        Ok(false) => pass("nothing to commit"),
        Err(err) => fail(format!("the commit failed: {err}")),
    }
}

// without this a false VERIFIED (nothing ran to check it) was indistinguishable from a real one
fn verdict(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }

    // STOP is the harness's own marker, never a lane's work, so it's excluded from the uncommitted count
    let stop = format!(" {}", rel(ctx, "STOP"));
    let left = porcelain(ctx.root)
        .into_iter()
        .filter(|l| !l.ends_with(&stop))
        .count();
    if left > 0 {
        let reason = format!(
            "the verifier returned done with {left} uncommitted path(s): the work is not on the branch"
        );
        return force_back(
            ctx,
            "verdict",
            &task,
            "ready",
            &reason,
            &format!(
                "chore({task}): enallagi gate rejected a done verdict with work off the branch"
            ),
            &format!("{task} was forced back to ready: done with {left} uncommitted path(s)."),
        );
    }

    let report = check_delta(ctx.root, ctx.cfg, false);
    if let Some(halt) = hung(ctx, &report) {
        return halt;
    }
    if report.accepts() {
        return pass("done, and the gate agrees.");
    }
    let sha = head(ctx.root).unwrap_or_default();
    let named = if report.unforgiven.is_empty() {
        "no failure could be named".to_string()
    } else {
        report.unforgiven.join(", ")
    };
    let tail3 = report.tail(3).join(" | ");
    let reason = format!(
        "the verifier returned done and the gate was red (exit {}) at {sha}. {named}; check tail: {tail3}",
        report.exit
    );
    let tail8 = report.tail(8).join("\n");
    force_back(
        ctx,
        "verdict",
        &task,
        "ready",
        &reason,
        &format!("chore({task}): enallagi gate rejected a false VERIFIED"),
        &format!(
            "{task} was forced back to ready by the gate: the verifier said done, the gate was red.\ncheck tail:\n{tail8}"
        ),
    )
}

// word boundaries, so one id never matches a longer id it prefixes
pub fn names_task(subject: &str, task: &str) -> bool {
    regex::Regex::new(&format!(r"\b{}\b", regex::escape(task))).is_ok_and(|re| re.is_match(subject))
}

// the files the task's own commits changed: a commit whose subject names no task, or another task,
// is an operator's or another round's and is not charged here. filter is a git --diff-filter value,
// empty for every change
fn commit_files(root: &Path, base: &str, task: &str, filter: &str) -> Vec<String> {
    let range = format!("{base}..HEAD");
    let log = git(
        root,
        &["log", "--reverse", "--no-merges", "--format=%h %s", &range],
    )
    .unwrap_or_default();
    let flag = format!("--diff-filter={filter}");
    let mut files: Vec<String> = Vec::new();
    for sha in log
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(_, subject)| names_task(subject, task))
        .map(|(sha, _)| sha.to_string())
    {
        let mut args = vec!["show", "--format=", "--name-only"];
        if !filter.is_empty() {
            args.push(&flag);
        }
        args.push(&sha);
        for f in git(root, &args).unwrap_or_default().lines() {
            if !f.is_empty() && !files.iter().any(|seen| seen == f) {
                files.push(f.to_string());
            }
        }
    }
    files
}

// the task's files: the product repository's, and in a nested install the harness
// directory's under its own prefix
fn task_files(ctx: &GateCtx, base: &str, task: &str, filter: &str) -> Vec<String> {
    let dir = &ctx.cfg.layout.harness_dir;
    let state = git::state_root(ctx.root, dir);
    let mut files = commit_files(ctx.root, base, task, filter);
    if let Some(state_base) = ctx.state_base.as_deref().filter(|_| state != ctx.root) {
        files.extend(
            commit_files(&state, state_base, task, filter)
                .into_iter()
                .map(|f| format!("{dir}/{f}")),
        );
    }
    files
}

// diffs the iteration's own commits against the task's scope: globs; also routes the three loops via rows: none — harness
fn scope(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }
    let base = ctx.iter_base.clone().unwrap_or_default();
    if base.is_empty() {
        return pass("no base");
    }

    let pats = scope_globs(&field_of(ctx, &task, "scope"));
    let skills_dir = skills_dir_for(ctx.cfg);
    let lock = rel(ctx, "harness.lock");
    let hashes = rel(ctx, "test-hashes.json");
    let bookkeeping: Vec<String> = BOOKKEEPING.iter().map(|name| rel(ctx, name)).collect();
    // not in BOOKKEEPING, which the launcher commits by name, but its state commit carries a STOP
    // written mid-run into whatever task the lane held
    let stop = rel(ctx, "STOP");
    let base_lock = lock_at(ctx, &lock, None);
    let head_lock = lock_at(ctx, &lock, Some("HEAD"));
    // ids the pipeline vendored fresh this iteration -- the task's scope: line never has to name them
    let added_skills = added_ids(&base_lock.skill, &head_lock.skill);
    let added_roles = added_ids(&base_lock.role, &head_lock.role);

    // a locked id re-vendored leaves harness.lock byte-identical, so added_skills never names it
    let locked_skills: Vec<String> = head_lock
        .skill
        .iter()
        .map(|e| format!("{skills_dir}/{}/", e.id))
        .collect();

    let mut out_of: Vec<String> = Vec::new();
    let mut harness_hit: Vec<String> = Vec::new();
    let changed = task_files(ctx, &base, &task, "");
    let added = task_files(ctx, &base, &task, "A");
    // cargo rewrites Cargo.lock from a manifest the range changed, and a commit without it leaves
    // the tree dirty after the next build; the manifest still has to be on the scope: line
    let manifest_in_scope = changed
        .iter()
        .any(|f| f.ends_with("Cargo.toml") && in_scope(f, &pats));
    for f in changed {
        if bookkeeping.contains(&f) || f == lock || f == stop {
            continue;
        }
        if added_skills
            .iter()
            .any(|id| f.starts_with(&format!("{skills_dir}/{id}/")))
            || added_roles.iter().any(|id| f == role_file(ctx.cfg, id))
        {
            continue;
        }
        // a file the range ADDS under a locked id is that id's re-vendoring; one it rewrites is a
        // hand edit, which the scope: line still has to name
        if added.contains(&f) && locked_skills.iter().any(|d| f.starts_with(d)) {
            continue;
        }
        if manifest_in_scope && (f == "Cargo.lock" || f.ends_with("/Cargo.lock")) {
            continue;
        }
        // test-hashes.json is exempt when every re-cut key (present at base too, with a new value)
        // is in scope; a key ADDED fresh (absent at base) needs no scope: line to cover it
        if let Some(key_re) = (f == hashes).then_some(HASHES_KEY) {
            let touched = recut_keys(ctx, &f, key_re);
            if !touched.is_empty() {
                let base_keys = keys_at(ctx, &f, key_re);
                let recut: Vec<String> = touched
                    .into_iter()
                    .filter(|k| base_keys.contains(k))
                    .collect();
                if recut.is_empty() {
                    continue;
                }
                let off: Vec<&str> = recut
                    .iter()
                    .filter(|k| !in_scope(k, &pats))
                    .map(String::as_str)
                    .collect();
                if off.is_empty() {
                    continue;
                }
                let named = format!("{f} ({})", off.join(" "));
                harness_hit.push(named.clone());
                out_of.push(named);
                continue;
            }
        }
        if is_harness_path(ctx.cfg, &f) {
            harness_hit.push(f.clone());
        }
        if !in_scope(&f, &pats) {
            out_of.push(f);
        }
    }

    // present at base and (gone, or a field differs) at HEAD: a re-cut or a removal, neither of
    // which is the pipeline's own fresh vendoring, so the id's vendored file still needs the scope: line
    let recut: Vec<(String, String)> = recut_ids(&base_lock.skill, &head_lock.skill)
        .into_iter()
        .map(|id| (format!("{skills_dir}/{id}/SKILL.md"), id))
        .chain(
            recut_ids(&base_lock.role, &head_lock.role)
                .into_iter()
                .map(|id| (role_file(ctx.cfg, &id), id)),
        )
        .collect();
    for (path, id) in recut {
        if !in_scope(&path, &pats) {
            let named = format!("{lock} ({id})");
            harness_hit.push(named.clone());
            out_of.push(named);
        }
    }

    let rows = field_of(ctx, &task, "rows");
    if rows.contains("none") && rows.contains("harness") {
        harness_hit.clear();
    }
    // the baseline only ever shrinks; a line ADDED is a red check made green by hand, whatever rows: says
    let grew = diff_since_base(ctx, &rel(ctx, ".check-baseline"))
        .map(|d| {
            d.lines()
                .any(|l| l.starts_with('+') && !l.starts_with("++") && !l.starts_with("+#"))
        })
        .unwrap_or(false);

    if out_of.is_empty() && harness_hit.is_empty() && !grew {
        return pass(format!("{task} stayed inside its scope."));
    }

    let mut parts: Vec<String> = Vec::new();
    if !out_of.is_empty() {
        parts.push(format!(
            "touched {}, which the scope line does not name",
            out_of.join(" ")
        ));
    }
    if !harness_hit.is_empty() {
        let rows = if rows.is_empty() {
            "unset"
        } else {
            rows.as_str()
        };
        parts.push(format!(
            "touched the harness ({}) with rows: {rows}, not `none — harness`",
            harness_hit.join(" ")
        ));
    }
    if grew {
        parts.push(
            "added a line to .check-baseline, and the baseline only ever shrinks".to_string(),
        );
    }
    let why = parts.join("; ");
    let reason = format!("the verifier returned done and the scope gate rejected it: {why}");
    force_back(
        ctx,
        "scope",
        &task,
        "ready",
        &reason,
        &format!("chore({task}): harness scope gate rejected a done verdict"),
        &format!("{task} was forced back to ready by the scope gate: {why}."),
    )
}

// a heading an agent deletes mid-edit leaves a block nothing counts: its body merges into its
// neighbour and archive moves both out, so the id is gone with the tree still consistent (T-020, 9c0b4af)
fn queue_intact(ctx: &mut GateCtx) -> GateOutcome {
    let Some(base) = ctx.iter_base.clone().filter(|b| !b.is_empty()) else {
        return pass("no base");
    };
    let tasks = rel(ctx, "TASKS.md");
    let before = ids_in_rev(ctx, &tasks);
    if before.is_empty() {
        return pass("no ids at base");
    }
    let Ok(text) = tasks_file(ctx.root, ctx.cfg).read() else {
        return pass("TASKS.md unreadable; queue-hygiene owns that");
    };
    let Ok(now) = queue::parse(&text) else {
        return pass("TASKS.md unparseable; queue-hygiene owns that");
    };
    let archived =
        std::fs::read_to_string(ctx.root.join(rel(ctx, "DECISIONS.md"))).unwrap_or_default();
    let lost: Vec<String> = before
        .into_iter()
        .filter(|id| !now.iter().any(|b| &b.id == id) && !archived.contains(&format!("[{id}]")))
        .collect();
    if lost.is_empty() {
        return pass("every id at the base is still in the queue or in DECISIONS.md");
    }
    let (_, inner, nested) = git::locate(ctx.root, &ctx.cfg.layout.harness_dir, &tasks);
    let show = if nested {
        format!("git -C {} show", ctx.cfg.layout.harness_dir)
    } else {
        "git show".to_string()
    };
    let base = at_base(ctx, &tasks).map(|(_, _, b)| b).unwrap_or(base);
    let reason = format!(
        "{} left TASKS.md without reaching DECISIONS.md; recover with `{show} {base}:{inner}`",
        lost.join(", "),
    );
    ctx.warnings.push(reason.clone());
    ctx.halts.push(reason.clone());
    GateOutcome {
        pass: false,
        reason,
        halt: true,
        ..GateOutcome::default()
    }
}

// queue::ids_at is ids at a status; this is ids at a revision
fn ids_in_rev(ctx: &GateCtx, tasks: &str) -> Vec<String> {
    show_at(ctx, tasks, None)
        .and_then(|text| queue::parse(&text).ok())
        .map(|blocks| blocks.into_iter().map(|b| b.id).collect())
        .unwrap_or_default()
}

fn added_ids(base: &[LockEntry], head: &[LockEntry]) -> Vec<String> {
    head.iter()
        .filter(|h| !base.iter().any(|b| b.id == h.id))
        .map(|h| h.id.clone())
        .collect()
}

fn recut_ids(base: &[LockEntry], head: &[LockEntry]) -> Vec<String> {
    base.iter()
        .filter(|b| {
            head.iter().find(|h| h.id == b.id).is_none_or(|h| {
                h.source != b.source
                    || h.rev != b.rev
                    || h.commit != b.commit
                    || h.sha256 != b.sha256
            })
        })
        .map(|b| b.id.clone())
        .collect()
}

pub(crate) fn role_file(cfg: &Config, id: &str) -> String {
    format!("{}/roles/{id}.md", cfg.layout.harness_dir)
}

fn scope_globs(line: &str) -> Vec<String> {
    line.replace(['`', ' '], "")
        .replace(',', " ")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn is_harness_path(cfg: &Config, f: &str) -> bool {
    let dir = &cfg.layout.harness_dir;
    (!dir.is_empty() && f.starts_with(&format!("{dir}/")))
        || f.contains("/hooks/")
        || matches!(
            f,
            ".check-baseline"
                | "enallagi.toml"
                | "harness.toml"
                | "harness.json"
                | "harness.lock"
                | "test-hashes.json"
        )
}

// `custom` passes validate but has no preset file, so the lookup may miss; skills_root owns that arm
pub(crate) fn skills_dir_for(cfg: &Config) -> String {
    let presets = crate::agent::presets();
    crate::init::skills_root(cfg, presets.get(&cfg.agent.preset))
        .to_string_lossy()
        .into_owned()
}

// harness.lock has its own struct and is compared with skills::parse_lock, not this text pattern
const HASHES_KEY: &str = r#"^[+-]\s*"([^"]+)"\s*:"#;

// the lock as toml::from_str reads it at a given commit; a missing file (nothing vendored yet at
// that commit) is an empty lock, not an error
fn lock_at(ctx: &GateCtx, lock: &str, rev: Option<&str>) -> crate::skills::Lock {
    show_at(ctx, lock, rev)
        .and_then(|text| crate::skills::parse_lock(&text).ok())
        .unwrap_or_default()
}

// same key regex as HASHES_KEY, applied to a revision's whole file rather than a diff, to
// tell an added key (present in HEAD, absent at base) from a re-cut of one already at base
fn keys_at(ctx: &GateCtx, file: &str, diff_key_re: &str) -> Vec<String> {
    let plain = format!("^{}", &diff_key_re[5..]);
    let (Ok(re), Some(text)) = (regex::Regex::new(&plain), show_at(ctx, file, None)) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = text
        .lines()
        .filter_map(|l| re.captures(l))
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

fn recut_keys(ctx: &GateCtx, file: &str, key_re: &str) -> Vec<String> {
    let (Ok(re), Some(diff)) = (regex::Regex::new(key_re), diff_since_base(ctx, file)) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = diff
        .lines()
        .filter_map(|l| re.captures(l))
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

pub fn in_scope(path: &str, globs: &[String]) -> bool {
    globs.iter().any(|pat| {
        !pat.is_empty()
            && globset::GlobBuilder::new(pat)
                .literal_separator(false)
                .build()
                .map(|g| g.compile_matcher().is_match(path))
                .unwrap_or(false)
    })
}

// the one sentinel `probes` also writes, so a caller can tell "never started" from "started and red"
pub const NEVER_RAN: &str = "the check could not be run:";

#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    pub red: bool,
    pub unforgiven: Vec<String>,
    pub forgiven: Vec<String>,
    pub unnamed: bool,
    pub output: String,
    pub exit: i32,
    /// The halt reason when the check ran past `[check] timeout`. Neither red nor green.
    pub timed_out: Option<String>,
}

impl CheckReport {
    pub fn accepts(&self) -> bool {
        self.timed_out.is_none() && (!self.red || (!self.unnamed && self.unforgiven.is_empty()))
    }

    // `ran` means the check STARTED and finished: a hang that reported green would be a green nobody ran
    pub fn outcome(&self) -> crate::probes::CheckOutcome {
        crate::probes::CheckOutcome {
            ran: self.timed_out.is_none() && !self.output.starts_with(NEVER_RAN),
            red: self.red,
            output: self.output.clone(),
        }
    }

    // the last n non-empty lines of output, so a rejection can show what the check actually saw
    pub fn tail(&self, n: usize) -> Vec<String> {
        let lines: Vec<&str> = self
            .output
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        lines[lines.len().saturating_sub(n)..]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
}

struct Run {
    exit: i32,
    output: String,
    timed_out: bool,
}

const POLL: Duration = Duration::from_millis(100);
// how long a SIGTERM gets to be honoured before the group is killed outright
const GRACE: Duration = Duration::from_secs(10);

// the check gets its own process group so a build tool's children die with it, not with the shell alone
fn run_bounded(root: &Path, command: &str, timeout: Duration) -> std::io::Result<Run> {
    use std::os::unix::process::CommandExt;
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let pgid = child.id();

    // stdout and stderr interleave into one buffer, and draining them keeps a chatty check off a full pipe
    let buffer = Arc::new(Mutex::new(String::new()));
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(drain(out, Arc::clone(&buffer)));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(drain(err, Arc::clone(&buffer)));
    }

    let started = Instant::now();
    let mut timed_out = false;
    let mut exited = None;
    // the shell exiting is not the end: a grandchild holding the pipe blocks the join below
    let status = loop {
        if exited.is_none() {
            exited = child.try_wait()?;
        }
        if let Some(status) = exited {
            if readers.iter().all(|r| r.is_finished()) {
                break status;
            }
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            break match exited {
                Some(status) => {
                    signal("KILL", &format!("-{pgid}"));
                    status
                }
                None => kill_group(&mut child, pgid)?,
            };
        }
        std::thread::sleep(POLL);
    };
    for reader in readers {
        let _ = reader.join();
    }
    let output = match buffer.lock() {
        Ok(text) => text.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    let exit = if timed_out {
        124
    } else {
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
    };
    Ok(Run {
        exit,
        output,
        timed_out,
    })
}

fn kill_group(child: &mut std::process::Child, pgid: u32) -> std::io::Result<ExitStatus> {
    let group = format!("-{pgid}");
    signal("TERM", &group);
    let deadline = Instant::now() + GRACE;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            break child.kill().and_then(|()| child.wait())?;
        }
        std::thread::sleep(POLL);
    };
    // the shell exiting says nothing about a grandchild it left behind
    signal("KILL", &group);
    Ok(status)
}

// a group already gone prints `No such process` on stderr, which is not the loop's output to carry
fn signal(name: &str, group: &str) {
    let _ = Command::new("kill")
        .args([&format!("-{name}"), group])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn drain<R: Read + Send + 'static>(
    pipe: R,
    buffer: Arc<Mutex<String>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut line = Vec::new();
        while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            if let Ok(mut text) = buffer.lock() {
                text.push_str(&String::from_utf8_lossy(&line));
            }
            line.clear();
        }
    })
}

pub fn check_delta(root: &Path, cfg: &Config, force: bool) -> CheckReport {
    let command = if force {
        &cfg.check.force
    } else {
        &cfg.check.command
    };
    let timeout = cfg
        .check
        .timeout_duration()
        .unwrap_or(crate::config::DEFAULT_CHECK_TIMEOUT);
    let run = match run_bounded(root, command, timeout) {
        Ok(run) => run,
        Err(err) => {
            return CheckReport {
                red: true,
                unnamed: true,
                output: format!("{NEVER_RAN} {err}"),
                exit: -1,
                ..CheckReport::default()
            };
        }
    };
    let Run {
        exit,
        output,
        timed_out,
    } = run;
    if timed_out {
        let reason = format!(
            "the check ran past {}s and was killed with its process group: `{command}`",
            timeout.as_secs()
        );
        return CheckReport {
            output: format!("{reason}\n{output}"),
            exit,
            timed_out: Some(reason),
            ..CheckReport::default()
        };
    }
    if exit == 0 {
        return CheckReport {
            output,
            exit,
            ..CheckReport::default()
        };
    }

    // a pattern that doesn't compile names nothing -- fail closed, not a crash
    let mut names: Vec<String> = match regex::Regex::new(&cfg.check.fail_name) {
        Ok(re) => output
            .lines()
            .filter_map(|l| re.captures(l))
            .filter_map(|c| c.get(1).map(|m| strip_duration(m.as_str())))
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names.dedup();

    let baseline = baseline(&crate::config::instance_path(
        root,
        &cfg.layout.harness_dir,
        ".check-baseline",
    ));
    let (forgiven, unforgiven): (Vec<String>, Vec<String>) =
        names.iter().cloned().partition(|n| baseline.contains(n));
    CheckReport {
        red: true,
        unnamed: names.is_empty(),
        unforgiven,
        forgiven,
        output,
        exit,
        timed_out: None,
    }
}

// strips a trailing " [<digits>(.<digits>)?(s|m)]" — a test-runner-printed duration, not part of the test's name
fn strip_duration(name: &str) -> String {
    let trimmed = name.trim_end();
    let Some(open) = trimmed.rfind(" [") else {
        return trimmed.to_string();
    };
    let Some(inner) = trimmed[open + 2..].strip_suffix(']') else {
        return trimmed.to_string();
    };
    let inner = inner.strip_suffix('s').unwrap_or(inner);
    let inner = inner.strip_suffix('m').unwrap_or(inner);
    if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit() || c == '.') {
        trimmed[..open].to_string()
    } else {
        trimmed.to_string()
    }
}

fn baseline(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

// a hang is not a failing test: a red would send the task back to ready and buy the same hang again
fn hung(ctx: &mut GateCtx, report: &CheckReport) -> Option<GateOutcome> {
    let reason = report.timed_out.clone()?;
    ctx.halts.push(reason.clone());
    ctx.events.emit(Kind::Halt {
        halt: ctx.task.clone().unwrap_or_else(|| "check".to_string()),
        reason: reason.clone(),
    });
    Some(GateOutcome {
        pass: false,
        reason,
        skip_rest: true,
        halt: true,
    })
}

fn check_gate(ctx: &mut GateCtx) -> GateOutcome {
    let report = check_delta(ctx.root, ctx.cfg, false);
    if let Some(halt) = hung(ctx, &report) {
        return halt;
    }
    if report.accepts() {
        return pass(if report.red {
            format!(
                "check RED, forgiven by .check-baseline: {}",
                report.forgiven.join(", ")
            )
        } else {
            "check green".to_string()
        });
    }
    let tail3 = report.tail(3).join(" | ");
    if report.unnamed {
        return fail(format!(
            "check RED, and no failure could be named — nothing to forgive; check tail: {tail3}"
        ));
    }
    fail(format!(
        "check RED, not on the baseline: {}; check tail: {tail3}",
        report.unforgiven.join(", ")
    ))
}

// a fix needing the spec or contract is neither a promotion nor a kill; the adjudicator halts and the run stops
fn adjudicator_halt(ctx: &mut GateCtx) -> GateOutcome {
    let Some(id) = halt_id(&ctx.stage_output) else {
        return pass("the adjudicator did not halt");
    };
    let reason = format!(
        "the adjudicator halted on {id}: a fix that needs {} or {} is a human's call.",
        ctx.cfg.layout.spec, ctx.cfg.layout.context_file
    );
    ctx.halts.push(reason.clone());
    ctx.events.emit(Kind::Halt {
        halt: id,
        reason: reason.clone(),
    });
    GateOutcome {
        pass: false,
        reason,
        skip_rest: true,
        halt: true,
    }
}

fn halt_id(output: &str) -> Option<String> {
    let starts_halt = regex::Regex::new(r"(?i)^\s*halt").ok()?;
    let task = regex::Regex::new(r"T-[0-9]+").ok()?;
    output
        .lines()
        .filter(|l| starts_halt.is_match(l))
        .find_map(|l| task.find(l).map(|m| m.as_str().to_string()))
}

// an empty queue alone isn't exhaustion; pipeline's end_after_dry_rounds decides how many in a row is
fn dry_round(ctx: &mut GateCtx) -> GateOutcome {
    match takeable(ctx.root, ctx.cfg) {
        Some(task) => {
            *ctx.dry_rounds = 0;
            pass(format!(
                "the round left takeable work ({task}). Dry counter reset to 0."
            ))
        }
        None => {
            *ctx.dry_rounds += 1;
            fail(format!(
                "dry round {}: nothing a lane can take.",
                ctx.dry_rounds
            ))
        }
    }
}

pub fn takeable(root: &Path, cfg: &Config) -> Option<String> {
    let text = tasks_file(root, cfg).read().ok()?;
    queue::ready_unattended(&queue::parse(&text).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, Log};
    use crate::fixture::Repo;

    struct Env {
        repo: Repo,
        cfg: Config,
        writer: Writer,
        stage_output: String,
        warnings: Vec<String>,
        halts: Vec<String>,
        dry_rounds: u32,
    }

    impl Env {
        fn new(check_body: &str) -> Env {
            Env::timed(check_body, "30m")
        }

        fn timed(check_body: &str, timeout: &str) -> Env {
            let repo = Repo::new();
            repo.write(".enallagi/.gitignore", "events.jsonl\n*.log\nlogs/\n");
            let cmd = repo.stub_check(check_body);
            repo.write(
                "enallagi.toml",
                &format!(
                    "[check]\ncommand = \"{cmd}\"\ntimeout = \"{timeout}\"\nfail_name = '\\(fail\\) (.+)$'\n"
                ),
            );
            repo.commit_all("harness");
            let cfg = crate::config::load(&repo.root).expect("load enallagi.toml");
            let writer = Writer::new(Log::open(&repo.root.join(".enallagi")));
            Env {
                repo,
                cfg,
                writer,
                stage_output: String::new(),
                warnings: Vec::new(),
                halts: Vec::new(),
                dry_rounds: 0,
            }
        }

        fn ctx(&mut self, task: Option<&str>, base: Option<&str>) -> GateCtx<'_> {
            GateCtx {
                root: &self.repo.root,
                cfg: &self.cfg,
                task: task.map(str::to_string),
                iter_base: base.map(str::to_string),
                state_base: base.map(str::to_string),
                stage_output: self.stage_output.clone(),
                events: &mut self.writer,
                dry_run: false,
                warnings: &mut self.warnings,
                halts: &mut self.halts,
                dry_rounds: &mut self.dry_rounds,
            }
        }

        fn queue(&self, status: &str, scope: &str, rows: &str) {
            self.repo.write(
                "TASKS.md",
                &format!(
                    "## [T-001] first\nscope: {scope}\nrows: {rows}\nblockedBy: none\nstatus: {status}\n"
                ),
            );
        }

        fn tasks_text(&self) -> String {
            std::fs::read_to_string(crate::config::instance_path(
                &self.repo.root,
                &self.cfg.layout.harness_dir,
                "TASKS.md",
            ))
            .expect("read TASKS.md")
        }

        fn log(&self) -> String {
            git(&self.repo.root, &["log", "--oneline"]).expect("git log")
        }

        fn head(&self) -> String {
            head(&self.repo.root).expect("head")
        }

        fn events(&self) -> Vec<Event> {
            self.writer.log.read().expect("read events")
        }

        fn assert_rejection_events(&self) {
            let events = self.events();
            assert!(events.iter().any(|e| matches!(&e.kind,
                Kind::Gate { gate, pass, .. } if gate == "scope" && !*pass)));
            assert!(events.iter().any(|e| matches!(&e.kind,
                Kind::TaskStatus { to, by, .. } if to == "ready" && by == "scope")));
        }
    }

    fn globs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn in_scope_matches_like_case_globs() {
        assert!(in_scope("src/a.ts", &globs(&["src/*.ts", "docs/*"])));
        assert!(!in_scope("docs/x.md", &globs(&["src/*.ts"])));
        assert!(in_scope("src/deep/a.ts", &globs(&["src/**"])));
        assert!(in_scope("src/deep/a.ts", &globs(&["src/*"])));
        assert!(!in_scope("src/a.ts", &[]));
    }

    #[test]
    fn red_not_on_baseline_is_a_rejection() {
        let env = Env::new("echo '(fail) alpha'\nexit 1\n");
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert!(r.red);
        assert_eq!(r.unforgiven, vec!["alpha".to_string()]);
        assert!(!r.accepts());
    }

    #[test]
    fn red_on_baseline_is_forgiven() {
        let env = Env::new("echo '(fail) alpha'\nexit 1\n");
        env.repo
            .write(".enallagi/.check-baseline", "# inherited\nalpha\n");
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert!(r.red);
        assert!(r.unforgiven.is_empty());
        assert_eq!(r.forgiven, vec!["alpha".to_string()]);
        assert!(r.accepts());
    }

    #[test]
    fn one_forgiven_one_new_is_still_red() {
        let env = Env::new("echo '(fail) alpha'\necho '(fail) beta [12ms]'\nexit 1\n");
        env.repo.write(".enallagi/.check-baseline", "alpha\n");
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert_eq!(r.unforgiven, vec!["beta".to_string()]);
        assert_eq!(r.forgiven, vec!["alpha".to_string()]);
        assert!(!r.accepts());
    }

    #[test]
    fn red_it_cannot_name_fails_closed() {
        let env = Env::new("echo boom\nexit 1\n");
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert!(r.red && r.unnamed);
        assert!(r.unforgiven.is_empty());
        assert!(!r.accepts());
    }

    #[test]
    fn green_is_green() {
        let env = Env::new("exit 0\n");
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert!(!r.red && r.accepts());
    }

    #[test]
    fn a_check_past_its_timeout_is_neither_red_nor_green() {
        let mut env = Env::timed("sleep 10\n", "2s");
        let started = Instant::now();
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the gate waited"
        );
        assert!(r.timed_out.is_some(), "{r:?}");
        assert!(!r.red && !r.accepts(), "{r:?}");

        let out = run("check-delta", &mut env.ctx(Some("T-001"), None));
        assert!(out.halt && !out.pass, "{out:?}");
        assert!(
            out.reason.contains("2s") && out.reason.contains("fakecheck"),
            "{}",
            out.reason
        );
        assert!(
            env.events()
                .iter()
                .any(|e| matches!(&e.kind, Kind::Halt { reason, .. } if reason == &out.reason)),
            "no halt event"
        );
        assert_eq!(env.halts, vec![out.reason]);
    }

    #[test]
    fn a_timed_out_check_kills_its_process_group() {
        let env = Env::timed("sleep 30 & echo $! >child.pid\nsleep 30\n", "4s");
        let started = Instant::now();
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert!(r.timed_out.is_some(), "{r:?}");
        // a grandchild still holding the pipe would hold the gate for its own 30 seconds
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the gate waited"
        );
        let pid = std::fs::read_to_string(env.repo.root.join("child.pid"))
            .expect("the check wrote its child's pid")
            .trim()
            .to_string();
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(&pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!alive(&pid), "the check's child {pid} outlived it");
    }

    #[test]
    fn a_grandchild_on_the_pipe_is_bound_by_timeout() {
        let env = Env::timed("sleep 25 & exit 0\n", "2s");
        let started = Instant::now();
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the gate waited"
        );
        let reason = r.timed_out.as_deref().unwrap_or_default();
        assert!(reason.contains("2s"), "{r:?}");
        assert!(!r.red && !r.accepts(), "{r:?}");
    }

    fn alive(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn a_done_verdict_over_a_hung_check_halts() {
        let mut env = Env::timed("sleep 10\n", "2s");
        env.queue("done", "src/a.ts", "none — harness");
        env.repo.commit_all("queue");
        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(out.halt && !out.pass, "{out:?}");
        assert!(out.reason.contains("2s"), "{}", out.reason);
        assert!(
            env.tasks_text().contains("status: done"),
            "a hang forced the verdict back"
        );
    }

    #[test]
    fn tail_skips_blank_lines_and_keeps_the_last_n() {
        let report = CheckReport {
            output: "one\n\ntwo\n\n\nthree\nfour\n".to_string(),
            ..CheckReport::default()
        };
        assert_eq!(report.tail(2), vec!["three", "four"]);
        assert_eq!(report.tail(8), vec!["one", "two", "three", "four"]);
    }

    #[test]
    fn verdict_forces_back_a_done_with_a_dirty_tree() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        env.repo.write("src/x.ts", "left behind");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        let text = env.tasks_text();
        assert!(text.contains("status: ready"), "{text}");
        assert!(
            text.contains("gate: the verifier returned done with 1 uncommitted path(s): the work is not on the branch"),
            "{text}"
        );
        assert!(env.log().contains(
            "chore(T-001): enallagi gate rejected a done verdict with work off the branch"
        ));
        assert_eq!(env.warnings.len(), 1);
        let events = env.events();
        assert!(events.iter().any(|e| matches!(&e.kind,
            Kind::TaskStatus { task, to, by, .. } if task == "T-001" && to == "ready" && by == "verdict")));
        assert!(events.iter().any(|e| matches!(&e.kind,
            Kind::Gate { gate, pass, .. } if gate == "verdict" && !*pass)));
    }

    #[test]
    fn verdict_forces_back_a_done_with_a_red_check() {
        let mut env = Env::new("echo boom\necho 'error: something'\necho done\nexit 101\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        assert!(out.reason.contains("gate was red"), "{}", out.reason);
        assert!(out.reason.contains("exit 101"), "{}", out.reason);
        assert!(out.reason.contains("check tail:"), "{}", out.reason);
        assert!(out.reason.contains("error: something"), "{}", out.reason);
        assert!(env.tasks_text().contains("status: ready"));
        assert!(env
            .log()
            .contains("chore(T-001): enallagi gate rejected a false VERIFIED"));
        assert_eq!(env.warnings.len(), 1);
        assert!(env.warnings[0].contains("boom"), "{}", env.warnings[0]);
        assert!(
            env.warnings[0].contains("error: something"),
            "{}",
            env.warnings[0]
        );
        assert!(env.warnings[0].contains("done"), "{}", env.warnings[0]);
    }

    #[test]
    fn verdict_agrees_with_a_clean_green_done() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
        assert!(env.warnings.is_empty());
    }

    #[test]
    fn no_task_is_not_a_rejection() {
        let mut env = Env::new("exit 0\n");
        assert!(run("verdict", &mut env.ctx(None, None)).pass);
        assert!(run("scope", &mut env.ctx(None, None)).pass);
    }

    #[test]
    fn a_task_id_lost_from_the_queue_halts() {
        let mut env = Env::new("exit 0\n");
        env.repo.write(
            "TASKS.md",
            "## [T-001] first\nscope: src/*\nstatus: review\n\n## [T-002] second\nscope: src/*\nstatus: ready\n",
        );
        env.repo.commit_all("queue");
        let base = env.head();
        env.repo
            .write("TASKS.md", "## [T-001] first\nscope: src/*\nstatus: done\n");
        env.repo.commit_all("heading lost");

        let out = run("queue-intact", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.halt);
        assert!(out.reason.contains("T-002"), "{}", out.reason);
        assert!(
            out.reason.contains(&format!("git show {base}:TASKS.md")),
            "{}",
            out.reason
        );
    }

    #[test]
    fn an_archived_id_is_not_a_loss() {
        let mut env = Env::new("exit 0\n");
        env.repo.write(
            "TASKS.md",
            "## [T-001] first\nscope: src/*\nstatus: review\n\n## [T-002] second\nscope: src/*\nstatus: done\n",
        );
        env.repo.commit_all("queue");
        let base = env.head();
        env.repo.write(
            "TASKS.md",
            "## [T-001] first\nscope: src/*\nstatus: review\n",
        );
        env.repo.write(
            "DECISIONS.md",
            "# DECISIONS\n\n## [T-002] second\nstatus: done\n",
        );
        env.repo.commit_all("archived");

        let out = run("queue-intact", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_file_outside_the_globs() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write("src/other.ts", "stray");
        env.repo.commit_all("T-001 stray");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.reason.contains("src/other.ts"), "{}", out.reason);
        assert!(
            out.reason.contains("which the scope line does not name"),
            "{}",
            out.reason
        );
        assert!(env.tasks_text().contains("status: ready"));
        assert!(env
            .log()
            .contains("chore(T-001): harness scope gate rejected a done verdict"));
    }

    #[test]
    fn a_commit_naming_no_task_is_not_charged() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write("src/other.ts", "an operator's own work");
        env.repo.commit_all("fix: a hand edit between two rounds");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn a_commit_naming_another_task_is_not_charged() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write("src/other.ts", "another task's work");
        env.repo
            .commit_all("feat(src): T-002 another task on the branch");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn scope_allows_a_harness_edit_under_a_harness_task() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".enallagi/*", "none — harness");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".enallagi/loop.sh", "# edited\n");
        env.repo.commit_all("T-001 harness edit");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn scope_rejects_the_same_edit_under_a_product_task() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".enallagi/*", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".enallagi/loop.sh", "# edited\n");
        env.repo.commit_all("T-001 harness edit");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.reason.contains("touched the harness"), "{}", out.reason);
        assert!(env.tasks_text().contains("status: ready"));
        assert!(env
            .log()
            .contains("chore(T-001): harness scope gate rejected a done verdict"));
        env.assert_rejection_events();
    }

    #[test]
    fn scope_rejects_a_grown_baseline() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".check-baseline", "none — harness");
        env.repo.write(".check-baseline", "alpha\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".check-baseline", "alpha\nbeta\n");
        env.repo.commit_all("T-001 baseline grew");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason
                .contains("added a line to .check-baseline, and the baseline only ever shrinks"),
            "{}",
            out.reason
        );
        assert!(env
            .log()
            .contains("chore(T-001): harness scope gate rejected a done verdict"));
        env.assert_rejection_events();
    }

    #[test]
    fn scope_exempts_an_in_scope_hash_recut() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo
            .write("test-hashes.json", "{\n  \"src/a.ts\": \"aaa\"\n}\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("test-hashes.json", "{\n  \"src/a.ts\": \"bbb\"\n}\n");
        env.repo.commit_all("T-001 recut");
        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);

        let mut off = Env::new("exit 0\n");
        off.queue("done", "src/a.ts", "§11 row 1");
        off.repo
            .write("test-hashes.json", "{\n  \"src/b.ts\": \"aaa\"\n}\n");
        off.repo.commit_all("verdict");
        let base = off.head();
        off.repo
            .write("test-hashes.json", "{\n  \"src/b.ts\": \"bbb\"\n}\n");
        off.repo.commit_all("T-001 recut");
        let out = run("scope", &mut off.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.reason.contains("test-hashes.json"), "{}", out.reason);
    }

    #[test]
    fn scope_exempts_a_lock_beside_an_in_scope_manifest() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "crates/**", "§11 row 1");
        env.repo
            .write("crates/a/Cargo.toml", "[package]\nname = \"old\"\n");
        env.repo
            .write("Cargo.lock", "[[package]]\nname = \"old\"\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("crates/a/Cargo.toml", "[package]\nname = \"new\"\n");
        env.repo
            .write("Cargo.lock", "[[package]]\nname = \"new\"\n");
        env.repo.commit_all("T-001 rename the package");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_lock_with_no_manifest_change() {
        let mut alone = Env::new("exit 0\n");
        alone.queue("done", "crates/**", "§11 row 1");
        alone
            .repo
            .write("Cargo.lock", "[[package]]\nname = \"old\"\n");
        alone.repo.commit_all("verdict");
        let base = alone.head();
        alone
            .repo
            .write("Cargo.lock", "[[package]]\nname = \"new\"\n");
        alone.repo.commit_all("T-001 lock alone");
        let out = run("scope", &mut alone.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.reason.contains("Cargo.lock"), "{}", out.reason);

        let mut off = Env::new("exit 0\n");
        off.queue("done", "crates/**", "§11 row 1");
        off.repo
            .write("vendor/b/Cargo.toml", "[package]\nname = \"old\"\n");
        off.repo
            .write("Cargo.lock", "[[package]]\nname = \"old\"\n");
        off.repo.commit_all("verdict");
        let base = off.head();
        off.repo
            .write("vendor/b/Cargo.toml", "[package]\nname = \"new\"\n");
        off.repo
            .write("Cargo.lock", "[[package]]\nname = \"new\"\n");
        off.repo.commit_all("T-001 rename an off-scope package");
        let out = run("scope", &mut off.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.reason.contains("Cargo.lock"), "{}", out.reason);
    }

    #[test]
    fn implementer_not_done_forces_back_and_skips_rest() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("implement");

        let out = run("implementer-not-done", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass && out.skip_rest);
        let text = env.tasks_text();
        assert!(text.contains("status: ready"), "{text}");
        assert!(text.contains(
            "gate: the implementer set done; only the verifier may, and the launcher gates that"
        ));
        assert!(env
            .log()
            .contains("chore(T-001): the implementer marked its own task done"));
    }

    #[test]
    fn stopping_short_of_review_skips_the_rest() {
        for status in ["blocked", "needs-spec", "deferred", "ready"] {
            let mut env = Env::new("exit 0\n");
            env.queue(status, "src/a.ts", "§11 row 1");
            env.repo.commit_all("implement");
            let out = run("implementer-not-done", &mut env.ctx(Some("T-001"), None));
            assert!(out.pass && out.skip_rest, "{status}: {out:?}");
            assert!(out.reason.contains(status), "{status}: {}", out.reason);
        }

        let mut missing = Env::new("exit 0\n");
        missing
            .repo
            .write("TASKS.md", "## [T-001] first\nscope: src/a.ts\n");
        missing.repo.commit_all("implement");
        let out = run(
            "implementer-not-done",
            &mut missing.ctx(Some("T-001"), None),
        );
        assert!(out.pass && out.skip_rest, "{out:?}");
        assert!(out.reason.contains("no status"), "{}", out.reason);

        let mut review = Env::new("exit 0\n");
        review.queue("review", "src/a.ts", "§11 row 1");
        review.repo.commit_all("implement");
        let out = run("implementer-not-done", &mut review.ctx(Some("T-001"), None));
        assert!(out.pass && !out.skip_rest, "{out:?}");
    }

    #[test]
    fn adjudicator_halt_reads_the_stage_output() {
        let mut env = Env::new("exit 0\n");
        env.stage_output = "promoted T-006\nHALT T-007 needs SPEC\n".to_string();
        let out = run("adjudicator-halt", &mut env.ctx(None, None));
        assert!(out.halt && !out.pass);
        assert_eq!(env.halts.len(), 1);
        assert!(env.halts[0].contains("T-007"), "{}", env.halts[0]);
        assert!(env
            .events()
            .iter()
            .any(|e| matches!(&e.kind, Kind::Halt { halt, .. } if halt == "T-007")));
    }

    #[test]
    fn dry_round_counts_and_resets() {
        let mut env = Env::new("exit 0\n");
        env.queue("proposed", "src/a.ts", "§11 row 1");
        run("dry-round", &mut env.ctx(None, None));
        assert_eq!(env.dry_rounds, 1);

        env.queue("ready", "src/a.ts", "§11 row 1");
        assert!(run("dry-round", &mut env.ctx(None, None)).pass);
        assert_eq!(env.dry_rounds, 0);
    }

    #[test]
    fn unreadable_queue_fails_the_gate() {
        let mut env = Env::new("exit 0\n");
        env.repo.write(
            "TASKS.md",
            "## [T-001] first\nstatus: done\n\n## [T-001] again\nstatus: done\n",
        );
        env.repo.commit_all("bad queue");
        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        assert_eq!(out.reason, "unreadable");
        assert_eq!(env.warnings.len(), 1);
        assert!(env.warnings[0].contains("TASKS.md could not be read"));
    }

    #[test]
    fn dry_run_reports_without_writing() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        env.repo.write("src/x.ts", "left behind");
        let mut ctx = env.ctx(Some("T-001"), None);
        ctx.dry_run = true;
        let out = run("verdict", &mut ctx);
        assert!(!out.pass);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn halt_detection_survives_multibyte_output() {
        let mut env = Env::new("exit 0\n");
        env.stage_output = "────────\nHALT T-007 needs SPEC\n".to_string();
        let out = run("adjudicator-halt", &mut env.ctx(None, None));
        assert!(out.halt);
        assert!(env.halts[0].contains("T-007"), "{}", env.halts[0]);

        let mut quiet = Env::new("exit 0\n");
        quiet.stage_output = "────────\n".to_string();
        assert!(run("adjudicator-halt", &mut quiet.ctx(None, None)).pass);
        assert!(quiet.halts.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_queue_it_cannot_rewrite_emits_no_status_change() {
        use std::os::unix::fs::PermissionsExt;
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        env.repo.write("src/x.ts", "left behind");
        let tasks =
            crate::config::instance_path(&env.repo.root, &env.cfg.layout.harness_dir, "TASKS.md");
        std::fs::set_permissions(&tasks, std::fs::Permissions::from_mode(0o444))
            .expect("chmod TASKS.md");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        assert!(env.tasks_text().contains("status: done"));
        assert!(
            env.warnings
                .iter()
                .any(|w| w.contains("TASKS.md could not be rewritten")),
            "{:?}",
            env.warnings
        );
        assert!(
            !env.events()
                .iter()
                .any(|e| matches!(&e.kind, Kind::TaskStatus { .. })),
            "a status change the file does not carry was emitted"
        );
    }

    fn lock_toml(entries: &[(&str, &str)]) -> String {
        let mut text = "version = 1\n".to_string();
        for (id, sha) in entries {
            text.push_str(&format!(
                "\n[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\nsha256 = \"{sha}\"\n"
            ));
        }
        text
    }

    fn role_lock_toml(entries: &[(&str, &str)]) -> String {
        let mut text = "version = 1\n".to_string();
        for (id, sha) in entries {
            text.push_str(&format!(
                "\n[[role]]\nid = \"{id}\"\nsource = \"path:vendor\"\nsha256 = \"{sha}\"\n"
            ));
        }
        text
    }

    #[test]
    fn scope_exempts_a_vendored_role() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/**", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".enallagi/roles/implementer.md", "# role\n");
        env.repo
            .write("harness.lock", &role_lock_toml(&[("implementer", "aaa")]));
        env.repo.commit_all("T-001 vendor implementer");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_recut_role_outside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/**", "§11 row 1");
        env.repo
            .write("harness.lock", &role_lock_toml(&[("implementer", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("harness.lock", &role_lock_toml(&[("implementer", "bbb")]));
        env.repo.commit_all("T-001 recut implementer");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason.contains("harness.lock (implementer)"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn scope_exempts_a_new_lock_key() {
        let mut env = Env::new("exit 0\n");
        env.queue(
            "done",
            ".enallagi/adapters/claude/skills/tdd/**",
            "§11 row 1",
        );
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        // the role fetched a second skill this stage; its id is new to the lock, absent at base
        env.repo.write(
            "harness.lock",
            &lock_toml(&[("tdd", "aaa"), ("tdd-old", "aaa")]),
        );
        env.repo.commit_all("T-001 vendor tdd-old");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_sha_only_recut_outside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue(
            "done",
            ".enallagi/adapters/claude/skills/tdd/**",
            "§11 row 1",
        );
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd-old", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        // only sha256 changes -- the id line itself never appears in the diff
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd-old", "bbb")]));
        env.repo.commit_all("T-001 recut tdd-old");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason.contains("harness.lock (tdd-old)"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn scope_allows_a_sha_only_recut_inside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue(
            "done",
            ".enallagi/adapters/claude/skills/tdd/**",
            "§11 row 1",
        );
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd", "bbb")]));
        env.repo.commit_all("T-001 recut tdd");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_removed_lock_key_outside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue(
            "done",
            ".enallagi/adapters/claude/skills/tdd/**",
            "§11 row 1",
        );
        env.repo.write(
            "harness.lock",
            &lock_toml(&[("tdd", "aaa"), ("tdd-old", "aaa")]),
        );
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd", "aaa")]));
        env.repo.commit_all("T-001 drop tdd-old");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason.contains("harness.lock (tdd-old)"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn scope_exempts_a_revendored_locked_skill() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/**", "§11 row 1");
        env.repo
            .write("harness.lock", &lock_toml(&[("demo", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        // the id is already locked, so the re-vendoring leaves harness.lock byte-identical
        env.repo
            .write(".enallagi/adapters/claude/skills/demo/SKILL.md", "# demo\n");
        env.repo.commit_all("chore(vendor): T-001 demo");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_rewritten_vendored_skill() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/**", "§11 row 1");
        env.repo
            .write("harness.lock", &lock_toml(&[("demo", "aaa")]));
        env.repo
            .write(".enallagi/adapters/claude/skills/demo/SKILL.md", "# demo\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(
            ".enallagi/adapters/claude/skills/demo/SKILL.md",
            "# demo, by hand\n",
        );
        env.repo.commit_all("T-001 hand edit under the skills dir");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason
                .contains(".enallagi/adapters/claude/skills/demo/SKILL.md"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn scope_rejects_a_stray_file_in_skills() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/**", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        // never went through skills::resolve, so it names no id the lock added this iteration
        env.repo.write(
            ".enallagi/adapters/claude/skills/other/NOTES.md",
            "hand-written",
        );
        env.repo.commit_all("T-001 stray file under the skills dir");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason
                .contains(".enallagi/adapters/claude/skills/other/NOTES.md"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn skills_dir_falls_back_layout_preset_dir() {
        let mut cfg = Config::default();
        cfg.layout.harness_dir = ".enallagi".to_string();
        assert_eq!(skills_dir_for(&cfg), ".enallagi/skills");
        cfg.agent.preset = "claude".to_string();
        assert_eq!(skills_dir_for(&cfg), ".enallagi/adapters/claude/skills");
        cfg.layout.skills_dir = Some("vendor/skills".to_string());
        assert_eq!(skills_dir_for(&cfg), "vendor/skills");
    }

    #[test]
    fn force_runs_the_force_form_of_the_check() {
        let repo = Repo::new();
        let cmd = repo.stub_check("echo \"$1\" >> ran.txt\nexit 0\n");
        repo.write(
            "enallagi.toml",
            &format!(
                "[check]\ncommand = \"{cmd} plain\"\nforce = \"{cmd} forced\"\nfail_name = '\\(fail\\) (.+)$'\n"
            ),
        );
        let cfg = crate::config::load(&repo.root).expect("load enallagi.toml");
        check_delta(&repo.root, &cfg, true);
        assert_eq!(
            std::fs::read_to_string(repo.root.join("ran.txt")).expect("ran.txt"),
            "forced\n"
        );
        check_delta(&repo.root, &cfg, false);
        assert_eq!(
            std::fs::read_to_string(repo.root.join("ran.txt")).expect("ran.txt"),
            "forced\nplain\n"
        );
    }

    #[test]
    fn takeable_is_none_on_an_unreadable_queue() {
        let env = Env::new("exit 0\n");
        env.repo.write(
            "TASKS.md",
            "## [T-001] first\nstatus: ready\n\n## [T-001] again\nstatus: ready\n",
        );
        assert!(takeable(&env.repo.root, &env.cfg).is_none());
    }
}
