//! The named checks the loop runs after a stage exits.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::block::{self, Block};
use crate::config::Config;
use crate::events::{Kind, Tally, Writer};
use crate::git::{self, git, head, porcelain};
use crate::skills::LockEntry;
use crate::tracker::{self, Tracker, SPECS};

pub(crate) const BOOKKEEPING: &[&str] = &[
    SPECS,
    "PROGRESS.md",
    "PROGRESS.archive.md",
    "DECISIONS.md",
    "LEARNINGS.md",
    ".out-of-scope",
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
    // the scanners and their reports; config::cache_root() outside a test
    pub cache: PathBuf,
    pub sandbox: &'a crate::sandbox::Sandbox,
}

#[derive(Debug, Clone, Default)]
pub struct GateOutcome {
    pub pass: bool,
    pub reason: String,
    pub skip_rest: bool,
    pub halt: bool,
    pub tally: Option<Tally>,
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
        "claims" => claims(ctx),
        "findings" => findings(ctx),
        "scope" => scope(ctx),
        "red-first" => red_first(ctx),
        "skip-added" => skip_added(ctx),
        "security" => security(ctx),
        "queue-intact" => queue_intact(ctx),
        "commit-identity" => commit_identity(ctx),
        "commit-task" => commit_task(ctx),
        "check-delta" => check_gate(ctx),
        "commit-round" => commit_round(ctx),
        "adjudicator-halt" => adjudicator_halt(ctx),
        "dry-round" => dry_round(ctx),
        "install-stale" => install_stale(ctx),
        "tickets" => tickets(ctx),
        "holdout" => holdout(ctx),
        other => fail(format!("no such gate: {other}")),
    };
    let task = ctx.task.clone().unwrap_or_default();
    ctx.events.emit(Kind::Gate {
        gate: name.to_string(),
        task,
        pass: outcome.pass,
        reason: outcome.reason.clone(),
        tally: outcome.tally,
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

fn queue_of(ctx: &GateCtx) -> Tracker {
    Tracker::open(ctx.root, &ctx.cfg.layout.harness_dir)
}

fn status_of(ctx: &GateCtx, task: &str) -> Result<Option<String>, block::QueueError> {
    Ok(queue_of(ctx)
        .get(task)?
        .and_then(|b| block::field(&b, "status"))
        .filter(|s| !s.is_empty()))
}

fn field_of(ctx: &GateCtx, task: &str, key: &str) -> String {
    queue_of(ctx)
        .get(task)
        .ok()
        .flatten()
        .and_then(|b| block::field(&b, key))
        .unwrap_or_default()
}

fn blocks_at_base(ctx: &GateCtx) -> Option<Vec<Block>> {
    let (_, _, base) = at_base(ctx, &rel(ctx, SPECS))?;
    tracker::blocks_at(ctx.root, &ctx.cfg.layout.harness_dir, &base)
}

// the field at a revision, the base when rev is None, the way lock_at reads

fn field_at(ctx: &GateCtx, task: &str, key: &str, rev: Option<&str>) -> Option<String> {
    let (_, _, base) = at_base(ctx, &rel(ctx, SPECS))?;
    let b = tracker::get_at(
        ctx.root,
        &ctx.cfg.layout.harness_dir,
        task,
        rev.unwrap_or(&base),
    )?;
    block::field(&b, key)
}

fn verdict_of(ctx: &GateCtx, task: &str) -> Option<tracker::Ruling> {
    let b = queue_of(ctx).get(task).ok()??;
    tracker::verdict(&b)
}

fn unreadable(ctx: &mut GateCtx, task: &str) -> GateOutcome {
    ctx.warnings.push(format!(
        "{task}: the queue could not be read; the gate did not run."
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
        match queue_of(ctx).set_status(task, to, reason) {
            Ok(()) => {
                if let Err(err) = commit_instance(ctx, &[SPECS], commit_msg) {
                    ctx.warnings
                        .push(format!("{task}: the queue was not committed: {err}"));
                }
            }
            // the queue still says done, so a task.status event here isn't a change the file carries
            Err(err) => {
                ctx.warnings
                    .push(format!("{task}: the queue could not be rewritten: {err}"));
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
            if !verdict_of(ctx, &task)
                .is_some_and(|r| matches!(r.verdict, tracker::Verdict::Rejected(_)))
            {
                return pass("the implementer left it at review");
            }
            let mut out = force_back(
                ctx,
                "implementer-not-done",
                &task,
                "ready-for-agent",
                "the implementer set review over a rejection; answer it and write `verdict: answered by implementer`",
                &format!("chore({task}): the implementer left a rejection standing"),
                &format!("{task} was forced back to ready: review with the rejection unanswered."),
            );
            out.skip_rest = true;
            out
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
                "ready-for-agent",
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
    if let Some(deferred) = deferred_finding(ctx, &task) {
        let phrase = &deferred.phrase;
        let reason = match deferred.source {
            DeferredIn::Field => format!(
                "the verdict's `deferred:` line says \"{phrase}\" and it added no needs-triage ticket; file that finding as a needs-triage ticket with the command that shows it"
            ),
            DeferredIn::Notes => format!(
                "the verdict's notes say \"{phrase}\" and it added no needs-triage ticket; file that finding as a needs-triage ticket with the command that shows it, and name it on a `deferred:` line"
            ),
        };
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
    commit(ctx, &[SPECS], &format!("verify: {task} verdict"))
}

enum DeferredIn {
    Field,
    Notes,
}

struct Deferred {
    phrase: String,
    source: DeferredIn,
}

// a finding left only in notes is archived with its block and never reaches the queue
fn deferred_finding(ctx: &GateCtx, task: &str) -> Option<Deferred> {
    at_base(ctx, &rel(ctx, SPECS))?;
    let before = blocks_at_base(ctx).unwrap_or_default();
    let now = queue_of(ctx).blocks().ok()?;
    let proposed_before = tracker::ids_in(&before, &["needs-triage"]);
    if tracker::ids_in(&now, &["needs-triage"])
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
    let added: Vec<&str> = now
        .iter()
        .filter(|b| b.id == task)
        .flat_map(|b| b.body.iter().map(|(_, l)| l.as_str()))
        .filter(|l| !old.contains(l))
        .collect();
    // the field is the verifier's own word; prose is read only when no field was written
    if let Some(phrase) = added
        .iter()
        .find_map(|l| l.trim().strip_prefix("deferred:"))
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        return Some(Deferred {
            phrase: phrase.to_string(),
            source: DeferredIn::Field,
        });
    }
    let re = regex::Regex::new(
        r"(?i)\b(not a reason for rejection|not a rejection reason|minor|for later)\b",
    )
    .ok()?;
    // "no minor issues" says there is nothing to propose
    let negated =
        regex::Regex::new(r"(?i)\b(no|nothing)\s+minor\b|\bminor\s+or\s+otherwise\b").ok()?;
    added
        .iter()
        .filter(|l| !l.trim_start().starts_with('>'))
        .find_map(|l| {
            re.find(&negated.replace_all(&prose_only(l), ""))
                .map(|m| m.as_str().to_lowercase())
        })
        .map(|phrase| Deferred {
            phrase,
            source: DeferredIn::Notes,
        })
}

// a file, a path, a test name or a code span is not the verifier talking
fn prose_only(line: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for word in line.split_inclusive('`') {
        let (text, tick) = match word.strip_suffix('`') {
            Some(t) => (t, true),
            None => (word, false),
        };
        if !in_code {
            out.push_str(text);
            out.push(' ');
        }
        if tick {
            in_code = !in_code;
        }
    }
    out.split_whitespace()
        .filter(|w| {
            let w = w.trim_end_matches(['.', ',', ';', ':', ')', ']']);
            !(w.contains('/') || w.contains('.') || w.contains('_') || w.contains("::"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn commit_round(ctx: &mut GateCtx) -> GateOutcome {
    let iter = ctx.events.iter;
    commit(
        ctx,
        &[SPECS, "DECISIONS.md"],
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
    let ruling = verdict_of(ctx, &task);
    let line = ruling.as_ref().map(|r| &r.verdict);
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() == Some("ready-for-agent") => {
            if field_at(ctx, &task, "status", None).as_deref() != Some("review")
                || matches!(&line, Some(tracker::Verdict::Rejected(r)) if !r.is_empty())
            {
                return pass("a reasoned rejection");
            }
            return force_back(
                ctx,
                "verdict",
                &task,
                "review",
                "the verifier rejected it without `verdict: rejected by verifier: <reason>`",
                &format!("chore({task}): enallagi gate refused a rejection with no reason"),
                &format!("{task} went back to review: a rejection with no verdict reason."),
            );
        }
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) if line != Some(&tracker::Verdict::Verified) => {
            return force_back(
                ctx,
                "verdict",
                &task,
                "review",
                "done without `verdict: verified by verifier`",
                &format!("chore({task}): enallagi gate refused a done with no verified verdict"),
                &format!("{task} went back to review: done with no verified verdict."),
            );
        }
        Ok(_) => {
            let r = ruling.as_ref().expect("verified");
            let missing: Vec<&str> = [("`spec:`", &r.spec), ("`standards:`", &r.standards)]
                .iter()
                .filter(|(_, v)| v.is_none())
                .map(|(k, _)| *k)
                .collect();
            if !missing.is_empty() {
                let reason = format!(
                    "done without a {} line under the verdict: each review axis names its findings or `none`",
                    missing.join(" or ")
                );
                return force_back(
                    ctx,
                    "verdict",
                    &task,
                    "ready-for-agent",
                    &reason,
                    &format!("chore({task}): enallagi gate refused a done missing a review axis"),
                    &format!("{task} was forced back to ready: done missing a review axis."),
                );
            }
        }
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
            "ready-for-agent",
            &reason,
            &format!(
                "chore({task}): enallagi gate rejected a done verdict with work off the branch"
            ),
            &format!("{task} was forced back to ready: done with {left} uncommitted path(s)."),
        );
    }

    let report = check_delta(ctx.sandbox, ctx.root, ctx.cfg, false);
    if let Some(halt) = hung(ctx, &report) {
        return halt;
    }
    let tally = counted(ctx, &report);
    if report.accepts() {
        return GateOutcome {
            tally,
            ..pass("done, and the gate agrees.")
        };
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
    let outcome = force_back(
        ctx,
        "verdict",
        &task,
        "ready-for-agent",
        &reason,
        &format!("chore({task}): enallagi gate rejected a false VERIFIED"),
        &format!(
            "{task} was forced back to ready by the gate: the verifier said done, the gate was red.\ncheck tail:\n{tail8}"
        ),
    );
    GateOutcome { tally, ..outcome }
}

fn claimed_runs(ctx: &GateCtx, task: &str) -> Vec<(String, i32)> {
    queue_of(ctx)
        .get(task)
        .ok()
        .flatten()
        .and_then(|b| tracker::verdict(&b))
        .map(|r| r.runs)
        .unwrap_or_default()
}

// a verdict's pasted exit is otherwise its word alone; the allowlist bounds what a lane's text may run here
fn claims(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }
    let allow = &ctx.cfg.check.claims_allow;
    if allow.is_empty() {
        return pass("claims_allow is empty");
    }
    let runs = claimed_runs(ctx, &task);
    if runs.is_empty() {
        return pass("no ran:/exit: pair under the verdict line");
    }
    let timeout = ctx
        .cfg
        .check
        .timeout_duration()
        .unwrap_or(crate::config::DEFAULT_CHECK_TIMEOUT);
    let (mut refuted, mut flaky, mut held, mut skipped) = (vec![], vec![], vec![], vec![]);
    for (cmd, claimed) in runs {
        // the command goes to `sh -c`, so a prefix match alone would let `<prefix>; <anything>` run
        if cmd.contains([';', '&', '|', '`', '$', '<', '>', '\n'])
            || !allow.iter().any(|p| allowed(&cmd, p))
        {
            skipped.push(format!("`{cmd}`"));
            continue;
        }
        let mut exits = [-1; 2];
        for exit in &mut exits {
            let Ok(run) = run_bounded(ctx.sandbox, ctx.root, &cmd, timeout) else {
                continue;
            };
            let report = CheckReport {
                timed_out: hang_reason("the claim", &cmd, timeout, &run),
                ..CheckReport::default()
            };
            if let Some(halt) = hung(ctx, &report) {
                return halt;
            }
            *exit = run.exit;
        }
        let [a, b] = exits;
        let line = format!("`{cmd}` claimed exit {claimed}, re-ran exit {a} and {b}");
        if a != claimed && b != claimed {
            refuted.push(line);
        } else if a != b {
            flaky.push(line);
        } else {
            held.push(line);
        }
    }
    if !refuted.is_empty() {
        let reason = format!(
            "the verifier returned done and a claimed exit did not reproduce: {}",
            refuted.join("; ")
        );
        return force_back(
            ctx,
            "claims",
            &task,
            "ready-for-agent",
            &reason,
            &format!("chore({task}): enallagi gate rejected a claimed exit"),
            &format!("{task} was forced back to ready: {}", refuted.join("; ")),
        );
    }
    let mut parts = Vec::new();
    if !flaky.is_empty() {
        parts.push(format!("flaky: {}", flaky.join("; ")));
    }
    if !held.is_empty() {
        parts.push(format!("reproduced: {}", held.join("; ")));
    }
    if !skipped.is_empty() {
        parts.push(format!(
            "not re-run, outside claims_allow or carrying a shell operator: {}",
            skipped.join(", ")
        ));
    }
    pass(parts.join(". "))
}

// every finding a returned ticket carries is answered once: an accept names a test the round's
// test files hold, a reject names a command the verdict ran
fn findings(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }
    let Some(b) = queue_of(ctx).get(&task).ok().flatten() else {
        return unreadable(ctx, &task);
    };
    let lines = |key: &str| -> Vec<String> {
        b.body
            .iter()
            .filter_map(|(_, l)| l.strip_prefix(key))
            .map(|l| l.trim().to_string())
            .collect()
    };
    let ids: Vec<String> = lines("finding:")
        .iter()
        .filter_map(|l| l.split_whitespace().next().map(String::from))
        .collect();
    if ids.is_empty() {
        return pass("no finding");
    }
    let answers = lines("answer:");
    let ran: Vec<String> = tracker::verdict(&b)
        .map(|r| r.runs.into_iter().map(|(cmd, _)| cmd).collect())
        .unwrap_or_default();
    let base = ctx.iter_base.clone().unwrap_or_default();
    let globs = &ctx.cfg.layout.test_glob;
    let tests: String = task_files(ctx, &base, &task, "")
        .unwrap_or_default()
        .iter()
        .filter(|f| globs.is_empty() || in_scope(f, globs))
        .filter_map(|f| git(ctx.root, &["show", &format!("HEAD:{f}")]).ok())
        .collect();
    let mut wrong = Vec::new();
    for id in &ids {
        let mine: Vec<&str> = answers
            .iter()
            .filter_map(|a| a.strip_prefix(id.as_str()))
            .filter(|a| a.starts_with(' '))
            .map(str::trim)
            .collect();
        let [answer] = mine[..] else {
            wrong.push(format!("{id} has {} answers, not one", mine.len()));
            continue;
        };
        if let Some(test) = answer.strip_prefix("accept ").map(str::trim) {
            if test.is_empty() || !names_task(&tests, test) {
                wrong.push(format!(
                    "{id}'s test `{test}` is in no test file the round changed"
                ));
            }
        } else if let Some(reject) = answer.strip_prefix("reject ") {
            match reject.split_once(':').map(|(_, c)| c.trim()) {
                None | Some("") => wrong.push(format!("{id}'s reject names no command")),
                Some(cmd) if !ran.iter().any(|r| r == cmd) => {
                    wrong.push(format!("{id}'s reject command `{cmd}` has no `ran:` line"))
                }
                Some(_) => {}
            }
        } else {
            wrong.push(format!("{id}'s answer is neither accept nor reject"));
        }
    }
    if wrong.is_empty() {
        return pass(format!("{} finding(s) answered", ids.len()));
    }
    let why = wrong.join("; ");
    force_back(
        ctx,
        "findings",
        &task,
        "ready-for-agent",
        &format!("the verifier returned done and the findings gate rejected it: {why}"),
        &format!("chore({task}): enallagi findings gate rejected a done verdict"),
        &format!("{task} was forced back to ready by the findings gate: {why}."),
    )
}

// a whole command word, so `cargo test` never admits `cargo testx`
fn allowed(cmd: &str, entry: &str) -> bool {
    let entry = entry.trim_end();
    cmd.strip_prefix(entry)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

// word boundaries, so one id never matches a longer id it prefixes
pub fn names_task(subject: &str, task: &str) -> bool {
    regex::Regex::new(&format!(r"\b{}\b", regex::escape(task))).is_ok_and(|re| re.is_match(subject))
}

// the files the task's own commits changed: a commit whose subject names no task, or another task,
// is an operator's or another round's and is not charged here. filter is a git --diff-filter value,
// empty for every change
fn commit_files(
    root: &Path,
    base: &str,
    task: &str,
    filter: &str,
) -> Result<Vec<String>, git::GitError> {
    let flag = format!("--diff-filter={filter}");
    let mut files: Vec<String> = Vec::new();
    for sha in task_commits(root, base, task)? {
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
    Ok(files)
}

// the baseline lines the task's own commits add and do not take back: each commit's removals are
// netted against what the earlier ones added, so an add a later commit reverses is not growth
fn baseline_lines_added(
    root: &Path,
    inner: &str,
    base: &str,
    task: &str,
) -> Result<Vec<String>, git::GitError> {
    let mut added: Vec<String> = Vec::new();
    for sha in task_commits(root, base, task)? {
        let diff = git(root, &["show", "--format=", &sha, "--", inner]).unwrap_or_default();
        // a failure name may itself start with + or -, so the file headers are told apart by
        // position, not by a doubled prefix: everything before the first hunk is a header
        let body: Vec<&str> = diff
            .lines()
            .skip_while(|l| !l.starts_with("@@"))
            .filter(|l| !l.starts_with("@@"))
            .collect();
        for gone in body.iter().filter(|l| l.starts_with('-')) {
            if let Some(at) = added.iter().position(|line| line == &gone[1..]) {
                added.remove(at);
            }
        }
        for new in body
            .iter()
            .filter(|l| l.starts_with('+') && !l.starts_with("+#"))
        {
            added.push(new[1..].to_string());
        }
    }
    Ok(added)
}

fn task_commits(root: &Path, base: &str, task: &str) -> Result<Vec<String>, git::GitError> {
    let range = format!("{base}..HEAD");
    Ok(git(
        root,
        &["log", "--reverse", "--no-merges", "--format=%h %s", &range],
    )?
    .lines()
    .filter_map(|l| l.split_once(' '))
    .filter(|(_, subject)| names_task(subject, task))
    .map(|(sha, _)| sha.to_string())
    .collect())
}

// the task's files: the product repository's, and in a nested install the harness
// directory's under its own prefix
fn task_files(
    ctx: &GateCtx,
    base: &str,
    task: &str,
    filter: &str,
) -> Result<Vec<String>, git::GitError> {
    let dir = &ctx.cfg.layout.harness_dir;
    let state = git::state_root(ctx.root, dir);
    let mut files = commit_files(ctx.root, base, task, filter)?;
    if let Some(state_base) = ctx.state_base.as_deref().filter(|_| state != ctx.root) {
        files.extend(
            commit_files(&state, state_base, task, filter)?
                .into_iter()
                .map(|f| format!("{dir}/{f}")),
        );
    }
    Ok(files)
}

// a proposed block's scope: line is the adjudicator's to write, so widening counts from its promotion
fn promoted_at(ctx: &GateCtx, task: &str, queued: &str) -> Option<String> {
    if field_at(ctx, task, "status", Some(queued)).as_deref() != Some("needs-triage") {
        return None;
    }
    let range = format!("{queued}..HEAD");
    tracker::ticket_revs(ctx.root, &ctx.cfg.layout.harness_dir, task, &range)?
        .into_iter()
        .find(|rev| field_at(ctx, task, "status", Some(rev)).is_some_and(|s| s != "needs-triage"))
}

fn filed_untriaged(ctx: &GateCtx, path: &str) -> bool {
    let name = Path::new(path).file_name().and_then(|n| n.to_str());
    let Some(number) = name.and_then(|n| n.strip_prefix("T-")?.split('-').next()) else {
        return false;
    };
    tracker::get_at(
        ctx.root,
        &ctx.cfg.layout.harness_dir,
        &format!("T-{number}"),
        "HEAD",
    )
    .and_then(|b| tracker::state(&b))
    .as_deref()
        == Some("needs-triage")
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

    // the lane edits the block it is graded against, so a path the line gained since the queue
    // passes only with a `widened:` reason, and the verdict names it
    let head_block = |key: &str| {
        field_at(ctx, &task, key, Some("HEAD")).unwrap_or_else(|| field_of(ctx, &task, key))
    };
    let pats = scope_globs(&head_block("scope"));
    // the commit that queued the block, not the round's base: in a review-only round the base is the
    // implement round's own state commit, which already carries the widening
    let queued_rev = tracker::queued_commit(ctx.root, &ctx.cfg.layout.harness_dir, &task)
        .ok()
        .flatten()
        .map(|rev| promoted_at(ctx, &task, &rev).unwrap_or(rev));
    let queued =
        field_at(ctx, &task, "scope", queued_rev.as_deref()).map(|line| scope_globs(&line));
    let gained: Vec<String> = queued
        .map(|q| pats.iter().filter(|p| !q.contains(p)).cloned().collect())
        .unwrap_or_default();
    let widened = head_block("widened");
    let widening =
        (!gained.is_empty()).then(|| format!("widened scope: with {}", gained.join(" ")));
    let skills_dir = skills_dir_for(ctx.cfg);
    let lock = rel(ctx, "harness.lock");
    let hashes = rel(ctx, "test-hashes.json");
    let bookkeeping: Vec<String> = BOOKKEEPING.iter().map(|name| rel(ctx, name)).collect();
    // the lane's own ticket is the one queue path it may edit; another ticket's file is out of scope
    let tickets = format!("{}/", rel(ctx, SPECS));
    let own = queue_of(ctx).path_of(&task).and_then(|p| {
        Some(
            p.strip_prefix(ctx.root)
                .ok()?
                .to_string_lossy()
                .into_owned(),
        )
    });
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
    let (changed, added) = match (
        task_files(ctx, &base, &task, ""),
        task_files(ctx, &base, &task, "A"),
    ) {
        (Ok(changed), Ok(added)) => (changed, added),
        (Err(e), _) | (_, Err(e)) => return fail(e.to_string()),
    };
    // cargo rewrites Cargo.lock from a manifest the range changed, and a commit without it leaves
    // the tree dirty after the next build; the manifest still has to be on the scope: line
    let manifest_in_scope = changed
        .iter()
        .any(|f| f.ends_with("Cargo.toml") && in_scope(f, &pats));
    for f in changed {
        if bookkeeping.contains(&f) || f == lock || own.as_ref() == Some(&f) {
            continue;
        }
        // a ticket the range adds is a filed finding, as appending a block was, so only untriaged
        if added.contains(&f) && f.starts_with(&tickets) && filed_untriaged(ctx, &f) {
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
    // the baseline only ever shrinks; a line the task's own commits ADD is a red check made green by
    // hand, whatever rows: says
    let grew = match at_base(ctx, &rel(ctx, ".check-baseline"))
        .map(|(repo, inner, base)| baseline_lines_added(&repo, &inner, &base, &task))
    {
        Some(Err(e)) => return fail(e.to_string()),
        Some(Ok(lines)) => !lines.is_empty(),
        None => false,
    };

    let unexplained = widening.is_some() && widened.is_empty();
    if out_of.is_empty() && harness_hit.is_empty() && !grew && !unexplained {
        return pass(match widening {
            Some(w) => format!("{task} stayed inside its scope. It {w}: {widened}"),
            None => format!("{task} stayed inside its scope."),
        });
    }

    let mut parts: Vec<String> = Vec::new();
    if let Some(w) = widening.filter(|_| unexplained) {
        parts.push(format!(
            "{w} since it was queued, and the block carries no reason; write it as a `widened:` line"
        ));
    }
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
    // to ready, not review: every scope refusal is answered by a code or block edit, and the
    // implementer is the one role that may make it; its confirm-only arm finds the iteration's
    // commits already on the branch and adds the `widened:` line or reverts the stray file
    force_back(
        ctx,
        "scope",
        &task,
        "ready-for-agent",
        &reason,
        &format!("chore({task}): harness scope gate rejected a done verdict"),
        &format!(
            "{task} was forced back to ready by the scope gate: {why}. The iteration's commits for {task} are on the branch; the next implement stage confirms them and answers the gate."
        ),
    )
}

// a test switched off passes every other gate, since the hashes guard only files already hashed (T-486)
fn skip_added(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }
    let Some(base) = ctx.iter_base.clone().filter(|b| !b.is_empty()) else {
        return pass("no base");
    };
    if ctx.cfg.layout.skip_patterns.is_empty() {
        return pass("skip_patterns is empty");
    }
    let res = match ctx
        .cfg
        .layout
        .skip_patterns
        .iter()
        .map(|p| regex::Regex::new(p))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(res) => res,
        Err(e) => return fail(format!("skip_patterns: {e}")),
    };
    let globs = &ctx.cfg.layout.test_glob;
    let (files, commits) = match (
        task_files(ctx, &base, &task, ""),
        task_commits(ctx.root, &base, &task),
    ) {
        (Ok(files), Ok(commits)) => (files, commits),
        (Err(e), _) | (_, Err(e)) => return fail(e.to_string()),
    };
    let mut hits: Vec<String> = Vec::new();
    // an empty test_glob reads every changed file, so an unset key never switches the gate off
    for f in files
        .iter()
        .filter(|f| globs.is_empty() || in_scope(f, globs))
    {
        let Ok(now) = git(ctx.root, &["show", &format!("HEAD:{f}")]) else {
            continue;
        };
        let now: Vec<&str> = now.lines().collect();
        for sha in &commits {
            let diff =
                git(ctx.root, &["show", "--format=", "-U0", sha, "--", f]).unwrap_or_default();
            for added in diff
                .lines()
                .skip_while(|l| !l.starts_with("@@"))
                .filter_map(|l| l.strip_prefix('+'))
                .filter(|l| res.iter().any(|re| re.is_match(l)))
            {
                // a line a later commit took back is not at HEAD, and is no skip
                if let Some(n) = now.iter().position(|l| *l == added) {
                    let hit = format!("{f}:{} {}", n + 1, added.trim());
                    if !hits.contains(&hit) {
                        hits.push(hit);
                    }
                }
            }
        }
    }
    if hits.is_empty() {
        return pass(format!("{task} added no skip"));
    }
    let why = format!(
        "added a skip with no stated reason: {}; state the reason or drop the skip",
        hits.join(", ")
    );
    force_back(
        ctx,
        "skip-added",
        &task,
        "ready-for-agent",
        &format!("the verifier returned done and the skip-added gate rejected it: {why}"),
        &format!("chore({task}): harness skip-added gate rejected a done verdict"),
        &format!("{task} was forced back to ready by the skip-added gate: {why}."),
    )
}

// a detached worktree at rev under `under`, removed on drop so a hang or a panic leaves the lane clean
struct Scratch {
    root: PathBuf,
    dir: tempfile::TempDir,
}

impl Scratch {
    fn add(root: &Path, under: &Path, rev: &str) -> Result<Scratch, String> {
        std::fs::create_dir_all(under).map_err(|e| e.to_string())?;
        let dir = tempfile::Builder::new()
            .prefix("enallagi-scratch-")
            .tempdir_in(under)
            .map_err(|e| e.to_string())?;
        let path = dir.path().to_string_lossy().into_owned();
        git(root, &["worktree", "add", "--detach", &path, rev]).map_err(|e| e.to_string())?;
        Ok(Scratch {
            root: root.to_path_buf(),
            dir,
        })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let path = self.dir.path().to_string_lossy().into_owned();
        let _ = git(&self.root, &["worktree", "remove", "--force", &path]);
        let _ = git(&self.root, &["worktree", "prune"]);
    }
}

// a test that passes with the task's other files back at base proves nothing (T-485)
fn red_first(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }
    let Some(base) = ctx.iter_base.clone().filter(|b| !b.is_empty()) else {
        return pass("no base");
    };
    let globs = &ctx.cfg.layout.test_only_glob;
    if globs.is_empty() {
        return pass("test_only_glob is empty");
    }
    let (tests, source): (Vec<String>, Vec<String>) = match task_files(ctx, &base, &task, "") {
        Ok(files) => files.into_iter().partition(|f| in_scope(f, globs)),
        Err(e) => return fail(e.to_string()),
    };
    if tests.is_empty() {
        return pass("no test-only file changed");
    }
    let tree = match Scratch::add(ctx.root, &std::env::temp_dir(), "HEAD") {
        Ok(tree) => tree,
        Err(e) => return fail(format!("red-first could not add a worktree: {e}")),
    };
    let at = tree.dir.path();
    for f in &source {
        // a file the task added is absent at base, so restoring it is deleting it
        if git(at, &["cat-file", "-e", &format!("{base}:{f}")]).is_ok() {
            let _ = git(at, &["checkout", &base, "--", f]);
        } else {
            let _ = std::fs::remove_file(at.join(f));
        }
    }
    let timeout = ctx
        .cfg
        .check
        .timeout_duration()
        .unwrap_or(crate::config::DEFAULT_CHECK_TIMEOUT);
    let run = run_bounded(ctx.sandbox, at, &ctx.cfg.check.command, timeout);
    drop(tree);
    let named = tests.join(" ");
    match run {
        Err(e) => fail(format!("red-first could not run the check: {e}")),
        Ok(run) if run.exit != 0 || run.timed_out || run.stopped.is_some() => {
            pass(format!("{named} red on base {base} (exit {})", run.exit))
        }
        Ok(_) => {
            let why = format!(
                "the check passes with {named} and every other changed file at base {base}, so the tests prove nothing"
            );
            force_back(
                ctx,
                "red-first",
                &task,
                "ready-for-agent",
                &format!("the verifier returned done and the red-first gate rejected it: {why}"),
                &format!("chore({task}): harness red-first gate rejected a done verdict"),
                &format!("{task} was forced back to ready by the red-first gate: {why}."),
            )
        }
    }
}

/// A spec's holdouts: the executable regular files in `holdouts/<slug>/`, sorted.
// a respond lane's state worktree lacks the ignored set; the main worktree holds it
fn holdout_root(root: &Path, cfg: &Config) -> PathBuf {
    let dir = &cfg.layout.harness_dir;
    git::main_worktree_path(&root.join(dir))
        .map(|main| main.join("holdouts"))
        .unwrap_or_else(|| crate::config::instance_path(root, dir, "holdouts"))
}

pub fn holdout_set(root: &Path, cfg: &Config, slug: &str) -> Vec<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dir = holdout_root(root, cfg).join(slug);
    let mut set: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
        .map(|e| e.path())
        .collect();
    set.sort();
    set
}

/// The specs with a holdout set.
pub fn holdout_specs(root: &Path, cfg: &Config) -> Vec<String> {
    let dir = holdout_root(root, cfg);
    let mut specs: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|slug| !holdout_set(root, cfg, slug).is_empty())
        .collect();
    specs.sort();
    specs
}

/// Whether a lane's transcript, or the product or state diff since its bases, names a set's path.
/// Concrete set paths only, so prose about holdouts never trips it (spec 6).
pub fn names_holdout(
    root: &Path,
    cfg: &Config,
    bases: &(Option<String>, Option<String>),
    output: &str,
) -> bool {
    let paths: Vec<String> = holdout_specs(root, cfg)
        .iter()
        .map(|s| format!("holdouts/{s}"))
        .collect();
    if paths.is_empty() {
        return false;
    }
    let state = git::state_root(root, &cfg.layout.harness_dir);
    let mut texts = vec![output.to_string()];
    for (repo, base) in [(root, &bases.0), (state.as_path(), &bases.1)] {
        if let Some(base) = base.as_deref().filter(|b| !b.is_empty()) {
            texts.extend(git(repo, &["diff", base]).ok());
        }
    }
    texts.iter().any(|t| paths.iter().any(|p| t.contains(p)))
}

/// Runs a spec's holdouts at `rev` in a scratch worktree; returns (run, red) and nothing else,
/// since anything more reaches the queue a writer reads (spec 6).
pub fn holdouts(
    sandbox: &crate::sandbox::Sandbox,
    root: &Path,
    cfg: &Config,
    cache: &Path,
    slug: &str,
    task: &str,
    rev: &str,
) -> Result<(usize, usize), String> {
    let set = holdout_set(root, cfg, slug);
    let tree = Scratch::add(
        root,
        &root.join(&cfg.layout.harness_dir).join("worktrees"),
        rev,
    )?;
    let logs = cache
        .join("holdouts")
        .join(tracker::repo_slug(root))
        .join(task);
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let timeout = cfg
        .check
        .timeout_duration()
        .unwrap_or(crate::config::DEFAULT_CHECK_TIMEOUT);
    let lifted = crate::sandbox::RolePolicy {
        holdouts: true,
        ..Default::default()
    };
    let mut red = 0;
    for h in &set {
        let mut w = sandbox.wrap_as(&[h.display().to_string()], tree.dir.path(), &[], &lifted);
        let dir = h.parent().unwrap_or(h).display().to_string();
        w.env.push(("ENALLAGI_HOLDOUTS".to_string(), dir));
        let name = h.file_name().unwrap_or_default().to_string_lossy();
        let output = match run_wrapped(&w, timeout) {
            Ok(run) if run.exit == 0 && !run.timed_out && run.stopped.is_none() => run.output,
            Ok(run) => {
                red += 1;
                run.output
            }
            Err(e) => {
                red += 1;
                e.to_string()
            }
        };
        let _ = std::fs::write(logs.join(format!("{name}.log")), output);
    }
    Ok((set.len(), red))
}

// a red holdout says only how many, so the writer has nothing to fit the code to (spec 6)
fn holdout(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }
    let Some(slug) = queue_of(ctx).spec_of(&task) else {
        return pass("no holdout set");
    };
    if holdout_set(ctx.root, ctx.cfg, &slug).is_empty() {
        return pass("no holdout set");
    }
    match holdouts(
        ctx.sandbox,
        ctx.root,
        ctx.cfg,
        &ctx.cache,
        &slug,
        &task,
        "HEAD",
    ) {
        Err(e) => halted(ctx, format!("{task}: the holdouts could not be run: {e}")),
        Ok((n, 0)) => pass(format!("holdouts: {n} of {n} green")),
        Ok((n, k)) => {
            let reason = format!("holdouts: {k} of {n} red");
            force_back(
                ctx,
                "holdout",
                &task,
                "ready-for-agent",
                &reason,
                &format!("chore({task}): harness holdout gate rejected a done verdict"),
                &format!("{task} was forced back to ready by the holdout gate: {reason}."),
            )
        }
    }
}

// a heading an agent deletes mid-edit leaves a block nothing counts: its body merges into its
// neighbour and archive moves both out, so the id is gone with the tree still consistent (T-020, 9c0b4af)
// a lane that sets its own author puts an identity the operator never chose into their history
fn commit_identity(ctx: &mut GateCtx) -> GateOutcome {
    let Some(base) = ctx.iter_base.clone().filter(|b| !b.is_empty()) else {
        return pass("no base");
    };
    let Ok(want) = git(ctx.root, &["config", "--get", "user.email"]) else {
        return pass("the repository configures no user.email");
    };
    if want.is_empty() {
        return pass("the repository configures no user.email");
    }
    let range = format!("{base}..HEAD");
    // a merge commit carries an author too, so it is read like any other
    let log = match git(ctx.root, &["log", "--format=%h %ae", &range]) {
        Ok(log) => log,
        Err(e) => return fail(e.to_string()),
    };
    let strangers: Vec<String> = log
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(_, email)| *email != want)
        .map(|(sha, email)| format!("{sha} by {email}"))
        .collect();
    if strangers.is_empty() {
        return pass(format!("every commit on {range} is {want}"));
    }
    fail(format!(
        "the repository commits as {want}, and these do not: {}",
        strangers.join(", ")
    ))
}

// an empty commit touches no file, so the identity and scope gates both pass it
fn commit_task(ctx: &mut GateCtx) -> GateOutcome {
    let Some(base) = ctx.iter_base.clone().filter(|b| !b.is_empty()) else {
        return pass("no base");
    };
    let id = regex::Regex::new(r"\bT-[0-9]+\b").expect("task id pattern");
    let state: Vec<String> = BOOKKEEPING.iter().map(|name| rel(ctx, name)).collect();
    let skills_dir = format!("{}/", skills_dir_for(ctx.cfg));
    // the launcher's own state and vendoring commits name no task, and touch nothing a lane writes
    let launcher = |files: &[&str]| {
        !files.is_empty()
            && files.iter().all(|f| {
                state
                    .iter()
                    .any(|s| f == s || f.starts_with(&format!("{s}/")))
                    || is_harness_path(ctx.cfg, f)
                    || f.starts_with(&skills_dir)
            })
    };
    let range = format!("{base}..HEAD");
    let log = match git(ctx.root, &["log", "--no-merges", "--format=%h %s", &range]) {
        Ok(log) => log,
        Err(e) => return fail(e.to_string()),
    };
    let unnamed: Vec<String> = log
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(_, subject)| !id.is_match(subject))
        .filter(|(sha, _)| {
            let files =
                git(ctx.root, &["show", "--format=", "--name-only", sha]).unwrap_or_default();
            !launcher(&files.lines().filter(|f| !f.is_empty()).collect::<Vec<_>>())
        })
        .map(|(sha, subject)| format!("{sha} {subject}"))
        .collect();
    if unnamed.is_empty() {
        return pass(format!("every commit on {range} names a task"));
    }
    fail(format!(
        "these commits on {range} name no task id: {}",
        unnamed.join(", ")
    ))
}

fn queue_intact(ctx: &mut GateCtx) -> GateOutcome {
    let Some(base) = ctx.iter_base.clone().filter(|b| !b.is_empty()) else {
        return pass("no base");
    };
    let before: Vec<String> = blocks_at_base(ctx)
        .unwrap_or_default()
        .into_iter()
        .map(|b| b.id)
        .collect();
    if before.is_empty() {
        return pass("no ids at base");
    }
    let q = queue_of(ctx);
    let Ok(now) = q.blocks() else {
        return pass("the queue is unreadable; queue-hygiene owns that");
    };
    let Tracker(path) = q;
    let tasks = path
        .strip_prefix(ctx.root)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    // a kill is wontfix, never a deletion, so DECISIONS.md accounts for nothing
    let lost: Vec<String> = before
        .into_iter()
        .filter(|id| !now.iter().any(|b| &b.id == id))
        .collect();
    if lost.is_empty() {
        return pass("every ticket at the base still has its file");
    }
    let (_, inner, nested) = git::locate(ctx.root, &ctx.cfg.layout.harness_dir, &tasks);
    let show = if nested {
        format!("git -C {} show", ctx.cfg.layout.harness_dir)
    } else {
        "git show".to_string()
    };
    let base = at_base(ctx, &tasks).map(|(_, _, b)| b).unwrap_or(base);
    let reason = format!(
        "{} lost its ticket file; a kill is wontfix, never a deletion; recover with `{show} {base}:{inner}`",
        lost.join(", "),
    );
    halted(ctx, reason)
}

// the decompose stage names a spec, not a ticket, so its slug rides in ctx.task
fn tickets(ctx: &mut GateCtx) -> GateOutcome {
    let Some(slug) = ctx.task.clone() else {
        return pass("no spec");
    };
    let problems = tracker::check(ctx.root, ctx.cfg, &slug);
    if !problems.is_empty() {
        return halted(
            ctx,
            format!(
                "tickets: the drafts of {slug} fail `enallagi tasks check {slug}`: {}",
                problems.join("; ")
            ),
        );
    }
    let q = queue_of(ctx);
    let decisions =
        std::fs::read_to_string(ctx.root.join(rel(ctx, "DECISIONS.md"))).unwrap_or_default();
    let archived = block::parse(&decisions).unwrap_or_default();
    let numbered = q
        .next_id(&archived)
        .map_err(|e| e.to_string())
        .and_then(|next| {
            q.number_drafts(&slug, next[2..].parse().unwrap_or(1))
                .map_err(|e| e.to_string())
        });
    match numbered {
        Ok(ids) => commit(
            ctx,
            &[SPECS],
            &format!("decompose {slug}: {} at needs-triage", ids.join(", ")),
        ),
        Err(e) => halted(ctx, format!("tickets: {slug}: {e}")),
    }
}

fn halted(ctx: &mut GateCtx, reason: String) -> GateOutcome {
    ctx.warnings.push(reason.clone());
    ctx.halts.push(reason.clone());
    GateOutcome {
        pass: false,
        reason,
        halt: true,
        ..GateOutcome::default()
    }
}

// a leak is never handed back to a lane: the history rewrite it needs is a person's (ADR 0009)
fn security(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }
    let Some(base) = ctx.iter_base.clone().filter(|b| !b.is_empty()) else {
        return pass("no base");
    };
    let leaks = match gitleaks(ctx, &base) {
        Ok(leaks) => leaks,
        Err(e) => return halted(ctx, format!("security: {e}")),
    };
    if leaks.is_empty() {
        return match advisories(ctx, &base, &task) {
            Err(e) => halted(ctx, format!("security: {e}")),
            Ok(new) if new.is_empty() => pass(format!("{task} added no secret and no advisory")),
            Ok(new) => {
                let why = format!(
                    "a dependency change adds an advisory the base did not carry: {}; bump past it",
                    new.join(", ")
                );
                force_back(
                    ctx,
                    "security",
                    &task,
                    "ready-for-agent",
                    &format!("the verifier returned done and the security gate rejected it: {why}"),
                    &format!("chore({task}): harness security gate rejected a done verdict"),
                    &format!("{task} was forced back to ready by the security gate: {why}."),
                )
            }
        };
    }
    let why = format!(
        "a secret in the task's commits: {}; the history needs rewriting by hand",
        leaks.join(", ")
    );
    let out = force_back(
        ctx,
        "security",
        &task,
        "ready-for-human",
        &format!("the verifier returned done and the security gate held it: {why}"),
        &format!("chore({task}): harness security gate held a leaked secret"),
        &format!("{task} was set to ready-for-human by the security gate: {why}."),
    );
    halted(ctx, out.reason)
}

// only rule, file, line and commit leave the report: events.jsonl and the ticket are committed
fn gitleaks(ctx: &GateCtx, base: &str) -> Result<Vec<String>, String> {
    let dir = &ctx.cfg.layout.harness_dir;
    let baseline = crate::config::instance_path(ctx.root, dir, crate::security::BASELINE);
    if !baseline.is_file() {
        return Err(format!(
            "no secrets baseline at {}; run `enallagi init` to record one",
            baseline.display()
        ));
    }
    let reports = ctx.cache.join("security");
    let io = |e: std::io::Error| format!("{}: {e}", reports.display());
    std::fs::create_dir_all(&reports).map_err(io)?;
    let scratch = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&reports)
        .map_err(io)?;
    let args = [
        format!("--log-opts={base}..HEAD"),
        "--baseline-path".to_string(),
        baseline.display().to_string(),
    ];
    let report = scratch.path().join("gitleaks.json");
    let findings = crate::security::gitleaks(ctx.root, dir, &ctx.cache, &args, &report)?;
    Ok(findings
        .iter()
        .map(|f| {
            let s = |k: &str| f[k].as_str().unwrap_or("?").to_string();
            format!(
                "{} {}:{} {}",
                s("RuleID"),
                s("File"),
                f["StartLine"],
                s("Commit")
            )
        })
        .collect())
}

// an advisory charged to the lane is one its lockfile change brought: present at HEAD, not at base
fn advisories(ctx: &GateCtx, base: &str, task: &str) -> Result<Vec<String>, String> {
    let locks: Vec<String> = task_files(ctx, base, task, "")
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|f| {
            let name = Path::new(f).file_name().and_then(|n| n.to_str());
            name.is_some_and(|n| crate::security::LOCKFILES.contains(&n))
                && ctx.root.join(f).is_file()
        })
        .collect();
    if locks.is_empty() {
        return Ok(Vec::new());
    }
    let dir = &ctx.cfg.layout.harness_dir;
    let reports = ctx.cache.join("security");
    let io = |e: std::io::Error| format!("{}: {e}", reports.display());
    std::fs::create_dir_all(&reports).map_err(io)?;
    let scratch = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&reports)
        .map_err(io)?;
    let mut new = Vec::new();
    for (i, f) in locks.iter().enumerate() {
        let out = |side: &str| scratch.path().join(format!("{side}-{i}.json"));
        let osv = |lock: &Path, report: PathBuf| {
            crate::security::osv(ctx.root, dir, &ctx.cache, lock, &report)
        };
        let head = osv(&ctx.root.join(f), out("head"))?;
        // the same file name, since osv-scanner picks its parser by name
        let at_base = match git(ctx.root, &["show", &format!("{base}:{f}")]) {
            Ok(text) => {
                let copy = scratch
                    .path()
                    .join(format!("base-{i}"))
                    .join(Path::new(f).file_name().unwrap_or_default());
                std::fs::create_dir_all(copy.parent().unwrap_or(scratch.path())).map_err(io)?;
                std::fs::write(&copy, text).map_err(io)?;
                osv(&copy, out("base"))?
            }
            Err(_) => Vec::new(),
        };
        for (name, version, id) in head {
            if !at_base.iter().any(|(n, _, a)| *n == name && *a == id) {
                new.push(format!("{f}: {name} {version} {id}"));
            }
        }
    }
    Ok(new)
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

pub(crate) fn scope_globs(line: &str) -> Vec<String> {
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

// conservative only because in_scope lets `*` cross `/`: two globs overlap unless their literal prefixes
// diverge, and a byte prefix counts across a path component (`src/a` vs `src/ab.rs`)
pub fn overlaps(a: &[String], b: &[String]) -> bool {
    let prefix = |g: &String| g[..g.find(['*', '?', '[', '{']).unwrap_or(g.len())].to_string();
    a.is_empty()
        || b.is_empty()
        || a.iter().any(|x| {
            b.iter().any(|y| {
                let (p, q) = (prefix(x), prefix(y));
                p.starts_with(&q) || q.starts_with(&p)
            })
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
    /// The halt reason when the check ran past `[check] timeout` or a signal stopped it. Neither red nor green.
    pub timed_out: Option<String>,
    pub tally: Tally,
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

    // a failure the exit hid, or a check that ran and printed no count, is a number nobody can quote.
    // `counts` is whether this repository's check prints a count the tally reads at all
    pub fn tally_warning(&self, command: &str, counts: bool) -> Option<String> {
        if self.timed_out.is_some() || self.output.starts_with(NEVER_RAN) {
            return None;
        }
        let t = self.tally;
        if t.failed > 0 && self.exit == 0 {
            return Some(format!(
                "the check exited 0 with {} failed over {} count lines: `{command}`",
                t.failed, t.lines
            ));
        }
        (counts && t.lines == 0).then(|| format!("the check printed no count line: `{command}`"))
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
    stopped: Option<&'static str>,
}

const POLL: Duration = Duration::from_millis(100);
// how long a SIGTERM gets to be honoured before the group is killed outright
const GRACE: Duration = Duration::from_secs(10);
// how long a killed group's pipes get to close before an unfinished reader is left detached
const DRAIN: Duration = Duration::from_secs(1);

// the check gets its own process group so a build tool's children die with it, not with the shell alone
fn run_bounded(
    sandbox: &crate::sandbox::Sandbox,
    root: &Path,
    command: &str,
    timeout: Duration,
) -> std::io::Result<Run> {
    run_wrapped(&sandbox.wrap(&["sh", "-c", command], root, &[]), timeout)
}

fn run_wrapped(wrapped: &crate::sandbox::Wrapped, timeout: Duration) -> std::io::Result<Run> {
    use std::os::unix::process::CommandExt;
    let mut child = wrapped
        .command()
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
    let mut stopped = None;
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
        stopped = crate::agent::stop_signal();
        if stopped.is_some() || started.elapsed() >= timeout {
            timed_out = stopped.is_none();
            break match exited {
                Some(status) => {
                    signal(libc::SIGKILL, pgid);
                    status
                }
                None => kill_group(&mut child, pgid)?,
            };
        }
        std::thread::sleep(POLL);
    };
    // a process that left the group with setsid never got the KILL and can hold the pipe forever
    let drained = Instant::now();
    while !readers.iter().all(|r| r.is_finished()) && drained.elapsed() < DRAIN {
        std::thread::sleep(POLL);
    }
    for reader in readers.into_iter().filter(|r| r.is_finished()) {
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
        stopped,
    })
}

pub(crate) fn kill_group(
    child: &mut std::process::Child,
    pgid: u32,
) -> std::io::Result<ExitStatus> {
    signal(libc::SIGTERM, pgid);
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
    // the leader exiting says nothing about a grandchild it left behind
    signal(libc::SIGKILL, pgid);
    Ok(status)
}

// `kill -TERM -<pgid>` is not portable: a GNU userland reads the negative pgid as a signal number
fn signal(sig: i32, pgid: u32) -> bool {
    let group = -i32::try_from(pgid).unwrap_or(i32::MAX);
    unsafe { libc::kill(group, sig) == 0 }
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

pub fn tally(output: &str, pattern: &str) -> Tally {
    if !pattern.is_empty() {
        return tally_by(output, pattern);
    }
    let mut t = Tally::default();
    for line in output.lines().filter(|l| l.starts_with("test result:")) {
        t.lines += 1;
        let words: Vec<&str> = line.split_whitespace().collect();
        for pair in words.windows(2) {
            let Ok(n) = pair[0].parse::<u64>() else {
                continue;
            };
            match pair[1].trim_end_matches([';', ',']) {
                "passed" => t.passed += n,
                "failed" => t.failed += n,
                "ignored" => t.ignored += n,
                _ => {}
            }
        }
    }
    t
}

// a pattern that doesn't compile counts nothing, and the gate records no tally rather than a zero
fn tally_by(output: &str, pattern: &str) -> Tally {
    let mut t = Tally::default();
    let Ok(re) = regex::Regex::new(pattern) else {
        return t;
    };
    let count = |c: &regex::Captures, name: &str| {
        c.name(name)
            .and_then(|m| m.as_str().parse::<u64>().ok())
            .unwrap_or(0)
    };
    for line in output.lines() {
        let mut matched = false;
        for c in re.captures_iter(line) {
            matched = true;
            t.passed += count(&c, "passed");
            t.failed += count(&c, "failed");
        }
        t.lines += u64::from(matched);
    }
    t
}

// the gate's copy of the tally, and the digest's warning when the tally contradicts the exit
// the runner whose summary `tally` parses, named by the fail_name the preset pins
fn counts_test_results(cfg: &Config) -> bool {
    !cfg.check.tally.is_empty()
        || crate::runners::presets()
            .into_iter()
            .any(|r| r.name == "cargo" && r.fail_name == cfg.check.fail_name)
}

fn counted(ctx: &mut GateCtx, report: &CheckReport) -> Option<Tally> {
    let counts = counts_test_results(ctx.cfg);
    if let Some(warning) = report.tally_warning(&ctx.cfg.check.command, counts) {
        ctx.warnings.push(warning);
    }
    // a runner with no count to read has no tally, not a tally of zero
    counts.then_some(report.tally)
}

pub fn check_delta(
    sandbox: &crate::sandbox::Sandbox,
    root: &Path,
    cfg: &Config,
    force: bool,
) -> CheckReport {
    let command = if force {
        &cfg.check.force
    } else {
        &cfg.check.command
    };
    let timeout = cfg
        .check
        .timeout_duration()
        .unwrap_or(crate::config::DEFAULT_CHECK_TIMEOUT);
    let mark = sandbox.denials_since(0).len();
    let run = match run_bounded(sandbox, root, command, timeout) {
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
    let hang = hang_reason("the check", command, timeout, &run);
    let Run { exit, output, .. } = run;
    // a runner forced into colour would otherwise need every fail_name to spell out its escape codes
    let mut output = strip_ansi(&output);
    let tally = tally(&output, &cfg.check.tally);
    if let Some(reason) = hang {
        return CheckReport {
            output: format!("{reason}\n{output}"),
            exit,
            timed_out: Some(reason),
            tally,
            ..CheckReport::default()
        };
    }
    if exit == 0 {
        return CheckReport {
            output,
            exit,
            tally,
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
    // after the names, so a refusal line never reads as a failing test
    for d in sandbox.denials_since(mark) {
        output.push_str(&format!(
            "\nsandbox refused {}:{} ({})",
            d.host, d.port, d.reason
        ));
    }

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
        tally,
    }
}

/// The text with its ANSI colour and cursor sequences removed.
pub fn strip_ansi(text: &str) -> String {
    static ANSI: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    ANSI.get_or_init(|| {
        regex::Regex::new(
            r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]",
        )
        .expect("literal pattern")
    })
    .replace_all(text, "")
    .into_owned()
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

fn hang_reason(what: &str, command: &str, timeout: Duration, run: &Run) -> Option<String> {
    match run.stopped {
        Some(signal) => Some(format!(
            "{what} was stopped by {signal} and its process group was killed: `{command}`"
        )),
        None if run.timed_out => Some(format!(
            "{what} ran past {}s and its process group was killed: `{command}`",
            timeout.as_secs()
        )),
        None => None,
    }
}

// a hang is not a failing test: a red would send the task back to ready and buy the same hang again
fn hung(ctx: &mut GateCtx, report: &CheckReport) -> Option<GateOutcome> {
    let reason = report.timed_out.clone()?;
    ctx.halts.push(reason.clone());
    // the round's boundary would name the signal, but a halted gate never reaches it
    let halt = match crate::agent::stop_signal() {
        Some(_) => "signal".to_string(),
        None => ctx.task.clone().unwrap_or_else(|| "check".to_string()),
    };
    ctx.events.emit(Kind::Halt {
        halt,
        reason: reason.clone(),
    });
    Some(GateOutcome {
        pass: false,
        reason,
        skip_rest: true,
        halt: true,
        tally: None,
    })
}

fn check_gate(ctx: &mut GateCtx) -> GateOutcome {
    let report = check_delta(ctx.sandbox, ctx.root, ctx.cfg, false);
    if let Some(halt) = hung(ctx, &report) {
        return halt;
    }
    let tally = counted(ctx, &report);
    GateOutcome {
        tally,
        ..judged(&report)
    }
}

fn judged(report: &CheckReport) -> GateOutcome {
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

// the probe answers from the templates its own binary compiled in, so the gate spawns the binary
// the check just rebuilt instead of reading them in this process
fn install_stale(ctx: &mut GateCtx) -> GateOutcome {
    let spawned = std::env::current_exe()
        .map_err(|e| e.to_string())
        .and_then(|exe| {
            ctx.sandbox
                .wrap(
                    &[&exe.to_string_lossy(), "probe", "install-stale"],
                    ctx.root,
                    &[],
                )
                .command()
                .output()
                .map_err(|e| e.to_string())
        });
    let text = match spawned {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Err(err) => return fail(format!("`enallagi probe install-stale` did not run: {err}")),
    };
    let first = match stale_path(&text) {
        Err(reason) => return fail(reason),
        Ok(None) => return pass("`enallagi probe install-stale` -> PROBE install-stale 0"),
        Ok(Some(path)) => path,
    };
    let reason = format!("the install is stale at {first}; re-run `enallagi init`");
    ctx.warnings.push(reason.clone());
    fail(reason)
}

// an absent count is never a zero: the probe has to have said what it read
fn stale_path(text: &str) -> Result<Option<String>, String> {
    let counted = text
        .lines()
        .find_map(|l| l.strip_prefix("PROBE install-stale "))
        .and_then(|rest| rest.trim().parse::<usize>().ok());
    let Some(count) = counted else {
        return Err(format!(
            "`enallagi probe install-stale` printed no count: {}",
            text.replace('\n', " ").trim()
        ));
    };
    if count == 0 {
        return Ok(None);
    }
    // an installed path may hold spaces, so it ends at the `:line` the probe printed, not at a space
    let at_line = regex::Regex::new(r"^(.+?):[0-9]+( |$)").ok();
    Ok(Some(
        text.lines()
            .find_map(|l| l.strip_prefix("FINDING install-stale "))
            .zip(at_line)
            .and_then(|(rest, at)| Some(at.captures(rest)?[1].to_string()))
            .unwrap_or_else(|| format!("{count} file(s) the FINDING lines did not name")),
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
        tally: None,
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
    match queue_of(ctx)
        .blocks()
        .ok()
        .and_then(|b| tracker::takeable(&b))
    {
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
        cache: tempfile::TempDir,
        sandbox: crate::sandbox::Sandbox,
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
                ".enallagi/enallagi.toml",
                &format!(
                    "[check]\ncommand = \"{cmd}\"\ntimeout = \"{timeout}\"\nfail_name = '\\(fail\\) (.+)$'\n"
                ),
            );
            repo.commit_all("harness");
            let cfg = crate::config::load(&repo.root).expect("load enallagi.toml");
            let writer = Writer::new(Log::open(&repo.root.join(".enallagi")));
            Env {
                writer,
                stage_output: String::new(),
                warnings: Vec::new(),
                halts: Vec::new(),
                dry_rounds: 0,
                cache: tempfile::tempdir().expect("cache"),
                sandbox: crate::sandbox::Sandbox::start(&repo.root, &cfg),
                repo,
                cfg,
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
                cache: self.cache.path().to_path_buf(),
                sandbox: &self.sandbox,
            }
        }

        // a done carries the verifier's verdict, as a lane's would
        fn queue(&self, status: &str, scope: &str, rows: &str) {
            let verdict = match status {
                "done" => "verdict: verified by verifier\nspec: none\nstandards: none\n",
                _ => "",
            };
            self.write_queue(&format!(
                "## [T-001] first\nscope: {scope}\nrows: {rows}\nblockedBy: none\nstatus: {status}\n{verdict}"
            ));
        }

        fn write_queue(&self, text: &str) {
            self.repo.queue(".enallagi", text);
        }

        // two files under one id, which append refuses, so the second is written by hand
        fn duplicate_queue(&self, status: &str) {
            let text = |title: &str| format!("## [T-001] {title}\nstatus: {status}\n");
            self.write_queue(&text("first"));
            let first = self.tracker().path_of("T-001").expect("T-001");
            let again = &block::parse(&text("again")).expect("parse")[0];
            std::fs::write(
                first.with_file_name(tracker::file_name(&again.id, &again.title)),
                tracker::ticket_text(again),
            )
            .expect("write the duplicate");
        }

        fn tracker(&self) -> Tracker {
            Tracker::open(&self.repo.root, &self.cfg.layout.harness_dir)
        }

        fn instance(&self, name: &str) -> String {
            format!(".enallagi/{name}")
        }

        fn tasks_text(&self) -> String {
            let blocks = self.tracker().blocks().expect("read the queue");
            blocks
                .iter()
                .map(block::block_text)
                .collect::<Vec<_>>()
                .join("\n\n")
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
                Kind::TaskStatus { to, by, .. } if to == "ready-for-agent" && by == "scope")));
            // the reason rides in the block, where the next implement stage reads it first
            assert!(
                self.tasks_text()
                    .contains("gate: the verifier returned done and the scope gate rejected it"),
                "{}",
                self.tasks_text()
            );
        }
    }

    fn draft(num: &str, blocked: &str) -> String {
        format!("# {num}: Step {num}\n\n**What to build:** step {num}.\n\n**Blocked by:** {blocked}\n\n**Status:** ready-for-agent\n\n- [ ] step {num} shows\n")
    }

    // T-001 queued, an accepted spec `app`, and its drafts left uncommitted as a decomposer leaves them
    fn drafted(drafts: &[(&str, &str)]) -> Env {
        let env = Env::new("exit 0\n");
        env.write_queue("## [T-001] open\nstatus: ready-for-agent\n");
        env.repo.write(
            ".enallagi/specs/app/spec.md",
            "# App\n\n**Status:** ready-for-agent\n",
        );
        env.repo.commit_all("spec");
        for (num, blocked) in drafts {
            env.repo.write(
                &format!(".enallagi/specs/app/issues/{num}-step.md"),
                &draft(num, blocked),
            );
        }
        env
    }

    #[test]
    fn tickets_numbers_drafts() {
        let mut env = drafted(&[("01", "None"), ("02", "None"), ("03", "01, 02")]);
        let out = run("tickets", &mut env.ctx(Some("app"), None));
        assert!(out.pass, "{out:?}");
        let t = env.tracker();
        let blocks = t.blocks().unwrap();
        let ids: Vec<&str> = blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["T-001", "T-002", "T-003", "T-004"]);
        for b in &blocks[1..] {
            assert_eq!(tracker::state(b).as_deref(), Some("needs-triage"), "{b:?}");
            assert_eq!(
                block::field(b, "probe").as_deref(),
                Some("decompose"),
                "{b:?}"
            );
            assert!(block::field(b, "criteria").is_some(), "{b:?}");
        }
        assert_eq!(block::blockers(&blocks[3]), ["T-002", "T-003"]);
        assert!(t
            .path_of("T-004")
            .unwrap()
            .ends_with("app/issues/T-004-step.md"));
        assert!(t.drafts("app").unwrap().is_empty());
        assert!(
            env.log()
                .contains("decompose app: T-002, T-003, T-004 at needs-triage"),
            "{}",
            env.log()
        );
    }

    #[test]
    fn tickets_halts_on_cycle() {
        let mut env = drafted(&[("01", "02"), ("02", "01")]);
        let issues = env.repo.root.join(".enallagi/specs/app/issues");
        let before = std::fs::read_to_string(issues.join("01-step.md")).unwrap();
        let out = run("tickets", &mut env.ctx(Some("app"), None));
        assert!(!out.pass && out.halt, "{out:?}");
        assert!(
            out.reason.contains("blocking cycle through 01, 02"),
            "{out:?}"
        );
        assert_eq!(env.halts, vec![out.reason]);
        assert_eq!(
            std::fs::read_to_string(issues.join("01-step.md")).unwrap(),
            before
        );
        assert_eq!(env.tracker().drafts("app").unwrap().len(), 2);
        assert_eq!(env.tracker().blocks().unwrap().len(), 1);
    }

    fn globs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_commit_by_a_stranger_is_refused() {
        let mut env = Env::new("exit 0\n");
        let base = head(&env.repo.root).expect("base");
        env.repo.write("src/schema.ts", "export const x = 2\n");
        git(
            &env.repo.root,
            &[
                "-c",
                "user.email=elsewhere@example.test",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qam",
                "T-001 by someone else",
            ],
        )
        .expect("a commit by someone else");
        let mut ctx = env.ctx(Some("T-001"), Some(&base));
        let out = commit_identity(&mut ctx);
        assert!(!out.pass, "{}", out.reason);
        assert!(
            out.reason.contains("elsewhere@example.test") && out.reason.contains("t@t"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn a_merge_commit_by_a_stranger_is_refused() {
        let mut env = Env::new("exit 0\n");
        let base = head(&env.repo.root).expect("base");
        git(&env.repo.root, &["checkout", "-q", "-b", "side"]).expect("branch");
        env.repo.write("src/other.ts", "export const y = 1\n");
        env.repo.commit_all("T-001 on the side");
        git(&env.repo.root, &["checkout", "-q", "-"]).expect("back");
        env.repo.write("src/schema.ts", "export const x = 4\n");
        env.repo.commit_all("T-001 on the trunk");
        git(
            &env.repo.root,
            &[
                "-c",
                "user.email=elsewhere@example.test",
                "-c",
                "commit.gpgsign=false",
                "merge",
                "--no-ff",
                "-q",
                "-m",
                "T-001 merge",
                "side",
            ],
        )
        .expect("a merge by someone else");
        let mut ctx = env.ctx(Some("T-001"), Some(&base));
        let out = commit_identity(&mut ctx);
        assert!(!out.pass, "{}", out.reason);
        assert!(
            out.reason.contains("elsewhere@example.test"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn the_repositorys_own_commits_pass() {
        let mut env = Env::new("exit 0\n");
        let base = head(&env.repo.root).expect("base");
        env.repo.write("src/schema.ts", "export const x = 3\n");
        env.repo.commit_all("T-001 by the repository");
        let mut ctx = env.ctx(Some("T-001"), Some(&base));
        let out = commit_identity(&mut ctx);
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn a_commit_naming_no_task_is_refused() {
        let mut env = Env::new("exit 0\n");
        let base = head(&env.repo.root).expect("base");
        env.repo.write("src/schema.ts", "export const x = 5\n");
        env.repo.commit_all("feat(schema): T-001 bump x");
        git(
            &env.repo.root,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "chore(release): v1",
            ],
        )
        .expect("an empty commit");
        let stray = git(&env.repo.root, &["rev-parse", "--short", "HEAD"]).expect("sha");
        let mut ctx = env.ctx(Some("T-001"), Some(&base));
        let out = commit_task(&mut ctx);
        assert!(!out.pass, "{}", out.reason);
        assert!(
            out.reason.contains(&stray) && out.reason.contains("chore(release): v1"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn commit_task_fails_on_a_bad_base() {
        let mut env = Env::new("exit 0\n");
        let out = commit_task(&mut env.ctx(Some("T-001"), Some("deadbeef")));
        assert!(
            !out.pass && out.reason.contains("deadbeef"),
            "{}",
            out.reason
        );
        assert!(!out.reason.contains("every commit on"), "{}", out.reason);
    }

    #[test]
    fn commit_identity_fails_on_a_bad_base() {
        let mut env = Env::new("exit 0\n");
        let out = commit_identity(&mut env.ctx(Some("T-001"), Some("deadbeef")));
        assert!(
            !out.pass && out.reason.contains("deadbeef"),
            "{}",
            out.reason
        );
        assert!(!out.reason.contains("every commit on"), "{}", out.reason);
    }

    #[test]
    fn scope_fails_on_a_bad_base() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        let out = run("scope", &mut env.ctx(Some("T-001"), Some("deadbeef")));
        assert!(
            !out.pass && out.reason.contains("deadbeef"),
            "{}",
            out.reason
        );
    }

    // src/lib.txt changes and tests/t.sh is the lane's test; the check runs it
    fn red_first_env(test: &str, check: &str, timeout: &str) -> (Env, String) {
        let mut env = Env::timed(check, timeout);
        env.cfg.layout.test_only_glob = globs(&["**/tests/**"]);
        env.repo.write("src/lib.txt", "old\n");
        env.queue("done", "src/* tests/*", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write("src/lib.txt", "new\n");
        env.repo.write("tests/t.sh", test);
        env.repo.commit_all("T-001 change");
        (env, base)
    }

    const RUN_TEST: &str = "[ -f tests/t.sh ] && sh tests/t.sh\n";

    #[test]
    fn red_first_rejects_a_test_green_on_base() {
        let (mut env, base) = red_first_env("true\n", RUN_TEST, "30m");
        let out = run("red-first", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass, "{}", out.reason);
        assert!(
            out.reason.contains("tests/t.sh") && out.reason.contains(&base),
            "{}",
            out.reason
        );
        assert!(env.tasks_text().contains("status: ready-for-agent"));
    }

    #[test]
    fn red_first_passes_a_test_red_on_base() {
        let (mut env, base) = red_first_env("grep -q new src/lib.txt\n", RUN_TEST, "30m");
        let out = run("red-first", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn red_first_leaves_the_lane_clean() {
        let (mut env, base) = red_first_env("true\n", "sleep 30\n", "1s");
        let trees = |env: &Env| git(&env.repo.root, &["worktree", "list"]).expect("list");
        let (status, before) = (porcelain(&env.repo.root), trees(&env));
        run("red-first", &mut env.ctx(Some("T-001"), Some(&base)));
        assert_eq!(porcelain(&env.repo.root), status);
        assert_eq!(trees(&env), before);
    }

    #[test]
    fn red_first_without_tests() {
        let (mut env, base) = red_first_env("true\n", RUN_TEST, "30m");
        env.cfg.layout.test_only_glob = globs(&["**/spec/**"]);
        let out = run("red-first", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(
            out.pass && out.reason == "no test-only file changed",
            "{}",
            out.reason
        );
        env.cfg.layout.test_only_glob.clear();
        let out = run("red-first", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(
            out.pass && out.reason == "test_only_glob is empty",
            "{}",
            out.reason
        );
    }

    fn skip_env(line: &str) -> (Env, String) {
        let mut env = Env::new("exit 0\n");
        env.cfg.layout.skip_patterns = globs(&[r"^\s*#\[ignore\]"]);
        env.cfg.layout.test_glob = globs(&["tests/*.rs"]);
        env.repo.write("tests/a.rs", "#[test]\nfn a() {}\n");
        env.queue("done", "tests/*", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write("tests/a.rs", &format!("#[test]\n{line}\nfn a() {{}}\n"));
        env.repo.commit_all("T-001 change");
        (env, base)
    }

    #[test]
    fn skip_added_rejects_a_bare_ignore() {
        let (mut env, base) = skip_env("#[ignore]");
        let out = run("skip-added", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass, "{}", out.reason);
        assert!(
            out.reason.contains("tests/a.rs:2 #[ignore]"),
            "{}",
            out.reason
        );
        assert!(env.tasks_text().contains("status: ready-for-agent"));
    }

    const LEAK: &str = r#"[{"RuleID":"aws-access-token","File":"src/key.txt","StartLine":1,"Commit":"abc123","Secret":"AKIAHIDDEN0SECRET99","Match":"key = AKIAHIDDEN0SECRET99"}]"#;

    // a done task with an empty baseline committed
    fn leak_env(report: &str, exit: i32) -> (Env, String) {
        let env = Env::new("exit 0\n");
        crate::fixture::stub_gitleaks(env.cache.path(), report, exit);
        env.repo
            .write(".enallagi/security/gitleaks-baseline.json", "[]\n");
        env.queue("done", "src/*", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write("src/key.txt", "key\n");
        env.repo.commit_all("T-001 change");
        (env, base)
    }

    #[test]
    fn security_holds_leak() {
        let (mut env, base) = leak_env(LEAK, 1);
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass && out.halt, "{}", out.reason);
        assert!(
            out.reason.contains("aws-access-token src/key.txt:1 abc123"),
            "{}",
            out.reason
        );
        assert!(env.tasks_text().contains("status: ready-for-human"));
        assert_eq!(env.halts, vec![out.reason]);
    }

    #[test]
    fn reason_hides_secret() {
        let (mut env, base) = leak_env(LEAK, 1);
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        let events =
            std::fs::read_to_string(env.repo.root.join(".enallagi/events.jsonl")).expect("events");
        assert!(events.contains("aws-access-token"), "{events}");
        for text in [&out.reason, &events, &env.tasks_text()] {
            assert!(!text.contains("AKIAHIDDEN0SECRET99"), "{text}");
        }
    }

    #[test]
    fn security_runs_gitleaks_redacted() {
        let (mut env, base) = leak_env("[]", 0);
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        let argv = std::fs::read_to_string(env.cache.path().join("gitleaks.argv")).expect("argv");
        let argv: Vec<&str> = argv.lines().collect();
        let dir = env.repo.root.join(".enallagi");
        let after = |flag: &str| argv.iter().position(|a| *a == flag).map(|i| argv[i + 1]);
        assert_eq!(argv[0], "git");
        assert!(
            argv.contains(&format!("--log-opts={base}..HEAD").as_str()),
            "{argv:?}"
        );
        assert!(argv.contains(&"--redact"), "{argv:?}");
        assert_eq!(after("--report-format"), Some("json"));
        let config = dir.join("gitleaks.toml").display().to_string();
        assert_eq!(after("--config"), Some(config.as_str()));
        let ignore = dir.join("gitleaksignore").display().to_string();
        assert_eq!(after("--gitleaks-ignore-path"), Some(ignore.as_str()));
        let report = after("--report-path").expect("--report-path");
        assert!(
            Path::new(report).starts_with(env.cache.path().join("security")),
            "{report}"
        );
    }

    #[test]
    fn security_baselined_leak() {
        let (mut env, base) = leak_env(LEAK, 1);
        let baseline = env
            .repo
            .root
            .join(".enallagi/security/gitleaks-baseline.json");
        std::fs::write(&baseline, LEAK).expect("baseline");
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        let argv = std::fs::read_to_string(env.cache.path().join("gitleaks.argv")).expect("argv");
        assert!(
            argv.contains(&format!("--baseline-path\n{}\n", baseline.display())),
            "{argv}"
        );
    }

    #[test]
    fn missing_baseline_halts() {
        let (mut env, base) = leak_env("[]", 0);
        std::fs::remove_dir_all(env.repo.root.join(".enallagi/security")).expect("rm");
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass && out.halt, "{}", out.reason);
        assert!(out.reason.contains("enallagi init"), "{}", out.reason);
    }

    // records its argv and reports GHSA-test-0001 on `bad 1.0` for a lockfile that names `bad`
    fn stub_osv(env: &Env) {
        let cache = env.cache.path();
        let pin = crate::security::default_pins()
            .into_iter()
            .find(|p| p.id == "osv-scanner" && p.platform == crate::security::platform())
            .expect("an osv-scanner pin for this platform");
        let c = cache.display();
        std::fs::write(
            crate::security::tool_path(&pin, cache),
            format!(
                r#"#!/bin/sh
printf '%s\n' "$@" >> {c}/osv.argv
lock= out=
while [ $# -gt 0 ]; do
  case "$1" in
    --lockfile) lock=$2 ;;
    --output) out=$2 ;;
  esac
  shift
done
if grep -q bad "$lock"; then
  echo '{{"results":[{{"packages":[{{"package":{{"name":"bad","version":"1.0"}},"vulnerabilities":[{{"id":"GHSA-test-0001"}}]}}]}}]}}' > "$out"
  exit 1
fi
echo '{{"results":[]}}' > "$out"
"#
            ),
        )
        .expect("stub osv-scanner");
    }

    // Cargo.lock holds `base_lock` at base and `head_lock` at HEAD
    fn osv_env(base_lock: &str, head_lock: &str) -> (Env, String) {
        let (env, _) = leak_env("[]", 0);
        stub_osv(&env);
        env.repo.write("Cargo.lock", base_lock);
        env.repo.commit_all("lock");
        let base = env.head();
        env.repo.write("Cargo.lock", head_lock);
        env.repo.commit_all("T-001 bump");
        (env, base)
    }

    #[test]
    fn security_new_advisory() {
        let (mut env, base) = osv_env("good 1.0\n", "bad 1.0\n");
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass && !out.halt, "{}", out.reason);
        assert!(
            out.reason.contains("bad 1.0 GHSA-test-0001"),
            "{}",
            out.reason
        );
        assert!(env.tasks_text().contains("status: ready-for-agent"));
    }

    #[test]
    fn security_base_advisory() {
        let (mut env, base) = osv_env("bad 1.0\n", "bad 1.0\nother 2.0\n");
        env.repo.write(".enallagi/osv-scanner.toml", "");
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        let argv = std::fs::read_to_string(env.cache.path().join("osv.argv")).expect("argv");
        let config = env.repo.root.join(".enallagi/osv-scanner.toml");
        assert!(
            argv.contains(&format!("--config\n{}\n", config.display())),
            "{argv}"
        );
        assert!(argv.contains("--format\njson\n"), "{argv}");
    }

    // a client-root osv-scanner.toml the lane wrote would otherwise steer the scan
    #[test]
    fn security_osv_config_pinned() {
        let (mut env, base) = osv_env("good 1.0\n", "bad 1.0\n");
        env.repo.write(
            "osv-scanner.toml",
            "[[IgnoredVulns]]\nid = \"GHSA-test-0001\"\n",
        );
        run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        let argv = std::fs::read_to_string(env.cache.path().join("osv.argv")).expect("argv");
        let scans = argv.matches("scan\n").count();
        assert!(
            scans > 0 && argv.matches("--config\n").count() == scans,
            "{argv}"
        );
    }

    #[test]
    fn security_skips_osv_untouched() {
        let (mut env, base) = leak_env("[]", 0);
        stub_osv(&env);
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(!env.cache.path().join("osv.argv").exists());
    }

    #[test]
    fn security_bad_exit_halts() {
        let (mut env, base) = leak_env("[]", 2);
        let out = run("security", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass && out.halt, "{}", out.reason);
        assert!(out.reason.contains("scanner-stderr"), "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn skip_added_passes_a_reasoned_ignore() {
        let (mut env, base) = skip_env(r#"#[ignore = "sleeps four minutes"]"#);
        let out = run("skip-added", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        let (mut env, base) = skip_env("#[ignore]");
        env.cfg.layout.skip_patterns.clear();
        let out = run("skip-added", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(
            out.pass && out.reason == "skip_patterns is empty",
            "{}",
            out.reason
        );
    }

    #[test]
    fn commits_naming_the_task_pass() {
        let mut env = Env::new("exit 0\n");
        let base = head(&env.repo.root).expect("base");
        env.repo.write("src/schema.ts", "export const x = 6\n");
        env.repo.commit_all("feat(schema): T-001 bump x");
        let mut ctx = env.ctx(Some("T-001"), Some(&base));
        let out = commit_task(&mut ctx);
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn launcher_state_commit_exempt() {
        let mut env = Env::new("exit 0\n");
        let base = head(&env.repo.root).expect("base");
        env.repo.write("src/schema.ts", "export const x = 7\n");
        env.repo.commit_all("feat(schema): T-001 bump x");
        env.queue("review", "src/schema.ts", "none — harness");
        commit_instance(&env.ctx(None, None), &[SPECS], "queue: verdict").expect("state");
        assert!(env.log().contains("queue: verdict"), "{}", env.log());
        let mut ctx = env.ctx(Some("T-001"), Some(&base));
        let out = commit_task(&mut ctx);
        assert!(out.pass, "{}", out.reason);
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
    fn overlaps_disjoint_prefixes() {
        assert!(!overlaps(
            &globs(&["src/tui/**"]),
            &globs(&["src/gates.rs"])
        ));
        assert!(!overlaps(
            &globs(&["docs/*.md", "src/a.rs"]),
            &globs(&["src/b/**", "README.md"])
        ));
        assert!(overlaps(&globs(&["src/a"]), &globs(&["src/ab.rs"])));
        assert!(overlaps(&globs(&["src/**"]), &globs(&["src/tui/view.rs"])));
    }

    #[test]
    fn overlaps_empty_or_wild() {
        for wild in [&[][..], &["**"][..], &["*.rs"][..]] {
            assert!(overlaps(&globs(wild), &globs(&["docs/x.md"])), "{wild:?}");
            assert!(overlaps(&globs(&["docs/x.md"]), &globs(wild)), "{wild:?}");
        }
    }

    #[test]
    fn red_not_on_baseline_is_a_rejection() {
        let env = Env::new("echo '(fail) alpha'\nexit 1\n");
        let r = check_delta(&env.sandbox, &env.repo.root, &env.cfg, false);
        assert!(r.red);
        assert_eq!(r.unforgiven, vec!["alpha".to_string()]);
        assert!(!r.accepts());
    }

    #[test]
    fn red_on_baseline_is_forgiven() {
        let env = Env::new("echo '(fail) alpha'\nexit 1\n");
        env.repo
            .write(".enallagi/.check-baseline", "# inherited\nalpha\n");
        let r = check_delta(&env.sandbox, &env.repo.root, &env.cfg, false);
        assert!(r.red);
        assert!(r.unforgiven.is_empty());
        assert_eq!(r.forgiven, vec!["alpha".to_string()]);
        assert!(r.accepts());
    }

    #[test]
    fn one_forgiven_one_new_is_still_red() {
        let env = Env::new("echo '(fail) alpha'\necho '(fail) beta [12ms]'\nexit 1\n");
        env.repo.write(".enallagi/.check-baseline", "alpha\n");
        let r = check_delta(&env.sandbox, &env.repo.root, &env.cfg, false);
        assert_eq!(r.unforgiven, vec!["beta".to_string()]);
        assert_eq!(r.forgiven, vec!["alpha".to_string()]);
        assert!(!r.accepts());
    }

    #[test]
    fn red_it_cannot_name_fails_closed() {
        let env = Env::new("echo boom\nexit 1\n");
        let r = check_delta(&env.sandbox, &env.repo.root, &env.cfg, false);
        assert!(r.red && r.unnamed);
        assert!(r.unforgiven.is_empty());
        assert!(!r.accepts());
    }

    #[test]
    fn green_is_green() {
        let env = Env::new("exit 0\n");
        let r = check_delta(&env.sandbox, &env.repo.root, &env.cfg, false);
        assert!(!r.red && r.accepts());
    }

    #[test]
    fn a_check_past_its_timeout_is_neither_red_nor_green() {
        let mut env = Env::timed("sleep 10\n", "2s");
        let started = Instant::now();
        let r = check_delta(&env.sandbox, &env.repo.root, &env.cfg, false);
        let waited = started.elapsed();
        assert!(
            waited < Duration::from_secs(5),
            "the gate waited {waited:?}"
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

    // judged by what the child does if it lives, not by a clock: a surviving child touches `survived`
    #[test]
    fn timeout_kills_group() {
        let mut runs = 0;
        let (env, pid) = loop {
            runs += 1;
            let env = Env::timed(
                "(sleep 60; touch survived) & echo $! >child.pid\nsleep 60\n",
                "4s",
            );
            let r = check_delta(&env.sandbox, &env.repo.root, &env.cfg, false);
            assert!(r.timed_out.is_some(), "{r:?}");
            let pid = std::fs::read_to_string(env.repo.root.join("child.pid"))
                .unwrap_or_default()
                .trim()
                .to_string();
            // under load the timeout can fire before the check reaches its fork, which proves nothing
            if !pid.is_empty() {
                break (env, pid);
            }
            assert!(
                runs < 3,
                "the check never wrote its child's pid in {runs} runs"
            );
        };
        while alive(&pid) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            !env.repo.root.join("survived").exists(),
            "the check's child {pid} outlived it"
        );
    }

    // bwrap's pid namespace ends a grandchild with the check, so only `off` can be held open by one
    fn unconfined(env: &Env) -> crate::sandbox::Sandbox {
        let mut cfg = env.cfg.clone();
        cfg.sandbox.mode = crate::config::SandboxMode::Off;
        crate::sandbox::Sandbox::start(&env.repo.root, &cfg)
    }

    #[test]
    fn a_grandchild_on_the_pipe_is_bound_by_timeout() {
        let env = Env::timed("sleep 25 & exit 0\n", "2s");
        let sandbox = unconfined(&env);
        let started = Instant::now();
        let r = check_delta(&sandbox, &env.repo.root, &env.cfg, false);
        let waited = started.elapsed();
        assert!(
            waited < Duration::from_secs(5),
            "the gate waited {waited:?}"
        );
        let reason = r.timed_out.as_deref().unwrap_or_default();
        assert!(reason.contains("2s"), "{r:?}");
        assert!(!r.red && !r.accepts(), "{r:?}");
    }

    #[test]
    fn a_grandchild_outside_the_group_is_bound() {
        let env = Env::timed("perl -e 'use POSIX; setsid(); sleep 12' & exit 0\n", "2s");
        let sandbox = unconfined(&env);
        let started = Instant::now();
        let r = check_delta(&sandbox, &env.repo.root, &env.cfg, false);
        let waited = started.elapsed();
        assert!(
            waited < Duration::from_secs(5),
            "the gate waited {waited:?}"
        );
        let reason = r.timed_out.as_deref().unwrap_or_default();
        assert!(reason.contains("2s"), "{r:?}");
        assert!(!r.red && !r.accepts(), "{r:?}");
    }

    fn alive(pid: &str) -> bool {
        std::process::Command::new("kill")
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

    // line 1 says 237; the fourteen lines sum to 471
    fn fourteen_result_lines() -> String {
        let mut out = String::from(
            "running 237 tests\ntest result: ok. 237 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s\n",
        );
        for i in 0..13 {
            let ignored = if i == 4 { 5 } else { 0 };
            out.push_str(&format!(
                "\nrunning 18 tests\ntest result: ok. 18 passed; 0 failed; {ignored} ignored; 0 measured; 0 filtered out; finished in 0.01s\n"
            ));
        }
        out
    }

    #[test]
    fn tally_sums_every_test_result_line() {
        let t = tally(&fourteen_result_lines(), "");
        assert_eq!(
            t,
            Tally {
                passed: 471,
                failed: 0,
                ignored: 5,
                lines: 14
            }
        );
    }

    #[test]
    fn a_tally_pattern_sums_its_named_groups() {
        let ava = r"^\s*(?:(?P<passed>\d+) tests? passed|(?P<failed>\d+) tests? failed)";
        let out = "  ✘ [fail]: a › b\n  1585 tests passed\n  1 test failed\n  4 known failures\n";
        let t = tally(out, ava);
        assert_eq!((t.passed, t.failed, t.lines), (1585, 1, 2));
        let jest = r"(?P<failed>\d+) failed|(?P<passed>\d+) passed";
        let t = tally("Tests:       1 failed, 3 passed, 4 total\n", jest);
        assert_eq!((t.passed, t.failed, t.lines), (3, 1, 1));
        assert_eq!(tally(out, "("), Tally::default());
    }

    #[test]
    fn check_delta_fills_the_tally() {
        let env = Env::new(&format!(
            "cat <<'EOF'\n{}EOF\nexit 0\n",
            fourteen_result_lines()
        ));
        let r = check_delta(&env.sandbox, &env.repo.root, &env.cfg, false);
        assert_eq!(r.exit, 0, "{}", r.output);
        assert_eq!(r.tally.passed, 471);
        assert_eq!(r.tally.lines, 14);
    }

    #[test]
    fn check_gate_event_carries_the_tally() {
        let mut env = Env::new(&format!(
            "cat <<'EOF'\n{}EOF\nexit 0\n",
            fourteen_result_lines()
        ));
        env.cfg.check.fail_name = cargo_fail_name();
        let out = run("check-delta", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert!(env.warnings.is_empty(), "{:?}", env.warnings);
        let tally = env.events().iter().find_map(|e| match &e.kind {
            Kind::Gate { gate, tally, .. } if gate == "check-delta" => *tally,
            _ => None,
        });
        assert_eq!(tally.map(|t| (t.passed, t.lines)), Some((471, 14)));
    }

    #[test]
    fn failed_tally_on_exit_zero_warns() {
        let mut env = Env::new(
            "echo 'test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\nexit 0\n",
        );
        run("check-delta", &mut env.ctx(Some("T-001"), None));
        let cmd = env.cfg.check.command.clone();
        assert_eq!(
            env.warnings,
            vec![format!(
                "the check exited 0 with 1 failed over 1 count lines: `{cmd}`"
            )]
        );
    }

    #[test]
    fn a_check_with_no_result_line_warns() {
        let mut env = Env::new("echo built\nexit 0\n");
        env.cfg.check.fail_name = cargo_fail_name();
        run("check-delta", &mut env.ctx(Some("T-001"), None));
        let cmd = env.cfg.check.command.clone();
        assert_eq!(
            env.warnings,
            vec![format!("the check printed no count line: `{cmd}`")]
        );
    }

    #[test]
    fn a_coloured_failure_line_is_named_as_plain_text() {
        let line = "\x1b[31m✘\x1b[39m \x1b[31m[fail]:\x1b[39m \x1b[1mfile \x1b[90m›\x1b[39m a test\x1b[22m \x1b[31mfailed\x1b[39m\n";
        assert_eq!(strip_ansi(line), "✘ [fail]: file › a test failed\n");
    }

    fn cargo_fail_name() -> String {
        crate::runners::presets()
            .into_iter()
            .find(|r| r.name == "cargo")
            .expect("the cargo preset")
            .fail_name
    }

    fn gate_tally(env: &Env) -> Option<Tally> {
        env.events().iter().find_map(|e| match &e.kind {
            Kind::Gate { gate, tally, .. } if gate == "check-delta" => *tally,
            _ => None,
        })
    }

    #[test]
    fn a_green_check_on_another_runner_is_quiet() {
        let mut env = Env::new("echo 'Tests:       3 passed, 3 total'\nexit 0\n");
        run("check-delta", &mut env.ctx(Some("T-001"), None));
        assert!(env.warnings.is_empty(), "{:?}", env.warnings);
        assert_eq!(gate_tally(&env), None);
    }

    #[test]
    fn a_tally_pattern_counts_another_runner() {
        let mut env = Env::new("echo 'Tests:       3 passed, 3 total'\nexit 0\n");
        env.cfg.check.tally = r"(?P<passed>\d+) passed".to_string();
        run("check-delta", &mut env.ctx(Some("T-001"), None));
        assert!(env.warnings.is_empty(), "{:?}", env.warnings);
        assert_eq!(gate_tally(&env).map(|t| t.passed), Some(3));
    }

    #[test]
    fn a_check_that_never_ran_does_not_warn() {
        let report = CheckReport {
            output: format!("{NEVER_RAN} no such file"),
            exit: -1,
            red: true,
            ..CheckReport::default()
        };
        assert_eq!(report.tally_warning("x", true), None);
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
    fn verdict_dirty_tree() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");
        env.repo.write("src/x.ts", "left behind");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        let text = env.tasks_text();
        assert!(text.contains("status: ready-for-agent"), "{text}");
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
            Kind::TaskStatus { task, to, by, .. } if task == "T-001" && to == "ready-for-agent" && by == "verdict")));
        assert!(events.iter().any(|e| matches!(&e.kind,
            Kind::Gate { gate, pass, .. } if gate == "verdict" && !*pass)));
    }

    #[test]
    fn verdict_red_check() {
        let mut env = Env::new(
            "echo boom\necho 'error: something'\necho 'test result: FAILED. 0 passed; 1 failed; 0 ignored'\necho done\nexit 101\n",
        );
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        assert!(out.reason.contains("gate was red"), "{}", out.reason);
        assert!(out.reason.contains("exit 101"), "{}", out.reason);
        assert!(out.reason.contains("check tail:"), "{}", out.reason);
        assert!(out.reason.contains("error: something"), "{}", out.reason);
        assert!(env.tasks_text().contains("status: ready-for-agent"));
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
        let mut env = Env::new("echo 'test result: ok. 1 passed; 0 failed; 0 ignored'\nexit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
        assert!(env.warnings.is_empty());
    }

    #[test]
    fn verdict_requires_verified() {
        for line in [
            "",
            "verdict: rejected by verifier: red\n",
            "verdict: VERIFIED\n",
        ] {
            let mut env = Env::new("exit 0\n");
            env.write_queue(&format!("## [T-001] first\nstatus: done\n{line}"));
            env.repo.commit_all("verify");
            let out = run("verdict", &mut env.ctx(Some("T-001"), None));
            assert!(!out.pass, "{line}");
            let text = env.tasks_text();
            assert!(text.contains("status: review"), "{text}");
            assert!(text.contains("gate: done without `verdict: verified by verifier`"));
        }
    }

    #[test]
    fn verdict_wants_both_axes() {
        for (axes, missing) in [
            ("standards: none\n", "spec:"),
            ("spec: none\n", "standards:"),
            ("spec:\nstandards: none\n", "spec:"),
        ] {
            let mut env = Env::new("exit 0\n");
            env.write_queue(&format!(
                "## [T-001] first\nstatus: done\nverdict: verified by verifier\n{axes}"
            ));
            env.repo.commit_all("verify");
            let out = run("verdict", &mut env.ctx(Some("T-001"), None));
            assert!(!out.pass, "{axes}");
            assert!(
                out.reason.contains(&format!("`{missing}`")),
                "{}",
                out.reason
            );
            let text = env.tasks_text();
            assert!(text.contains("status: ready-for-agent"), "{text}");
        }
    }

    #[test]
    fn rejected_needs_reason() {
        for (line, refused) in [
            ("", true),
            ("verdict: rejected by verifier\n", true),
            ("verdict: rejected by verifier:  \n", true),
            ("verdict: answered by implementer\n", true),
            (
                "verdict: rejected by verifier: `cargo test` is red\n",
                false,
            ),
        ] {
            let mut env = Env::new("exit 0\n");
            env.queue("review", "src/a.ts", "none");
            env.repo.commit_all("implement");
            let base = env.head();
            env.write_queue(&format!(
                "## [T-001] first\nstatus: ready-for-agent\n{line}"
            ));
            env.repo.commit_all("verify");
            let out = run("verdict", &mut env.ctx(Some("T-001"), Some(&base)));
            assert_eq!(out.pass, !refused, "{line}: {}", out.reason);
            let want = if refused { "review" } else { "ready-for-agent" };
            assert!(
                env.tasks_text().contains(&format!("status: {want}")),
                "{line}"
            );
        }

        // a gate's force-back, not a verifier's rejection, is not read
        let mut env = Env::new("exit 0\n");
        env.queue("ready-for-agent", "src/a.ts", "none");
        env.repo.commit_all("implement");
        let base = env.head();
        let out = run("verdict", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn review_over_rejection() {
        let mut env = Env::new("exit 0\n");
        env.write_queue("## [T-001] first\nstatus: review\nverdict: rejected by verifier: red\n");
        env.repo.commit_all("implement");
        let out = run("implementer-not-done", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass && out.skip_rest, "{out:?}");
        let text = env.tasks_text();
        assert!(text.contains("status: ready-for-agent"), "{text}");
        assert!(text.contains("gate: the implementer set review over a rejection"));

        let mut env = Env::new("exit 0\n");
        env.write_queue("## [T-001] first\nstatus: review\nverdict: answered by implementer\n");
        env.repo.commit_all("implement");
        let out = run("implementer-not-done", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass && !out.skip_rest, "{out:?}");
    }

    impl Env {
        fn claimed(&mut self, allow: &[&str], runs: &str) {
            let path = self.repo.root.join(".enallagi/enallagi.toml");
            let toml = std::fs::read_to_string(&path).expect("read enallagi.toml");
            let allow: Vec<String> = allow.iter().map(|p| format!("'{p}'")).collect();
            self.repo.write(
                ".enallagi/enallagi.toml",
                &format!("{toml}claims_allow = [{}]\n", allow.join(", ")),
            );
            self.cfg = crate::config::load(&self.repo.root).expect("load enallagi.toml");
            self.write_queue(&format!(
                "## [T-001] first\nscope: src/a.ts\nrows: none — harness\nblockedBy: none\nstatus: done\nverdict: verified by verifier\nspec: none\nstandards: none\n{runs}notes: IMPLEMENTER: did it.\n"
            ));
            self.repo.commit_all("verdict");
        }
    }

    // a done round answering `F-1` and `F-2` with `answers`, its test file holding `fn held`
    fn answering(answers: &str, runs: &str) -> (Env, String) {
        let env = Env::new("exit 0\n");
        let base = env.head();
        env.repo.write("tests/t.rs", "fn held() {}\n");
        env.repo.commit_all("T-001 fix");
        env.write_queue(&format!(
            "## [T-001] first\nscope: tests/*\nstatus: done\nfinding: F-1 thread round 1\n  > one\nfinding: F-2 comment round 1\n  > two\n{answers}verdict: verified by verifier\nspec: none\nstandards: none\n{runs}"
        ));
        env.repo.commit_all("verdict");
        (env, base)
    }

    fn findings_of(answers: &str, runs: &str) -> (GateOutcome, String) {
        let (mut env, base) = answering(answers, runs);
        let out = run("findings", &mut env.ctx(Some("T-001"), Some(&base)));
        (out, env.tasks_text())
    }

    #[test]
    fn findings_rejects_unanswered() {
        let (out, text) = findings_of("answer: F-1 accept held\n", "");
        assert!(!out.pass && out.reason.contains("F-2"), "{}", out.reason);
        assert!(text.contains("status: ready-for-agent"), "{text}");
        let twice = "answer: F-1 accept held\nanswer: F-1 accept held\nanswer: F-2 accept held\n";
        let (out, _) = findings_of(twice, "");
        assert!(!out.pass && out.reason.contains("F-1"), "{}", out.reason);
        let unheld = "answer: F-1 accept held\nanswer: F-2 accept missing\n";
        let (out, _) = findings_of(unheld, "");
        assert!(
            !out.pass && out.reason.contains("missing"),
            "{}",
            out.reason
        );
        let (out, text) = findings_of("answer: F-1 accept held\nanswer: F-2 accept held\n", "");
        assert!(out.pass, "{}", out.reason);
        assert!(text.contains("status: done"), "{text}");
    }

    #[test]
    fn findings_rejects_bare_reject() {
        let bare = "answer: F-1 accept held\nanswer: F-2 reject naming:\n";
        let (out, text) = findings_of(bare, "");
        assert!(!out.pass && out.reason.contains("F-2"), "{}", out.reason);
        assert!(text.contains("status: ready-for-agent"), "{text}");
        let unran = "answer: F-1 accept held\nanswer: F-2 reject naming: grep -q x README\n";
        let (out, _) = findings_of(unran, "ran: true\nexit: 0\n");
        assert!(!out.pass && out.reason.contains("ran:"), "{}", out.reason);
        let (out, _) = findings_of(unran, "ran: grep -q x README\nexit: 1\n");
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn claimed_pass_fails() {
        let mut env = Env::new("exit 0\n");
        env.claimed(&["true", "false"], "ran: false\nexit: 0\n");
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass, "{}", out.reason);
        assert!(out.reason.contains("`false`"), "{}", out.reason);
        assert!(out.reason.contains("claimed exit 0"), "{}", out.reason);
        assert!(out.reason.contains("1 and 1"), "{}", out.reason);
        assert!(
            env.tasks_text().contains("status: ready-for-agent"),
            "{}",
            env.tasks_text()
        );
        assert!(env.events().iter().any(|e| matches!(&e.kind,
            Kind::TaskStatus { to, by, .. } if to == "ready-for-agent" && by == "claims")));
    }

    #[test]
    fn claims_pairs_under_verdict() {
        let mut env = Env::new("exit 0\n");
        env.claimed(&["true", "false"], "");
        let path = env.tracker().path_of("T-001").expect("T-001");
        let text = std::fs::read_to_string(&path).expect("ticket");
        let text = text
            .replace("## Comments\n\n", "## Comments\n\nran: false\nexit: 0\n")
            .replace(
                "verdict: verified by verifier\n",
                "verdict: verified by verifier\nran: true\nexit: 0\n\nverdict: rejected by verifier: old\nran: false\nexit: 0\n",
            );
        std::fs::write(&path, text).expect("ticket");
        env.repo.commit_all("verdict");
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert!(
            out.reason.contains("`true` claimed exit 0"),
            "{}",
            out.reason
        );
        assert!(!out.reason.contains("false"), "{}", out.reason);
    }

    #[test]
    fn claim_reproduces() {
        let mut env = Env::new("exit 0\n");
        env.claimed(&["true", "false"], "ran: true\nexit: 0\n");
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn claim_flaky() {
        let mut env = Env::new("exit 0\n");
        env.repo.write(
            "flaky.sh",
            "test -e flaky.mark && exit 1; touch flaky.mark\n",
        );
        env.claimed(&["sh "], "ran: sh flaky.sh\nexit: 0\n");
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert!(out.reason.starts_with("flaky:"), "{}", out.reason);
        assert!(out.reason.contains("sh flaky.sh"), "{}", out.reason);
        assert!(out.reason.contains("0 and 1"), "{}", out.reason);
    }

    #[test]
    fn claim_outside_allowlist() {
        let mut env = Env::new("exit 0\n");
        env.claimed(&["true"], "ran: touch outside.mark\nexit: 0\n");
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert!(!env.repo.root.join("outside.mark").exists());
        assert!(out.reason.contains("not re-run"), "{}", out.reason);
        assert!(out.reason.contains("touch outside.mark"), "{}", out.reason);
    }

    #[test]
    fn claim_prefix_whole_word() {
        let mut env = Env::new("exit 0\n");
        env.claimed(
            &["true"],
            "ran: truex\nexit: 0\nran: true\nexit: 0\nran: true --flag\nexit: 0\n",
        );
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert!(
            out.reason
                .contains("reproduced: `true` claimed exit 0, re-ran exit 0 and 0; `true --flag`"),
            "{}",
            out.reason
        );
        assert!(
            out.reason.ends_with("shell operator: `truex`"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn chained_claim_skipped() {
        let mut env = Env::new("exit 0\n");
        env.claimed(
            &["true"],
            "ran: true; touch a.mark\nexit: 0\nran: true && touch b.mark\nexit: 0\nran: true | touch c.mark\nexit: 0\nran: true $(touch d.mark)\nexit: 0\nran: true `touch e.mark`\nexit: 0\nran: true > f.mark\nexit: 0\n",
        );
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        for mark in ["a", "b", "c", "d", "e", "f"] {
            let path = env.repo.root.join(format!("{mark}.mark"));
            assert!(!path.exists(), "{mark}.mark: {}", out.reason);
        }
    }

    #[test]
    fn hung_claim_halts() {
        let mut env = Env::timed("exit 0\n", "2s");
        env.repo.write("hang.sh", "echo run >>runs.log\nsleep 10\n");
        env.claimed(&["sh "], "ran: sh hang.sh\nexit: 0\n");
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(out.halt && !out.pass, "{out:?}");
        assert!(out.reason.contains("2s"), "{}", out.reason);
        let runs = std::fs::read_to_string(env.repo.root.join("runs.log")).unwrap_or_default();
        assert_eq!(runs.lines().count(), 1, "{runs}");
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn empty_claims_allow() {
        let mut env = Env::new("exit 0\n");
        env.claimed(&[], "ran: false\nexit: 0\n");
        let out = run("claims", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert_eq!(out.reason, "claims_allow is empty");
    }

    #[test]
    fn no_task_is_not_a_rejection() {
        let mut env = Env::new("exit 0\n");
        assert!(run("verdict", &mut env.ctx(None, None)).pass);
        assert!(run("scope", &mut env.ctx(None, None)).pass);
    }

    #[test]
    fn intact_refuses_deletion() {
        let mut env = Env::new("exit 0\n");
        env.write_queue(
            "## [T-001] first\nscope: src/*\nstatus: review\n\n## [T-002] second\nscope: src/*\nstatus: ready-for-agent\n",
        );
        env.repo.commit_all("queue");
        let base = env.head();
        env.write_queue("## [T-001] first\nscope: src/*\nstatus: done\n");
        env.repo.write(
            &env.instance("DECISIONS.md"),
            "# DECISIONS\n\n## Rejected findings\n\n- [2026-11-06] T-002 claimed a gap\n\n## [T-002] second\n",
        );
        env.repo.commit_all("ticket deleted");

        let out = run("queue-intact", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass && out.halt, "{}", out.reason);
        assert!(out.reason.contains("T-002"), "{}", out.reason);
        assert!(
            out.reason
                .contains(&format!("git show {base}:{}", env.instance(SPECS))),
            "{}",
            out.reason
        );
    }

    #[test]
    fn intact_passes_wontfix() {
        let mut env = Env::new("exit 0\n");
        env.write_queue(
            "## [T-001] first\nscope: src/*\nstatus: review\n\n## [T-002] second\nscope: src/*\nstatus: needs-triage\n",
        );
        env.repo.commit_all("queue");
        let base = env.head();
        env.tracker()
            .set_status("T-002", "wontfix", "out of scope: a gap already closed")
            .expect("wontfix");
        env.repo.commit_all("killed");

        let out = run("queue-intact", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_another_tickets_file() {
        let mut env = Env::new("exit 0\n");
        env.write_queue(
            "## [T-001] first\nscope: src/a.ts\nrows: §11 row 1\nstatus: review\n\n## [T-002] second\nscope: src/b.ts\nstatus: ready-for-agent\n",
        );
        env.repo.commit_all("queue");
        let base = env.head();
        env.repo.write("src/a.ts", "a");
        env.repo.commit_all("feat: T-001 a");
        env.tracker().set_status("T-001", "done", "").expect("done");
        env.repo.commit_all("verify: T-001 verdict");
        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);

        let filed = "## [T-003] filed\nstatus: needs-triage\n";
        env.tracker().append("repo", filed).expect("file a finding");
        env.repo.commit_all("verify: T-001 verdict");
        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);

        let other = env.tracker().path_of("T-002").expect("T-002");
        std::fs::write(&other, "# T-002: second\n\n**Status:** wontfix\n").expect("edit");
        env.repo.commit_all("verify: T-001 verdict");
        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.reason.contains("T-002-second.md"), "{}", out.reason);
    }

    #[test]
    fn scope_rejects_an_added_ticket_past_triage() {
        let mut env = Env::new("exit 0\n");
        env.write_queue("## [T-001] first\nscope: src/a.ts\nrows: §11 row 1\nstatus: review\n");
        env.repo.commit_all("queue");
        let base = env.head();
        env.repo.write("src/a.ts", "a");
        env.repo.commit_all("feat: T-001 a");
        env.tracker().set_status("T-001", "done", "").expect("done");
        let filed = "## [T-002] filed ready\nstatus: ready-for-agent\n";
        env.tracker().append("repo", filed).expect("file a ticket");
        env.repo.commit_all("verify: T-001 verdict");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason.contains("T-002-filed-ready.md"),
            "{}",
            out.reason
        );
    }

    #[test]
    fn scope_outside_globs() {
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
        assert!(env.tasks_text().contains("status: ready-for-agent"));
        assert!(
            env.warnings
                .iter()
                .any(|w| w.contains("forced back to ready")
                    && w.contains("commits for T-001 are on the branch")),
            "{:?}",
            env.warnings
        );
        assert!(env
            .log()
            .contains("chore(T-001): harness scope gate rejected a done verdict"));
        env.assert_rejection_events();
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
    fn scope_product_task_edit() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".enallagi/*", "§11 row 1");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(".enallagi/loop.sh", "# edited\n");
        env.repo.commit_all("T-001 harness edit");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(out.reason.contains("touched the harness"), "{}", out.reason);
        assert!(env.tasks_text().contains("status: ready-for-agent"));
        assert!(env
            .log()
            .contains("chore(T-001): harness scope gate rejected a done verdict"));
        env.assert_rejection_events();
    }

    #[test]
    fn scope_rejects_a_grown_baseline() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", &env.instance(".check-baseline"), "none — harness");
        env.repo.write(&env.instance(".check-baseline"), "alpha\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write(&env.instance(".check-baseline"), "alpha\nbeta\n");
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
    fn scope_nets_a_baseline_line_added_then_removed() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", &env.instance(".check-baseline"), "none — harness");
        env.repo.write(&env.instance(".check-baseline"), "alpha\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write(&env.instance(".check-baseline"), "alpha\nbeta\n");
        env.repo.commit_all("T-001 baseline grew");
        env.repo.write(&env.instance(".check-baseline"), "alpha\n");
        env.repo.commit_all("T-001 remove");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn scope_rejects_a_baseline_add_with_a_removal() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", &env.instance(".check-baseline"), "none — harness");
        env.repo.write(&env.instance(".check-baseline"), "alpha\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write(&env.instance(".check-baseline"), "alpha\nbeta\n");
        env.repo.commit_all("T-001 baseline grew");
        env.repo.write(&env.instance(".check-baseline"), "beta\n");
        env.repo.commit_all("T-001 drop alpha");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason
                .contains("added a line to .check-baseline, and the baseline only ever shrinks"),
            "{}",
            out.reason
        );
        env.assert_rejection_events();
    }

    #[test]
    fn scope_rejects_a_plus_prefixed_baseline_add() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", &env.instance(".check-baseline"), "none — harness");
        env.repo.write(&env.instance(".check-baseline"), "alpha\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write(&env.instance(".check-baseline"), "alpha\n+beta\n");
        env.repo.commit_all("T-001 baseline grew");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason
                .contains("added a line to .check-baseline, and the baseline only ever shrinks"),
            "{}",
            out.reason
        );
        env.assert_rejection_events();
    }

    #[test]
    fn scope_nets_a_minus_prefixed_baseline_add() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", &env.instance(".check-baseline"), "none — harness");
        env.repo.write(&env.instance(".check-baseline"), "alpha\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write(&env.instance(".check-baseline"), "alpha\n-beta\n");
        env.repo.commit_all("T-001 baseline grew");
        env.repo.write(&env.instance(".check-baseline"), "alpha\n");
        env.repo.commit_all("T-001 remove");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn scope_ignores_an_operator_baseline_line() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", &env.instance(".check-baseline"), "none — harness");
        env.repo.write(&env.instance(".check-baseline"), "alpha\n");
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write(&env.instance(".check-baseline"), "alpha\nbeta\n");
        env.repo.commit_all("fix: an operator baseline line");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
        assert!(env.tasks_text().contains("status: done"));
    }

    #[test]
    fn scope_exempts_an_in_scope_hash_recut() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.write(
            &env.instance("test-hashes.json"),
            "{\n  \"src/a.ts\": \"aaa\"\n}\n",
        );
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(
            &env.instance("test-hashes.json"),
            "{\n  \"src/a.ts\": \"bbb\"\n}\n",
        );
        env.repo.commit_all("T-001 recut");
        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);

        let mut off = Env::new("exit 0\n");
        off.queue("done", "src/a.ts", "§11 row 1");
        off.repo.write(
            &off.instance("test-hashes.json"),
            "{\n  \"src/b.ts\": \"aaa\"\n}\n",
        );
        off.repo.commit_all("verdict");
        let base = off.head();
        off.repo.write(
            &off.instance("test-hashes.json"),
            "{\n  \"src/b.ts\": \"bbb\"\n}\n",
        );
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
    fn implementer_not_done_forced() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("implement");

        let out = run("implementer-not-done", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass && out.skip_rest);
        let text = env.tasks_text();
        assert!(text.contains("status: ready-for-agent"), "{text}");
        assert!(text.contains(
            "gate: the implementer set done; only the verifier may, and the launcher gates that"
        ));
        assert!(env
            .log()
            .contains("chore(T-001): the implementer marked its own task done"));
    }

    #[test]
    fn short_of_review_skips() {
        for status in [
            "needs-info",
            "wontfix",
            "ready-for-agent",
            "ready-for-human",
        ] {
            let mut env = Env::new("exit 0\n");
            env.queue(status, "src/a.ts", "§11 row 1");
            env.repo.commit_all("implement");
            let out = run("implementer-not-done", &mut env.ctx(Some("T-001"), None));
            assert!(out.pass && out.skip_rest, "{status}: {out:?}");
            assert!(out.reason.contains(status), "{status}: {}", out.reason);
        }

        let mut missing = Env::new("exit 0\n");
        missing.write_queue("## [T-001] first\nscope: src/a.ts\n");
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
    fn dry_round_counts() {
        let mut env = Env::new("exit 0\n");
        env.queue("needs-triage", "src/a.ts", "§11 row 1");
        run("dry-round", &mut env.ctx(None, None));
        assert_eq!(env.dry_rounds, 1);

        env.queue("ready-for-agent", "src/a.ts", "§11 row 1");
        assert!(run("dry-round", &mut env.ctx(None, None)).pass);
        assert_eq!(env.dry_rounds, 0);
    }

    #[test]
    fn unreadable_queue_fails_the_gate() {
        let mut env = Env::new("exit 0\n");
        env.duplicate_queue("done");
        env.repo.commit_all("bad queue");
        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        assert_eq!(out.reason, "unreadable");
        assert_eq!(env.warnings.len(), 1);
        assert!(env.warnings[0].contains("the queue could not be read"));
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
        let tasks = env.tracker().path_of("T-001").expect("T-001");
        std::fs::set_permissions(&tasks, std::fs::Permissions::from_mode(0o444))
            .expect("chmod the queue");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        assert!(env.tasks_text().contains("status: done"));
        assert!(
            env.warnings
                .iter()
                .any(|w| w.contains("the queue could not be rewritten")),
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
        env.repo.write(
            &env.instance("harness.lock"),
            &role_lock_toml(&[("implementer", "aaa")]),
        );
        env.repo.commit_all("T-001 vendor implementer");

        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
    }

    #[test]
    fn scope_rejects_a_recut_role_outside_scope() {
        let mut env = Env::new("exit 0\n");
        env.queue("done", "src/**", "§11 row 1");
        env.repo.write(
            &env.instance("harness.lock"),
            &role_lock_toml(&[("implementer", "aaa")]),
        );
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write(
            &env.instance("harness.lock"),
            &role_lock_toml(&[("implementer", "bbb")]),
        );
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
            .write(&env.instance("harness.lock"), &lock_toml(&[("tdd", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        // the role fetched a second skill this stage; its id is new to the lock, absent at base
        env.repo.write(
            &env.instance("harness.lock"),
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
        env.repo.write(
            &env.instance("harness.lock"),
            &lock_toml(&[("tdd-old", "aaa")]),
        );
        env.repo.commit_all("verdict");
        let base = env.head();
        // only sha256 changes -- the id line itself never appears in the diff
        env.repo.write(
            &env.instance("harness.lock"),
            &lock_toml(&[("tdd-old", "bbb")]),
        );
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
            .write(&env.instance("harness.lock"), &lock_toml(&[("tdd", "aaa")]));
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write(&env.instance("harness.lock"), &lock_toml(&[("tdd", "bbb")]));
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
            &env.instance("harness.lock"),
            &lock_toml(&[("tdd", "aaa"), ("tdd-old", "aaa")]),
        );
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo
            .write(&env.instance("harness.lock"), &lock_toml(&[("tdd", "aaa")]));
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
        env.repo.write(
            &env.instance("harness.lock"),
            &lock_toml(&[("demo", "aaa")]),
        );
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
        env.repo.write(
            &env.instance("harness.lock"),
            &lock_toml(&[("demo", "aaa")]),
        );
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
    fn force_runs_force_form() {
        let repo = Repo::new();
        let cmd = repo.stub_check("echo \"$1\" >> ran.txt\nexit 0\n");
        repo.write(
            ".enallagi/enallagi.toml",
            &format!(
                "[check]\ncommand = \"{cmd} plain\"\nforce = \"{cmd} forced\"\nfail_name = '\\(fail\\) (.+)$'\n"
            ),
        );
        let cfg = crate::config::load(&repo.root).expect("load enallagi.toml");
        let sandbox = crate::sandbox::Sandbox::start(&repo.root, &cfg);
        check_delta(&sandbox, &repo.root, &cfg, true);
        assert_eq!(
            std::fs::read_to_string(repo.root.join("ran.txt")).expect("ran.txt"),
            "forced\n"
        );
        check_delta(&sandbox, &repo.root, &cfg, false);
        assert_eq!(
            std::fs::read_to_string(repo.root.join("ran.txt")).expect("ran.txt"),
            "forced\nplain\n"
        );
    }

    #[test]
    fn a_count_of_zero_leaves_no_stale_path() {
        assert_eq!(stale_path("PROBE install-stale 0\n"), Ok(None));
    }

    #[test]
    fn the_first_finding_line_names_the_path() {
        let text = "PROBE install-stale 2\n\
                    FINDING install-stale .enallagi/RAILS.md:0 the installed copy differs\n\
                    FINDING install-stale .enallagi/roles/scout.md:0 the installed copy differs\n";
        assert_eq!(stale_path(text), Ok(Some(".enallagi/RAILS.md".to_string())));
    }

    #[test]
    fn a_stale_path_may_hold_spaces() {
        let text = "PROBE install-stale 1\n\
                    FINDING install-stale my skills/foo.md:0 the installed copy differs\n";
        assert_eq!(stale_path(text), Ok(Some("my skills/foo.md".to_string())));
    }

    #[test]
    fn a_run_with_no_count_line_is_an_error() {
        assert!(stale_path("running 0 tests\n").is_err());
    }

    #[test]
    fn unreadable_queue_not_takeable() {
        let mut env = Env::new("exit 0\n");
        env.duplicate_queue("ready-for-agent");
        assert!(!run("dry-round", &mut env.ctx(None, None)).pass);
    }

    // one holdout in the ticket's spec set, written after the last commit so no repo tracks it
    fn holdout(env: &Env, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let rel = format!(
            ".enallagi/holdouts/{}/{name}",
            tracker::repo_slug(&env.repo.root)
        );
        env.repo.write(&rel, &format!("#!/bin/sh\n{body}"));
        std::fs::set_permissions(
            env.repo.root.join(rel),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
    }

    fn holdout_env() -> Env {
        let env = Env::new("exit 0\n");
        env.queue("done", "src/*", "none");
        env.repo.commit_all("verdict");
        env
    }

    #[test]
    fn holdout_red_forces_back() {
        let mut env = holdout_env();
        holdout(&env, "a.sh", "exit 0\n");
        holdout(&env, "b.sh", "exit 1\n");
        let out = run("holdout", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        assert_eq!(out.reason, "holdouts: 1 of 2 red");
        assert!(env.tasks_text().contains("status: ready-for-agent"));
        assert!(env.tasks_text().contains("gate: holdouts: 1 of 2 red"));
    }

    #[test]
    fn holdout_reason_names_none() {
        let mut env = holdout_env();
        holdout(&env, "leaky.sh", "echo LEAKYLINE\nexit 1\n");
        run("holdout", &mut env.ctx(Some("T-001"), None));
        let events = format!("{:?}", env.events());
        let seen = [env.tasks_text(), events, env.warnings.join("\n")];
        for text in &seen {
            for word in ["leaky", "holdouts/", "LEAKYLINE"] {
                assert!(!text.contains(word), "{word} in {text}");
            }
        }
        let log = env
            .cache
            .path()
            .join("holdouts")
            .join(tracker::repo_slug(&env.repo.root))
            .join("T-001/leaky.sh.log");
        let text = std::fs::read_to_string(&log).expect("the holdout log");
        assert!(text.contains("LEAKYLINE"), "{text}");
    }

    #[test]
    fn holdout_leaves_tree_clean() {
        let mut env = holdout_env();
        holdout(
            &env,
            "copy.sh",
            "cp \"$ENALLAGI_HOLDOUTS/data.txt\" ./copied.txt\n",
        );
        env.repo.write(
            &format!(
                ".enallagi/holdouts/{}/data.txt",
                tracker::repo_slug(&env.repo.root)
            ),
            "data\n",
        );
        let trees = |env: &Env| git(&env.repo.root, &["worktree", "list"]).expect("list");
        let (status, before) = (porcelain(&env.repo.root), trees(&env));
        let out = run("holdout", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass, "{}", out.reason);
        assert_eq!(porcelain(&env.repo.root), status);
        assert_eq!(trees(&env), before);
        assert!(!env.repo.root.join("copied.txt").exists());
    }

    #[test]
    fn holdout_without_set() {
        let mut env = holdout_env();
        let out = run("holdout", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass && out.reason == "no holdout set", "{}", out.reason);
        env.repo.write(
            &format!(
                ".enallagi/holdouts/{}/data.txt",
                tracker::repo_slug(&env.repo.root)
            ),
            "not executable\n",
        );
        let out = run("holdout", &mut env.ctx(Some("T-001"), None));
        assert!(out.pass && out.reason == "no holdout set", "{}", out.reason);
    }
}
