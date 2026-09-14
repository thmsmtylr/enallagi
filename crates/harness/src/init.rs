//! Seeds a repo with the harness's files: its own files are rewritten every run, the project's documents are seeded once.

use crate::agent::{self, Preset};
use crate::config::{self, Config};
use crate::git;
use crate::skills;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const ROLES: &[(&str, &str)] = &[
    (
        "adjudicator.md",
        include_str!("../../../roles/adjudicator.md"),
    ),
    (
        "implementer.md",
        include_str!("../../../roles/implementer.md"),
    ),
    (
        "researcher.md",
        include_str!("../../../roles/researcher.md"),
    ),
    ("scout.md", include_str!("../../../roles/scout.md")),
    ("verifier.md", include_str!("../../../roles/verifier.md")),
];

// ships as dot.check-baseline so the package's own tooling doesn't read it as a baseline
const DOCS: &[(&str, &str)] = &[
    ("TASKS.md", include_str!("../../../templates/TASKS.md")),
    (
        "PROGRESS.md",
        include_str!("../../../templates/PROGRESS.md"),
    ),
    (
        "LEARNINGS.md",
        include_str!("../../../templates/LEARNINGS.md"),
    ),
    (
        "DECISIONS.md",
        include_str!("../../../templates/DECISIONS.md"),
    ),
    (
        ".check-baseline",
        include_str!("../../../templates/dot.check-baseline"),
    ),
];

const SKILL: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../../skills/running-the-loop/SKILL.md"),
    ),
    (
        "references/task-block.md",
        include_str!("../../../skills/running-the-loop/references/task-block.md"),
    ),
];

const RAILS: &str = include_str!("../../../templates/RAILS.md");
const CONTEXT: &str = include_str!("../../../templates/AGENTS.md");
const POINTER: &str = include_str!("../../../templates/pointer.md");
const SPEC_SECTION: &str = include_str!("../../../templates/SPEC.section.md");
const EVALS_README: &str = include_str!("../../../evals/README.md");
const CLAUDE_SETTINGS: &str = include_str!("../../../adapters/claude/settings.json");

// scoped to the harness dir, never touches the repo's own .gitignore
// vendored skills are committed (controller ruling), so skills/ is not ignored here
const GITIGNORE: &str = "events.jsonl\n*.log\nlogs/\nworktrees/\nloop.pid\nrun/\n__pycache__/\n";

#[derive(Debug, Default, Clone)]
pub struct InitOpts {
    pub adapter: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Default)]
pub struct InitReport {
    /// Relative paths this run wrote, or would write under `--dry-run`.
    pub wrote: Vec<String>,
    /// Relative paths that already had content and were left alone.
    pub kept: Vec<String>,
    /// `"<old> -> <new>"` for every key lifted out of a `harness.json`.
    pub migrated_keys: Vec<String>,
    /// The top-level paths of everything above, for the `git add` line.
    pub track: Vec<String>,
    /// Everything else the operator has to read: advice on a kept file, and the
    /// adapters that write nothing.
    pub notes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error(
        "{0} is not a git repository. The harness records its own history there; git init first."
    )]
    NotGit(String),
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error("no adapter named {0}")]
    UnknownAdapter(String),
    #[error(
        "{path} is not valid JSON ({message}); fix it or move it aside, nothing here will guess"
    )]
    InvalidJson { path: String, message: String },
    #[error("a token survived substitution, so harness.toml is missing a key: {}: {}", .0, .1.join(" "))]
    TokenSurvived(String, Vec<String>),
    #[error("the leftover-token pattern does not compile ({0}); this is a defect in the binary")]
    BadPattern(String),
}

fn io(path: impl std::fmt::Display) -> impl FnOnce(std::io::Error) -> InitError {
    let path = path.to_string();
    move |source| InitError::Io { path, source }
}

// what install-stale diffs against: the installed (substituted) form, not the templates
pub fn planned_files(root: &Path, opts: &InitOpts) -> Result<Vec<(String, String)>, InitError> {
    let (plan, _) = prepare(root, opts)?;
    Ok(plan.into_iter().map(|p| (p.path, p.content)).collect())
}

pub fn install(root: &Path, opts: &InitOpts) -> Result<InitReport, InitError> {
    let (plan, mut report) = prepare(root, opts)?;

    // assert before the first write, so a missing token leaves no half-install
    let pattern = regex::Regex::new(TOKEN).map_err(|e| InitError::BadPattern(e.to_string()))?;
    for file in &plan {
        let left = tokens(&pattern, &file.content);
        if !left.is_empty() {
            return Err(InitError::TokenSurvived(file.path.clone(), left));
        }
    }

    for file in &plan {
        if !opts.dry_run {
            let path = root.join(&file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(io(parent.display()))?;
            }
            fs::write(&path, &file.content).map_err(io(path.display()))?;
        }
        // reported only once it's actually on disk
        report.wrote.push(file.path.clone());
    }

    // the old answers move aside only once the new file holds them, and only for the run that read them
    let migrated = root.join("harness.json").is_file()
        && report.wrote.iter().any(|path| path == "harness.toml");
    if !opts.dry_run && migrated {
        let json = root.join("harness.json");
        fs::rename(&json, root.join("harness.json.migrated")).map_err(io(json.display()))?;
    }
    Ok(report)
}

struct Planned {
    path: String,
    content: String,
}

// all tree reads happen here; install() only writes
fn prepare(root: &Path, opts: &InitOpts) -> Result<(Vec<Planned>, InitReport), InitError> {
    if !git::git_ok(root, &["rev-parse", "--git-dir"]) {
        return Err(InitError::NotGit(root.display().to_string()));
    }
    let mut plan: Vec<Planned> = Vec::new();
    let mut report = InitReport::default();

    let pending = seed_config(root, &mut plan, &mut report)?;
    let cfg = match &pending {
        Some(text) => config_from(text)?,
        None => config::load(root)?,
    };
    let sub = |text: &str| config::subst(text, &cfg);
    let dir = cfg.layout.harness_dir.clone();
    let presets = agent::presets();
    let skills_dir = skills_root(&cfg, presets.get(&cfg.agent.preset));

    for (name, text) in ROLES {
        plan.push(write(format!("{dir}/roles/{name}"), sub(text)));
    }
    plan.push(write(format!("{dir}/RAILS.md"), sub(RAILS)));
    plan.push(write(format!("{dir}/.gitignore"), GITIGNORE.to_string()));
    // per tool, not per repo: this ships the project's own skill in the shared format
    for (name, text) in SKILL {
        let path = skills_dir.join("running-the-loop").join(name);
        plan.push(write(rel(&path), sub(text)));
    }

    seed(
        root,
        &mut plan,
        &mut report,
        "evals/README.md",
        sub(EVALS_README),
    );
    for (name, text) in DOCS {
        seed(root, &mut plan, &mut report, name, sub(text));
    }
    seed_context(root, &mut plan, &mut report, &cfg, &sub);

    let spec = cfg.layout.spec.clone();
    if has_content(&root.join(&spec)) {
        report.kept.push(spec.clone());
        report.notes.push(format!(
            "{spec} was kept — merge the SPEC.section.md template into it by hand"
        ));
    } else {
        seed(root, &mut plan, &mut report, &spec, sub(SPEC_SECTION));
    }

    for pointer in &cfg.layout.pointer_files {
        seed_pointer(root, &mut plan, &mut report, &cfg, pointer, &sub);
    }

    if let Some(name) = &opts.adapter {
        let preset = presets
            .get(name)
            .ok_or_else(|| InitError::UnknownAdapter(name.clone()))?;
        adapter(root, &mut plan, &mut report, &cfg, preset, &sub)?;
    }

    let track = track_paths(&plan, &report);
    report.track = track;
    Ok((plan, report))
}

fn write(path: String, content: String) -> Planned {
    Planned { path, content }
}

fn seed(
    root: &Path,
    plan: &mut Vec<Planned>,
    report: &mut InitReport,
    path: &str,
    content: String,
) {
    if has_content(&root.join(path)) {
        report.kept.push(path.to_string());
    } else {
        plan.push(write(path.to_string(), content));
    }
}

// returns the text only when this run wrote it -- config::load can't see an unwritten file
fn seed_config(
    root: &Path,
    plan: &mut Vec<Planned>,
    report: &mut InitReport,
) -> Result<Option<String>, InitError> {
    if has_content(&root.join("harness.toml")) {
        return Ok(None);
    }
    let json = root.join("harness.json");
    let text = if has_content(&json) {
        let read = fs::read_to_string(&json).map_err(io(json.display()))?;
        let (text, renamed) = config::migrate_json(&read)?;
        report.notes.push(
            "migrated: harness.json -> harness.toml; the old file is now harness.json.migrated"
                .to_string(),
        );
        report.migrated_keys = renamed;
        text
    } else {
        report
            .notes
            .push("no harness.toml — seeding the defaults. Edit it, then re-run.".to_string());
        config::DEFAULT_TOML.to_string()
    };
    plan.push(write("harness.toml".to_string(), text.clone()));
    Ok(Some(text))
}

// config::load reads a directory, so give the pending text a temporary one
fn config_from(text: &str) -> Result<Config, InitError> {
    let dir = tempfile::TempDir::new().map_err(io("a temporary directory"))?;
    let path = dir.path().join("harness.toml");
    fs::write(&path, text).map_err(io(path.display()))?;
    Ok(config::load(dir.path())?)
}

// resync rewrites only backticked defaults the template wrote, never the user's own prose
fn seed_context(
    root: &Path,
    plan: &mut Vec<Planned>,
    report: &mut InitReport,
    cfg: &Config,
    sub: &dyn Fn(&str) -> String,
) {
    let path = cfg.layout.context_file.clone();
    if !has_content(&root.join(&path)) {
        plan.push(write(path, sub(CONTEXT)));
        return;
    }
    let Ok(before) = fs::read_to_string(root.join(&path)) else {
        report.kept.push(path);
        return;
    };
    match resync(&before, default_check().as_ref(), &cfg.check) {
        None => report.kept.push(path),
        Some(text) => {
            report
                .notes
                .push(format!("updated: {path} now names `{}`", cfg.check.command));
            plan.push(write(path, text));
        }
    }
}

fn resync(
    text: &str,
    defaults: Option<&(String, String)>,
    check: &config::CheckConfig,
) -> Option<String> {
    let (command, force) = defaults?;
    let mut out = text.to_string();
    for (default, configured) in [(command, &check.command), (force, &check.force)] {
        if default != configured {
            out = out.replace(&format!("`{default}`"), &format!("`{configured}`"));
        }
    }
    (out != text).then_some(out)
}

fn default_check() -> Option<(String, String)> {
    let cfg = config_from("").ok()?;
    Some((cfg.check.command, cfg.check.force))
}

// Claude Code doesn't read AGENTS.md natively; the @ import is its documented workaround
fn seed_pointer(
    root: &Path,
    plan: &mut Vec<Planned>,
    report: &mut InitReport,
    cfg: &Config,
    pointer: &str,
    sub: &dyn Fn(&str) -> String,
) {
    let context = &cfg.layout.context_file;
    if pointer == context {
        return;
    }
    if !has_content(&root.join(pointer)) {
        plan.push(write(pointer.to_string(), sub(POINTER)));
        return;
    }
    report.kept.push(pointer.to_string());
    let points_at = fs::read_to_string(root.join(pointer))
        .map(|t| t.contains(context.as_str()))
        .unwrap_or(false);
    if !points_at {
        report.notes.push(format!(
            "{pointer} was kept — add a line pointing at {context}, or delete it"
        ));
    }
}

// install-stale also has to know this path to tell an overwritten file from a seeded one
pub(crate) fn skills_root(cfg: &Config, preset: Option<&Preset>) -> PathBuf {
    skills::skills_dir(cfg, preset)
}

// order matters: this is the order the tool fires the events in
const HOOKS: &[(&str, &[&str])] = &[
    ("pre_tool_use", &["immutable", "one-writer"]),
    ("stop", &["verify-done"]),
    ("prompt_submit", &["skills"]),
];

fn adapter(
    root: &Path,
    plan: &mut Vec<Planned>,
    report: &mut InitReport,
    cfg: &Config,
    preset: &Preset,
    sub: &dyn Fn(&str) -> String,
) -> Result<(), InitError> {
    let name = &preset.name;
    if name == "claude" {
        // role prompts double as the Claude subagent definitions here
        for (role, text) in ROLES {
            plan.push(write(format!(".claude/agents/{role}"), sub(text)));
        }
    }

    // written before hooks: a tool with no hooks file still reads its own instruction file
    if let Some(file) = preset.instruction_file.as_deref() {
        let planned = plan.iter().any(|p| p.path == file);
        if file != cfg.layout.context_file && !planned && !has_content(&root.join(file)) {
            plan.push(write(file.to_string(), sub(POINTER)));
        }
    }

    let Some(hooks_file) = preset.hooks_file.as_deref() else {
        report.notes.push(format!(
            "adapter {name}: no hooks file; hooks are code or absent for this tool"
        ));
        return Ok(());
    };

    let ours = if name == "claude" {
        claude_settings(sub)?
    } else {
        // non-claude vendors share this hook shape; only the event names differ, per preset
        hooks_value(preset)
    };
    let existing = fs::read_to_string(root.join(hooks_file)).ok();
    let content = match existing.as_deref().filter(|t| !t.trim().is_empty()) {
        Some(theirs) => merge_json(&ours, theirs, hooks_file)?,
        None => pretty(&ours),
    };
    plan.push(write(hooks_file.to_string(), content));
    Ok(())
}

fn claude_settings(sub: &dyn Fn(&str) -> String) -> Result<Value, InitError> {
    let mut value: Value =
        serde_json::from_str(&sub(CLAUDE_SETTINGS)).map_err(|e| InitError::InvalidJson {
            path: "adapters/claude/settings.json".to_string(),
            message: e.to_string(),
        })?;
    rewrite_commands(&mut value);
    Ok(value)
}

// e.g. "hooks/immutable.sh" -> "harness hook immutable", wherever "command" appears
fn rewrite_commands(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(command)) = map.get("command") {
                let name = hook_name(command);
                map.insert("command".to_string(), json!(format!("harness hook {name}")));
                return;
            }
            for (_, child) in map.iter_mut() {
                rewrite_commands(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(rewrite_commands),
        _ => {}
    }
}

// skill-hook is the one script name that doesn't match its subcommand (-> skills)
fn hook_name(command: &str) -> String {
    let base = command
        .trim_end_matches('"')
        .rsplit('/')
        .next()
        .unwrap_or(command)
        .trim_end_matches(".sh");
    match base {
        "skill-hook" => "skills".to_string(),
        other => other.to_string(),
    }
}

fn hooks_value(preset: &Preset) -> Value {
    let mut events = serde_json::Map::new();
    for (event, names) in HOOKS {
        let Some(key) = preset.hook_events.get(*event) else {
            continue;
        };
        let hooks: Vec<Value> = names
            .iter()
            .map(|n| json!({ "type": "command", "command": format!("harness hook {n}") }))
            .collect();
        events.insert(key.clone(), json!([{ "hooks": hooks }]));
    }
    json!({ "hooks": events })
}

// hooks dedup by command text, deny rules by exact match; everything else in the file is untouched
fn merge_json(ours: &Value, theirs: &str, path: &str) -> Result<String, InitError> {
    let bad = |message: String| InitError::InvalidJson {
        path: path.to_string(),
        message,
    };
    let mut merged: Value = serde_json::from_str(theirs).map_err(|e| bad(e.to_string()))?;
    let merged_obj = merged
        .as_object_mut()
        .ok_or_else(|| bad("expected an object".to_string()))?;

    if let Some(events) = ours.get("hooks").and_then(|h| h.as_object()) {
        let all = merged_obj
            .entry("hooks")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| bad("hooks is not an object".to_string()))?;
        for (event, groups) in events {
            let have = all
                .entry(event.clone())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| bad(format!("hooks.{event} is not a list")))?;
            let known: BTreeSet<String> = have
                .iter()
                .filter_map(|g| g.get("hooks")?.as_array())
                .flatten()
                .filter_map(command_of)
                .collect();
            for group in groups.as_array().into_iter().flatten() {
                let new: Vec<Value> = group
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|hooks| {
                        hooks
                            .iter()
                            .filter(|h| !command_of(h).is_some_and(|c| known.contains(&c)))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                if new.is_empty() {
                    continue;
                }
                let mut group = group.clone();
                if let Some(map) = group.as_object_mut() {
                    map.insert("hooks".to_string(), Value::Array(new));
                    have.push(group);
                }
            }
        }
    }

    if let Some(rules) = ours.pointer("/permissions/deny").and_then(|d| d.as_array()) {
        let deny = merged_obj
            .entry("permissions")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| bad("permissions is not an object".to_string()))?
            .entry("deny")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| bad("permissions.deny is not a list".to_string()))?;
        for rule in rules {
            if !deny.contains(rule) {
                deny.push(rule.clone());
            }
        }
    }
    Ok(pretty(&merged))
}

fn command_of(hook: &Value) -> Option<String> {
    Some(hook.get("command")?.as_str()?.to_string())
}

fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    text.push('\n');
    text
}

// gate_verdict treats an untracked path as work off the branch, so nothing here may go untracked
fn track_paths(plan: &[Planned], report: &InitReport) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for path in plan
        .iter()
        .map(|p| p.path.as_str())
        .chain(report.kept.iter().map(String::as_str))
    {
        let top = path.split('/').next().unwrap_or(path).to_string();
        if !out.contains(&top) {
            out.push(top);
        }
    }
    out
}

const TOKEN: &str = r"__[A-Z][A-Z_]+__";

fn tokens(pattern: &regex::Regex, text: &str) -> Vec<String> {
    let found: BTreeSet<String> = pattern
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();
    found.into_iter().collect()
}

fn has_content(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}

fn rel(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resync_that_cannot_read_the_default_rewrites_nothing() {
        let cfg = config_from("[check]\ncommand = \"make check\"\n").expect("config");
        let text = "- Verify, and this is what done means: `bun run check`\n";
        assert_eq!(resync(text, None, &cfg.check), None);

        let defaults = (
            "bun run check".to_string(),
            "bun run check -- --force".to_string(),
        );
        assert_eq!(
            resync(text, Some(&defaults), &cfg.check).as_deref(),
            Some("- Verify, and this is what done means: `make check`\n")
        );
    }
}
