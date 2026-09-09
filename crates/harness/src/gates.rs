//! The named checks the loop runs after a stage exits.

use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::events::{Kind, Writer};
use crate::git::{commit_paths, diff_names, git, head, porcelain};
use crate::queue::{self, Queue};
use crate::skills::LockEntry;

const BOOKKEEPING: &[&str] = &[
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

fn tasks_file(root: &Path) -> Queue {
    Queue {
        path: root.join("TASKS.md"),
    }
}

fn status_of(root: &Path, task: &str) -> Result<Option<String>, queue::QueueError> {
    let text = tasks_file(root).read()?;
    let blocks = queue::parse(&text)?;
    Ok(blocks
        .iter()
        .find(|b| b.id == task)
        .and_then(|b| queue::field(b, "status")))
}

fn field_of(root: &Path, task: &str, key: &str) -> String {
    let Ok(text) = tasks_file(root).read() else {
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
    reason: &str,
    commit_msg: &str,
    warning: &str,
) -> GateOutcome {
    ctx.warnings.push(warning.to_string());
    if !ctx.dry_run {
        let q = tasks_file(ctx.root);
        let written = q
            .read()
            .and_then(|text| queue::set_status(&text, task, "ready", reason))
            .and_then(|text| q.write(&text));
        match written {
            Ok(()) => {
                if let Err(err) = commit_paths(ctx.root, &["TASKS.md"], commit_msg) {
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
        from: "done".to_string(),
        to: "ready".to_string(),
        reason: reason.to_string(),
        by: gate.to_string(),
    });
    fail(reason)
}

// a done written by the implementer itself (not the verifier) is forced back to ready; nothing left to verify
fn implementer_not_done(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx.root, &task) {
        Err(_) => unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => {
            pass("the implementer left it at review")
        }
        Ok(_) => {
            let mut out = force_back(
                ctx,
                "implementer-not-done",
                &task,
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
    commit(ctx, &["TASKS.md"], &format!("verify: {task} verdict"))
}

fn commit_round(ctx: &mut GateCtx) -> GateOutcome {
    let iter = ctx.events.iter;
    commit(
        ctx,
        &["TASKS.md", "DECISIONS.md"],
        &format!("queue: scout and adjudicator round (iteration {iter})"),
    )
}

fn commit(ctx: &mut GateCtx, paths: &[&str], msg: &str) -> GateOutcome {
    if ctx.dry_run {
        return pass(format!("dry run: would commit {msg}"));
    }
    match commit_paths(ctx.root, paths, msg) {
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
    match status_of(ctx.root, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }

    // STOP is the harness's own marker, never a lane's work, so it's excluded from the uncommitted count
    let left = porcelain(ctx.root)
        .into_iter()
        .filter(|l| !l.ends_with(" STOP"))
        .count();
    if left > 0 {
        let reason = format!(
            "the verifier returned done with {left} uncommitted path(s): the work is not on the branch"
        );
        return force_back(
            ctx,
            "verdict",
            &task,
            &reason,
            &format!(
                "chore({task}): harness gate rejected a done verdict with work off the branch"
            ),
            &format!("{task} was forced back to ready: done with {left} uncommitted path(s)."),
        );
    }

    let report = check_delta(ctx.root, ctx.cfg, false);
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
        &reason,
        &format!("chore({task}): harness gate rejected a false VERIFIED"),
        &format!(
            "{task} was forced back to ready by the gate: the verifier said done, the gate was red.\ncheck tail:\n{tail8}"
        ),
    )
}

// diffs the iteration's own commits against the task's scope: globs; also routes the three loops via rows: none — harness
fn scope(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx.root, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }
    let base = ctx.iter_base.clone().unwrap_or_default();
    if base.is_empty() {
        return pass("no base");
    }

    let pats = scope_globs(&field_of(ctx.root, &task, "scope"));
    let skills_dir = skills_dir_for(ctx.cfg);
    let base_lock = lock_at(ctx.root, &base);
    let head_lock = lock_at(ctx.root, "HEAD");
    // ids the pipeline vendored fresh this iteration -- the task's scope: line never has to name them
    let added_skills = added_ids(&base_lock.skill, &head_lock.skill);
    let added_roles = added_ids(&base_lock.role, &head_lock.role);

    let mut out_of: Vec<String> = Vec::new();
    let mut harness_hit: Vec<String> = Vec::new();
    for f in diff_names(ctx.root, &base) {
        if BOOKKEEPING.contains(&f.as_str()) || f == "harness.lock" {
            continue;
        }
        if added_skills
            .iter()
            .any(|id| f.starts_with(&format!("{skills_dir}/{id}/")))
            || added_roles.iter().any(|id| f == role_file(ctx.cfg, id))
        {
            continue;
        }
        // test-hashes.json is exempt when every re-cut key (present at base too, with a new value)
        // is in scope; a key ADDED fresh (absent at base) needs no scope: line to cover it
        if let Some(key_re) = recut_keys_pattern(&f) {
            let touched = recut_keys(ctx.root, &base, &f, key_re);
            if !touched.is_empty() {
                let base_keys = keys_at(ctx.root, &base, &f, key_re);
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
            let named = format!("harness.lock ({id})");
            harness_hit.push(named.clone());
            out_of.push(named);
        }
    }

    let rows = field_of(ctx.root, &task, "rows");
    if rows.contains("none") && rows.contains("harness") {
        harness_hit.clear();
    }
    // the baseline only ever shrinks; a line ADDED is a red check made green by hand, whatever rows: says
    let grew = git(ctx.root, &["diff", &base, "HEAD", "--", ".check-baseline"])
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
        &reason,
        &format!("chore({task}): harness scope gate rejected a done verdict"),
        &format!("{task} was forced back to ready by the scope gate: {why}."),
    )
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
                | "harness.toml"
                | "harness.json"
                | "harness.lock"
                | "test-hashes.json"
        )
}

// ponytail: unify with skills::skills_dir (Task 8) once it resolves the same sources.
pub(crate) fn skills_dir_for(cfg: &Config) -> String {
    if let Some(dir) = &cfg.layout.skills_dir {
        return dir.clone();
    }
    if let Some(dir) = crate::agent::presets()
        .get(&cfg.agent.preset)
        .and_then(|p| p.skills_dir.clone())
    {
        return dir;
    }
    format!("{}/skills", cfg.layout.harness_dir)
}

// harness.lock has its own struct and is compared with skills::parse_lock, not this text pattern
fn recut_keys_pattern(f: &str) -> Option<&'static str> {
    match f {
        "test-hashes.json" => Some(r#"^[+-]\s*"([^"]+)"\s*:"#),
        _ => None,
    }
}

// the lock as toml::from_str reads it at a given commit; a missing file (nothing vendored yet at
// that commit) is an empty lock, not an error
fn lock_at(root: &Path, rev: &str) -> crate::skills::Lock {
    git(root, &["show", &format!("{rev}:harness.lock")])
        .ok()
        .and_then(|text| crate::skills::parse_lock(&text).ok())
        .unwrap_or_default()
}

// same key regex as recut_keys_pattern, applied to a revision's whole file rather than a diff, to
// tell an added key (present in HEAD, absent at base) from a re-cut of one already at base
fn keys_at(root: &Path, rev: &str, file: &str, diff_key_re: &str) -> Vec<String> {
    let plain = format!("^{}", &diff_key_re[5..]);
    let (Ok(re), Ok(text)) = (
        regex::Regex::new(&plain),
        git(root, &["show", &format!("{rev}:{file}")]),
    ) else {
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

fn recut_keys(root: &Path, base: &str, file: &str, key_re: &str) -> Vec<String> {
    let (Ok(re), Ok(diff)) = (
        regex::Regex::new(key_re),
        git(root, &["diff", base, "HEAD", "--", file]),
    ) else {
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

#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    pub red: bool,
    pub unforgiven: Vec<String>,
    pub forgiven: Vec<String>,
    pub unnamed: bool,
    pub output: String,
    pub exit: i32,
}

impl CheckReport {
    pub fn accepts(&self) -> bool {
        !self.red || (!self.unnamed && self.unforgiven.is_empty())
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

pub fn check_delta(root: &Path, cfg: &Config, force: bool) -> CheckReport {
    let command = if force {
        &cfg.check.force
    } else {
        &cfg.check.command
    };
    let out = match Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(root)
        .output()
    {
        Ok(out) => out,
        Err(err) => {
            return CheckReport {
                red: true,
                unnamed: true,
                // the one sentinel `probes` also writes, so a caller can tell "never started" from "started and red"
                output: format!("the check could not be run: {err}"),
                exit: -1,
                ..CheckReport::default()
            };
        }
    };
    let exit = out.status.code().unwrap_or(-1);
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
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

    let baseline = baseline(root);
    let (forgiven, unforgiven): (Vec<String>, Vec<String>) =
        names.iter().cloned().partition(|n| baseline.contains(n));
    CheckReport {
        red: true,
        unnamed: names.is_empty(),
        unforgiven,
        forgiven,
        output,
        exit,
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

fn baseline(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join(".check-baseline"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn check_gate(ctx: &mut GateCtx) -> GateOutcome {
    let report = check_delta(ctx.root, ctx.cfg, false);
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

pub fn takeable(root: &Path, _cfg: &Config) -> Option<String> {
    let text = tasks_file(root).read().ok()?;
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
            let repo = Repo::new();
            repo.write(".harness/.gitignore", "events.jsonl\n*.log\nlogs/\n");
            let cmd = repo.stub_check(check_body);
            repo.write(
                "harness.toml",
                &format!("[check]\ncommand = \"{cmd}\"\nfail_name = '\\(fail\\) (.+)$'\n"),
            );
            repo.commit_all("harness");
            let cfg = crate::config::load(&repo.root).expect("load harness.toml");
            let writer = Writer::new(Log::open(&repo.root.join(".harness")));
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
            std::fs::read_to_string(self.repo.root.join("TASKS.md")).expect("read TASKS.md")
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
        env.repo.write(".check-baseline", "# inherited\nalpha\n");
        let r = check_delta(&env.repo.root, &env.cfg, false);
        assert!(r.red);
        assert!(r.unforgiven.is_empty());
        assert_eq!(r.forgiven, vec!["alpha".to_string()]);
        assert!(r.accepts());
    }

    #[test]
    fn one_forgiven_one_new_is_still_red() {
        let env = Env::new("echo '(fail) alpha'\necho '(fail) beta [12ms]'\nexit 1\n");
        env.repo.write(".check-baseline", "alpha\n");
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
            "chore(T-001): harness gate rejected a done verdict with work off the branch"
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
            .contains("chore(T-001): harness gate rejected a false VERIFIED"));
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
    fn scope_rejects_a_file_outside_the_globs() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write("src/other.ts", "stray");
        env.repo.commit_all("stray");

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
    fn scope_allows_a_harness_edit_under_a_harness_task() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".harness/*", "none — harness");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".harness/loop.sh", "# edited\n");
        env.repo.commit_all("harness edit");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn scope_rejects_the_same_edit_under_a_product_task() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".harness/*", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".harness/loop.sh", "# edited\n");
        env.repo.commit_all("harness edit");

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
    fn scope_rejects_a_baseline_that_grew_even_under_a_harness_task() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".check-baseline", "none — harness");
        env.repo.write(".check-baseline", "alpha\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".check-baseline", "alpha\nbeta\n");
        env.repo.commit_all("baseline grew");

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
    fn scope_exempts_a_hash_recut_for_a_file_on_the_scope_line() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo
            .write("test-hashes.json", "{\n  \"src/a.ts\": \"aaa\"\n}\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("test-hashes.json", "{\n  \"src/a.ts\": \"bbb\"\n}\n");
        env.repo.commit_all("recut");
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
        off.repo.commit_all("recut");
        let out = run("scope", &mut off.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.reason.contains("test-hashes.json"), "{}", out.reason);
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
        let tasks = env.repo.root.join("TASKS.md");
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
    fn scope_exempts_a_freshly_vendored_role_outside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/**", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".harness/roles/implementer.md", "# role\n");
        env.repo
            .write("harness.lock", &role_lock_toml(&[("implementer", "aaa")]));
        env.repo.commit_all("vendor implementer");

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
        env.repo.commit_all("recut implementer");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason.contains("harness.lock (implementer)"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn scope_exempts_a_freshly_added_lock_key_outside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".claude/skills/tdd/**", "§11 row 1");
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        // the role fetched a second skill this stage; its id is new to the lock, absent at base
        env.repo.write(
            "harness.lock",
            &lock_toml(&[("tdd", "aaa"), ("tdd-old", "aaa")]),
        );
        env.repo.commit_all("vendor tdd-old");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_sha_only_recut_outside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".claude/skills/tdd/**", "§11 row 1");
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd-old", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        // only sha256 changes -- the id line itself never appears in the diff
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd-old", "bbb")]));
        env.repo.commit_all("recut tdd-old");

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
        env.queue("done", ".claude/skills/tdd/**", "§11 row 1");
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd", "bbb")]));
        env.repo.commit_all("recut tdd");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_removed_lock_key_outside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".claude/skills/tdd/**", "§11 row 1");
        env.repo.write(
            "harness.lock",
            &lock_toml(&[("tdd", "aaa"), ("tdd-old", "aaa")]),
        );
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("harness.lock", &lock_toml(&[("tdd", "aaa")]));
        env.repo.commit_all("drop tdd-old");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason.contains("harness.lock (tdd-old)"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn scope_rejects_an_unrelated_file_dropped_under_the_skills_dir() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/**", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        // never went through skills::resolve, so it names no id the lock added this iteration
        env.repo
            .write(".claude/skills/other/NOTES.md", "hand-written");
        env.repo.commit_all("stray file under the skills dir");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason.contains(".claude/skills/other/NOTES.md"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn skills_dir_falls_back_from_layout_to_preset_to_harness_dir() {
        let mut cfg = Config::default();
        cfg.layout.harness_dir = ".harness".to_string();
        assert_eq!(skills_dir_for(&cfg), ".harness/skills");
        cfg.agent.preset = "claude".to_string();
        assert_eq!(skills_dir_for(&cfg), ".claude/skills");
        cfg.layout.skills_dir = Some("vendor/skills".to_string());
        assert_eq!(skills_dir_for(&cfg), "vendor/skills");
    }

    #[test]
    fn force_runs_the_force_form_of_the_check() {
        let repo = Repo::new();
        let cmd = repo.stub_check("echo \"$1\" >> ran.txt\nexit 0\n");
        repo.write(
            "harness.toml",
            &format!(
                "[check]\ncommand = \"{cmd} plain\"\nforce = \"{cmd} forced\"\nfail_name = '\\(fail\\) (.+)$'\n"
            ),
        );
        let cfg = crate::config::load(&repo.root).expect("load harness.toml");
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
