//! the launcher: runs pipelines from `enallagi.toml` with halts and gates

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
use crate::sandbox::Sandbox;
use crate::skills::{self, ResolveOpts};
use crate::tracker::{self, Tracker};
use crate::{archive, audit, git, pr, review};

// a lane running ps to check for competing writers must ignore its parent
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

// an installed role file already names the default spec, so the ticket's replaces it at render
fn role_text(source: &str, cfg: &Config, spec: &str) -> String {
    let text = config::subst(source, cfg);
    match cfg.layout.spec.is_empty() {
        true => text,
        false => text.replace(&cfg.layout.spec, spec),
    }
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
    // overrides [pr] per_task only toward true
    pub pr_per_task: bool,
    // a writer lane's one ticket: no other is taken and no queue-writing stage runs
    pub task: Option<String>,
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
            pr_per_task: false,
            task: None,
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

// empty promoted/killed means nothing to decide only if the stage spawned and ended cleanly
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

/// refuses a `ready` block whose scope or criteria an import left for the adjudicator
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

/// refuses on drift from the configured check; a probe that could not run refuses too
pub fn preflight(root: &Path) -> anyhow::Result<()> {
    let cfg = config::load(root).map_err(|e| Refused(e.to_string()))?;
    if let Some(reason) = crate::sandbox::refusal(root, &cfg) {
        return Err(Refused(reason).into());
    }
    refuse_unfilled(root, &cfg)?;
    // a set outcome keeps run_all from running the check; neither probe reads it
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

fn taken(blocks: &[block::Block], pin: Option<&str>) -> Option<String> {
    tracker::takeable_by(blocks, |b| pin.is_none_or(|p| b.id == p))
}

fn reviewed(blocks: &[block::Block], pin: Option<&str>) -> Option<String> {
    tracker::ids_in(blocks, &["review"])
        .into_iter()
        .find(|id| pin.is_none_or(|p| id == p))
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
    let sandbox = Sandbox::start(root, cfg);

    for pipeline in cfg.pipeline.iter() {
        if !pipelines.is_empty() && !pipelines.contains(&pipeline.name) {
            continue;
        }
        let when = config::parse_when(&pipeline.when)?;
        if !holds(&sandbox, root, cfg, &when, None, &mut warnings) {
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
        // ENALLAGI_DRIVER only here and in scout: it installs throwaway repos and costs wall clock
        let check = check_outcome(&sandbox, root, cfg);
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
    let cfg = config::load(root).map_err(|e| Refused(e.to_string()))?;
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

    let refusals = auto_refusals(root, &cfg, opts);
    if !refusals.is_empty() {
        return Err(Refused(refusals.join("\n")).into());
    }

    if opts.dry_run {
        print!(
            "{}",
            plan(root, &cfg, &opts.pipelines).map_err(|e| Refused(e.to_string()))?
        );
        return Ok(Digest::default());
    }

    let sandbox = Sandbox::start(root, &cfg);
    let mut looper = Loop {
        root,
        cfg: &cfg,
        sandbox: &sandbox,
        denied: 0,
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
        refusals: std::collections::HashMap::new(),
        stopped: false,
        built: Vec::new(),
        noted: Default::default(),
    };
    looper.go()
}

// every reason `merge = "auto"` may not start, so the operator fixes them in one pass
fn auto_refusals(root: &Path, cfg: &Config, opts: &RunOpts) -> Vec<String> {
    if cfg.pr.merge != config::MergeMode::Auto {
        return Vec::new();
    }
    let mut out = Vec::new();
    let t = tracker::Tracker::open(root, &cfg.layout.harness_dir);
    let open = t
        .blocks()
        .unwrap_or_default()
        .into_iter()
        .filter(|b| !matches!(tracker::state(b).as_deref(), Some("done" | "wontfix")));
    let sets = gates::holdout_specs(root, cfg);
    // an accepted spec not yet decomposed is open work too
    let undecomposed = tracker::undecomposed(root, &cfg.layout.harness_dir);
    if !open
        .filter_map(|b| t.spec_of(&b.id))
        .chain(undecomposed)
        .any(|spec| sets.contains(&spec))
    {
        out.push(format!(
            "merge = \"auto\": no spec with open work has a holdout set under {}/",
            config::instance_rel(&cfg.layout.harness_dir, "holdouts")
        ));
    }
    if opts.budget_usd.is_none() && opts.budget_tokens.is_none() {
        out.push(
            "merge = \"auto\" needs BUDGET_USD or BUDGET_TOKENS (or --budget-usd / --budget-tokens); BUDGET_SECONDS alone does not bound spend".into(),
        );
    }
    let rel = config::instance_rel(&cfg.layout.harness_dir, "holdouts");
    let (state, inner, _) = git::locate(root, &cfg.layout.harness_dir, &rel);
    for (repo, path) in [(root, rel.as_str()), (state.as_path(), inner.as_str())] {
        if git::git(repo, &["ls-files", "--", path]).is_ok_and(|f| !f.is_empty()) {
            out.push(format!(
                "merge = \"auto\": {} tracks a holdout, which any lane can read through git",
                repo.display()
            ));
        }
    }
    if cfg.sandbox.mode == config::SandboxMode::Off {
        out.push("merge = \"auto\" refuses [sandbox] mode = \"off\"".into());
    }
    out
}

// "is a loop running here?" for archive_done and the one-writer hook; Drop removes it
struct PidFile(PathBuf);

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct Loop<'a> {
    root: &'a Path,
    cfg: &'a Config,
    sandbox: &'a Sandbox,
    // the proxy refusals already emitted
    denied: usize,
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
    // pipelines whose dry rounds are spent; `choose` skips them until takeable work returns
    spent: Vec<String>,
    // scope and holdout refusals per (gate, task) this run: a second is one no lane will answer
    refusals: std::collections::HashMap<(String, String), u32>,
    stopped: bool,
    // tasks whose pull-request branch this run built, in order, so a dependent task stacks on one
    built: Vec<String>,
    // the parent tip the last concurrent-change note reached
    noted: std::cell::RefCell<Option<String>>,
}

/// vendors in the parent, so parallel lanes never both write `harness.lock`
pub fn vendor_lanes(root: &Path, cfg: &Config) -> Result<(), String> {
    let presets = agent::presets();
    let preset = presets.get(&cfg.agent.preset);
    let opts = ResolveOpts::default();
    let mut events = Writer::new(Log::open(&root.join(&cfg.layout.harness_dir)));
    let ids: Vec<String> = cfg.skill.iter().map(|s| s.id.clone()).collect();
    let names: Vec<String> = cfg.role.iter().map(|r| r.name.clone()).collect();
    // ponytail: skills vendor for the default preset; a role on another agent still vendors in its lane
    let skills =
        skills::resolve(root, cfg, preset, &ids, &opts, &mut events).map_err(|e| e.to_string())?;
    let roles = roles::resolve(root, cfg, &names, &opts, &mut events).map_err(|e| e.to_string())?;
    commit_vendored(root, cfg, &skills, &roles)
}

// a fetched skill or role is new to the tree; committed so a gate never reads it as left work
fn commit_vendored(
    root: &Path,
    cfg: &Config,
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
    if vendored.is_empty() || !vendored_dirty(root, cfg, &vendored) {
        return Ok(());
    }
    let dir = &cfg.layout.harness_dir;
    let mut paths = vec![config::instance_rel(dir, "harness.lock")];
    for (_, path, _) in &vendored {
        if let Ok(rel) = path.strip_prefix(root) {
            paths.push(rel.to_string_lossy().to_string());
        }
    }
    paths.retain(|p| !git::locate(root, dir, p).2);
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let ids: Vec<&str> = vendored.iter().map(|(id, _, _)| *id).collect();
    let msg = format!("chore(vendor): {}", ids.join(" "));
    git::commit_paths(root, &refs, &msg)
        .and_then(|_| git::commit_instance(root, dir, &[], &msg))
        .map(|_| ())
        .map_err(|err| format!("vendored skills could not be committed: {err}"))
}

// porcelain also catches a vendored path an earlier run left untracked
fn vendored_dirty(root: &Path, cfg: &Config, vendored: &[(&str, &Path, &str)]) -> bool {
    if vendored.iter().any(|(_, _, result)| *result == "fetched") {
        return true;
    }
    let lock = config::instance_rel(&cfg.layout.harness_dir, "harness.lock");
    git::porcelain(root).iter().any(|line| {
        let path = line.get(3..).unwrap_or("");
        path == lock
            || vendored.iter().any(|(_, dir, _)| {
                dir.strip_prefix(root)
                    .map(|rel| path.starts_with(rel.to_string_lossy().as_ref()))
                    .unwrap_or(false)
            })
    })
}

// other lanes' landings on `parent` since this lane's base, each named once per run
fn landings(root: &Path, parent: &str, noted: &mut Option<String>) -> Option<String> {
    let tip = git::git(root, &["rev-parse", parent]).ok()?;
    let from = match noted.take() {
        Some(sha) => sha,
        None => git::git(root, &["merge-base", "HEAD", parent]).ok()?,
    };
    *noted = Some(tip.clone());
    let log = git::git(
        root,
        &[
            "log",
            "--format=%x00%h %s",
            "--name-only",
            &format!("{from}..{tip}"),
        ],
    )
    .ok()?;
    let lines: Vec<String> = log
        .split('\0')
        .filter_map(|c| {
            let mut lines = c.lines().filter(|l| !l.trim().is_empty());
            let head = lines.next()?;
            Some(format!(
                "- {head} ({})",
                lines.collect::<Vec<_>>().join(", ")
            ))
        })
        .collect();
    (!lines.is_empty()).then(|| {
        format!(
            "Other lanes landed these commits on {parent} since your base; build on them and do not redo them:\n{}",
            lines.join("\n")
        )
    })
}

// a respond round's worktree on its pull request's branch, and the checkout the loop returns to
struct Lane<'a> {
    checkout: &'a Path,
    dir: PathBuf,
    // the state repository's branch the lane's ticket edits are committed on
    named: String,
    branch: String,
    base: String,
}

enum Flow {
    Go,
    SkipRest,
    Stop,
}

impl<'a> Loop<'a> {
    fn file(&self, name: &str) -> PathBuf {
        config::instance_path(self.root, &self.cfg.layout.harness_dir, name)
    }

    fn stop_found(&self) -> bool {
        config::stop_files(self.root, &self.cfg.layout.harness_dir)
            .iter()
            .any(|f| f.is_file())
    }

    fn go(&mut self) -> anyhow::Result<Digest> {
        self.needs_spec_at_start = self.ids_at(&["needs-info"]);
        self.learnings_at_start = waiting_classes(self.root, self.cfg);
        self.decisions_at_start =
            std::fs::read_to_string(self.file("DECISIONS.md")).unwrap_or_default();
        self.emit(Kind::RunStart {
            config_sha256: config_sha256(self.root),
            pipeline: None,
            sandbox: self.sandbox.name().into(),
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
            let it = format!("{}/{i}", self.writer.run);
            git::ITERATION.set(Some(it));
            iterations = i;
            if !self.iteration() {
                break;
            }
        }

        let digest = self.finish(iterations);
        git::ITERATION.set(None);
        Ok(digest)
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
        let stopped =
            agent::stop_signal().is_some() || self.stop_found() || self.over_budget().is_some();
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

        self.poll();
        if self.cfg.pr.merge == config::MergeMode::Auto {
            self.merge_green();
            if self.stopped {
                return false;
            }
        }
        // a block a human queued by hand is recorded against the product HEAD it was written at
        self.commit_state("queue");
        self.archive();

        let ready_before = self.ids_at(&tracker::READY);
        self.proposed_at_start = self.ids_at(&["needs-triage"]);
        let rejections_before = self.rejections();
        let pin = self.opts.task.clone();
        let takeable = taken(&self.blocks(), pin.as_deref());
        if let Some(id) = pin.filter(|_| takeable.is_none() && self.first_at_review().is_none()) {
            self.digest.warnings.push(format!(
                "{id} is neither takeable nor at review; the pinned run ends."
            ));
            return false;
        }
        let mut iter_base = git::head(self.root);
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
            // a pushed pull request still inside its quiet window is work the loop waits for
            let wait = self
                .blocks()
                .iter()
                .filter(|b| tracker::state(b).as_deref() == Some("open"))
                .filter_map(|b| self.quiet_left(block::field(b, "pushed").as_deref()))
                .min();
            if let Some(wait) = wait {
                println!(
                    "  waiting {}s for a pushed pull request's [respond] quiet window",
                    wait.as_secs()
                );
                std::thread::sleep(wait);
                return self.iteration();
            }
            self.digest.warnings.push(reason);
            return false;
        };

        // dry rounds are consecutive rounds of one pipeline; a task round emptying the queue is not one
        if self.dry_pipeline.as_deref() != Some(pipeline.name.as_str()) {
            self.dry_rounds = 0;
            self.dry_pipeline = Some(pipeline.name.clone());
        }

        // only after discovery found nothing takeable; ready-for-human alone is the ordinary wait
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

        // review takes a ticket stranded at review; decompose takes a spec, not a ticket
        let task = match config::parse_when(&pipeline.when) {
            Ok(Predicate::QueueReviewing) => self.first_at_review(),
            Ok(Predicate::SpecUndecomposed) => None,
            _ => takeable,
        };

        let lane = match task.as_deref().map(|t| self.open_lane(t)) {
            // the round has nowhere to run, and every later one would take the same ticket
            Some(Err(err)) => {
                self.digest.warnings.push(err);
                return false;
            }
            Some(Ok(lane)) => lane,
            None => None,
        };
        if let Some(lane) = &lane {
            iter_base = Some(lane.base.clone());
        }
        let mut stopped = false;
        for name in &pipeline.stages {
            let Some(stage) = self.cfg.stage.iter().find(|s| &s.name == name).cloned() else {
                continue;
            };
            // a pinned lane's filings wait for the unpinned round, so two lanes never edit one entry
            if self.opts.task.is_some()
                && matches!(
                    stage.role.as_deref(),
                    Some("scout" | "adjudicator" | "decomposer")
                )
            {
                continue;
            }
            let bases = (iter_base.clone(), state_base.clone());
            match self.stage(&stage, task.clone(), bases) {
                Flow::Go => {}
                // the skipped stages' boundary check is owed here instead
                Flow::SkipRest => {
                    self.boundary(false);
                    break;
                }
                Flow::Stop => {
                    stopped = true;
                    break;
                }
            }
        }
        if let Some(lane) = &lane {
            self.close_lane(lane);
        }
        // a stage that exhausted its turns still decided the blocks it decided before it did
        if stopped {
            self.promotions(&ready_before, &rejections_before);
            return false;
        }

        self.refuse_restated(task.as_deref(), &ready_before);
        self.promotions(&ready_before, &rejections_before);
        match &task {
            Some(task) => self.task_outcome(task, &pipeline, progress_before, lane.as_ref()),
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
        // a spent ceiling closes this pipeline, not the run, so an undecidable block can't hold triage
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

        // stash create snapshots without touching the tree, keeping earlier edits out of this diff
        let snapshot = |repo: &Path| {
            git::git(repo, &["stash", "create"])
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| git::head(repo))
        };
        let stage_bases = (snapshot(self.root), snapshot(&self.state_root()));
        let stop_files = config::stop_files(self.root, &self.cfg.layout.harness_dir);
        let mut result = match agent::spawn(
            &spawn,
            self.sandbox,
            &mut self.writer,
            &stop_files,
            &self.rate_limit,
        ) {
            Ok(result) => result,
            Err(err) => {
                // Stopped is the operator ending a rate-limit wait: their halt, not a failed start
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
        // out of turns only when the preset takes the cap as a flag; None, Config and Time do not
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
        // cache reads and writes are billed apart from input_tokens
        let spent_tokens = result.usage.input_tokens.unwrap_or(0)
            + result.usage.output_tokens.unwrap_or(0)
            + result.usage.cache_creation_input_tokens.unwrap_or(0)
            + result.usage.cache_read_input_tokens.unwrap_or(0);
        // a budget over unreported cost can't be enforced; over_budget() halts at the next boundary
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
        let flow = if gates::names_holdout(self.root, self.cfg, &iter_bases, &result.output) {
            self.burned(stage, task.as_deref());
            Flow::Stop
        } else if let Some(signal) = agent::stop_signal() {
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
        // a halted decompose leaves its drafts uncommitted, so the next check still sees them
        if role.as_deref() != Some("decomposer") || !matches!(flow, Flow::Stop) {
            let msg = format!("{} {}", stage.name, task.unwrap_or_default());
            self.commit_state(msg.trim_end());
        }
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
            // verdict gates read the verifier's own commit and the status it was handed
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
                sandbox: self.sandbox,
            };
            let outcome = gates::run(gate, &mut ctx);
            self.check_log();
            if outcome.halt {
                self.stopped = true;
                return Flow::Stop;
            }
            // a second scope or holdout refusal in one run means no lane will answer it
            if matches!(gate.as_str(), "scope" | "holdout") && !outcome.pass {
                if let Some(task) = &task {
                    let key = (gate.clone(), task.clone());
                    let seen = self.refusals.entry(key).or_insert(0);
                    *seen += 1;
                    if *seen >= 2 {
                        let reason = match gate.as_str() {
                            "scope" => format!(
                                "{task} was refused by the scope gate twice this run, and the implement round between them did not answer it: {}",
                                outcome.reason
                            ),
                            _ => format!(
                                "{task} was red on holdouts twice this run: {}",
                                outcome.reason
                            ),
                        };
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

        // CI-implies-frozen is the CLI's call, so a test's RunOpts ignores the CI env
        let frozen = self.opts.frozen;
        // a declared role is vendored where role_source reads first, so the fallback stays as is
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
        let source = role_text(
            &source,
            self.cfg,
            &self.stage_spec(role, handed, task.as_deref()),
        );
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

        if let Err(reason) = commit_vendored(self.root, self.cfg, &resolved_skills, &resolved_roles)
        {
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
            role: roles::policy(&source),
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
            role: Default::default(),
        }
    }

    // one wording for the operator's two halts, read here and by the spawn arm in stage
    fn halt_operator_stop(&mut self) -> bool {
        if let Some(signal) = agent::stop_signal() {
            self.halt("signal", format!("stopped by {signal}, exiting."));
            return true;
        }
        if self.stop_found() {
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

    // not a gate: a stage whose post list left it out would void the holdouts under auto
    fn burned(&mut self, stage: &config::Stage, task: Option<&str>) {
        const BURNED: &str = "a lane named a holdout path; the set may be burned";
        if let Some(task) = task {
            let t = tracker::Tracker::open(self.root, &self.cfg.layout.harness_dir);
            let from = t
                .get(task)
                .ok()
                .flatten()
                .and_then(|b| tracker::state(&b))
                .unwrap_or_default();
            if t.set_status(task, "ready-for-human", BURNED).is_ok() {
                self.emit(Kind::TaskStatus {
                    task: task.to_string(),
                    from,
                    to: "ready-for-human".into(),
                    reason: BURNED.into(),
                    by: "holdout".into(),
                });
            }
        }
        self.halt("holdout", format!("{}: {BURNED}", stage.name));
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
            if holds(
                self.sandbox,
                self.root,
                self.cfg,
                &when,
                self.opts.task.as_deref(),
                &mut self.digest.warnings,
            ) {
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

    fn task_outcome(
        &mut self,
        task: &str,
        pipeline: &config::Pipeline,
        progress_before: u64,
        lane: Option<&Lane<'a>>,
    ) {
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
                if let Some(lane) = lane {
                    self.push_lane(task, lane);
                } else if self.opts.pr_per_task || self.cfg.pr.per_task {
                    self.open_pull(task);
                }
                self.settle_thread(task);
            }
            _ => self.digest.warnings.push(format!(
                "{task} ended the iteration at {}, not done.",
                status.as_deref().unwrap_or("no status")
            )),
        }
        if let Some(lane) = lane {
            let dir = lane.dir.to_string_lossy();
            let _ = git::git(self.root, &["worktree", "remove", "--force", &dir]);
        }
        // the entry is all the next iteration inherits; a verify-only round has no other record
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
        let body = format!("Answered in {} by enallagi.", sha.trim());
        if let Err(err) = review::settle(self.token().as_deref(), &thread, &body) {
            let err = self.denied(err);
            self.digest
                .warnings
                .push(format!("{task}: thread {thread} was left open: {err}"));
        }
    }

    // `[respond] token_env`'s value, for the gh calls that write to a pull request
    fn token(&self) -> Option<String> {
        std::env::var(&self.cfg.respond.token_env)
            .ok()
            .filter(|t| !t.is_empty())
    }

    // a write gh's auth may not make names the key that picks another token
    fn denied(&self, err: review::ReviewError) -> String {
        let text = err.to_string();
        let lower = text.to_lowercase();
        if lower.contains("not accessible") || lower.contains("permission") {
            return format!(
                "{text} (set [respond] token_env, now `{}`, to a token that may)",
                self.cfg.respond.token_env
            );
        }
        text
    }

    // each `open` ticket's pull: merged is `done`, closed `wontfix`; only a changed one is read
    fn poll(&mut self) {
        let t = Tracker::open(self.root, &self.cfg.layout.harness_dir);
        let spec = tracker::repo_slug(self.root);
        let decisions = std::fs::read_to_string(self.file("DECISIONS.md")).unwrap_or_default();
        let open: Vec<(String, String, Option<String>, Option<String>)> = self
            .blocks()
            .iter()
            .filter(|b| tracker::state(b).as_deref() == Some("open"))
            .filter_map(|b| {
                Some((
                    b.id.clone(),
                    block::field(b, "pr")?,
                    block::field(b, "polled"),
                    block::field(b, "pushed"),
                ))
            })
            .collect();
        for (id, url, polled, pushed) in open {
            let light = match review::light(&url) {
                Ok(light) => light,
                Err(err) => {
                    self.digest.warnings.push(err.to_string());
                    continue;
                }
            };
            let moved = match light.state.as_str() {
                "MERGED" => t.set_status(&id, "done", "").map_err(|e| e.to_string()),
                "CLOSED" => t
                    .set_status(&id, "wontfix", "pull request closed unmerged")
                    .map_err(|e| e.to_string()),
                // the bots get the window to review the push before anything is judged
                _ if self.quiet_left(pushed.as_deref()).is_some() => Ok(()),
                _ if polled.as_deref() == Some(light.key.as_str()) => Ok(()),
                _ => self
                    .route(&t, &spec, &decisions, &id, &url, &light.rollup)
                    .and_then(|_| {
                        t.set_field(&id, "polled", &light.key)
                            .map_err(|e| e.to_string())
                    }),
            };
            if let Err(err) = moved {
                self.digest.warnings.push(format!("{id}: {err}"));
            }
        }
    }

    // `merge = "auto"`: each open pull request whose ticket is green on every count merges
    fn merge_green(&mut self) {
        let t = Tracker::open(self.root, &self.cfg.layout.harness_dir);
        let open: Vec<(String, String, Option<String>, Option<String>)> = self
            .blocks()
            .iter()
            .filter(|b| tracker::state(b).as_deref() == Some("open"))
            .filter_map(|b| {
                Some((
                    b.id.clone(),
                    block::field(b, "pr")?,
                    block::field(b, "respond"),
                    block::field(b, "pushed"),
                ))
            })
            .collect();
        for (id, url, respond, pushed) in open {
            let fields = (respond.as_deref(), pushed.as_deref());
            let line = match self.merge_one(&t, &id, &url, fields) {
                Ok(()) => format!("{id}: merged {url}"),
                Err(why) => format!("{id}: left open, {why}"),
            };
            self.digest.pulls.push(line);
            if self.stopped {
                return;
            }
        }
    }

    fn merge_one(
        &mut self,
        t: &Tracker,
        id: &str,
        url: &str,
        (respond, pushed): (Option<&str>, Option<&str>),
    ) -> Result<(), String> {
        let slug = t.spec_of(id).unwrap_or_default();
        if gates::holdout_set(self.root, self.cfg, &slug).is_empty() {
            return Err("no holdout set".to_string());
        }
        let head = pr::view(self.root, url)?;
        if head.state != "OPEN" {
            return Err(format!("the pull request is {}", head.state));
        }
        let default = pr::default_branch(self.root).map_err(|e| e.to_string())?;
        let rev = format!("refs/heads/{}", head.head_ref_name);
        let tip = git::git(self.root, &["ls-remote", "origin", &rev]).unwrap_or_default();
        let tip = tip.split_whitespace().next().unwrap_or_default();
        let found = review::read(url).map_err(|e| e.to_string())?;
        if found.short {
            return Err("more than 100 reviews, threads or comments; not all read".to_string());
        }
        if let Some(why) = pr::blocked(&head, id, &default, tip, found.open.len()) {
            return Err(why);
        }
        // GitHub's head and origin's tip agree after a third party's push, not after our own
        let own = match pushed.and_then(|p| p.split_whitespace().next()) {
            Some(sha) => sha.to_string(),
            None => git::git(self.root, &["rev-parse", "-q", "--verify", &rev]).unwrap_or_default(),
        };
        if tip != own {
            return Err("head moved".to_string());
        }
        // a clean written before the last push vouches for nothing at this head
        let sha = head.head_ref_oid;
        if respond != Some(format!("clean at {sha}").as_str()) {
            return Err(format!("respond: not clean at {sha}"));
        }
        git::git(self.root, &["fetch", "-q", "origin", &rev]).map_err(|e| e.to_string())?;
        let cache = config::cache_root();
        let ran = gates::holdouts(self.sandbox, self.root, self.cfg, &cache, &slug, id, &sha);
        match ran.map_err(|e| format!("the holdouts could not be run: {e}"))? {
            (_, 0) => {}
            (n, k) => {
                let why = format!("holdouts: {k} of {n} red");
                t.set_status(id, "ready-for-agent", &why)
                    .map_err(|e| e.to_string())?;
                self.commit_state(&format!("queue: {id} red on holdouts"));
                let seen = self
                    .refusals
                    .entry(("holdout".into(), id.into()))
                    .or_insert(0);
                *seen += 1;
                if *seen >= 2 {
                    self.halt(
                        id,
                        format!("{id} was red on holdouts twice this run: {why}"),
                    );
                }
                return Err(why);
            }
        }
        if let Err(err) = pr::merge(self.root, url, &sha) {
            self.digest
                .warnings
                .push(format!("{id}: {url} was not merged: {err}"));
            return Err(format!(
                "merge refused: {}",
                err.lines().next().unwrap_or_default()
            ));
        }
        t.set_status(id, "done", &format!("merged {url}"))
            .map_err(|e| e.to_string())?;
        self.commit_state(&format!("queue: {id} merged"));
        Ok(())
    }

    // how much of `[respond] quiet` is left since `pushed: <sha> <time>`
    fn quiet_left(&self, pushed: Option<&str>) -> Option<Duration> {
        let at: jiff::Timestamp = pushed?.split_whitespace().nth(1)?.parse().ok()?;
        let quiet = self.cfg.respond.quiet_duration().ok()?;
        let since =
            Duration::try_from(jiff::Timestamp::now().duration_since(at)).unwrap_or_default();
        quiet.checked_sub(since).filter(|left| !left.is_zero())
    }

    // in scope or unanchored returns to the ticket; else a new needs-triage ticket, thread resolved
    fn route(
        &mut self,
        t: &Tracker,
        spec: &str,
        decisions: &str,
        id: &str,
        url: &str,
        rollup: &str,
    ) -> Result<(), String> {
        let found = match review::read(url) {
            Ok(found) => found,
            Err(err) => {
                self.digest.warnings.push(err.to_string());
                return Ok(());
            }
        };
        if found.short {
            self.digest.warnings.push(format!(
                "{url}: only the first 100 reviews, threads and comments were read"
            ));
        }
        let blocks = t.blocks().map_err(|e| e.to_string())?;
        let Some(ticket) = blocks.iter().find(|b| b.id == id) else {
            return Ok(());
        };
        // a thread answered on an earlier push, re-read now, since a bot may have resolved it itself
        let token = self.token();
        let answered = ticket
            .body
            .iter()
            .filter_map(|(_, l)| l.strip_prefix("settled: ")?.split_whitespace().next())
            .filter_map(|f| tracker::finding_field(ticket, f, "thread-id:"));
        let mut resolved = Vec::new();
        for thread in answered.filter(|th| found.open.contains(th)) {
            match review::resolve(token.as_deref(), &thread) {
                Ok(()) => resolved.push(thread),
                Err(err) => {
                    let err = self.denied(err);
                    self.digest
                        .warnings
                        .push(format!("{id}: thread {thread} was left open: {err}"));
                }
            }
        }
        let scope = gates::scope_globs(&block::field(ticket, "scope").unwrap_or_default());
        let had = ticket
            .body
            .iter()
            .filter(|(_, l)| l.starts_with("finding:"))
            .count();
        let round = block::field(ticket, "rounds")
            .and_then(|r| r.parse::<u64>().ok())
            .unwrap_or(0)
            + 1;
        let prior = tracker::prior_requests(&self.root.join(&self.cfg.layout.harness_dir))
            .map_err(|e| e.to_string())?;
        // a ticket of any state, `wontfix` too, or an `.out-of-scope/` request already carries it
        let carried = |f: &review::Finding| {
            let key = format!("  key: {}", f.key);
            blocks.iter().any(|b| b.body.iter().any(|(_, l)| *l == key))
                || !f.reused
                    && (crate::issue::carrier(blocks.iter(), &f.url).is_some()
                        || prior.iter().any(|l| crate::issue::carries(l, &f.url)))
        };
        // a rejected finding's gist, the URL of the reply that rejected it, and its id
        let rejected: Vec<(String, String, &str)> = ticket
            .body
            .iter()
            .filter_map(|(_, l)| l.strip_prefix("answer: "))
            .filter_map(|a| a.split_once(" reject ").map(|(f, _)| f.trim()))
            .filter_map(|f| {
                let settled = format!("settled: {f} ");
                let reply = ticket
                    .body
                    .iter()
                    .find_map(|(_, l)| l.strip_prefix(&settled))?
                    .split_whitespace()
                    .next()?
                    .to_string();
                let lines = tracker::finding_lines(ticket, f);
                let by = tracker::finding_field(ticket, f, "by:")?;
                let anchor = tracker::finding_field(ticket, f, "anchor:");
                let path = anchor.as_deref().map(|a| a.split(':').next().unwrap_or(a));
                let said: Vec<&str> = lines.iter().filter_map(|l| l.strip_prefix('>')).collect();
                Some((review::gist(&by, path, &said.join("\n")), reply, f))
            })
            .collect();
        let mut fresh = Vec::new();
        let mut pointed = Vec::new();
        for f in found.findings.iter().filter(|f| !carried(f)) {
            let gist = review::gist(&f.by, f.path.as_deref(), &f.body);
            let Some((_, first, was)) = rejected.iter().find(|(g, ..)| *g == gist) else {
                fresh.push(f);
                continue;
            };
            // raised again after a reject: point at the reply, resolve, and start no round
            let body = format!(
                "Raised before and rejected: {first}. Not taken up again by enallagi.\n\n{}{was} -->",
                review::OWN
            );
            let sent = match &f.thread {
                Some(thread) => review::settle(token.as_deref(), thread, &body).inspect(|_| {
                    resolved.push(thread.clone());
                }),
                None => review::comment(token.as_deref(), url, &body),
            };
            match sent {
                Ok(reply) => pointed.push(format!("settled: {was} {reply} for {}", f.url)),
                Err(err) => {
                    let err = self.denied(err);
                    self.digest
                        .warnings
                        .push(format!("{id}: no pointer on {}: {err}", f.url));
                }
            }
        }
        if !pointed.is_empty() {
            t.add_lines(id, &pointed.join("\n"))
                .map_err(|e| e.to_string())?;
        }
        let mut entries = Vec::new();
        for f in fresh.iter().copied() {
            if f.path.as_deref().is_none_or(|p| gates::in_scope(p, &scope)) {
                let n = had + entries.len() + 1;
                entries.push(review::entry(f, &format!("F-{n}"), round));
                continue;
            }
            let one = review::Found {
                findings: vec![f.clone()],
                ..review::Found::default()
            };
            let appended = t
                .blocks()
                .map_err(|e| e.to_string())
                .and_then(|b| review::append(&b, decisions, &one).map_err(|e| e.to_string()))?;
            for text in &appended.blocks {
                t.append(spec, text).map_err(|e| e.to_string())?;
                let filed = block::parse(text)
                    .ok()
                    .and_then(|b| b.into_iter().next())
                    .map(|b| b.id)
                    .unwrap_or_default();
                println!("  proposed: {filed} from {}", f.url);
                let Some(thread) = &f.thread else {
                    continue;
                };
                let body = format!(
                    "Outside {id}'s scope; filed as {filed} by enallagi.\n\n{}{filed} -->",
                    review::OWN
                );
                if let Err(err) = review::settle(self.token().as_deref(), thread, &body) {
                    let err = self.denied(err);
                    self.digest
                        .warnings
                        .push(format!("{filed}: thread {thread} was left open: {err}"));
                }
            }
        }
        if entries.is_empty() {
            let left = found.open.iter().any(|th| !resolved.contains(th));
            let clean = format!("clean at {}", found.head);
            if fresh.is_empty()
                && rollup == "SUCCESS"
                && !left
                && !found.short
                && block::field(ticket, "respond").as_deref() != Some(clean.as_str())
            {
                t.set_field(id, "respond", &clean)
                    .map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        let max = self.cfg.respond.max_rounds;
        if round > max {
            let reason = format!(
                "respond: max_rounds ({max}) reached, {} findings open",
                entries.len()
            );
            return t
                .add_lines(id, &entries.join("\n"))
                .and_then(|_| t.set_field(id, "respond", "max_rounds"))
                .and_then(|_| t.set_status(id, "ready-for-human", &reason))
                .map_err(|e| e.to_string());
        }
        t.add_lines(id, &entries.join("\n"))
            .and_then(|_| t.set_field(id, "rounds", &round.to_string()))
            .and_then(|_| t.set_status(id, "ready-for-agent", ""))
            .map_err(|e| e.to_string())
    }

    // a returned ticket's round runs in its own worktree on the pull's branch, never the checkout's
    fn open_lane(&mut self, task: &str) -> Result<Option<Lane<'a>>, String> {
        let state_root = self.state_root();
        let branch = self
            .blocks()
            .iter()
            .find(|b| b.id == task)
            .and_then(|b| block::field(b, "branch"));
        let Some(branch) = branch.filter(|_| state_root != self.root) else {
            return Ok(None);
        };
        let dir = self.file("worktrees").join(format!("respond-{task}"));
        let state = dir.join(&self.cfg.layout.harness_dir);
        let named = format!("respond/{task}");
        let _ = git::git(
            self.root,
            &["worktree", "remove", "--force", &dir.to_string_lossy()],
        );
        let _ = git::git(&state_root, &["worktree", "prune"]);
        let base = git::git(self.root, &["fetch", "-q", "origin", &branch])
            .and_then(|_| git::git(self.root, &["rev-parse", "FETCH_HEAD"]))
            .and_then(|base| {
                git::git(
                    self.root,
                    &[
                        "worktree",
                        "add",
                        "-q",
                        "--detach",
                        &dir.to_string_lossy(),
                        &base,
                    ],
                )?;
                git::git(
                    &state_root,
                    &[
                        "worktree",
                        "add",
                        "-q",
                        "-B",
                        &named,
                        &state.to_string_lossy(),
                    ],
                )?;
                Ok(base)
            })
            .map_err(|e| format!("{task}: no lane on {branch}: {e}"))?;
        // ponytail: one leaked path per respond round, so the loop's borrowed root can point at the lane
        let dir: &'static Path = Box::leak(dir.into_boxed_path());
        Ok(Some(Lane {
            checkout: std::mem::replace(&mut self.root, dir),
            dir: dir.to_path_buf(),
            named,
            branch,
            base,
        }))
    }

    // the lane's state commits are the round's record, so they return to the checkout's state repo
    fn close_lane(&mut self, lane: &Lane<'a>) {
        self.root = lane.checkout;
        let state_root = self.state_root();
        let named = &lane.named;
        let state = lane.dir.join(&self.cfg.layout.harness_dir);
        match git::git(&state_root, &["merge", "-q", "--ff-only", named]) {
            Ok(_) => {
                let _ = git::git(
                    &state_root,
                    &["worktree", "remove", "--force", &state.to_string_lossy()],
                );
                let _ = git::git(&state_root, &["branch", "-D", named]);
            }
            Err(err) => self.digest.warnings.push(format!(
                "{named} did not fast-forward into {}, left in {}: {err}",
                state_root.display(),
                state.display()
            )),
        }
    }

    // fast-forward only, and never a state commit: the pull request is what a reviewer reads
    fn push_lane(&mut self, task: &str, lane: &Lane<'a>) {
        let range = format!("{}..HEAD", lane.base);
        let subjects = git::git(&lane.dir, &["log", "--format=%s", &range]).unwrap_or_default();
        let head = git::git(&lane.dir, &["rev-parse", "HEAD"]).unwrap_or_default();
        let remote = git::git(
            self.root,
            &[
                "ls-remote",
                "origin",
                &format!("refs/heads/{}", lane.branch),
            ],
        )
        .unwrap_or_default();
        let refused = if let Some(s) = subjects.lines().find(|s| git::recorded_sha(s).is_some()) {
            Some(format!("push refused: state commit `{s}` in {range}"))
        } else if remote.split_whitespace().next() != Some(lane.base.as_str()) {
            Some(format!("push refused: origin's {} moved", lane.branch))
        } else {
            git::git(
                &lane.dir,
                &[
                    "push",
                    "-q",
                    "origin",
                    &format!("HEAD:refs/heads/{}", lane.branch),
                ],
            )
            .err()
            .map(|e| format!("push refused: {e}"))
        };
        let t = Tracker::open(self.root, &self.cfg.layout.harness_dir);
        let now = jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ");
        let set = match &refused {
            Some(reason) => t.set_status(task, "open", reason),
            None => t
                .set_status(task, "open", "")
                .and_then(|_| t.set_field(task, "pushed", &format!("{head} {now}"))),
        };
        if let Err(err) = set {
            self.digest.warnings.push(format!("{task}: {err}"));
        }
        match refused {
            Some(reason) => self.digest.warnings.push(format!("{task}: {reason}")),
            None => {
                self.record_rejects(task);
                self.reply(task, head.trim());
            }
        }
        self.commit_state(&format!("queue: {task} pushed"));
    }

    // one marked reply per answered finding, on its thread or in one listing; its URL is `settled:`
    fn reply(&mut self, task: &str, sha: &str) {
        let Some(b) = self.blocks().into_iter().find(|b| b.id == task) else {
            return;
        };
        let Some(pr) = block::field(&b, "pr") else {
            return;
        };
        let lines: Vec<&str> = b.body.iter().map(|(_, l)| l.as_str()).collect();
        let settled: Vec<&str> = lines
            .iter()
            .filter_map(|l| l.strip_prefix("settled: ")?.split_whitespace().next())
            .collect();
        let token = self.token();
        for who in &self.cfg.respond.rereview {
            if let Err(err) = review::rereview(token.as_deref(), &pr, who) {
                self.digest
                    .warnings
                    .push(format!("{task}: no re-review from {who}: {err}"));
            }
        }
        let mut done = Vec::new();
        let mut listed = Vec::new();
        for answer in lines.iter().filter_map(|l| l.strip_prefix("answer: ")) {
            let Some((id, rest)) = answer.split_once(' ') else {
                continue;
            };
            let text = match rest.split_once(' ') {
                _ if settled.contains(&id) => continue,
                Some(("accept", test)) => {
                    format!("Fixed in {sha}; `{}` fails without the fix.", test.trim())
                }
                Some(("reject", why)) => match why.split_once(':') {
                    Some((concept, cmd)) => format!(
                        "Not changed: {}. `{}` shows why.",
                        concept.trim(),
                        cmd.trim()
                    ),
                    None => continue,
                },
                _ => continue,
            };
            let mark = format!("{}{id} -->", review::OWN);
            let Some(thread) = tracker::finding_field(&b, id, "thread-id:") else {
                let url = tracker::finding_field(&b, id, "url:").unwrap_or_default();
                listed.push((id, format!("- {url}: {text} {mark}")));
                continue;
            };
            match review::reply(token.as_deref(), &thread, &format!("{text}\n\n{mark}")) {
                Ok(url) => done.push(format!("settled: {id} {url}")),
                Err(err) => {
                    let err = self.denied(err);
                    self.digest
                        .warnings
                        .push(format!("{task}: no reply on {thread}: {err}"));
                }
            }
        }
        if !listed.is_empty() {
            let body: Vec<&str> = listed.iter().map(|(_, l)| l.as_str()).collect();
            match review::comment(token.as_deref(), &pr, &body.join("\n")) {
                Ok(url) => done.extend(listed.iter().map(|(id, _)| format!("settled: {id} {url}"))),
                Err(err) => {
                    let err = self.denied(err);
                    self.digest
                        .warnings
                        .push(format!("{task}: no reply on {pr}: {err}"));
                }
            }
        }
        if done.is_empty() {
            return;
        }
        let t = Tracker::open(self.root, &self.cfg.layout.harness_dir);
        if let Err(err) = t.add_lines(task, &done.join("\n")) {
            self.digest.warnings.push(format!("{task}: {err}"));
        }
    }

    // each verified reject, in Pocock's `.out-of-scope/` format, so the poll never routes it again
    fn record_rejects(&mut self, task: &str) {
        let Some(b) = self.blocks().into_iter().find(|b| b.id == task) else {
            return;
        };
        let runs = tracker::verdict(&b).map(|r| r.runs).unwrap_or_default();
        let lines: Vec<&str> = b.body.iter().map(|(_, l)| l.as_str()).collect();
        // a reject settled on an earlier push is already on file
        let settled: Vec<&str> = lines
            .iter()
            .filter_map(|l| l.strip_prefix("settled: ")?.split_whitespace().next())
            .collect();
        for answer in lines.iter().filter_map(|l| l.strip_prefix("answer: ")) {
            let Some((id, reject)) = answer.split_once(" reject ") else {
                continue;
            };
            if settled.contains(&id) {
                continue;
            }
            let Some((concept, cmd)) = reject.split_once(':') else {
                continue;
            };
            let (concept, cmd) = (concept.trim(), cmd.trim());
            let url = tracker::finding_field(&b, id, "url:").unwrap_or_default();
            let said = tracker::finding_field(&b, id, ">").unwrap_or_default();
            let exit = runs
                .iter()
                .find(|(ran, _)| ran == cmd)
                .map_or("no".to_string(), |(_, e)| e.to_string());
            let dir = self.file(tracker::OUT_OF_SCOPE);
            let path = dir.join(format!("{}.md", tracker::slug(concept)));
            let request = format!("- {url}: \"{said}\"\n");
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => format!("{}\n{request}", text.trim_end()),
                Err(_) => format!(
                    "# {concept}\n\n{said}\n\n## Why this is out of scope\n\nRejected on {task} as {id}: `{cmd}` exited {exit}.\n\n## Prior requests\n\n{request}"
                ),
            };
            if let Err(err) =
                std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, text))
            {
                self.digest
                    .warnings
                    .push(format!("{task}: {} was not written: {err}", path.display()));
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
                if opened.starts_with("http") {
                    let t = Tracker::open(self.root, &self.cfg.layout.harness_dir);
                    let set = t
                        .set_status(task, "open", "")
                        .and_then(|_| t.set_field(task, "pr", &opened))
                        .and_then(|_| t.set_field(task, "branch", &report.branch));
                    match set {
                        Ok(()) => self.commit_state(&format!("queue: {task} open")),
                        Err(err) => self.digest.warnings.push(format!("{task}: {err}")),
                    }
                }
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
        reviewed(&self.blocks(), self.opts.task.as_deref())
    }

    // the prompt carries the ids the stage was sized for, so it decides those and no others
    fn stage_spec(&self, role: &str, ids: &[String], task: Option<&str>) -> String {
        let ticket: Vec<&str> = match task {
            Some(t) => vec![t],
            None => ids.iter().map(String::as_str).collect(),
        };
        match (role, task) {
            ("decomposer", Some(slug)) => config::instance_rel(
                &self.cfg.layout.harness_dir,
                &format!("{}/{slug}/spec.md", tracker::SPECS),
            ),
            _ => spec_for(self.root, self.cfg, &ticket),
        }
    }

    fn stage_prompt(&self, role: &str, ids: &[String], task: Option<&str>) -> String {
        let spec = self.stage_spec(role, ids, task);
        let mut prompt = prompt_for(role, self.cfg, &spec);
        if let (Some(id), "implementer") = (&self.opts.task, role) {
            prompt = prompt.replace("the one `enallagi tasks ready` prints.", &format!("{id}."));
        }
        if let (Some(id), "verifier") = (&self.opts.task, role) {
            prompt = prompt
                .replace(
                    "every ticket at review (`enallagi tasks ids-at review`)",
                    id,
                )
                .replace(
                    "Verify every ticket at review.",
                    &format!("Verify {id} only; leave every other ticket at review alone."),
                );
        }
        if role == "decomposer" {
            prompt.push_str(&format!(" The spec to decompose is {spec}."));
        }
        if role == "implementer" {
            if let Some(clause) = task.and_then(|t| self.prior_attempts(t)) {
                prompt.push(' ');
                prompt.push_str(&clause);
            }
            if let Ok(parent) = std::env::var("ENALLAGI_PARENT") {
                if let Some(note) = landings(self.root, &parent, &mut self.noted.borrow_mut()) {
                    prompt.push(' ');
                    prompt.push_str(&note);
                }
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

    // a commit naming a task still takeable is an attempt no verdict accepted
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

    // a fresh rejection is already the task's own work; a block promoted over it duplicates the job
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
        // the proxy's threads hold no writer, so their refusals reach the log ahead of the next event
        for d in self.sandbox.denials_since(self.denied) {
            self.denied += 1;
            self.writer.emit(Kind::SandboxDenied {
                host: d.host,
                port: d.port,
                reason: d.reason,
            });
        }
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

// shared by the no-TUI path and cli::run, so the two never print different digests
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

// a claim under five words recurs by chance; a quoted command or path is shared boilerplate
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

fn holds(
    sandbox: &Sandbox,
    root: &Path,
    cfg: &Config,
    when: &Predicate,
    pin: Option<&str>,
    warnings: &mut Vec<String>,
) -> bool {
    match when {
        Predicate::Not(inner) => !holds(sandbox, root, cfg, inner, pin, warnings),
        Predicate::QueueTakeable => taken(&queue_blocks(root, cfg), pin).is_some(),
        // an unreadable queue is no evidence of a task at review: fails closed like `takeable`
        Predicate::QueueReviewing => match queue_read(root, cfg) {
            Ok(blocks) => reviewed(&blocks, pin).is_some(),
            Err(err) => {
                warnings.push(format!(
                    "the queue: {err}; queue.reviewing treated as false (fail closed)"
                ));
                false
            }
        },
        // an unreadable queue is no evidence of a block awaiting triage: fails closed like `takeable`
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
        // an unreadable queue is no evidence the queue is empty: fails closed like `takeable`
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
        Predicate::CheckRed => gates::check_delta(sandbox, root, cfg, false).red,
        Predicate::Probe(name) => {
            let check = check_outcome(sandbox, root, cfg);
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

// `ran` means the check STARTED; ran:false there would turn a real finding into an ERROR
fn check_outcome(sandbox: &Sandbox, root: &Path, cfg: &Config) -> CheckOutcome {
    let mut outcome = gates::check_delta(sandbox, root, cfg, true).outcome();
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
        hosts: Vec::new(),
        write: Vec::new(),
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
        let sandbox = Sandbox::start(&repo.root, &cfg);
        let held = || holds(&sandbox, &repo.root, &cfg, &when, None, &mut Vec::new());
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
        // the role file as init installs it names the default spec until the stage renders it
        let installed = config::subst(include_str!("../../../roles/implementer.md"), &cfg);
        repo.write(".enallagi/roles/implementer.md", &installed);
        let source = role_source(&repo.root, &cfg, "implementer").expect("role");
        let role = role_text(&source, &cfg, &spec_for(&repo.root, &cfg, &["T-002"]));
        assert!(role.contains(".enallagi/specs/c/spec.md"), "{role}");
        assert!(!role.contains("specs/a/"), "{role}");
    }

    #[test]
    fn note_lists_landings() {
        let r = crate::fixture::Repo::new();
        let parent = git::git(&r.root, &["rev-parse", "--abbrev-ref", "HEAD"]).expect("branch");
        git::git(&r.root, &["checkout", "-q", "-b", "lane"]).expect("lane");
        let land = |file: &str, msg: &str| {
            git::git(&r.root, &["checkout", "-q", &parent]).expect("parent");
            r.write(file, "x");
            r.commit_all(msg);
            git::git(&r.root, &["checkout", "-q", "lane"]).expect("lane");
        };
        let mut noted = None;
        assert_eq!(landings(&r.root, &parent, &mut noted), None);
        land("one.rs", "feat: first landing");
        let first = landings(&r.root, &parent, &mut noted).expect("a note");
        assert!(
            first.contains("first landing") && first.contains("one.rs"),
            "{first}"
        );
        land("two.rs", "feat: second landing");
        let second = landings(&r.root, &parent, &mut noted).expect("a note");
        assert!(
            second.contains("second landing") && second.contains("two.rs"),
            "{second}"
        );
        assert!(!second.contains("first landing"), "{second}");
        assert_eq!(landings(&r.root, &parent, &mut noted), None);
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
