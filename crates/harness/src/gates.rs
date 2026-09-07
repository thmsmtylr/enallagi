//! gates: the named checks the loop runs after a stage exits. Ported from
//! `harness/lib/gates.sh`, `harness/hooks/check-gate.sh` and the three inline
//! gates in `harness/loop.sh` (the implementer's own verdict, the
//! adjudicator's HALT line, and the dry-round counter).
//!
//! Every gate that forces a task back to `ready` writes the queue with
//! `queue::set_status`, commits `TASKS.md` with the message the shell used,
//! and emits `gate` + `task.status` events. `dry_run` computes and reports
//! the same outcome and writes nothing.

use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::events::{Kind, Writer};
use crate::git::{commit_paths, diff_names, git, head, porcelain};
use crate::queue::{self, Queue};

/// Files every task writes by protocol, so they are in scope for all of them.
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

/// Dispatch by name. A name outside `config::GATE_NAMES` fails closed.
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

/// The task's `status` word, or `Err` when the queue cannot be read at all. A queue no tool can
/// address is not a pass for the task in it.
fn status_of(root: &Path, task: &str) -> Result<Option<String>, queue::QueueError> {
    let text = tasks_file(root).read()?;
    let blocks = queue::parse(&text)?;
    Ok(blocks
        .iter()
        .find(|b| b.id == task)
        .and_then(|b| queue::field(b, "status")))
}

/// `field(task, <key>)` over the queue, empty when absent.
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

/// Rewrite the task to `ready`, commit the queue, and record the refusal.
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
            // The queue still says `done`, so a `task.status` here would be a status change the
            // file does not carry. The refusal is the warning; the gate still fails.
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

// `verifier-not-implementer`: the task was `ready` when the implement stage started, so `done`
// now was written by the agent whose work it is. Forced back, and the rest of the round skipped
// -- there is nothing at review to verify.
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

// The verifier writes its verdict into the working tree and stops. Nothing else commits it, so a
// rejection could otherwise sit uncommitted until a human noticed.
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

// Nothing between "the verifier wrote done" and "TASKS.md says done" ever executed a command, so
// a false VERIFIED was indistinguishable from a real one. This runs the gate and forces a `done`
// the tree cannot support back to `ready`.
fn verdict(ctx: &mut GateCtx) -> GateOutcome {
    let Some(task) = ctx.task.clone() else {
        return pass("no task");
    };
    match status_of(ctx.root, &task) {
        Err(_) => return unreadable(ctx, &task),
        Ok(status) if status.as_deref() != Some("done") => return pass("the verdict is not done"),
        Ok(_) => {}
    }

    // A lane that never commits leaves the work in the tree and the check is green either way, so
    // the task reaches `done` with the implementation on no branch. STOP is the harness's own
    // marker and never a lane's work.
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
    let reason = format!("the verifier returned done and the gate was red at {sha}. {named}");
    force_back(
        ctx,
        "verdict",
        &task,
        &reason,
        &format!("chore({task}): harness gate rejected a false VERIFIED"),
        &format!(
            "{task} was forced back to ready by the gate: the verifier said done, the gate was red."
        ),
    )
}

// The iteration's own commits, diffed against the task's `scope:` globs. It also routes the three
// loops: a diff touching the harness under a task that did not declare `rows: none — harness` is
// a product task editing the measure of its own product lever.
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
    let mut out_of: Vec<String> = Vec::new();
    let mut harness_hit: Vec<String> = Vec::new();
    for f in diff_names(ctx.root, &base) {
        if BOOKKEEPING.contains(&f.as_str()) {
            continue;
        }
        // test-hashes.json and harness.lock are exempt from BOTH axes when every key they re-cut
        // names a file the scope line already covers: the keys are the thing being authorised, so
        // the file itself never needs naming. Clearing only the harness axis moved the deadlock
        // rather than lifting it -- no task names test-hashes.json on its scope line, so the file
        // came back as out_of instead.
        if let Some(key_re) = recut_keys_pattern(&f) {
            let keys = recut_keys(ctx.root, &base, &f, key_re);
            if !keys.is_empty() {
                let off: Vec<&str> = keys
                    .iter()
                    .filter(|k| !in_scope(k, &pats))
                    .map(String::as_str)
                    .collect();
                if off.is_empty() {
                    continue;
                }
                // The key is what was authorised, so the rejection names it rather than leaving
                // the lane to diff the file itself.
                let named = format!("{f} ({})", off.join(" "));
                harness_hit.push(named.clone());
                out_of.push(named);
                continue;
            }
            // A recut file whose keys could not be read is judged like any other path.
        }
        if is_harness_path(ctx.cfg, &f) {
            harness_hit.push(f.clone());
        }
        if !in_scope(&f, &pats) {
            out_of.push(f);
        }
    }

    let rows = field_of(ctx.root, &task, "rows");
    if rows.contains("none") && rows.contains("harness") {
        harness_hit.clear();
    }
    // The baseline only ever shrinks. A harness task may edit it -- that is what clearing an
    // inherited failure looks like -- but a line ADDED is a red check made green by hand, under
    // any rows: value.
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

/// A `scope:` line as globs: backticks and spaces dropped, commas are separators.
fn scope_globs(line: &str) -> Vec<String> {
    line.replace(['`', ' '], "")
        .replace(',', " ")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// The harness set is this literal list, so a project whose check script lives elsewhere adds it
/// here.
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

/// The two files whose *keys* are what a task authorises, and the pattern that reads a key off a
/// changed diff line.
fn recut_keys_pattern(f: &str) -> Option<&'static str> {
    match f {
        "test-hashes.json" => Some(r#"^[+-]\s*"([^"]+)"\s*:"#),
        "harness.lock" => Some(r#"^[+-]\s*id\s*=\s*"([^"]+)""#),
        _ => None,
    }
}

/// The keys the file re-cut between `base` and HEAD, sorted and deduped. SPEC.md §0.2 states the
/// conditional form -- "a re-cut key that does not correspond to a file on the task's scope: line
/// is a rejection" -- which makes one that does an ordinary event. The unconditional form
/// deadlocked the queue: every product row's test file is hashed, so any task turning a row green
/// had to re-cut a hash, which forced `rows: none — harness`, which forbade it from claiming the
/// row it had just turned green.
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

/// The matcher every scope rejection turns on. `case` globs are permissive -- `*` crosses `/`
/// here, unlike pathname expansion -- so `src/*` covers `src/a/b.ts`. An empty scope line puts
/// every file out of scope.
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

/// The portable floor gate's answer: `green` is verified on DELTA, never on absolute zero. A tree
/// whose exit criteria are not all covered yet cannot reach zero failures, and a gate whose
/// passing state is unreachable is not strict, it is broken. `.check-baseline` is the recorded red
/// and it only ever shrinks.
#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    pub red: bool,
    pub unforgiven: Vec<String>,
    pub forgiven: Vec<String>,
    pub unnamed: bool,
    pub output: String,
}

impl CheckReport {
    /// What `check-gate.sh` exiting 0 means. Fails closed: a red run whose failures it cannot NAME
    /// forgives nothing, because a gate asserts what it executed and never merely that nothing
    /// failed.
    pub fn accepts(&self) -> bool {
        !self.red || (!self.unnamed && self.unforgiven.is_empty())
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
                output: format!("check could not be run: {err}"),
                ..CheckReport::default()
            }
        }
    };
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        return CheckReport {
            output,
            ..CheckReport::default()
        };
    }

    // One failure name per line, extracted with the project's own pattern. A pattern that does not
    // compile names nothing, which is the fail-closed answer rather than a crash.
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
    }
}

/// ` [12ms]`, ` [1.4s]` and friends are timings, not part of a test's name.
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

/// `.check-baseline`, comments and blanks removed.
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
    if report.unnamed {
        return fail("check RED, and no failure could be named — nothing to forgive");
    }
    fail(format!(
        "check RED, not on the baseline: {}",
        report.unforgiven.join(", ")
    ))
}

// A fix that needs the spec or the contract is neither a promotion nor a kill: the adjudicator
// leaves the block at proposed and prints a line beginning HALT that names it. The run stops.
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

/// A line beginning HALT that names a block. Both patterns are regexes: slicing the first four
/// bytes of a line splits a multibyte character, and stage output is full of box drawing.
fn halt_id(output: &str) -> Option<String> {
    let starts_halt = regex::Regex::new(r"(?i)^\s*halt").ok()?;
    let task = regex::Regex::new(r"T-[0-9]+").ok()?;
    output
        .lines()
        .filter(|l| starts_halt.is_match(l))
        .find_map(|l| task.find(l).map(|m| m.as_str().to_string()))
}

// A dry round is one that leaves nothing a lane can legally take -- `ready_unattended` empty after
// the pair. An empty queue on its own is not exhaustion; the pipeline's `end_after_dry_rounds`
// decides when a run of them is.
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

/// The first task a lane may take, or `None` -- including when the queue cannot be read at all.
/// Nothing is recorded here; the caller decides what an unreadable queue means to it.
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
        /// A repo carrying `harness.toml` with the fake check, a `.harness/.gitignore` covering
        /// what the harness itself writes (without it the event log makes every tree dirty), and
        /// everything committed so a test's own base sha starts clean.
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

        /// What a scope rejection puts on the log.
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
        let mut env = Env::new("echo boom\nexit 1\n");
        env.queue("done", "src/a.ts", "§11 row 1");
        env.repo.commit_all("verdict");

        let out = run("verdict", &mut env.ctx(Some("T-001"), None));
        assert!(!out.pass);
        assert!(out.reason.contains("gate was red"), "{}", out.reason);
        assert!(env.tasks_text().contains("status: ready"));
        assert!(env
            .log()
            .contains("chore(T-001): harness gate rejected a false VERIFIED"));
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

    #[test]
    fn a_lock_recut_is_read_off_the_skill_ids() {
        // The lock's keys are `[[skill]]` ids, not paths, so a task editing a skill names the id
        // on its scope line beside the skill's own files.
        let lock =
            |id: &str| format!("version = 1\n\n[[skill]]\nid = \"{id}\"\nsha256 = \"aaa\"\n");
        let mut env = Env::new("exit 0\n");
        env.queue("done", ".claude/skills/tdd/**, tdd", "§11 row 1");
        env.repo.write("harness.lock", &lock("tdd-old"));
        env.repo.commit_all("verdict");
        let base = env.head();
        env.repo.write("harness.lock", &lock("tdd"));
        env.repo.commit_all("relock");
        let out = run("scope", &mut env.ctx(Some("T-001"), Some(&base)));
        assert!(!out.pass);
        assert!(
            out.reason.contains("harness.lock (tdd-old)"),
            "{}",
            out.reason
        );

        let mut named = Env::new("exit 0\n");
        named.queue("done", ".claude/skills/tdd/**, tdd, tdd-old", "§11 row 1");
        named.repo.write("harness.lock", &lock("tdd-old"));
        named.repo.commit_all("verdict");
        let base = named.head();
        named.repo.write("harness.lock", &lock("tdd"));
        named.repo.commit_all("relock");
        let out = run("scope", &mut named.ctx(Some("T-001"), Some(&base)));
        assert!(out.pass, "{}", out.reason);
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
