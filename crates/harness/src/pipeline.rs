//! The launcher: runs pipelines from `harness.toml` with halts and gates.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use sha2::{Digest as _, Sha256};

use crate::agent::{self, Preset, StageSpawn, TurnCap};
use crate::config::{self, Config, ConfigError, Predicate};
use crate::events::{Kind, Log, Sink, Writer};
use crate::gates::{self, GateCtx};
use crate::probes::{self, CheckOutcome, ProbeCtx};
use crate::queue::{self, Queue};
use crate::roles;
use crate::skills::{self, ResolveOpts};
use crate::{archive, git};

// A lane running ps to check for competing writers must ignore its parent.
const LANE: &str = "You are this loop's own lane, spawned by the harness. There is no human in this session
and no answer will come, so never end a turn on a question -- decide and act. A running harness
or agent process in ps is your PARENT process, not a competing writer: LEARNINGS.md's one-checkout-one-writer
rule is about a second operator, and it does not apply to the process that started you.";

const SCOUT: &str = "Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __HARNESS_DIR__/run/roles/scout.md: read that file first and follow it exactly. Run `harness probe` and append to TASKS.md one 'status: proposed' block per FINDING line, each carrying probe:, command:, output: and rows:. Zero FINDING lines is zero blocks, which is a valid outcome and not something to escalate. Never promote, never fix, never edit any file a finding names. Then stop.";

const ADJUDICATOR: &str = "Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __HARNESS_DIR__/run/roles/adjudicator.md: read that file first and follow it exactly. Act on every block with 'status: proposed' in TASKS.md, in file order. Promote it to 'status: ready' with a scope and criteria an agent that has read only __CONTEXT_FILE__, __SPEC__, LEARNINGS.md and the block can run, or kill it and append one line to '## Rejected findings' in DECISIONS.md. A finding whose fix needs a change to __SPEC__ or __CONTEXT_FILE__ is neither: leave it at proposed and print a line beginning HALT that names the block's id. Do not commit; this loop commits your round. Then stop.";

const IMPLEMENTER: &str = "Read __CONTEXT_FILE__, __SPEC__, LEARNINGS.md, TASKS.md, git log --oneline -20, and the TAIL of PROGRESS.md (tail -200 PROGRESS.md -- it is append-only and newest-last, so reading it from the top gives you the oldest entries and none of the handoff). The tail and the log are what the one-row rail has you re-read at the start of an iteration. Your role is defined in __HARNESS_DIR__/run/roles/implementer.md: read that file first and follow it exactly. Complete exactly ONE task: the first with status 'ready' whose blockers are done and which is NOT marked 'attended: true'. If that task's scope files already carry uncommitted work, a prior lane was terminated mid-flight: finish it, never restart it and never discard it. Follow the task protocol strictly. Before you stop you MUST git add the product paths named on the task's scope: line (never git add -A, LEARNINGS.md 2026-08-26), commit them, paste the exact commands and their output into the task's notes:, and set status: review. You MUST also append this iteration's PROGRESS.md entry in the format written at the top of that file -- what happened, which rows moved, and any BLOCKED with its written reason. Never stage or commit TASKS.md, PROGRESS.md or any other file under __HARNESS_DIR__: this loop commits them when your stage ends. An implementation left uncommitted is a lost iteration.";

const VERIFIER: &str = "Read __CONTEXT_FILE__, __SPEC__ and TASKS.md. Your role is defined in __HARNESS_DIR__/run/roles/verifier.md: read that file first and follow it exactly. Verify every task with status 'review'. Promote to done or reject to ready with concrete reasons; this loop commits the verdict. If nothing is at review, say so in one line and stop; that is a valid outcome, not something to escalate. Then stop.";

const GENERIC: &str = "Read __CONTEXT_FILE__ and LEARNINGS.md. Your role is defined in __HARNESS_DIR__/run/roles/__ROLE__.md: read that file first and follow it exactly. Then stop.";

fn prompt_for(role: &str, cfg: &Config) -> String {
    let body = match role {
        "scout" => SCOUT.to_string(),
        "adjudicator" => ADJUDICATOR.to_string(),
        "implementer" => IMPLEMENTER.to_string(),
        "verifier" => VERIFIER.to_string(),
        other => GENERIC.replace("__ROLE__", other),
    };
    config::subst(&format!("{LANE} {body}"), cfg)
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
    pub dry_run: bool,
    pub frozen: bool,
    pub tui: bool,
    pub budget_seconds: Option<u64>,
    pub budget_usd: Option<f64>,
    pub budget_tokens: Option<u64>,
}

impl Default for RunOpts {
    fn default() -> Self {
        RunOpts {
            max_iter: 3,
            dry_run: false,
            frozen: false,
            tui: false,
            budget_seconds: None,
            budget_usd: None,
            budget_tokens: None,
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
    pub stages_run: usize,
    pub role_seconds: BTreeMap<String, u64>,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Refused(pub String);

pub fn plan(root: &Path, cfg: &Config) -> Result<String, ConfigError> {
    let presets = agent::presets();
    let mut out = String::new();
    let mut scouting = false;
    let mut warnings = Vec::new();

    for pipeline in cfg.pipeline.iter() {
        if !holds(
            root,
            cfg,
            &config::parse_when(&pipeline.when)?,
            &mut warnings,
        ) {
            continue;
        }
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
                    let resolved = agent::resolve(&cfg.agent, role, &presets)
                        .map_err(|e| ConfigError::UnknownPreset(e.to_string()))?;
                    let via = resolved.argv.first().cloned().unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "  DRY_RUN would spawn: {name} as role {role} via {via} (turns: {})",
                        stage.turns
                    );
                    for line in prompt_for(role, cfg).lines() {
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
        // HARNESS_DRIVER only here and the scout stage -- it installs throwaway repos and costs wall clock elsewhere
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

    if opts.dry_run {
        print!("{}", plan(root, &cfg).map_err(|e| Refused(e.to_string()))?);
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
        tokens_missing: false,
        spent_tokens: 0,
        needs_spec_at_start: Vec::new(),
        dry_rounds: 0,
        stopped: false,
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
    tokens_missing: bool,
    spent_tokens: u64,
    needs_spec_at_start: Vec<String>,
    dry_rounds: u32,
    stopped: bool,
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
        self.needs_spec_at_start = self.ids_at("needs-spec");
        self.emit(Kind::RunStart {
            config_sha256: config_sha256(self.root),
            pipeline: None,
        });

        // a backticked mention is prose about the marker; only a bare one halts the run
        if let Some(lines) = clarifications(&self.file(&self.cfg.layout.spec)) {
            self.halt(
                "clarification",
                format!(
                    "{} carries [NEEDS CLARIFICATION]. The loop does not start.",
                    self.cfg.layout.spec
                ),
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
                if let Some(refused) = report.refused {
                    self.digest.warnings.push(refused);
                }
            }
            Err(err) => self.digest.warnings.push(format!("archive: {err}")),
        }
    }

    // every exit path funnels through here, so every run ends with one digest
    fn finish(&mut self, iterations: u32) -> Digest {
        // the task the last iteration landed is archived by the run that landed it, not the next one
        self.archive();
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

        // a block a human queued by hand is recorded against the product HEAD it was written at
        self.commit_state("queue");
        self.unblock();
        self.archive();

        let ready_before = self.ids_at("ready");
        let rejections_before = self.rejections();
        let takeable = gates::takeable(self.root, self.cfg);
        let iter_base = git::head(self.root);
        let state_base = git::head(&self.state_root());
        let progress_before = file_len(&self.file("PROGRESS.md"));

        // fires only after a discovery round already found nothing takeable; attended:true blocks alone are the ordinary human-wait state
        if takeable.is_none() && self.dry_rounds >= 1 {
            if let Some(id) = self.first_attended_ready() {
                self.halt(
                    &id,
                    format!(
                        "{id} is attended: true and it is all that is left. A human has to run it."
                    ),
                );
                return false;
            }
        }

        let Some(pipeline) = self.choose() else {
            self.digest
                .warnings
                .push("no pipeline's `when` held; nothing to run.".to_string());
            return false;
        };

        // the review pipeline verifies a task stranded at review, not the ordinary ready-and-unattended one
        let task = if matches!(
            config::parse_when(&pipeline.when),
            Ok(Predicate::QueueReviewing)
        ) {
            self.first_at_review()
        } else {
            takeable
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
                Flow::Stop => return false,
            }
        }

        self.promotions(&ready_before, &rejections_before);
        match &task {
            Some(task) => self.task_outcome(task, &pipeline, progress_before),
            None => {
                if let Some(reason) = self.proposed_names_contract() {
                    self.halt("proposed", reason);
                    return false;
                }
            }
        }
        if let Some(reason) = self.new_needs_spec() {
            self.halt("needs-spec", reason);
            return false;
        }
        if self.stopped {
            return false;
        }
        // an empty queue isn't exhaustion by itself; only end_after_dry_rounds consecutive empties are
        pipeline.end_after_dry_rounds == 0 || self.dry_rounds < pipeline.end_after_dry_rounds
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

        let timeout = match stage.timeout_duration() {
            Ok(t) => t,
            Err(err) => {
                self.halt("stage", err.to_string());
                return Flow::Stop;
            }
        };
        let mut env = stage.env.clone();
        env.insert("HARNESS_ROOT".into(), self.root.display().to_string());
        env.insert("HARNESS_STAGE".into(), stage.name.clone());
        env.insert("HARNESS_TASK".into(), task.clone().unwrap_or_default());
        env.insert("HARNESS_ITERATION".into(), self.writer.iter.to_string());

        let (spawn, role) = match (&stage.role, &stage.command) {
            (Some(role), _) => match self.role_spawn(stage, role, task.clone(), env, timeout) {
                Ok(spawn) => (spawn, Some(role.clone())),
                Err(flow) => return flow,
            },
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
        let result = match agent::spawn(&spawn, &mut self.writer, &stop_file, &self.rate_limit) {
            Ok(result) => result,
            Err(err) => {
                self.halt("stage", format!("{} could not start: {err}", stage.name));
                return Flow::Stop;
            }
        };

        self.digest.stages_run += 1;
        self.digest.seconds += result.seconds;
        if let Some(role) = &role {
            *self.digest.role_seconds.entry(role.clone()).or_default() += result.seconds;
        }
        if let Some(cost) = result.usage.cost {
            self.digest.cost = round4(self.digest.cost + cost);
        }
        let spent_tokens =
            result.usage.input_tokens.unwrap_or(0) + result.usage.output_tokens.unwrap_or(0);
        // cost_missing is read by over_budget() at the next boundary; a budget over unreported cost can't be enforced
        if role.is_some() {
            if self.opts.budget_usd.is_some() && result.usage.cost.is_none() {
                self.cost_missing = true;
            }
            if self.opts.budget_tokens.is_some()
                && (result.usage.input_tokens.is_none() || result.usage.output_tokens.is_none())
            {
                self.tokens_missing = true;
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
            turns: result.usage.turns,
        });

        let flow = if result.exit != 0 {
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
            // commit-verdict judges only what the verifier wrote, not the implementer's notes from the same iteration
            let (base, state_base) = if gate == "commit-verdict" {
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
            };
            let outcome = gates::run(gate, &mut ctx);
            self.check_log();
            if outcome.halt {
                self.stopped = true;
                return Flow::Stop;
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
    ) -> Result<StageSpawn<'a>, Flow> {
        let resolved = match agent::resolve(&self.cfg.agent, role, &self.presets) {
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
            prompt: prompt_for(role, self.cfg),
            turns: stage.turns,
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

    // order matters: STOP file, then budgets, then a new needs-spec task
    fn boundary(&mut self, needs_spec: bool) -> bool {
        if self.stopped {
            return true;
        }
        if self.file("STOP").is_file() {
            self.halt("stop", "STOP file found, exiting.".to_string());
            return true;
        }
        if let Some(reason) = self.over_budget() {
            self.halt("budget", reason);
            return true;
        }
        if needs_spec {
            if let Some(reason) = self.new_needs_spec() {
                self.halt("needs-spec", reason);
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
        if self.tokens_missing {
            return Some("BUDGET_TOKENS is set and the last stage reported no tokens, so the budget cannot be enforced. The agent command must print the counts [agent.usage].input_tokens and .output_tokens can read, or unset BUDGET_TOKENS.".to_string());
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
        self.blocks()
            .iter()
            .filter(|b| queue::field(b, "status").as_deref() == Some("proposed"))
            .find(|b| {
                let text = queue::block_text(b);
                text.contains(spec.as_str()) || text.contains(context.as_str())
            })
            .map(|b| {
                format!(
                    "{} is still proposed and its fix names {spec} or {context}. Neither is a lane's to edit.",
                    b.id
                )
            })
    }

    // halts only on a NEW needs-spec task; pre-existing ones are the ordinary state of an open contract
    fn new_needs_spec(&self) -> Option<String> {
        self.ids_at("needs-spec")
            .into_iter()
            .find(|id| !self.needs_spec_at_start.contains(id))
            .map(|id| {
                format!(
                    "{id} went to needs-spec. The contract does not answer it and no lane may decide it."
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
        for id in self.ids_at("ready") {
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
        let status = self
            .blocks()
            .iter()
            .find(|b| b.id == task)
            .and_then(|b| queue::field(b, "status"));
        match status.as_deref() {
            Some("done") => {
                self.digest.landed.push(task.to_string());
                let rows = self
                    .blocks()
                    .iter()
                    .find(|b| b.id == task)
                    .and_then(|b| queue::field(b, "rows"));
                if let Some(rows) = rows.filter(|r| !r.is_empty()) {
                    self.digest.rows.push(format!("{task}: {rows}"));
                }
            }
            other => self.digest.warnings.push(format!(
                "{task} ended the iteration at {}, not done.",
                other.unwrap_or("no status")
            )),
        }
        // the entry is all the next iteration inherits, so the launcher writes the facts it has
        // rather than warning that a role left none -- a verify-only round has no other record
        if file_len(&self.file("PROGRESS.md")) <= progress_before {
            self.progress_stub(task, pipeline, status.as_deref());
        }
    }

    fn progress_stub(&mut self, task: &str, pipeline: &config::Pipeline, status: Option<&str>) {
        // `friction: none` exactly: friction-repeat skips that word and would group any other phrasing
        let entry = format!(
            "\n## {date} — {task} — {name} pipeline, left at {status}\n\
             what happened: no role wrote an entry this iteration {iter}, so the launcher wrote \
             this one. Stages: {stages}. The round's reasoning is in the task's own notes in \
             TASKS.md.\nfriction: none\n",
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

    fn unblock(&mut self) {
        let queue = Queue {
            path: self.file("TASKS.md"),
        };
        let text = match queue.read() {
            Ok(text) => text,
            // a missing file reads as empty, so this is a real IO failure
            Err(err) => {
                self.digest.warnings.push(format!("TASKS.md: {err}"));
                return;
            }
        };
        match queue::unblock(&text) {
            Ok(out) if out != text => {
                if let Err(err) = queue.write(&out) {
                    self.digest.warnings.push(format!("TASKS.md: {err}"));
                }
            }
            Ok(_) => {}
            Err(err) => self.digest.warnings.push(format!("TASKS.md: {err}")),
        }
    }

    fn blocks(&self) -> Vec<queue::Block> {
        let path = self.file("TASKS.md");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| queue::parse(&t).ok())
            .unwrap_or_default()
    }

    fn ids_at(&self, status: &str) -> Vec<String> {
        queue::ids_at(&self.blocks(), status)
    }

    fn first_at_review(&self) -> Option<String> {
        self.ids_at("review").into_iter().next()
    }

    fn first_attended_ready(&self) -> Option<String> {
        self.blocks()
            .iter()
            .find(|b| {
                queue::field(b, "status").as_deref() == Some("ready")
                    && queue::field(b, "attended").as_deref() == Some("true")
            })
            .map(|b| b.id.clone())
    }

    fn rejections(&self) -> Vec<String> {
        std::fs::read_to_string(self.file("DECISIONS.md"))
            .map(|t| queue::rejections(&t))
            .unwrap_or_default()
    }

    fn emit(&mut self, kind: Kind) {
        self.writer.emit(kind);
        self.check_log();
    }

    // an event that never reached the log is one `harness watch` and the probes will never see
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
    let _ = writeln!(out, "findings promoted:{}", inline(&digest.promoted));
    listing(&mut out, "findings killed:", &digest.killed);
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

fn holds(root: &Path, cfg: &Config, when: &Predicate, warnings: &mut Vec<String>) -> bool {
    match when {
        Predicate::Not(inner) => !holds(root, cfg, inner, warnings),
        Predicate::QueueTakeable => gates::takeable(root, cfg).is_some(),
        // an unreadable or unparseable queue is not evidence of a task at review, so this fails closed like `takeable`
        Predicate::QueueReviewing => {
            match std::fs::read_to_string(config::instance_path(
                root,
                &cfg.layout.harness_dir,
                "TASKS.md",
            ))
            .map_err(|e| e.to_string())
            .and_then(|t| queue::parse(&t).map_err(|e| e.to_string()))
            {
                Ok(blocks) => !queue::ids_at(&blocks, "review").is_empty(),
                Err(err) => {
                    warnings.push(format!(
                        "TASKS.md: {err}; queue.reviewing treated as false (fail closed)"
                    ));
                    false
                }
            }
        }
        // an unreadable or unparseable queue is not evidence the queue is empty, so this fails closed like `takeable`
        Predicate::QueueEmpty => {
            match std::fs::read_to_string(config::instance_path(
                root,
                &cfg.layout.harness_dir,
                "TASKS.md",
            ))
            .map_err(|e| e.to_string())
            .and_then(|t| queue::parse(&t).map_err(|e| e.to_string()))
            {
                Ok(blocks) => blocks.is_empty(),
                Err(err) => {
                    warnings.push(format!(
                        "TASKS.md: {err}; queue.empty treated as false (fail closed)"
                    ));
                    false
                }
            }
        }
        Predicate::TaskAttended => {
            let blocks = std::fs::read_to_string(config::instance_path(
                root,
                &cfg.layout.harness_dir,
                "TASKS.md",
            ))
            .ok()
            .and_then(|t| queue::parse(&t).ok())
            .unwrap_or_default();
            blocks
                .iter()
                .find(|b| queue::field(b, "status").as_deref() == Some("ready"))
                .is_some_and(|b| queue::field(b, "attended").as_deref() == Some("true"))
        }
        Predicate::CheckRed => gates::check_delta(root, cfg, false).red,
        Predicate::Probe(name) => {
            let check = check_outcome(root, cfg);
            let ctx = ProbeCtx {
                root,
                cfg,
                check: Some(&check),
                driver: std::env::var("HARNESS_DRIVER").as_deref() == Ok("1"),
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
    let report = gates::check_delta(root, cfg, true);
    let started = !report.output.starts_with(NEVER_RAN)
        // nested under turbo the check would recurse, so it is not run at all
        && std::env::var_os("TURBO_HASH").is_none();
    CheckOutcome {
        ran: started,
        red: report.red,
        output: report.output,
    }
}

const NEVER_RAN: &str = "the check could not be run:";

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
    }
}
