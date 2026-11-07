//! The launcher: runs pipelines from `enallagi.toml` with halts and gates.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use sha2::{Digest as _, Sha256};

use crate::agent::{self, Preset, StageSpawn, TurnCap};
use crate::block;
use crate::config::{self, Config, ConfigError, Predicate};
use crate::events::{Kind, Log, Sink, Writer};
use crate::gates::{self, GateCtx};
use crate::probes::{self, CheckOutcome, ProbeCtx};
use crate::roles;
use crate::skills::{self, ResolveOpts};
use crate::tracker::{self, Tracker};
use crate::{archive, audit, git, pr, review};

// A lane running ps to check for competing writers must ignore its parent.
const LANE: &str = "You are this loop's own lane, spawned by the harness. There is no human in this session
and no answer will come, so never end a turn on a question -- decide and act. A running harness
or agent process in ps is your PARENT process, not a competing writer: LEARNINGS.md's one-checkout-one-writer
rule is about a second operator, and it does not apply to the process that started you.
__TRACKER__";

const SCOUT: &str = "Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __ENALLAGI_DIR__/run/roles/scout.md: read that file first and follow it exactly. Run `enallagi probe` and write one ticket file at needs-triage per FINDING line, each carrying probe:, command:, output: and rows:. Zero FINDING lines is zero tickets, which is a valid outcome and not something to escalate. Never promote, never fix, never edit any file a finding names. Then stop.";

const ADJUDICATOR: &str = "Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __ENALLAGI_DIR__/run/roles/adjudicator.md: read that file first and follow it exactly. Act on exactly the needs-triage tickets this prompt names, in the order it names them. Promote each to 'ready-for-agent' with a scope and criteria an agent that has read only __CONTEXT_FILE__, __SPEC__, LEARNINGS.md and the ticket can run, or kill it: set it 'wontfix' and record its kill line in .out-of-scope/<concept>.md as your role file says. Decide each learning under '## Proposed learnings' in DECISIONS.md the way your role file says. A finding whose fix needs a change to __SPEC__ or __CONTEXT_FILE__ is neither: leave it at needs-triage and print a line beginning HALT that names the ticket's id. Do not commit; this loop commits your round. Then stop.";

const IMPLEMENTER: &str = "Read __CONTEXT_FILE__, __SPEC__, LEARNINGS.md, your ticket file, git log --oneline -20, and the TAIL of PROGRESS.md (tail -200 PROGRESS.md -- it is append-only and newest-last, so reading it from the top gives you the oldest entries and none of the handoff). The tail and the log are what the one-row rail has you re-read at the start of an iteration. Read the rules this loop earned with `sed -n '/^## Earned rules/,/^## /p' __ENALLAGI_DIR__/DECISIONS.md`. Your role is defined in __ENALLAGI_DIR__/run/roles/implementer.md: read that file first and follow it exactly. Complete exactly ONE ticket: the one `enallagi tasks ready` prints. If that ticket's scope files already carry uncommitted work, a prior lane was terminated mid-flight: finish it, never restart it and never discard it. Follow the task protocol strictly. Before you stop you MUST git add the product paths named on the ticket's scope: line (never git add -A, LEARNINGS.md 2026-08-26), commit them, paste the exact commands and their output under the ticket's Comments section, and set its status to review. You MUST also append this iteration's PROGRESS.md entry in the format written at the top of that file -- what happened, which rows moved, and any BLOCKED with its written reason. Never stage or commit a ticket file, PROGRESS.md or any other file under __ENALLAGI_DIR__: this loop commits them when your stage ends. An implementation left uncommitted is a lost iteration.";

const VERIFIER: &str = "Read __CONTEXT_FILE__, __SPEC__ and every ticket at review (`enallagi tasks ids-at review`). Read the rules this loop earned with `sed -n '/^## Earned rules/,/^## /p' __ENALLAGI_DIR__/DECISIONS.md`. Your role is defined in __ENALLAGI_DIR__/run/roles/verifier.md: read that file first and follow it exactly. Verify every ticket at review. Promote to done or reject to ready-for-agent with concrete reasons; this loop commits the verdict. If nothing is at review, say so in one line and stop; that is a valid outcome, not something to escalate. Then stop.";

const GENERIC: &str = "Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __ENALLAGI_DIR__/run/roles/__ROLE__.md: read that file first and follow it exactly. Then stop.";

// one ticket reads its own spec; any other count reads the spec beside each
fn spec_for(root: &Path, cfg: &Config, ids: &[&str]) -> String {
    let own = match ids {
        [id] => Tracker::open(root, &cfg.layout.harness_dir)
            .path_of(id)
            .and_then(|p| Some(p.parent()?.parent()?.join("spec.md")))
            .and_then(|p| Some(p.strip_prefix(root).ok()?.display().to_string())),
        _ => None,
    };
    own.unwrap_or_else(|| "the `spec.md` beside each ticket's `issues/` directory".into())
}

fn prompt_for(role: &str, cfg: &Config, spec: &str) -> String {
    let body = match role {
        "scout" => SCOUT.to_string(),
        "adjudicator" => ADJUDICATOR.to_string(),
        "implementer" => IMPLEMENTER.to_string(),
        "verifier" => VERIFIER.to_string(),
        other => GENERIC.replace("__ROLE__", other),
    };
    config::subst(&format!("{LANE} {body}").replace("__SPEC__", spec), cfg)
}

// shared with cli::skills, which validates config before resolving any skill
pub(crate) fn role_source(root: &Path, cfg: &Config, role: &str) -> Option<String> {
    let path = role_path(root, cfg, role);
    if path.is_file() {
        return std::fs::read_to_string(path).ok();
    }
    Some(
        match role {
            "scout" => include_str!("../../../roles/scout.md"),
            "adjudicator" => include_str!("../../../roles/adjudicator.md"),
            "implementer" => include_str!("../../../roles/implementer.md"),
            "verifier" => include_str!("../../../roles/verifier.md"),
            "researcher" => include_str!("../../../roles/researcher.md"),
            "decomposer" => include_str!("../../../roles/decomposer.md"),
            _ => return None,
        }
        .to_string(),
    )
}

fn role_path(root: &Path, cfg: &Config, role: &str) -> PathBuf {
    root.join(&cfg.layout.harness_dir)
        .join("roles")
        .join(format!("{role}.md"))
}

fn rendered_role_path(root: &Path, cfg: &Config, role: &str) -> PathBuf {
    root.join(&cfg.layout.harness_dir)
        .join("run")
        .join("roles")
        .join(format!("{role}.md"))
}

#[derive(Debug, Clone)]
pub struct RunOpts {
    pub max_iter: u32,
    // empty runs every pipeline whose `when` holds; a name here is the only one the round may choose
    pub pipelines: Vec<String>,
    pub dry_run: bool,
    pub frozen: bool,
    pub tui: bool,
    pub budget_seconds: Option<u64>,
    pub budget_usd: Option<f64>,
    pub budget_tokens: Option<u64>,
    // overrides [agent] dangerously_skip_permissions only toward true
    pub dangerously_skip_permissions: bool,
    // overrides [pr] per_task only toward true
    pub pr_per_task: bool,
}

impl Default for RunOpts {
    fn default() -> Self {
        RunOpts {
            max_iter: 3,
            pipelines: Vec::new(),
            dry_run: false,
            frozen: false,
            tui: false,
            budget_seconds: None,
            budget_usd: None,
            budget_tokens: None,
            dangerously_skip_permissions: false,
            pr_per_task: false,
        }
    }
}

impl RunOpts {
    // a BUDGET_SECONDS/USD/TOKENS value that does not parse is silently no budget
    pub fn with_env_budgets(mut self) -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        self.budget_seconds = self.budget_seconds.or(var("BUDGET_SECONDS")
            .and_then(|v| v.trim().parse().ok())
            .filter(|s: &u64| *s > 0));
        self.budget_usd = self
            .budget_usd
            .or(var("BUDGET_USD").and_then(|v| v.trim().parse().ok()));
        self.budget_tokens = self.budget_tokens.or(var("BUDGET_TOKENS")
            .and_then(|v| v.trim().parse().ok())
            .filter(|t: &u64| *t > 0));
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct Digest {
    pub iterations: u32,
    pub seconds: u64,
    pub cost: f64,
    pub landed: Vec<String>,
    pub rows: Vec<String>,
    pub promoted: Vec<String>,
    pub killed: Vec<String>,
    pub halts: Vec<String>,
    pub warnings: Vec<String>,
    // one line per landed task under --pr-per-task: its URL, or why none was opened
    pub pulls: Vec<String>,
    pub stages_run: usize,
    pub role_seconds: BTreeMap<String, u64>,
    pub turn_caps: Vec<String>,
    pub timeouts: Vec<String>,
    pub adjudication: Adjudication,
    pub proposed_standing: usize,
    pub proposed_oldest: usize,
    pub expired: usize,
    pub learnings: Learnings,
}

// each names the learning's class
#[derive(Debug, Clone, Default)]
pub struct Learnings {
    pub proposed: Vec<String>,
    pub promoted: Vec<String>,
    pub killed: Vec<String>,
    pub expired: Vec<String>,
    pub removed: Vec<String>,
}

// an empty promoted/killed pair means nothing to decide only if the stage both spawned and ended
// cleanly; a stage that halted or exited nonzero decided nothing because it could not
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Adjudication {
    #[default]
    NoStage,
    Ran,
    Undecided,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Refused(pub String);

// the two probes that answer "would this lane be told the wrong check?"
const PREFLIGHT: [&str; 2] = ["check-unnamed", "install-stale"];

/// Refuses a `ready` block whose scope or criteria an import left for the adjudicator to write.
pub fn refuse_unfilled(root: &Path, cfg: &Config) -> Result<(), Refused> {
    let unfilled = block::unfilled_ready(&queue_blocks(root, cfg));
    if unfilled.is_empty() {
        return Ok(());
    }
    Err(Refused(format!(
        "{} is ready but still carries the import's placeholder scope or criteria. \
         Set it back to `needs-triage` so the adjudicator writes them, or write them yourself.",
        unfilled.join(", ")
    )))
}

/// Refuses before the first stage when the installed files or the context file have drifted from
/// the configured check. A probe that could not run refuses too: its zero is not a pass.
pub fn preflight(root: &Path) -> anyhow::Result<()> {
    let cfg = config::load(root).map_err(|e| Refused(e.to_string()))?;
    refuse_unfilled(root, &cfg)?;
    // Some(..) keeps run_all from running the check itself; neither of these probes reads it
    let check = CheckOutcome {
        ran: false,
        red: false,
        output: String::new(),
    };
    let ctx = ProbeCtx {
        root,
        cfg: &cfg,
        check: Some(&check),
        driver: false,
    };
    let names: Vec<String> = PREFLIGHT.iter().map(|n| n.to_string()).collect();
    let mut lines = Vec::new();
    for (name, result) in probes::run_all(&ctx, &names) {
        match result {
            probes::ProbeResult::Count(found) => lines.extend(
                found
                    .iter()
                    .map(|f| format!("{name} {}:{} {}", f.path, f.line, f.message)),
            ),
            probes::ProbeResult::Error(reason) => lines.push(format!("{name} ERROR {reason}")),
            probes::ProbeResult::Off(_) => {}
        }
    }
    if lines.is_empty() {
        return Ok(());
    }
    if cfg.check.command.trim().is_empty() {
        return Err(Refused(format!(
            "enallagi.toml sets no check.command, so no lane can be told a check. \
             Set it, then run `enallagi init`.\n{}",
            lines.join("\n")
        ))
        .into());
    }
    Err(Refused(format!(
        "the install does not match enallagi.toml, so every lane would be told the wrong check. \
         Run `enallagi init`, then run again.\n{}",
        lines.join("\n")
    ))
    .into())
}

fn queue_read(root: &Path, cfg: &Config) -> Result<Vec<block::Block>, String> {
    Tracker::open(root, &cfg.layout.harness_dir)
        .blocks()
        .map_err(|e| e.to_string())
}

fn queue_blocks(root: &Path, cfg: &Config) -> Vec<block::Block> {
    queue_read(root, cfg).unwrap_or_default()
}

// the same task the round would take, so the plan prices the stage the run will actually spawn
fn plan_task(blocks: &[block::Block], when: &Predicate) -> Option<String> {
    match when {
        Predicate::QueueReviewing => tracker::ids_in(blocks, &["review"]).into_iter().next(),
        Predicate::SpecUndecomposed => None,
        _ => tracker::takeable(blocks),
    }
}

// the round's own proposals, then the oldest standing ones the drain allows
fn handed_ids(root: &Path, cfg: &Config, at_start: &[String]) -> Vec<String> {
    let all = tracker::ids_in(&queue_blocks(root, cfg), &["needs-triage"]);
    let (mut standing, fresh): (Vec<String>, Vec<String>) =
        all.into_iter().partition(|id| at_start.contains(id));
    let ages = archive::ages(root, cfg, &standing);
    standing.sort_by_key(|id| std::cmp::Reverse(ages.get(id).copied().unwrap_or(0)));
    standing.truncate(cfg.queue.drain);
    fresh.into_iter().chain(standing).collect()
}

// a stage handed n blocks is sized for n, not for the one the config names
fn turn_cap(cfg: &Config, stage: &config::Stage, handed: usize) -> u32 {
    stage.turns + cfg.queue.turns_per_block * handed as u32
}

// the plan has no round behind it, so every proposed block counts as standing
fn planned_cap(root: &Path, cfg: &Config, stage: &config::Stage) -> u32 {
    if stage.role.as_deref() != Some("adjudicator") {
        return stage.turns;
    }
    let at_start = tracker::ids_in(&queue_blocks(root, cfg), &["needs-triage"]);
    turn_cap(cfg, stage, handed_ids(root, cfg, &at_start).len())
}

fn task_levels(blocks: &[block::Block], task: Option<&str>) -> agent::Levels {
    let Some(b) = task.and_then(|id| blocks.iter().find(|b| b.id == id)) else {
        return agent::Levels::default();
    };
    let value = |key: &str| block::field(b, key).filter(|v| !v.is_empty());
    agent::Levels {
        model: value("model"),
        effort: value("effort"),
    }
}

// no level anywhere means no flag is passed, so the agent's own default is what runs
fn level_word(level: &Option<String>) -> &str {
    level.as_deref().unwrap_or("default")
}

pub fn plan(root: &Path, cfg: &Config, pipelines: &[String]) -> Result<String, ConfigError> {
    let presets = agent::presets();
    let mut out = String::new();
    let mut scouting = false;
    let mut warnings = Vec::new();

    for pipeline in cfg.pipeline.iter() {
        if !pipelines.is_empty() && !pipelines.contains(&pipeline.name) {
            continue;
        }
        let when = config::parse_when(&pipeline.when)?;
        if !holds(root, cfg, &when, &mut warnings) {
            continue;
        }
        let blocks = queue_blocks(root, cfg);
        let task = plan_task(&blocks, &when);
        let levels = task_levels(&blocks, task.as_deref());
        let _ = writeln!(
            out,
            "=== pipeline {} ({}) ===",
            pipeline.name, pipeline.when
        );
        for name in &pipeline.stages {
            let Some(stage) = cfg.stage.iter().find(|s| &s.name == name) else {
                continue;
            };
            match (&stage.role, &stage.command) {
                (Some(role), _) => {
                    scouting |= role == "scout";
                    let resolved = agent::resolve_task(&cfg.agent, role, &presets, &levels)
                        .map_err(|e| ConfigError::UnknownPreset(e.to_string()))?;
                    let via = resolved.argv.first().cloned().unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "  DRY_RUN would spawn: {name} as role {role} via {via} \
                         (turns: {}, model: {}, effort: {})",
                        planned_cap(root, cfg, stage),
                        level_word(&resolved.levels.model),
                        level_word(&resolved.levels.effort)
                    );
                    for line in prompt_for(role, cfg, &cfg.layout.spec).lines() {
                        let _ = writeln!(out, "    | {line}");
                    }
                }
                (None, Some(command)) => {
                    let _ = writeln!(
                        out,
                        "  DRY_RUN would run: {name} as `{}`",
                        config::subst(command, cfg)
                    );
                }
                (None, None) => {}
            }
            let gates = if stage.post.is_empty() {
                "none".to_string()
            } else {
                stage.post.join(", ")
            };
            let _ = writeln!(out, "  gates: {gates}");
        }
    }

    if scouting {
        out.push_str("  probes, which are the scout's whole input:\n");
        // ENALLAGI_DRIVER only here and the scout stage -- it installs throwaway repos and costs wall clock elsewhere
        let check = check_outcome(root, cfg);
        let ctx = ProbeCtx {
            root,
            cfg,
            check: Some(&check),
            driver: true,
        };
        out.push_str(&probes::render(&probes::run_all(&ctx, &[])));
    }
    Ok(out)
}

pub fn run(root: &Path, opts: &RunOpts, sink: Sink) -> anyhow::Result<Digest> {
    let mut cfg = config::load(root).map_err(|e| Refused(e.to_string()))?;
    cfg.agent.dangerously_skip_permissions |= opts.dangerously_skip_permissions;
    let presets = agent::presets();
    config::validate(&cfg, &presets, &|role| role_source(root, &cfg, role)).map_err(|errs| {
        Refused(
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })?;
    let rate_limit = Regex::new(&format!("(?i){}", cfg.agent.rate_limit_pattern))
        .map_err(|e| Refused(format!("agent.rate_limit_pattern: {e}")))?;
    if let Some(unknown) = opts
        .pipelines
        .iter()
        .find(|name| !cfg.pipeline.iter().any(|p| &p.name == *name))
    {
        let names: Vec<&str> = cfg.pipeline.iter().map(|p| p.name.as_str()).collect();
        return Err(Refused(format!(
            "--pipeline {unknown}: enallagi.toml names {}",
            names.join(", ")
        ))
        .into());
    }

    if opts.dry_run {
        print!(
            "{}",
            plan(root, &cfg, &opts.pipelines).map_err(|e| Refused(e.to_string()))?
        );
        return Ok(Digest::default());
    }

    let mut looper = Loop {
        root,
        cfg: &cfg,
        presets,
        opts,
        rate_limit,
        writer: {
            let mut w = Writer::new(Log::open(&root.join(&cfg.layout.harness_dir)));
            w.set_sink(sink);
            w
        },
        digest: Digest::default(),
        cost_missing: false,
        tokens_missing: Vec::new(),
        spent_tokens: 0,
        needs_spec_at_start: Vec::new(),
        proposed_at_start: Vec::new(),
        learnings_at_start: Vec::new(),
        decisions_at_start: String::new(),
        dry_rounds: 0,
        dry_pipeline: None,
        spent: Vec::new(),
        scope_refusals: std::collections::HashMap::new(),
        stopped: false,
        built: Vec::new(),
    };
    looper.go()
}

// answers "is a loop running in this checkout?" for archive_done and the one-writer hook; removed on every exit via Drop
struct PidFile(PathBuf);

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct Loop<'a> {
    root: &'a Path,
    cfg: &'a Config,
    presets: agent::Presets,
    opts: &'a RunOpts,
    rate_limit: Regex,
    writer: Writer,
    digest: Digest,
    cost_missing: bool,
    // the declared [agent.usage] lanes the last stage left unreported
    tokens_missing: Vec<&'static str>,
    spent_tokens: u64,
    needs_spec_at_start: Vec<String>,
    proposed_at_start: Vec<String>,
    learnings_at_start: Vec<String>,
    decisions_at_start: String,
    dry_rounds: u32,
    dry_pipeline: Option<String>,
    // pipelines whose end_after_dry_rounds this run has spent; `choose` passes over them until a
    // round leaves takeable work again
    spent: Vec<String>,
    // scope refusals per task this run: a second one is a widening no lane will explain
    scope_refusals: std::collections::HashMap<String, u32>,
    stopped: bool,
    // tasks whose pull-request branch this run built, in order, so a dependent task stacks on one
    built: Vec<String>,
}

enum Flow {
    Go,
    SkipRest,
    Stop,
}

impl<'a> Loop<'a> {
    fn rel(&self, name: &str) -> String {
        config::instance_rel(self.root, &self.cfg.layout.harness_dir, name)
    }

    fn file(&self, name: &str) -> PathBuf {
        config::instance_path(self.root, &self.cfg.layout.harness_dir, name)
    }

    fn go(&mut self) -> anyhow::Result<Digest> {
        self.needs_spec_at_start = self.ids_at(&["needs-info"]);
        self.learnings_at_start = waiting_classes(self.root, self.cfg);
        self.decisions_at_start =
            std::fs::read_to_string(self.file("DECISIONS.md")).unwrap_or_default();
        self.emit(Kind::RunStart {
            config_sha256: config_sha256(self.root),
            pipeline: None,
            permissions_skipped: self.cfg.agent.dangerously_skip_permissions,
            binary: Some(crate::events::binary()),
        });

        // a backticked mention is prose about the marker; only a bare one halts the run
        let spec = &self.cfg.layout.spec;
        if let Some(lines) = clarifications(&self.root.join(spec)) {
            self.halt(
                "clarification",
                format!("{spec} carries [NEEDS CLARIFICATION]. The loop does not start."),
            );
            for line in &lines {
                println!("  {line}");
            }
            return Ok(self.finish(0));
        }

        let pid_path = self
            .root
            .join(&self.cfg.layout.harness_dir)
            .join("loop.pid");
        if let Some(parent) = pid_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _pid = PidFile(pid_path.clone());
        let _ = std::fs::write(&pid_path, format!("{}\n", std::process::id()));

        let mut iterations = 0u32;
        for i in 1..=self.opts.max_iter {
            self.writer.set_iter(i);
            iterations = i;
            if !self.iteration() {
                break;
            }
        }

        Ok(self.finish(iterations))
    }

    // neither a refusal nor an error is a halt: the queue is still readable and the round can run
    fn archive(&mut self) {
        match archive::archive_done(self.root, self.cfg, false) {
            Ok(report) => {
                self.digest.expired += report.expired.len();
                if let Some(refused) = report.refused {
                    self.digest.warnings.push(refused);
                }
            }
            Err(err) => self.digest.warnings.push(format!("archive: {err}")),
        }
    }

    // once per run and never between stages: the audit reads what every iteration recorded
    fn learnings(&mut self) {
        let (promoted, killed) = audit::decided(
            self.root,
            self.cfg,
            &self.learnings_at_start,
            &self.decisions_at_start,
        );
        self.digest.learnings.promoted = promoted;
        self.digest.learnings.killed = killed;
        let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
        match audit::expire(self.root, self.cfg, &today) {
            Ok(expired) => self.digest.learnings.expired = expired,
            Err(err) => self.digest.warnings.push(err.to_string()),
        }
        // the auditor is a paid call, so a stop or a spent budget skips it like a stage
        let stopped = agent::stop_signal().is_some()
            || self.file("STOP").is_file()
            || self.over_budget().is_some();
        let audited = (!stopped).then(|| audit::run(self.root, self.cfg, &mut self.writer));
        let spent = match &audited {
            Some(Ok(report)) => report.spent,
            Some(Err(err)) => err.spent(),
            None => None,
        };
        if let Some((seconds, cost)) = spent {
            self.digest.seconds += seconds;
            *self
                .digest
                .role_seconds
                .entry("auditor".to_string())
                .or_default() += seconds;
            if let Some(cost) = cost {
                self.digest.cost = round4(self.digest.cost + cost);
            }
        }
        match audited {
            Some(Ok(report)) => {
                self.digest.learnings.proposed =
                    report.proposed.into_iter().map(|p| p.class).collect();
                self.digest.learnings.removed = report.removed;
            }
            Some(Err(err)) => self.digest.warnings.push(err.to_string()),
            None => {}
        }
        if let Err(err) = git::commit_instance(
            self.root,
            &self.cfg.layout.harness_dir,
            &["DECISIONS.md", "LEARNINGS.md"],
            "audit",
        ) {
            self.digest
                .warnings
                .push(format!("the audit's learnings are uncommitted: {err}"));
        }
    }

    // every exit path funnels through here, so every run ends with one digest
    fn finish(&mut self, iterations: u32) -> Digest {
        // the task the last iteration landed is archived by the run that landed it, not the next one
        self.archive();
        if iterations > 0 {
            self.learnings();
        }
        let proposed = self.ids_at(&["needs-triage"]);
        self.digest.proposed_oldest = archive::ages(self.root, self.cfg, &proposed)
            .into_values()
            .max()
            .unwrap_or(0);
        self.digest.proposed_standing = proposed.len();
        self.digest.iterations = iterations;
        self.emit(Kind::RunEnd {
            halts: self.digest.halts.clone(),
            landed: self.digest.landed.clone(),
            promoted: self.digest.promoted.clone(),
            killed: self.digest.killed.clone(),
            warnings: self.digest.warnings.clone(),
        });
        if !self.opts.tui {
            print!("{}", digest_text(&self.digest));
        }
        std::mem::take(&mut self.digest)
    }

    fn iteration(&mut self) -> bool {
        if self.boundary(true) {
            return false;
        }

        self.read_reviews();
        // a block a human queued by hand is recorded against the product HEAD it was written at
        self.commit_state("queue");
        self.archive();

        let ready_before = self.ids_at(&tracker::READY);
        self.proposed_at_start = self.ids_at(&["needs-triage"]);
        let rejections_before = self.rejections();
        let takeable = tracker::takeable(&self.blocks());
        let iter_base = git::head(self.root);
        let state_base = git::head(&self.state_root());
        let progress_before = file_len(&self.file("PROGRESS.md"));

        let Some(pipeline) = self.choose() else {
            let reason = if self.opts.pipelines.is_empty() {
                "no pipeline's `when` held; nothing to run.".to_string()
            } else {
                format!(
                    "--pipeline {}: no `when` held; nothing to run.",
                    self.opts.pipelines.join(", ")
                )
            };
            self.digest.warnings.push(reason);
            return false;
        };

        // dry rounds are consecutive rounds of one pipeline: a task round that empties the queue is
        // not a discovery round, and only the pipeline holding end_after_dry_rounds spends it
        if self.dry_pipeline.as_deref() != Some(pipeline.name.as_str()) {
            self.dry_rounds = 0;
            self.dry_pipeline = Some(pipeline.name.clone());
        }

        // fires only after a discovery round already found nothing takeable; ready-for-human tickets alone are the ordinary human-wait state
        if takeable.is_none() && self.dry_rounds >= 1 {
            if let Some(id) = self.first_attended_ready() {
                self.halt(
                    &id,
                    format!(
                        "{id} is ready-for-human and it is all that is left. A human has to run it."
                    ),
                );
                return false;
            }
        }

        // the review pipeline verifies a task stranded at review, not the ordinary ready-and-unattended one
        // the decompose pipeline takes a spec, which its decompose stage picks, not a ticket
        let task = match config::parse_when(&pipeline.when) {
            Ok(Predicate::QueueReviewing) => self.first_at_review(),
            Ok(Predicate::SpecUndecomposed) => None,
            _ => takeable,
        };

        for name in &pipeline.stages {
            let Some(stage) = self.cfg.stage.iter().find(|s| &s.name == name).cloned() else {
                continue;
            };
            let bases = (iter_base.clone(), state_base.clone());
            match self.stage(&stage, task.clone(), bases) {
                Flow::Go => {}
                // no stage after this one runs, but the round's own outcome is still recorded, and
                // the boundary the skipped stages would have checked is owed here instead
                Flow::SkipRest => {
                    self.boundary(false);
                    break;
                }
                // a stage that exhausted its turns still decided the blocks it decided before it did
                Flow::Stop => {
                    self.promotions(&ready_before, &rejections_before);
                    return false;
                }
            }
        }

        self.refuse_restated(task.as_deref(), &ready_before);
        self.promotions(&ready_before, &rejections_before);
        match &task {
            Some(task) => self.task_outcome(task, &pipeline, progress_before),
            None => {
                if let Some(reason) = self.proposed_names_contract() {
                    self.halt("needs-triage", reason);
                    return false;
                }
            }
        }
        if let Some(reason) = self.new_needs_spec() {
            self.halt("needs-info", reason);
            return false;
        }
        if self.stopped {
            return false;
        }
        // a round that left takeable work re-opens every pipeline a dry streak closed
        if self.dry_rounds == 0 {
            self.spent.clear();
        }
        // an empty queue isn't exhaustion by itself; only end_after_dry_rounds consecutive empties are
        if pipeline.end_after_dry_rounds == 0 || self.dry_rounds < pipeline.end_after_dry_rounds {
            return true;
        }
        // a spent ceiling closes this pipeline, not the run: the run goes on while a later
        // pipeline's `when` still holds, so a block the adjudicator cannot decide hands the next
        // round to discover rather than holding every round at triage
        self.spent.push(pipeline.name.clone());
        let next = self.choose();
        if let Some(next) = &next {
            self.digest.warnings.push(format!(
                "{} spent its {} dry round(s); the next round takes {}.",
                pipeline.name, pipeline.end_after_dry_rounds, next.name
            ));
        }
        next.is_some()
    }

    fn state_root(&self) -> PathBuf {
        git::state_root(self.root, &self.cfg.layout.harness_dir)
    }

    fn commit_state(&mut self, msg: &str) {
        if let Err(err) = git::commit_instance(
            self.root,
            &self.cfg.layout.harness_dir,
            gates::BOOKKEEPING,
            msg,
        ) {
            self.digest
                .warnings
                .push(format!("the harness directory was not committed: {err}"));
        }
    }

    fn stage(
        &mut self,
        stage: &config::Stage,
        task: Option<String>,
        iter_bases: (Option<String>, Option<String>),
    ) -> Flow {
        if self.boundary(false) {
            return Flow::Stop;
        }

        // the spec's slug stands in for a ticket id, so the prompt and the tickets gate name it
        let task = match stage.role.as_deref() {
            Some("decomposer") => tracker::undecomposed(self.root, &self.cfg.layout.harness_dir)
                .into_iter()
                .next(),
            _ => task,
        };
        let adjudicating = stage.role.as_deref() == Some("adjudicator");
        let handed = match adjudicating {
            true => handed_ids(self.root, self.cfg, &self.proposed_at_start),
            false => Vec::new(),
        };
        // nothing filed this round and nothing standing to drain: the stage has no input at all
        if adjudicating && handed.is_empty() && waiting_classes(self.root, self.cfg).is_empty() {
            let stage_bases = iter_bases.clone();
            return self.gates(stage, task, iter_bases, stage_bases, String::new());
        }
        let turns = turn_cap(self.cfg, stage, handed.len());

        let timeout = match stage.timeout_duration() {
            Ok(t) => t,
            Err(err) => {
                self.halt("stage", err.to_string());
                return Flow::Stop;
            }
        };
        let mut env = stage.env.clone();
        env.insert("ENALLAGI_ROOT".into(), self.root.display().to_string());
        env.insert("ENALLAGI_STAGE".into(), stage.name.clone());
        env.insert("ENALLAGI_TASK".into(), task.clone().unwrap_or_default());
        env.insert("ENALLAGI_ITERATION".into(), self.writer.iter.to_string());

        let (spawn, role) = match (&stage.role, &stage.command) {
            (Some(role), _) => {
                match self.role_spawn(stage, role, task.clone(), env, timeout, &handed) {
                    Ok(spawn) => (spawn, Some(role.clone())),
                    Err(flow) => return flow,
                }
            }
            (None, Some(command)) => (
                self.command_spawn(stage, command, task.clone(), env, timeout),
                None,
            ),
            (None, None) => return Flow::Go,
        };

        self.emit(Kind::StageStart {
            stage: stage.name.clone(),
            role: role.clone(),
            command: Some(match &role {
                Some(_) => spawn.preset.name.clone(),
                None => spawn.argv.last().cloned().unwrap_or_default(),
            }),
            task: task.clone(),
        });

        // stash create snapshots the tree without touching it, so uncommitted edits from an earlier stage stay out of this one's diff
        let snapshot = |repo: &Path| {
            git::git(repo, &["stash", "create"])
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| git::head(repo))
        };
        let stage_bases = (snapshot(self.root), snapshot(&self.state_root()));
        let stop_file = self.file("STOP");
        let mut result = match agent::spawn(&spawn, &mut self.writer, &stop_file, &self.rate_limit)
        {
            Ok(result) => result,
            Err(err) => {
                // Stopped means the operator ended a rate-limit wait, which is their halt and not a stage that could not start
                let stopped =
                    matches!(err, agent::AgentError::Stopped) && self.halt_operator_stop();
                if !stopped {
                    self.halt("stage", format!("{} could not start: {err}", stage.name));
                }
                return Flow::Stop;
            }
        };

        self.digest.stages_run += 1;
        self.digest.seconds += result.seconds;
        if let Some(role) = &role {
            *self.digest.role_seconds.entry(role.clone()).or_default() += result.seconds;
        }
        // an agent ran out of turns only if the preset spawned was handed this number: None passes no cap, Config reads one from the agent's own file and Time spends a clock
        if matches!(spawn.preset.turn_cap, TurnCap::Flag)
            && result
                .usage
                .turns
                .is_some_and(|t| config::spent_turn_cap(turns, t))
        {
            self.digest
                .turn_caps
                .push(format!("{}: turns {turns}", stage.name));
        }
        if result.timed_out {
            let spent = timeout.map_or(result.seconds, |t| t.as_secs());
            self.digest
                .timeouts
                .push(format!("{}: timeout {spent}s", stage.name));
        }
        if let Some(cost) = result.usage.cost {
            self.digest.cost = round4(self.digest.cost + cost);
        }
        // the provider bills cached reads and cache writes as their own lanes, disjoint from input_tokens
        let spent_tokens = result.usage.input_tokens.unwrap_or(0)
            + result.usage.output_tokens.unwrap_or(0)
            + result.usage.cache_creation_input_tokens.unwrap_or(0)
            + result.usage.cache_read_input_tokens.unwrap_or(0);
        // cost_missing is read by over_budget() at the next boundary; a budget over unreported cost can't be enforced
        if role.is_some() {
            if self.opts.budget_usd.is_some() && result.usage.cost.is_none() {
                self.cost_missing = true;
            }
            if self.opts.budget_tokens.is_some() {
                let declared = &spawn.preset.usage;
                let u = &result.usage;
                // input and output stay required so a preset with no usage table still halts
                let missing: Vec<&'static str> = [
                    ("input_tokens", true, u.input_tokens),
                    ("output_tokens", true, u.output_tokens),
                    (
                        "cache_creation_input_tokens",
                        declared.cache_creation_input_tokens.is_some(),
                        u.cache_creation_input_tokens,
                    ),
                    (
                        "cache_read_input_tokens",
                        declared.cache_read_input_tokens.is_some(),
                        u.cache_read_input_tokens,
                    ),
                ]
                .into_iter()
                .filter(|(_, required, value)| *required && value.is_none())
                .map(|(lane, _, _)| lane)
                .collect();
                if !missing.is_empty() {
                    self.tokens_missing = missing;
                }
            }
        }
        self.spent_tokens += spent_tokens;

        self.emit(Kind::StageEnd {
            stage: stage.name.clone(),
            task: task.clone(),
            seconds: result.seconds,
            exit: result.exit,
            cost: result.usage.cost,
            input_tokens: result.usage.input_tokens,
            output_tokens: result.usage.output_tokens,
            cache_creation_input_tokens: result.usage.cache_creation_input_tokens,
            cache_read_input_tokens: result.usage.cache_read_input_tokens,
            turns: result.usage.turns,
            turn_cap: Some(turns),
        });

        let beyond_wait = result.reset_beyond_wait.take();
        let flow = if let Some(signal) = agent::stop_signal() {
            self.halt(
                "signal",
                format!("{} was stopped by {signal}, exiting.", stage.name),
            );
            Flow::Stop
        } else if let Some(reset) = beyond_wait {
            self.halt(
                "stage",
                format!(
                    "{} hit a rate limit that resets {reset}, further out than the wait ceiling.",
                    stage.name
                ),
            );
            Flow::Stop
        } else if result.exit != 0 {
            self.halt(
                "stage",
                format!(
                    "{} exited {} -- halting rather than reporting a finished iteration.",
                    stage.name, result.exit
                ),
            );
            Flow::Stop
        } else {
            self.gates(stage, task.clone(), iter_bases, stage_bases, result.output)
        };
        // read from how the stage ended, so a halt or a nonzero exit never reads as a decision
        if role.as_deref() == Some("adjudicator") {
            self.digest.adjudication = match result.timed_out || matches!(flow, Flow::Stop) {
                true => Adjudication::Undecided,
                false => Adjudication::Ran,
            };
        }
        let msg = format!("{} {}", stage.name, task.unwrap_or_default());
        self.commit_state(msg.trim_end());
        flow
    }

    fn gates(
        &mut self,
        stage: &config::Stage,
        task: Option<String>,
        iter_bases: (Option<String>, Option<String>),
        stage_bases: (Option<String>, Option<String>),
        output: String,
    ) -> Flow {
        for gate in &stage.post {
            // commit-verdict judges only what the verifier wrote, not the implementer's notes from the same iteration;
            // verdict reads the status the verifier was handed
            let (base, state_base) = if matches!(gate.as_str(), "commit-verdict" | "verdict") {
                &stage_bases
            } else {
                &iter_bases
            };
            let mut ctx = GateCtx {
                root: self.root,
                cfg: self.cfg,
                task: task.clone(),
                iter_base: base.clone(),
                state_base: state_base.clone(),
                stage_output: output.clone(),
                events: &mut self.writer,
                dry_run: false,
                warnings: &mut self.digest.warnings,
                halts: &mut self.digest.halts,
                dry_rounds: &mut self.dry_rounds,
                cache: crate::config::cache_root(),
            };
            let outcome = gates::run(gate, &mut ctx);
            self.check_log();
            if outcome.halt {
                self.stopped = true;
                return Flow::Stop;
            }
            // the first refusal hands the task back to the implementer with the reason; a second
            // in the same run means no lane will answer it, and the run is not spent finding out
            if gate == "scope" && !outcome.pass {
                if let Some(task) = &task {
                    let seen = self.scope_refusals.entry(task.clone()).or_insert(0);
                    *seen += 1;
                    if *seen >= 2 {
                        let reason = format!(
                            "{task} was refused by the scope gate twice this run, and the implement round between them did not answer it: {}",
                            outcome.reason
                        );
                        self.halt(task, reason);
                        return Flow::Stop;
                    }
                }
            }
            if outcome.skip_rest {
                return Flow::SkipRest;
            }
        }
        Flow::Go
    }

    fn role_spawn(
        &mut self,
        stage: &config::Stage,
        role: &str,
        task: Option<String>,
        env: BTreeMap<String, String>,
        timeout: Option<Duration>,
        handed: &[String],
    ) -> Result<StageSpawn<'a>, Flow> {
        let levels = task_levels(&self.blocks(), task.as_deref());
        let resolved = match agent::resolve_task(&self.cfg.agent, role, &self.presets, &levels) {
            Ok(resolved) => resolved,
            Err(err) => {
                self.halt("stage", format!("{}: {err}", stage.name));
                return Err(Flow::Stop);
            }
        };

        // the CI-implies-frozen decision is the CLI's, so a test's RunOpts is not overruled by the
        // environment it happens to run in
        let frozen = self.opts.frozen;
        // a declared role is vendored to the path role_source reads first, so the fallback below stays as is
        let mut resolved_roles = Vec::new();
        if self.cfg.role.iter().any(|r| r.name == role) {
            resolved_roles = match roles::resolve(
                self.root,
                self.cfg,
                &[role.to_string()],
                &ResolveOpts {
                    frozen,
                    ..ResolveOpts::default()
                },
                &mut self.writer,
            ) {
                Ok(list) => list,
                Err(err) => {
                    self.halt("role", err.to_string());
                    return Err(Flow::Stop);
                }
            };
        }

        // the prompt tells the agent to read its role file, so the rendered file must exist before it spawns
        let Some(source) = role_source(self.root, self.cfg, role) else {
            self.halt("stage", format!("role {role} has no prompt file"));
            return Err(Flow::Stop);
        };
        let source = config::subst(&source, self.cfg);
        let ids = skills::required_ids(&source);
        let resolved_skills = match skills::resolve(
            self.root,
            self.cfg,
            Some(&resolved.preset),
            &ids,
            &ResolveOpts {
                frozen,
                ..ResolveOpts::default()
            },
            &mut self.writer,
        ) {
            Ok(list) => list,
            Err(err) => {
                self.halt("skill", err.to_string());
                return Err(Flow::Stop);
            }
        };

        if let Err(reason) = self.commit_vendored(&resolved_skills, &resolved_roles) {
            self.halt("skill", reason);
            return Err(Flow::Stop);
        }

        let rendered = skills::render(&source, &resolved_skills, &resolved.preset, self.cfg);
        let path = rendered_role_path(self.root, self.cfg, role);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(err) = std::fs::write(&path, &rendered) {
            self.halt("stage", format!("{}: {err}", path.display()));
            return Err(Flow::Stop);
        }

        Ok(StageSpawn {
            argv: agent::fill_layout(&resolved.argv, &self.cfg.layout),
            env,
            cwd: self.root,
            timeout,
            prompt: self.stage_prompt(role, handed, task.as_deref()),
            turns: turn_cap(self.cfg, stage, handed.len()),
            stage: stage.name.clone(),
            task,
            preset: resolved.preset,
        })
    }

    // a fetched skill or role and its lock entry are new to the tree; committed here so the verdict
    // gate sees them as part of the branch, not as work the implementer or verifier left uncommitted.
    fn commit_vendored(
        &mut self,
        skills: &[skills::ResolvedSkill],
        roles: &[roles::ResolvedRole],
    ) -> Result<(), String> {
        let vendored: Vec<(&str, &Path, &str)> = skills
            .iter()
            .map(|s| (s.id.as_str(), s.dir.as_path(), s.result.as_str()))
            .chain(
                roles
                    .iter()
                    .map(|r| (r.name.as_str(), r.path.as_path(), r.result.as_str())),
            )
            .collect();
        if vendored.is_empty() || !self.vendored_dirty(&vendored) {
            return Ok(());
        }
        let dir = &self.cfg.layout.harness_dir;
        let mut paths = vec![self.rel("harness.lock")];
        for (_, path, _) in &vendored {
            if let Ok(rel) = path.strip_prefix(self.root) {
                paths.push(rel.to_string_lossy().to_string());
            }
        }
        paths.retain(|p| !git::locate(self.root, dir, p).2);
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        let ids: Vec<&str> = vendored.iter().map(|(id, _, _)| *id).collect();
        let msg = format!("chore(vendor): {}", ids.join(" "));
        git::commit_paths(self.root, &refs, &msg)
            .and_then(|_| git::commit_instance(self.root, dir, &[], &msg))
            .map(|_| ())
            .map_err(|err| format!("vendored skills could not be committed: {err}"))
    }

    // "fetched" is the ordinary signal; the porcelain fallback also catches a vendored path left
    // untracked by an earlier run.
    fn vendored_dirty(&self, vendored: &[(&str, &Path, &str)]) -> bool {
        if vendored.iter().any(|(_, _, result)| *result == "fetched") {
            return true;
        }
        let lock = self.rel("harness.lock");
        git::porcelain(self.root).iter().any(|line| {
            let path = line.get(3..).unwrap_or("");
            path == lock
                || vendored.iter().any(|(_, dir, _)| {
                    dir.strip_prefix(self.root)
                        .map(|rel| path.starts_with(rel.to_string_lossy().as_ref()))
                        .unwrap_or(false)
                })
        })
    }

    fn command_spawn(
        &self,
        stage: &config::Stage,
        command: &str,
        task: Option<String>,
        env: BTreeMap<String, String>,
        timeout: Option<Duration>,
    ) -> StageSpawn<'a> {
        StageSpawn {
            argv: vec![
                "sh".to_string(),
                "-c".to_string(),
                config::subst(command, self.cfg),
            ],
            env,
            cwd: self.root,
            timeout,
            prompt: String::new(),
            turns: stage.turns,
            stage: stage.name.clone(),
            task,
            preset: shell_preset(),
        }
    }

    // one wording for the operator's two halts, read here and by the spawn arm in stage
    fn halt_operator_stop(&mut self) -> bool {
        if let Some(signal) = agent::stop_signal() {
            self.halt("signal", format!("stopped by {signal}, exiting."));
            return true;
        }
        if self.file("STOP").is_file() {
            self.halt("stop", "STOP file found, exiting.".to_string());
            return true;
        }
        false
    }

    // order matters: a signal, then the STOP file, then budgets, then a new needs-info task
    fn boundary(&mut self, needs_spec: bool) -> bool {
        if self.stopped {
            return true;
        }
        if self.halt_operator_stop() {
            return true;
        }
        if let Some(reason) = self.over_budget() {
            self.halt("budget", reason);
            return true;
        }
        if needs_spec {
            if let Some(reason) = self.new_needs_spec() {
                self.halt("needs-info", reason);
                return true;
            }
        }
        false
    }

    // checked before each stage, never during one -- a half-finished stage is worse than a slow run
    fn over_budget(&self) -> Option<String> {
        if self.cost_missing {
            return Some("BUDGET_USD is set and the last stage reported no cost, so the budget cannot be enforced. The agent command must print a cost [agent.usage].cost can read (claude: add --output-format json), or unset BUDGET_USD.".to_string());
        }
        if !self.tokens_missing.is_empty() {
            return Some(format!("BUDGET_TOKENS is set and the last stage left a declared token lane unreported, so the budget cannot be enforced. The agent command must print the counts [agent.usage].{} can read, or unset BUDGET_TOKENS.", self.tokens_missing.join(" and .")));
        }
        if let Some(budget) = self.opts.budget_seconds.filter(|b| *b > 0) {
            if self.digest.seconds >= budget {
                return Some(format!(
                    "the run has spent {}s of its {budget}s budget.",
                    self.digest.seconds
                ));
            }
        }
        if let Some(budget) = self.opts.budget_usd {
            if self.digest.cost >= budget {
                return Some(format!(
                    "the run has spent ${} of its ${budget} budget.",
                    self.digest.cost
                ));
            }
        }
        if let Some(budget) = self.opts.budget_tokens.filter(|b| *b > 0) {
            if self.spent_tokens >= budget {
                return Some(format!(
                    "the run has spent {} tokens of its {budget} token budget.",
                    self.spent_tokens
                ));
            }
        }
        None
    }

    // catches a proposed block whose fix needs the spec/context but didn't say so and halt
    fn proposed_names_contract(&self) -> Option<String> {
        let spec = &self.cfg.layout.spec;
        let context = &self.cfg.layout.context_file;
        let short = spec
            .strip_prefix(&format!("{}/", self.cfg.layout.harness_dir))
            .unwrap_or(spec);
        self.blocks()
            .iter()
            .filter(|b| tracker::state(b).as_deref() == Some("needs-triage"))
            .find(|b| {
                let text = block::block_text(b);
                text.contains(short) || text.contains(context.as_str())
            })
            .map(|b| {
                format!(
                    "{} is still proposed and its fix names {spec} or {context}. Neither is a lane's to edit.",
                    b.id
                )
            })
    }

    // halts only on a NEW needs-info task; pre-existing ones are the ordinary state of an open contract
    fn new_needs_spec(&self) -> Option<String> {
        self.ids_at(&["needs-info"])
            .into_iter()
            .find(|id| !self.needs_spec_at_start.contains(id))
            .map(|id| {
                format!(
                    "{id} went to needs-info. The contract does not answer it and no lane may decide it."
                )
            })
    }

    fn halt(&mut self, name: &str, reason: String) {
        println!("HALT: {reason}");
        self.digest.halts.push(reason.clone());
        self.stopped = true;
        self.emit(Kind::Halt {
            halt: name.to_string(),
            reason,
        });
    }

    fn choose(&mut self) -> Option<config::Pipeline> {
        for i in 0..self.cfg.pipeline.len() {
            if !self.opts.pipelines.is_empty()
                && !self.opts.pipelines.contains(&self.cfg.pipeline[i].name)
            {
                continue;
            }
            if self.spent.contains(&self.cfg.pipeline[i].name) {
                continue;
            }
            let Ok(when) = config::parse_when(&self.cfg.pipeline[i].when) else {
                continue;
            };
            if holds(self.root, self.cfg, &when, &mut self.digest.warnings) {
                return Some(self.cfg.pipeline[i].clone());
            }
        }
        None
    }

    fn promotions(&mut self, ready_before: &[String], rejections_before: &[String]) {
        for id in self.ids_at(&tracker::READY) {
            if !ready_before.contains(&id) {
                self.digest.promoted.push(id);
            }
        }
        for line in self.rejections() {
            if !rejections_before.contains(&line) {
                self.digest.killed.push(line);
            }
        }
    }

    fn task_outcome(&mut self, task: &str, pipeline: &config::Pipeline, progress_before: u64) {
        let block = self.blocks().into_iter().find(|b| b.id == task);
        let status = block.as_ref().and_then(|b| block::field(b, "status"));
        match block.as_ref().and_then(tracker::state).as_deref() {
            Some("done") => {
                self.digest.landed.push(task.to_string());
                let rows = self
                    .blocks()
                    .iter()
                    .find(|b| b.id == task)
                    .and_then(|b| block::field(b, "rows"));
                if let Some(rows) = rows.filter(|r| !r.is_empty()) {
                    self.digest.rows.push(format!("{task}: {rows}"));
                }
                if self.opts.pr_per_task || self.cfg.pr.per_task {
                    self.open_pull(task);
                }
                self.settle_thread(task);
            }
            _ => self.digest.warnings.push(format!(
                "{task} ended the iteration at {}, not done.",
                status.as_deref().unwrap_or("no status")
            )),
        }
        // the entry is all the next iteration inherits, so the launcher writes the facts it has
        // rather than warning that a role left none -- a verify-only round has no other record
        if file_len(&self.file("PROGRESS.md")) <= progress_before {
            self.progress_stub(task, pipeline, status.as_deref());
        }
    }

    // a landed block answers the thread that raised it, or the next read queues the same comment again
    fn settle_thread(&mut self, task: &str) {
        let thread = self
            .blocks()
            .iter()
            .find(|b| b.id == task)
            .and_then(review::thread_of);
        let Some(thread) = thread else {
            return;
        };
        let sha = git::git(
            self.root,
            &[
                "log",
                "-1",
                "--format=%H",
                "--fixed-strings",
                "--grep",
                task,
            ],
        )
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| git::git(self.root, &["rev-parse", "HEAD"]).ok())
        .unwrap_or_default();
        if let Err(err) = review::settle(self.root, &thread, sha.trim()) {
            self.digest
                .warnings
                .push(format!("{task}: thread {thread} was left open: {err}"));
        }
    }

    // the loop cannot wait on a reviewer: an anchored comment becomes a proposed block the next lane takes
    fn read_reviews(&mut self) {
        let pulls = match review::open_pulls(self.root) {
            Ok(pulls) => pulls,
            Err(err) => {
                self.digest.warnings.push(err.to_string());
                return;
            }
        };
        let t = Tracker::open(self.root, &self.cfg.layout.harness_dir);
        let spec = tracker::repo_slug(self.root);
        let decisions = std::fs::read_to_string(self.file("DECISIONS.md")).unwrap_or_default();
        for pull in &pulls {
            let found = match review::read(&pull.url) {
                Ok(found) => found,
                Err(err) => {
                    self.digest.warnings.push(err.to_string());
                    continue;
                }
            };
            if found.short {
                self.digest.warnings.push(format!(
                    "{}: only the first 100 reviews, threads and comments were read",
                    pull.url
                ));
            }
            let appended = t
                .blocks()
                .map_err(|e| e.to_string())
                .and_then(|blocks| {
                    review::append(&blocks, &decisions, &found).map_err(|e| e.to_string())
                })
                .and_then(|a| {
                    for block in &a.blocks {
                        t.append(&spec, block).map_err(|e| e.to_string())?;
                    }
                    Ok(a.blocks.len())
                });
            match appended {
                Ok(0) => {}
                Ok(n) => println!("  proposed: {n} from {}", pull.url),
                Err(err) => self.digest.warnings.push(format!("the queue: {err}")),
            }
        }
    }

    // the launcher pushes, never the lane: the lane's argv denies `git push`
    fn open_pull(&mut self, task: &str) {
        let opts = pr::PrOpts {
            push: true,
            policy_read: false,
            stack_on: self.built.clone(),
        };
        match pr::build(self.root, &[task.to_string()], &opts) {
            Ok(report) => {
                self.built.push(task.to_string());
                let opened = report.opened.unwrap_or_default();
                self.digest.pulls.push(format!("{task}: {opened}"));
            }
            Err(err) => {
                let reason = err.to_string();
                let first = reason.lines().next().unwrap_or_default();
                self.digest
                    .pulls
                    .push(format!("{task}: none opened, {first}"));
                self.digest
                    .warnings
                    .push(format!("{task} opened no pull request: {reason}"));
            }
        }
    }

    fn progress_stub(&mut self, task: &str, pipeline: &config::Pipeline, status: Option<&str>) {
        // `friction: none` exactly: friction-repeat skips that word and would group any other phrasing
        let entry = format!(
            "\n## {date} — {task} — {name} pipeline, left at {status}\n\
             what happened: no role wrote an entry this iteration {iter}, so the launcher wrote \
             this one. Stages: {stages}. The round's reasoning is in the task's own notes in \
             its ticket.\nfriction: none\n",
            date = jiff::Zoned::now().strftime("%Y-%m-%d"),
            name = pipeline.name,
            status = status.unwrap_or("no status"),
            iter = self.writer.iter,
            stages = pipeline.stages.join(", "),
        );
        let path = self.file("PROGRESS.md");
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        if let Err(err) = std::fs::write(&path, format!("{existing}{entry}")) {
            self.digest
                .warnings
                .push(format!("PROGRESS.md: {task} got no entry: {err}"));
            return;
        }
        if let Err(err) = git::commit_instance(
            self.root,
            &self.cfg.layout.harness_dir,
            &["PROGRESS.md"],
            &format!("chore(progress): {task} iteration {}", self.writer.iter),
        ) {
            self.digest.warnings.push(format!(
                "PROGRESS.md: the launcher's entry is uncommitted: {err}"
            ));
        }
    }

    fn blocks(&self) -> Vec<block::Block> {
        queue_blocks(self.root, self.cfg)
    }

    fn ids_at(&self, states: &[&str]) -> Vec<String> {
        tracker::ids_in(&self.blocks(), states)
    }

    fn first_at_review(&self) -> Option<String> {
        self.ids_at(&["review"]).into_iter().next()
    }

    // the prompt carries the ids the stage was sized for, so it decides those and no others
    fn stage_prompt(&self, role: &str, ids: &[String], task: Option<&str>) -> String {
        let ticket: Vec<&str> = match task {
            Some(t) => vec![t],
            None => ids.iter().map(String::as_str).collect(),
        };
        let spec = match (role, task) {
            ("decomposer", Some(slug)) => config::instance_rel(
                self.root,
                &self.cfg.layout.harness_dir,
                &format!("{}/{slug}/spec.md", tracker::SPECS),
            ),
            _ => spec_for(self.root, self.cfg, &ticket),
        };
        let mut prompt = prompt_for(role, self.cfg, &spec);
        if role == "decomposer" {
            prompt.push_str(&format!(" The spec to decompose is {spec}."));
        }
        if role == "implementer" {
            if let Some(clause) = task.and_then(|t| self.prior_attempts(t)) {
                prompt.push(' ');
                prompt.push_str(&clause);
            }
        }
        if ids.is_empty() {
            return prompt;
        }
        format!(
            "{prompt} Act on exactly these tickets, in this order: {}. Leave every other needs-triage ticket alone.",
            ids.join(", ")
        )
    }

    // a commit naming the task the implementer is about to take is an attempt no verdict accepted:
    // an accepted one leaves the task `done`, which no implementer takes
    fn prior_attempts(&self, task: &str) -> Option<String> {
        let log = git::git(
            self.root,
            &["log", "--format=%h %s", &format!("--grep={task}")],
        )
        .ok()?;
        let shows: Vec<String> = log
            .lines()
            .filter_map(|line| line.split_once(' '))
            .filter(|(_, subject)| names(subject, task))
            .map(|(sha, _)| format!("`git show {sha}`"))
            .collect();
        if shows.is_empty() {
            return None;
        }
        let answer = self
            .blocks()
            .iter()
            .find(|b| b.id == task)
            .and_then(rejection_text)
            .and_then(|text| text.lines().next().map(str::trim).map(String::from))
            .filter(|line| !line.is_empty())
            .map(|line| format!(" The line to answer is: {line}"))
            .unwrap_or_default();
        Some(format!(
            "{task} carries a prior implementation attempt no verdict accepted. \
             Read {} and the ticket's `verdict:` and `gate:` lines before you write any code. \
             Do not re-send a diff an attempt already had rejected for the same reason.{answer}",
            shows.join(", ")
        ))
    }

    // a rejection the verifier just wrote is already queued as the task's own work; a block promoted
    // over the top of it is a second copy of one job
    fn refuse_restated(&mut self, task: Option<&str>, ready_before: &[String]) {
        let Some(task) = task else {
            return;
        };
        let blocks = self.blocks();
        let rejected = blocks
            .iter()
            .find(|b| b.id == task)
            .filter(|b| tracker::state(b).is_some_and(|s| tracker::READY.contains(&s.as_str())));
        let Some(claims) = rejected.and_then(rejection_text).map(|v| claims(&v)) else {
            return;
        };
        let restated: Vec<(String, String)> = self
            .ids_at(&tracker::READY)
            .into_iter()
            .filter(|id| id != task && !ready_before.contains(id))
            .filter_map(|id| {
                let text = blocks
                    .iter()
                    .find(|b| b.id == id)
                    .map(|b| squash(&block::block_text(b)))?;
                let claim = claims.iter().find(|c| text.contains(&squash(c)))?;
                Some((id, claim.clone()))
            })
            .collect();
        for (id, verdict) in restated {
            let reason = format!("{id} restates the verdict on {task}: {verdict}");
            let t = Tracker::open(self.root, &self.cfg.layout.harness_dir);
            if let Err(err) = t.set_status(&id, "needs-triage", &reason) {
                self.digest.warnings.push(format!("the queue: {err}"));
                continue;
            }
            self.commit_state(&format!("queue: {reason}"));
            self.digest.warnings.push(format!(
                "{id} was put back to proposed: it restates the verdict on {task} ({verdict})."
            ));
        }
    }

    fn first_attended_ready(&self) -> Option<String> {
        self.blocks()
            .iter()
            .find(|b| tracker::state(b).as_deref() == Some("ready-for-human"))
            .map(|b| b.id.clone())
    }

    fn rejections(&self) -> Vec<String> {
        tracker::kill_lines(&self.file("")).unwrap_or_default()
    }

    fn emit(&mut self, kind: Kind) {
        self.writer.emit(kind);
        self.check_log();
    }

    // an event that never reached the log is one `enallagi watch` and the probes will never see
    fn check_log(&mut self) {
        let Some(err) = self.writer.last_error().map(str::to_string) else {
            return;
        };
        let reason = format!("the event log cannot be written: {err}");
        if !self.digest.halts.contains(&reason) {
            println!("HALT: {reason}");
            self.digest.halts.push(reason);
        }
        self.stopped = true;
    }
}

// shared by the no-TUI path (finish, above) and the TUI path (cli::run, after the view is left),
// so the two never drift into printing different things for the same run.
pub fn digest_text(digest: &Digest) -> String {
    let mut out = format!("\n=== digest: {} iteration(s) ===\n", digest.iterations);
    let cost = if digest.cost > 0.0 {
        format!(", cost ${}", digest.cost)
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        "wall clock: {}s across {} stage(s){cost}",
        digest.seconds, digest.stages_run
    );
    for (role, seconds) in &digest.role_seconds {
        let _ = writeln!(out, "  {role}: {seconds}s");
    }
    let _ = writeln!(out, "tasks landed:{}", inline(&digest.landed));
    listing(&mut out, "rows turned green:", &digest.rows);
    if !digest.pulls.is_empty() {
        listing(&mut out, "pull requests:", &digest.pulls);
    }
    // nothing decided reads as nothing to decide, so say which of the three states the round was in
    if digest.promoted.is_empty() && digest.killed.is_empty() {
        let state = match digest.adjudication {
            Adjudication::NoStage => "no adjudicate stage ran",
            Adjudication::Ran => "the adjudicator ran and decided nothing",
            Adjudication::Undecided => "the adjudicator could not decide",
        };
        let _ = writeln!(out, "findings: {state}");
    } else {
        let _ = writeln!(out, "findings promoted:{}", inline(&digest.promoted));
        listing(&mut out, "findings killed:", &digest.killed);
    }
    let _ = writeln!(
        out,
        "proposed: {} standing, oldest {} rounds, expired {}",
        digest.proposed_standing, digest.proposed_oldest, digest.expired
    );
    let l = &digest.learnings;
    let _ = writeln!(
        out,
        "learnings: {} proposed, {} promoted, {} killed, {} expired, {} removed",
        l.proposed.len(),
        l.promoted.len(),
        l.killed.len(),
        l.expired.len(),
        l.removed.len()
    );
    for (word, classes) in [
        ("proposed", &l.proposed),
        ("promoted", &l.promoted),
        ("killed", &l.killed),
        ("expired", &l.expired),
        ("removed", &l.removed),
    ] {
        for class in classes {
            let _ = writeln!(out, "  {word} `{class}`");
        }
    }
    if !digest.turn_caps.is_empty() {
        listing(&mut out, "turn caps hit:", &digest.turn_caps);
    }
    if !digest.timeouts.is_empty() {
        listing(&mut out, "timeouts hit:", &digest.timeouts);
    }
    listing(&mut out, "halts:", &digest.halts);
    listing(&mut out, "warnings:", &digest.warnings);
    let _ = writeln!(
        out,
        "Loop finished after {} iteration(s).",
        digest.iterations
    );
    out
}

fn inline(items: &[String]) -> String {
    if items.is_empty() {
        " none".to_string()
    } else {
        format!(" {}", items.join(" "))
    }
}

fn listing(out: &mut String, heading: &str, items: &[String]) {
    let _ = writeln!(out, "{heading}");
    if items.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for item in items {
        let _ = writeln!(out, "  {item}");
    }
}

// the verifier's reason, else the newest gate's: set_status writes it first
fn rejection_text(b: &block::Block) -> Option<String> {
    match tracker::verdict(b).map(|r| r.verdict) {
        Some(tracker::Verdict::Rejected(reason)) if !reason.is_empty() => Some(reason),
        _ => block::field(b, "gate").filter(|v| !v.trim().is_empty()),
    }
}

// a verdict is a paragraph and a promotion copies one claim of it, never the whole thing. A run
// under five words recurs by chance, and one quoting a command or a path is boilerplate two blocks
// share without either restating the other.
fn claims(verdict: &str) -> Vec<String> {
    verdict
        .split(['.', '\n'])
        .filter(|s| !s.contains('`') && s.split_whitespace().count() >= 5)
        .map(|s| s.trim().to_string())
        .collect()
}

// `T-13` must not be read out of the subject of `T-134`: an id is named as a whole word
fn names(subject: &str, task: &str) -> bool {
    subject.match_indices(task).any(|(at, _)| {
        let before = subject[..at].chars().next_back();
        let after = subject[at + task.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

// lowercase, one space between words: two statements of the same sentence compare equal
fn squash(text: &str) -> String {
    text.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn clarifications(spec: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(spec).ok()?;
    let hits: Vec<String> = text
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            l.contains("[NEEDS CLARIFICATION]") && !l.contains("`[NEEDS CLARIFICATION]`")
        })
        .take(3)
        .map(|(n, l)| format!("{}:{l}", n + 1))
        .collect();
    (!hits.is_empty()).then_some(hits)
}

fn waiting_classes(root: &Path, cfg: &Config) -> Vec<String> {
    audit::waiting(root, cfg)
        .map(|w| w.into_iter().map(|w| w.class).collect())
        .unwrap_or_default()
}

fn holds(root: &Path, cfg: &Config, when: &Predicate, warnings: &mut Vec<String>) -> bool {
    match when {
        Predicate::Not(inner) => !holds(root, cfg, inner, warnings),
        Predicate::QueueTakeable => tracker::takeable(&queue_blocks(root, cfg)).is_some(),
        // an unreadable or unparseable queue is not evidence of a task at review, so this fails closed like `takeable`
        Predicate::QueueReviewing => match queue_read(root, cfg) {
            Ok(blocks) => !tracker::ids_in(&blocks, &["review"]).is_empty(),
            Err(err) => {
                warnings.push(format!(
                    "the queue: {err}; queue.reviewing treated as false (fail closed)"
                ));
                false
            }
        },
        // an unreadable or unparseable queue is not evidence of a block waiting on the adjudicator, so this fails closed like `takeable`
        Predicate::QueueProposed => match queue_read(root, cfg) {
            Ok(blocks) => {
                !tracker::ids_in(&blocks, &["needs-triage"]).is_empty()
                    || !waiting_classes(root, cfg).is_empty()
            }
            Err(err) => {
                warnings.push(format!(
                    "the queue: {err}; queue.proposed treated as false (fail closed)"
                ));
                false
            }
        },
        // an unreadable or unparseable queue is not evidence the queue is empty, so this fails closed like `takeable`
        Predicate::QueueEmpty => match queue_read(root, cfg) {
            Ok(blocks) => blocks.is_empty(),
            Err(err) => {
                warnings.push(format!(
                    "the queue: {err}; queue.empty treated as false (fail closed)"
                ));
                false
            }
        },
        Predicate::TaskAttended => {
            let first = queue_blocks(root, cfg)
                .iter()
                .filter_map(tracker::state)
                .find(|s| tracker::READY.contains(&s.as_str()));
            first.as_deref() == Some("ready-for-human")
        }
        Predicate::SpecUndecomposed => {
            !tracker::undecomposed(root, &cfg.layout.harness_dir).is_empty()
        }
        Predicate::CheckRed => gates::check_delta(root, cfg, false).red,
        Predicate::Probe(name) => {
            let check = check_outcome(root, cfg);
            let ctx = ProbeCtx {
                root,
                cfg,
                check: Some(&check),
                driver: std::env::var("ENALLAGI_DRIVER").as_deref() == Ok("1"),
            };
            probes::run_all(&ctx, std::slice::from_ref(name))
                .iter()
                .any(|(_, result)| matches!(result, probes::ProbeResult::Count(f) if !f.is_empty()))
        }
    }
}

// `ran` means the check STARTED, not that it named what failed --
// reporting ran:false there would turn a real finding into an ERROR
fn check_outcome(root: &Path, cfg: &Config) -> CheckOutcome {
    let mut outcome = gates::check_delta(root, cfg, true).outcome();
    // nested under turbo the check would recurse, so it is not run at all
    if std::env::var_os("TURBO_HASH").is_some() {
        outcome.ran = false;
    }
    outcome
}

fn config_sha256(root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(config::DEFAULT_TOML.as_bytes());
    if let Ok(text) = std::fs::read_to_string(config::config_path(root)) {
        hasher.update(text.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn shell_preset() -> Preset {
    Preset {
        name: "sh".to_string(),
        argv: vec!["sh".to_string(), "-c".to_string()],
        turn_cap: TurnCap::None,
        usage: agent::UsagePaths::default(),
        skills_dir: None,
        invocation: None,
        instruction_file: None,
        hooks_file: None,
        hook_events: BTreeMap::new(),
        env: BTreeMap::new(),
        model_flag: None,
        effort_flag: None,
        bypass_flag: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_quotes_reason() {
        let ticket = |harness: &str| {
            tracker::parse_ticket(&format!("# T-001: t\n\n**Status:** ready-for-agent\n\n## Comments\n\nREJECTED: prose\n\n## Harness\n\ngate: the newest gate\ngate: an older gate\n{harness}\n")).expect("ticket")
        };
        let text = |harness| rejection_text(&ticket(harness));
        assert_eq!(
            text("verdict: rejected by verifier: the red covers no branch").as_deref(),
            Some("the red covers no branch")
        );
        for fallback in [
            "verdict: rejected by verifier",
            "verdict: answered by implementer",
            "",
        ] {
            assert_eq!(
                text(fallback).as_deref(),
                Some("the newest gate"),
                "{fallback}"
            );
        }
    }

    #[test]
    fn undecomposed_spec() {
        let repo = crate::fixture::Repo::new();
        let mut cfg = Config::default();
        cfg.layout.harness_dir = ".enallagi".into();
        let when = config::parse_when("spec.undecomposed").expect("a predicate");
        let held = || holds(&repo.root, &cfg, &when, &mut Vec::new());
        repo.write(
            ".enallagi/specs/app/spec.md",
            "# App\n\n**Status:** needs-triage\n",
        );
        assert!(!held());
        repo.write(
            ".enallagi/specs/app/spec.md",
            "# App\n\n**Status:** ready-for-agent\n",
        );
        repo.write(".enallagi/specs/app/issues/01-draft.md", "# 01: draft\n");
        assert!(held());
        assert_eq!(tracker::undecomposed(&repo.root, ".enallagi"), ["app"]);
        Tracker::open(&repo.root, ".enallagi")
            .append("app", "## [T-001] one\nstatus: needs-triage\n")
            .expect("append");
        assert!(!held());
    }

    #[test]
    fn spec_token_ticket() {
        let repo = crate::fixture::Repo::new();
        let t = Tracker::open(&repo.root, ".enallagi");
        t.append("b", "## [T-001] one\nstatus: ready-for-agent\n")
            .expect("append");
        t.append("c", "## [T-002] two\nstatus: review\n")
            .expect("append");
        let mut cfg = Config::default();
        cfg.layout.harness_dir = ".enallagi".into();
        cfg.layout.spec = ".enallagi/specs/a/spec.md".into();
        let prompt =
            |ids: &[&str]| prompt_for("implementer", &cfg, &spec_for(&repo.root, &cfg, ids));
        let one = prompt(&["T-002"]);
        assert!(one.contains(", .enallagi/specs/c/spec.md,"), "{one}");
        assert!(!one.contains("specs/a/"), "{one}");
        let two = prompt(&["T-001", "T-002"]);
        assert!(
            two.contains("the `spec.md` beside each ticket's `issues/` directory"),
            "{two}"
        );
    }

    #[test]
    fn lane_prompt_names_tracker() {
        let mut cfg = Config::default();
        cfg.layout.harness_dir = ".enallagi".into();
        let prompt = prompt_for("implementer", &cfg, "");
        for want in [
            "`.enallagi/specs/<spec>/issues/T-NNN-<slug>.md`",
            "global",
            "`needs-triage`",
            "`needs-info`",
            "`ready-for-agent`",
            "`ready-for-human`",
            "`wontfix`",
            "`review`",
            "`done`",
            "`## Comments`",
            "`## Harness`",
            "/setup-matt-pocock-skills",
        ] {
            assert!(prompt.contains(want), "{want}: {prompt}");
        }
        assert!(!prompt.contains("__"), "{prompt}");
    }

    #[test]
    fn lanes_read_the_earned_rules_range() {
        use std::io::Write as _;
        use std::process::{Command, Stdio};
        let cfg = Config::default();
        let script = |role: &str| {
            let prompt = prompt_for(role, &cfg, "");
            let from = prompt.find("sed -n '").expect(role) + "sed -n '".len();
            let len = prompt[from..].find('\'').expect(role);
            prompt[from..from + len].to_string()
        };
        let sed = script("implementer");
        assert_eq!(sed, script("verifier"));
        // an older install holds its proposed learnings between the earned and rejected sections
        let mut child = Command::new("sed")
            .args(["-n", &sed])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("sed");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(b"# D\n\n## Earned rules\n\n- earned\n\n## Proposed learnings\n\n- [proposed] `p`: x\n\n## Rejected findings\n\n- r\n")
            .expect("write");
        let out = child.wait_with_output().expect("sed").stdout;
        let out = String::from_utf8_lossy(&out);
        assert!(out.contains("- earned"), "{out}");
        assert!(!out.contains("[proposed]"), "{out}");
    }
}
