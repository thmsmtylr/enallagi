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
const CLAUDE_PLUGIN: &str = "adapters/claude";
const CLAUDE_HOOKS: &str = include_str!("../../../adapters/claude/hooks/hooks.json");

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
    /// `(old, new)` for every instance file outside the harness directory.
    pub moves: Vec<(String, String)>,
    /// Paths outside the harness directory that the product does not track, hidden in its info/exclude.
    pub excluded: Vec<String>,
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
    #[error("{0} already exists; move it aside, nothing is overwritten")]
    MoveTarget(String),
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
    report.moves = moves(root)?;

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

    let dir = config::load(root)?.layout.harness_dir;
    if !opts.dry_run && own_repository(root, &dir) {
        state_repository(root, &dir, &report.excluded)?;
    }

    // the old answers move aside only once the new file holds them, and only for the run that read them
    let toml = config_rel(root);
    let migrated = root.join("harness.json").is_file() && report.wrote.contains(&toml);
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
    let mut cfg = match &pending {
        Some(text) => config_from(text)?,
        None => config::load(root)?,
    };
    let dir = cfg.layout.harness_dir.clone();
    // config_from resolves an unset context file in a scratch directory, which never has a root-layout queue
    let unset = |text: &str| {
        toml::from_str::<Value>(text)
            .ok()
            .and_then(|v| v.get("layout")?.get("context_file").cloned())
            .is_none()
    };
    if pending.as_deref().is_some_and(unset) {
        cfg.layout.context_file = config::instance_rel(root, &dir, "AGENTS.md");
    }
    config::root_layout_skills(root, &mut cfg);
    let at = |name: &str| config::instance_rel(root, &dir, name);
    let toml = config_rel(root);
    // before subst, which would put a root-layout queue under the harness directory
    let sub = |text: &str| {
        let text = ROOT_INSTANCE_FILES
            .iter()
            .fold(text.to_string(), |acc, name| {
                acc.replace(&format!("__HARNESS_DIR__/{name}"), &at(name))
            })
            .replace("__HARNESS_DIR__/harness.toml", &toml);
        config::subst(&text, &cfg)
    };
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
        &at("evals/README.md"),
        sub(EVALS_README),
    );
    for (name, text) in DOCS {
        seed(root, &mut plan, &mut report, &at(name), sub(text));
    }
    seed_context(root, &mut plan, &mut report, &cfg, &sub);

    let spec = at(&cfg.layout.spec);
    if has_content(&root.join(&spec)) {
        report.kept.push(spec.clone());
        report.notes.push(format!(
            "{spec} was kept — merge the SPEC.section.md template into it by hand"
        ));
    } else {
        seed(root, &mut plan, &mut report, &spec, sub(SPEC_SECTION));
    }

    let tool = match &opts.adapter {
        Some(name) => Some(
            presets
                .get(name)
                .ok_or_else(|| InitError::UnknownAdapter(name.clone()))?,
        ),
        None => None,
    };
    let spawned = tool.cloned().or_else(|| {
        agent::resolve(&cfg.agent, "default", &presets)
            .ok()
            .map(|r| r.preset)
    });
    // a tool that takes the context file on its command line needs no pointer to it at the root
    if !spawned.is_some_and(|p| p.argv.iter().any(|w| w.contains("{context_file}"))) {
        // written before hooks: a tool with no hooks file still reads its own instruction file
        let own = tool.and_then(|t| t.instruction_file.as_ref());
        for pointer in cfg.layout.pointer_files.iter().chain(own) {
            seed_pointer(root, &mut plan, &mut report, &cfg, pointer, &sub);
        }
    }

    // without a manifest claude names the plugin after its directory, and every skill invocation says harness:<id>
    let plugin = format!("{dir}/{CLAUDE_PLUGIN}");
    if skills_dir.starts_with(&plugin) || tool.is_some_and(|t| t.name == "claude") {
        let manifest = json!({
            "name": "harness",
            "description": "The harness's role agents, vendored skills and hooks",
        });
        plan.push(write(
            format!("{plugin}/.claude-plugin/plugin.json"),
            pretty(&manifest),
        ));
    }

    if let Some(preset) = tool {
        adapter(root, &dir, &mut plan, &mut report, preset, &sub)?;
    }

    if own_repository(root, &dir) {
        report.excluded = untracked_entry_points(root, &dir, &plan, &report.kept);
    }
    let mut track = track_paths(&plan, &report);
    if own_repository(root, &dir) {
        track.retain(|top| *top != dir);
    }
    report.track = track;
    Ok((plan, report))
}

// a harness directory the product already tracks, or a root-layout queue, stays in the product's history
fn own_repository(root: &Path, dir: &str) -> bool {
    !dir.is_empty()
        && config::instance_rel(root, dir, "TASKS.md") != "TASKS.md"
        && git::git(root, &["ls-files", "--", dir]).is_ok_and(|out| out.is_empty())
}

// a tracked file is never excluded: exclude does not hide it, and `git add` on an excluded path exits 1
fn untracked_entry_points(
    root: &Path,
    dir: &str,
    plan: &[Planned],
    kept: &[String],
) -> Vec<String> {
    let outside: Vec<&str> = plan
        .iter()
        .map(|p| p.path.as_str())
        .chain(kept.iter().map(String::as_str))
        .filter(|path| !path.starts_with(&format!("{dir}/")))
        .collect();
    let mut args = vec!["ls-files", "--"];
    args.extend(&outside);
    let tracked = git::git(root, &args).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for path in outside {
        if !tracked.lines().any(|t| t == path) && !out.iter().any(|seen| seen == path) {
            out.push(path.to_string());
        }
    }
    out
}

const EXCLUDE_OPEN: &str = "# >>> harness";
const EXCLUDE_CLOSE: &str = "# <<< harness";

fn state_repository(root: &Path, dir: &str, entries: &[String]) -> Result<(), InitError> {
    let git_err = |e: git::GitError| InitError::Io {
        path: dir.to_string(),
        source: std::io::Error::other(e.to_string()),
    };
    if !root.join(dir).join(".git").exists() {
        git::git(&root.join(dir), &["init", "-q"]).map_err(git_err)?;
    }
    let exclude = git::git(root, &["rev-parse", "--git-path", "info/exclude"]).map_err(git_err)?;
    let exclude = root.join(exclude);
    let text = fs::read_to_string(&exclude).unwrap_or_default();
    let bare = format!("/{dir}/");
    let mut kept: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        match line {
            EXCLUDE_OPEN => inside = true,
            EXCLUDE_CLOSE => inside = false,
            _ if inside || line == bare => {}
            _ => kept.push(line),
        }
    }
    // ponytail: entry paths are written unescaped, so a configured name holding `*`, `?`, `[` or `!` matches as a pattern
    let block = std::iter::once(bare)
        .chain(entries.iter().map(|e| format!("/{e}")))
        .fold(String::new(), |acc, line| acc + &line + "\n");
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("{EXCLUDE_OPEN}\n{block}{EXCLUDE_CLOSE}\n"));
    if let Some(parent) = exclude.parent() {
        fs::create_dir_all(parent).map_err(io(parent.display()))?;
    }
    fs::write(&exclude, out).map_err(io(exclude.display()))?;
    Ok(())
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
    if has_content(&config::config_path(root)) {
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
    plan.push(write(config_rel(root), text.clone()));
    Ok(Some(text))
}

fn config_rel(root: &Path) -> String {
    let path = config::config_path(root);
    rel(path.strip_prefix(root).unwrap_or(&path))
}

const ROOT_INSTANCE_FILES: &[&str] = &[
    "TASKS.md",
    "PROGRESS.md",
    "PROGRESS.archive.md",
    "LEARNINGS.md",
    "DECISIONS.md",
    ".check-baseline",
    "test-hashes.json",
    "harness.lock",
    "evals/README.md",
];

// a legacy directory is the one config::load substitutes when harness_dir is unset, so it differs from the default only then
pub fn moves(root: &Path) -> Result<Vec<(String, String)>, InitError> {
    let cfg = config::load(root)?;
    let configured = fs::read_to_string(config::config_path(root))
        .ok()
        .and_then(|text| toml::from_str::<Value>(&text).ok())
        .and_then(|v| Some(v.get("layout")?.get("harness_dir")?.as_str()?.to_string()));
    let default = config_from("")?.layout.harness_dir;
    let legacy = configured.is_none() && cfg.layout.harness_dir != default;
    let dir = configured.unwrap_or(default);

    let mut out = Vec::new();
    if legacy {
        let mut files = Vec::new();
        files_under(root, &cfg.layout.harness_dir, &mut files);
        files.sort();
        for old in files {
            let name = &old[cfg.layout.harness_dir.len() + 1..];
            out.push((old.clone(), format!("{dir}/{name}")));
        }
    }
    let root_toml = config::config_path(root) == root.join("harness.toml");
    let names = ROOT_INSTANCE_FILES
        .iter()
        .copied()
        .chain(["harness.toml", cfg.layout.spec.as_str()]);
    for name in names {
        let at_root = if name == "harness.toml" {
            root_toml
        } else {
            config::instance_rel(root, &dir, name) == name
        };
        if at_root && root.join(name).is_file() {
            out.push((name.to_string(), format!("{dir}/{name}")));
        }
    }
    Ok(out)
}

fn files_under(root: &Path, dir: &str, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(root.join(dir)) else {
        return;
    };
    for entry in entries.flatten() {
        let path = format!("{dir}/{}", entry.file_name().to_string_lossy());
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            files_under(root, &path, out);
        } else {
            out.push(path);
        }
    }
}

// every destination is checked before the first rename, so a collision leaves the tree as it was
pub fn relocate(root: &Path, moves: &[(String, String)]) -> Result<(), InitError> {
    let mut seen = BTreeSet::new();
    for (_, new) in moves {
        if !seen.insert(new) || root.join(new).symlink_metadata().is_ok() {
            return Err(InitError::MoveTarget(new.clone()));
        }
    }
    for (old, new) in moves {
        let (from, to) = (root.join(old), root.join(new));
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(io(parent.display()))?;
        }
        fs::rename(&from, &to).map_err(io(from.display()))?;
        let mut dir = from.parent();
        while let Some(emptied) = dir.filter(|d| *d != root && fs::remove_dir(d).is_ok()) {
            dir = emptied.parent();
        }
    }
    Ok(())
}

// config::load reads a directory, so give the pending text a temporary one
fn config_from(text: &str) -> Result<Config, InitError> {
    let dir = tempfile::TempDir::new().map_err(io("a temporary directory"))?;
    let path = config::config_path(dir.path());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io(parent.display()))?;
    }
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
    if pointer == context
        || plan.iter().any(|p| p.path == pointer)
        || report.kept.iter().any(|k| k == pointer)
    {
        return;
    }
    let tracked = git::git(root, &["ls-files", "--", pointer]).is_ok_and(|out| !out.is_empty());
    if !tracked && !has_content(&root.join(pointer)) {
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
    dir: &str,
    plan: &mut Vec<Planned>,
    report: &mut InitReport,
    preset: &Preset,
    sub: &dyn Fn(&str) -> String,
) -> Result<(), InitError> {
    let name = &preset.name;
    if name == "claude" {
        // role prompts double as the Claude subagent definitions here
        for (role, text) in ROLES {
            plan.push(write(
                format!("{dir}/{CLAUDE_PLUGIN}/agents/{role}"),
                sub(text),
            ));
        }
    }

    let Some(hooks_file) = preset.hooks_file.as_deref() else {
        report.notes.push(format!(
            "adapter {name}: no hooks file; hooks are code or absent for this tool"
        ));
        return Ok(());
    };

    let hooks_file = hooks_file.replace("{harness_dir}", dir);
    let hooks_file = hooks_file.as_str();
    let ours = if name == "claude" {
        claude_hooks(sub)?
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

fn claude_hooks(sub: &dyn Fn(&str) -> String) -> Result<Value, InitError> {
    let mut value: Value =
        serde_json::from_str(&sub(CLAUDE_HOOKS)).map_err(|e| InitError::InvalidJson {
            path: "adapters/claude/hooks/hooks.json".to_string(),
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

    // lane commits carry no co-author trailer; attribution text a repo already set is kept
    if let Some(ours) = ours.get("attribution").and_then(|a| a.as_object()) {
        let theirs = merged_obj
            .entry("attribution")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| bad("attribution is not an object".to_string()))?;
        for (key, value) in ours {
            theirs.entry(key.clone()).or_insert_with(|| value.clone());
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
        .filter(|path| !report.excluded.iter().any(|e| e == path))
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
