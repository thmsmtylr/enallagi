//! config: the answers the harness substitutes into roles, gates and probes.
//!
//! `harness.default.toml` is embedded in the binary and is the base. A repo's
//! `harness.toml` is deep-merged over it: tables key by key, arrays whole.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

pub use crate::agent::UsagePaths;
use crate::agent::{Presets, TurnCap};

/// The default answers, embedded so a fresh repo needs no file.
pub const DEFAULT_TOML: &str = include_str!("../harness.default.toml");

/// The roles a stage can name, and so the only keys `[agent.<role>]` accepts.
pub const ROLE_NAMES: &[&str] = &[
    "scout",
    "adjudicator",
    "implementer",
    "verifier",
    "researcher",
];

/// Post-stage gates, by name. A `stage.post` entry outside this list is refused.
pub const GATE_NAMES: &[&str] = &[
    "implementer-not-done",
    "commit-verdict",
    "verdict",
    "scope",
    "check-delta",
    "commit-round",
    "adjudicator-halt",
    "dry-round",
];

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(
        "when: `{0}` is not a predicate (one of queue.takeable, queue.empty, task.attended, check.red, probe.<name>, optionally prefixed with !)"
    )]
    BadWhen(String),
    #[error("pipeline {pipeline}: stage `{stage}` is not defined by any [[stage]]")]
    UnknownStage { pipeline: String, stage: String },
    #[error("stage {stage}: post gate `{gate}` is not one of {}", GATE_NAMES.join(", "))]
    UnknownGate { stage: String, gate: String },
    #[error("stage {0}: set exactly one of role or command")]
    RoleOrCommand(String),
    #[error("stage {stage}: role `{role}` has no prompt file")]
    MissingRole { stage: String, role: String },
    #[error("role {role}: {{{{skill:{id}}}}} has no matching [[skill]] entry")]
    UndeclaredSkill { role: String, id: String },
    #[error(
        "stage {stage}: preset {preset} caps turns by neither a flag nor its own config, so the stage needs a timeout"
    )]
    NoTurnCapNoTimeout { stage: String, preset: String },
    #[error("agent preset `{0}` is not known")]
    UnknownPreset(String),
    #[error("agent preset custom needs a command list ({0})")]
    CustomWithoutCommand(String),
    #[error("check.command is empty")]
    EmptyCheck,
    #[error("[agent.{role}] is not a role ({})", ROLE_NAMES.join(", "))]
    UnknownRole { role: String },
    #[error("two [[stage]] tables are both named {0}")]
    DuplicateStage(String),
    #[error("two [[skill]] tables both have id {0}")]
    DuplicateSkill(String),
    #[error("skill id `{0}` must match ^[a-z0-9-]+$")]
    BadSkillId(String),
    #[error("skill {id}: path `{path}` must be relative and free of `..`")]
    BadSkillPath { id: String, path: String },
    #[error("stage {stage}: timeout `{value}` is not <n>s, <n>m or <n>h")]
    BadTimeout { stage: String, value: String },
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub agent: AgentConfig,
    pub check: CheckConfig,
    pub layout: Layout,
    pub pipeline: Vec<Pipeline>,
    pub stage: Vec<Stage>,
    pub skill: Vec<SkillDecl>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub preset: String,
    pub command: Option<Vec<String>>,
    pub model: Option<String>,
    pub usage: Option<UsagePaths>,
    pub rate_limit_pattern: String,
    /// Per-role overrides: any other key of `[agent]` is `[agent.<role>]`.
    /// Flattened, so `deny_unknown_fields` cannot also apply here; `validate`
    /// refuses a key that is not a role instead.
    #[serde(flatten)]
    pub roles: BTreeMap<String, AgentOverride>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentOverride {
    pub preset: Option<String>,
    pub command: Option<Vec<String>>,
    pub model: Option<String>,
    pub usage: Option<UsagePaths>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CheckConfig {
    pub command: String,
    /// The check with its cache defeated. Follows `command` unless pinned.
    pub force: String,
    pub fail_name: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layout {
    pub harness_dir: String,
    /// None means the agent preset's own skills directory.
    pub skills_dir: Option<String>,
    pub spec: String,
    pub rows_heading: String,
    pub rows_end_heading: String,
    pub context_file: String,
    pub pointer_files: Vec<String>,
    pub contract_file: String,
    pub source_root: String,
    pub source_ext: Vec<String>,
    pub test_file_suffix_re: String,
    pub test_decl_patterns: Vec<String>,
    pub harness_files: Vec<String>,
    pub harness_globs: Vec<String>,
    pub allowed_prefixes: Vec<String>,
    pub docs: Vec<String>,
    pub harness_allow: Vec<String>,
    pub machinery: Vec<String>,
    pub learnings_cap: usize,
    pub driver_command: String,
    pub skill_invocation: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pipeline {
    pub name: String,
    pub when: String,
    pub stages: Vec<String>,
    pub end_after_dry_rounds: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Stage {
    pub name: String,
    pub role: Option<String>,
    pub command: Option<String>,
    pub turns: u32,
    pub timeout: Option<String>,
    pub env: BTreeMap<String, String>,
    pub post: Vec<String>,
}

impl Default for Stage {
    fn default() -> Self {
        Stage {
            name: String::new(),
            role: None,
            command: None,
            turns: 40,
            timeout: None,
            env: BTreeMap::new(),
            post: Vec::new(),
        }
    }
}

impl Stage {
    /// `30m` -> 1800s. `None` when the stage sets no timeout.
    pub fn timeout_duration(&self) -> Result<Option<Duration>, ConfigError> {
        let Some(raw) = &self.timeout else {
            return Ok(None);
        };
        let value = raw.trim();
        let bad = || ConfigError::BadTimeout {
            stage: self.name.clone(),
            value: value.to_string(),
        };
        // char, not byte: `30м` (Cyrillic) must be refused, not split mid-scalar.
        let unit = value.chars().last().ok_or_else(bad)?;
        let scale: u64 = match unit {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            _ => return Err(bad()),
        };
        let digits = &value[..value.len() - unit.len_utf8()];
        let n: u64 = digits.parse().map_err(|_| bad())?;
        // `18446744073709551615h` is a config typo, not a duration.
        Ok(Some(Duration::from_secs(
            n.checked_mul(scale).ok_or_else(bad)?,
        )))
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkillDecl {
    pub id: String,
    pub source: String,
    pub path: String,
    pub rev: Option<String>,
    pub gate: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    QueueTakeable,
    QueueEmpty,
    TaskAttended,
    CheckRed,
    Probe(String),
    Not(Box<Predicate>),
}

/// The whole `when` vocabulary. No conjunction, no disjunction: a pipeline
/// that needs two conditions is two pipelines.
pub fn parse_when(s: &str) -> Result<Predicate, ConfigError> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('!') {
        return Ok(Predicate::Not(Box::new(parse_when(rest)?)));
    }
    match s {
        "queue.takeable" => Ok(Predicate::QueueTakeable),
        "queue.empty" => Ok(Predicate::QueueEmpty),
        "task.attended" => Ok(Predicate::TaskAttended),
        "check.red" => Ok(Predicate::CheckRed),
        _ => match s.strip_prefix("probe.") {
            Some(name) if !name.is_empty() && !name.contains(char::is_whitespace) => {
                Ok(Predicate::Probe(name.to_string()))
            }
            _ => Err(ConfigError::BadWhen(s.to_string())),
        },
    }
}

/// `harness.toml` deep-merged over the embedded defaults.
pub fn load(root: &Path) -> Result<Config, ConfigError> {
    let mut base: toml::Value = parse_toml(DEFAULT_TOML, "harness.default.toml")?;
    let path = root.join("harness.toml");
    if path.is_file() {
        let text = std::fs::read_to_string(&path)?;
        let user: toml::Value = parse_toml(&text, "harness.toml")?;
        // check.force follows check.command: overriding the check without
        // pinning the force variant must not leave the old tool behind.
        let user_check = user.get("check");
        if let (Some(command), None) = (
            user_check.and_then(|c| c.get("command")),
            user_check.and_then(|c| c.get("force")),
        ) {
            if let Some(base_check) = base.get_mut("check").and_then(|c| c.as_table_mut()) {
                base_check.insert("force".to_string(), command.clone());
            }
        }
        merge(&mut base, &user);
    }
    base.try_into()
        .map_err(|e: toml::de::Error| ConfigError::Parse {
            path: "harness.toml".to_string(),
            message: e.to_string(),
        })
}

fn parse_toml(text: &str, path: &str) -> Result<toml::Value, ConfigError> {
    toml::from_str(text).map_err(|e: toml::de::Error| ConfigError::Parse {
        path: path.to_string(),
        message: e.to_string(),
    })
}

/// Tables merge key by key, everything else replaces -- an array of tables
/// such as `[[stage]]` is a whole list, not a list to append to.
fn merge(base: &mut toml::Value, over: &toml::Value) {
    match (base, over) {
        (toml::Value::Table(b), toml::Value::Table(o)) => {
            for (key, value) in o {
                match b.get_mut(key) {
                    Some(slot) => merge(slot, value),
                    None => {
                        b.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (slot, value) => *slot = value.clone(),
    }
}

/// Every problem at once: a half-reported config costs a round trip per error.
pub fn validate(
    cfg: &Config,
    presets: &Presets,
    role_files: &dyn Fn(&str) -> Option<String>,
) -> Result<(), Vec<ConfigError>> {
    let mut errs = Vec::new();

    if cfg.check.command.trim().is_empty() {
        errs.push(ConfigError::EmptyCheck);
    }

    // [agent.<role>] is flattened, so a misspelled role parses happily into
    // the map and would silently never apply. Catch it here instead.
    for role in cfg.agent.roles.keys() {
        if !ROLE_NAMES.contains(&role.as_str()) {
            errs.push(ConfigError::UnknownRole { role: role.clone() });
        }
    }

    let mut stage_names: BTreeSet<&str> = BTreeSet::new();
    for st in &cfg.stage {
        if !stage_names.insert(st.name.as_str()) {
            errs.push(ConfigError::DuplicateStage(st.name.clone()));
        }
    }
    for p in &cfg.pipeline {
        if let Err(e) = parse_when(&p.when) {
            errs.push(e);
        }
        for stage in &p.stages {
            if !stage_names.contains(stage.as_str()) {
                errs.push(ConfigError::UnknownStage {
                    pipeline: p.name.clone(),
                    stage: stage.clone(),
                });
            }
        }
    }

    let mut declared: BTreeSet<&str> = BTreeSet::new();
    for sk in &cfg.skill {
        if !declared.insert(sk.id.as_str()) {
            errs.push(ConfigError::DuplicateSkill(sk.id.clone()));
        }
        // Both become filesystem paths in `skills::resolve`, so they are
        // checked here, before anything is fetched or written.
        if !crate::skills::valid_id(&sk.id) {
            errs.push(ConfigError::BadSkillId(sk.id.clone()));
        }
        if !crate::skills::valid_path(&sk.path) {
            errs.push(ConfigError::BadSkillPath {
                id: sk.id.clone(),
                path: sk.path.clone(),
            });
        }
    }
    let mut checked_roles: BTreeSet<&str> = BTreeSet::new();

    for st in &cfg.stage {
        match (&st.role, &st.command) {
            (Some(_), None) | (None, Some(_)) => {}
            _ => errs.push(ConfigError::RoleOrCommand(st.name.clone())),
        }
        for gate in &st.post {
            if !GATE_NAMES.contains(&gate.as_str()) {
                errs.push(ConfigError::UnknownGate {
                    stage: st.name.clone(),
                    gate: gate.clone(),
                });
            }
        }
        if let Err(e) = st.timeout_duration() {
            errs.push(e);
        }

        let Some(role) = &st.role else { continue };
        match role_files(role) {
            None => errs.push(ConfigError::MissingRole {
                stage: st.name.clone(),
                role: role.clone(),
            }),
            Some(text) => {
                if checked_roles.insert(role.as_str()) {
                    for id in crate::skills::required_ids(&text) {
                        if !declared.contains(id.as_str()) {
                            errs.push(ConfigError::UndeclaredSkill {
                                role: role.clone(),
                                id,
                            });
                        }
                    }
                }
            }
        }

        let over = cfg.agent.roles.get(role);
        let preset = over
            .and_then(|o| o.preset.clone())
            .unwrap_or_else(|| cfg.agent.preset.clone());
        if preset == "custom" {
            let command = over
                .and_then(|o| o.command.as_ref())
                .or(cfg.agent.command.as_ref());
            if !command.is_some_and(|c| !c.is_empty()) {
                errs.push(ConfigError::CustomWithoutCommand(role.clone()));
            }
        } else {
            match presets.get(&preset) {
                None => errs.push(ConfigError::UnknownPreset(preset)),
                Some(p) if p.turn_cap == TurnCap::None && st.timeout.is_none() => {
                    errs.push(ConfigError::NoTurnCapNoTimeout {
                        stage: st.name.clone(),
                        preset,
                    })
                }
                Some(_) => {}
            }
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// The `__SCREAMING_SNAKE__` tokens of a role prompt or template.
pub fn subst(text: &str, cfg: &Config) -> String {
    let l = &cfg.layout;
    let cap = l.learnings_cap.to_string();
    let mut tokens: Vec<(&str, &str)> = vec![
        ("__CHECK__", &cfg.check.command),
        ("__CHECK_FORCE__", &cfg.check.force),
        ("__SPEC__", &l.spec),
        ("__HARNESS_DIR__", &l.harness_dir),
        ("__CONTEXT_FILE__", &l.context_file),
        ("__CONTRACT_FILE__", &l.contract_file),
        ("__SKILL_INVOCATION__", &l.skill_invocation),
        ("__DRIVER_COMMAND__", &l.driver_command),
        ("__ROWS_HEADING__", &l.rows_heading),
        ("__ROWS_END_HEADING__", &l.rows_end_heading),
        ("__SOURCE_ROOT__", &l.source_root),
        ("__LEARNINGS_CAP__", &cap),
    ];
    // Unset means the preset's own directory, which only skills::skills_dir
    // knows. Leave the token standing rather than substituting an empty path.
    if let Some(dir) = l.skills_dir.as_deref() {
        tokens.push(("__SKILLS_DIR__", dir));
    }
    tokens.iter().fold(text.to_string(), |acc, (token, value)| {
        acc.replace(token, value)
    })
}

// ---------------------------------------------------------------- migration

/// A `harness.json` from the bash harness, as `harness.toml` text plus the
/// list of `"<old> -> <new>"` renames to show the user.
pub fn migrate_json(json: &str) -> Result<(String, Vec<String>), ConfigError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| ConfigError::Parse {
        path: "harness.json".to_string(),
        message: e.to_string(),
    })?;
    let Some(obj) = value.as_object() else {
        return Err(ConfigError::Parse {
            path: "harness.json".to_string(),
            message: "expected a JSON object".to_string(),
        });
    };

    let mut renamed = Vec::new();
    let mut agent: Vec<String> = Vec::new();
    let mut agent_usage: Vec<String> = Vec::new();
    let mut agent_roles: Vec<String> = Vec::new();
    let mut check: Vec<String> = Vec::new();
    let mut layout: Vec<String> = Vec::new();
    let mut skills: Vec<String> = Vec::new();

    for (key, value) in obj {
        // `_comment`, `_agent`, ... are prose, not answers.
        if key.starts_with('_') {
            continue;
        }
        match key.as_str() {
            "check" => {
                check.push(format!("command = {}", scalar(value)));
                renamed.push("check -> check.command".to_string());
            }
            "checkForce" => {
                check.push(format!("force = {}", scalar(value)));
                renamed.push("checkForce -> check.force".to_string());
            }
            "failNameSed" => {
                if let Some(re) = value.as_str().and_then(fail_name_from_sed) {
                    check.push(format!("fail_name = {}", toml_string(&re)));
                    renamed.push("failNameSed -> check.fail_name".to_string());
                }
            }
            "rateLimitPattern" => {
                agent.push(format!("rate_limit_pattern = {}", scalar(value)));
                renamed.push("rateLimitPattern -> agent.rate_limit_pattern".to_string());
            }
            "costSed" => {
                if value.as_str().is_some_and(|s| s.contains("total_cost_usd")) {
                    agent_usage.push("cost = \"total_cost_usd\"".to_string());
                    renamed.push("costSed -> agent.usage.cost".to_string());
                }
            }
            "agentCommand" => {
                agent.push("preset = \"custom\"".to_string());
                renamed.push("agentCommand -> agent.command".to_string());
                match value {
                    serde_json::Value::Object(per_role) => {
                        for (role, argv) in per_role {
                            if role == "default" {
                                agent.push(format!("command = {}", scalar(argv)));
                            } else {
                                agent_roles
                                    .push(format!("\n[agent.{role}]\ncommand = {}", scalar(argv)));
                            }
                        }
                    }
                    argv => agent.push(format!("command = {}", scalar(argv))),
                }
            }
            "skills" => {
                for skill in value.as_array().unwrap_or(&Vec::new()) {
                    skills.push(skill_decl(skill));
                }
                renamed.push("skills -> skill".to_string());
            }
            _ => {
                let snake = snake_case(key);
                let line = json_to_toml(value).map(|v| format!("{snake} = {v}"));
                // A key with no Layout field -- rowCountFile, or anything a
                // repo invented -- would make the migrated file unloadable now
                // that unknown keys are refused. Report it as dropped instead.
                match line.filter(|l| toml::from_str::<Layout>(l).is_ok()) {
                    Some(line) => {
                        layout.push(line);
                        renamed.push(format!("{key} -> layout.{snake}"));
                    }
                    None => renamed.push(format!("{key} -> dropped, no longer used")),
                }
            }
        }
    }

    let mut out = String::from("# Migrated from harness.json.\n");
    for (heading, body) in [
        ("[agent]", &agent),
        ("[agent.usage]", &agent_usage),
        ("[check]", &check),
        ("[layout]", &layout),
    ] {
        if !body.is_empty() {
            out.push_str(&format!("\n{heading}\n{}\n", body.join("\n")));
        }
        if heading == "[agent.usage]" {
            for role in &agent_roles {
                out.push_str(role);
                out.push('\n');
            }
        }
    }
    for skill in &skills {
        out.push_str(skill);
    }
    Ok((out, renamed))
}

/// `s/PATTERN/REPL/p` -> a Rust regex whose group 1 is the failing test name.
/// sed's BRE spells a group `\(...\)` and a literal paren `(`; a regex spells
/// them the other way around, so the escaping swaps.
fn fail_name_from_sed(sed: &str) -> Option<String> {
    let mut chars = sed.chars();
    if chars.next()? != 's' {
        return None;
    }
    let delim = chars.next()?;
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut escaped = false;
    for c in chars {
        if escaped {
            cur.push('\\');
            cur.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == delim {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    parts.push(cur);
    let pattern = parts.first()?;
    if pattern.is_empty() {
        return None;
    }

    let mut out = String::new();
    let mut escaped = false;
    for c in pattern.chars() {
        match (escaped, c) {
            (false, '\\') => escaped = true,
            (true, '(' | ')') => {
                out.push(c);
                escaped = false;
            }
            (true, c) => {
                out.push('\\');
                out.push(c);
                escaped = false;
            }
            (false, '(' | ')') => {
                out.push('\\');
                out.push(c);
            }
            (false, c) => out.push(c),
        }
    }
    // No group in the sed pattern means it deleted the prefix and kept the
    // rest; the regex has to capture that rest instead.
    if !pattern.contains("\\(") {
        out.push_str("(.+)$");
    }
    Some(out)
}

/// A skill entry of the old `skills` list. Nothing was fetched then, so the
/// source and rev are supplied here.
fn skill_decl(skill: &serde_json::Value) -> String {
    let name = skill.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let id = &skill_id(name);
    let (source, rev) = if id == "ponytail" {
        ("github:thmsmtylr/ponytail", "main")
    } else {
        ("github:obra/superpowers", "v6.3.0")
    };
    let field = |key: &str| toml_string(skill.get(key).and_then(|v| v.as_str()).unwrap_or(""));
    format!(
        "\n[[skill]]\nid = {}\nsource = {}\npath = {}\nrev = {}\ngate = {}\nwhy = {}\n",
        toml_string(id),
        toml_string(source),
        toml_string(&format!("skills/{id}")),
        toml_string(rev),
        field("gate"),
        field("why"),
    )
}

/// The old `name` was a tool-specific reference like
/// `superpowers:test-driven-development`. The id is its last segment, cut down
/// to what `validate` accepts.
fn skill_id(name: &str) -> String {
    name.rsplit(':')
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn snake_case(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn json_to_toml(v: &serde_json::Value) -> Option<toml::Value> {
    Some(match v {
        serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => toml::Value::Integer(i),
            None => toml::Value::Float(n.as_f64()?),
        },
        serde_json::Value::String(s) => toml::Value::String(s.clone()),
        serde_json::Value::Array(a) => {
            toml::Value::Array(a.iter().filter_map(json_to_toml).collect())
        }
        serde_json::Value::Null | serde_json::Value::Object(_) => return None,
    })
}

/// A JSON scalar or list as the TOML that denotes it.
fn scalar(v: &serde_json::Value) -> String {
    json_to_toml(v).map_or_else(|| "\"\"".to_string(), |t| t.to_string())
}

fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_load_when_no_file() {
        let d = tempfile::tempdir().unwrap();
        let c = load(d.path()).unwrap();
        assert_eq!(c.check.command, "bun run check");
        assert_eq!(c.pipeline.len(), 2);
        assert_eq!(c.stage[0].turns, 120);
    }

    #[test]
    fn user_file_overrides_defaults() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("harness.toml"),
            "[check]\ncommand='make'\nfail_name='x'\n",
        )
        .unwrap();
        let c = load(d.path()).unwrap();
        assert_eq!(c.check.command, "make");
        assert_eq!(c.check.force, "make");
    }

    #[test]
    fn when_vocabulary() {
        assert!(matches!(
            parse_when("!queue.takeable").unwrap(),
            Predicate::Not(_)
        ));
        assert!(
            matches!(parse_when("probe.litter").unwrap(), Predicate::Probe(ref n) if n == "litter")
        );
        assert!(parse_when("queue.takeable && x").is_err());
    }

    #[test]
    fn unknown_gate_is_refused() {
        let mut c = load(tempfile::tempdir().unwrap().path()).unwrap();
        c.stage[0].post.push("nope".into());
        let errs = validate(&c, &crate::agent::presets(), &|_| Some(String::new())).unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().contains("nope")));
    }

    #[test]
    fn no_turn_cap_and_no_timeout_is_refused() {
        let mut c = load(tempfile::tempdir().unwrap().path()).unwrap();
        c.agent.preset = "aider".into();
        let errs = validate(&c, &crate::agent::presets(), &|_| Some(String::new())).unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().contains("timeout")));
    }

    #[test]
    fn skill_token_without_declaration_is_refused() {
        let c = load(tempfile::tempdir().unwrap().path()).unwrap();
        let errs = validate(&c, &crate::agent::presets(), &|_| {
            Some("{{skill:nope}}".into())
        })
        .unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().contains("nope")));
    }

    /// Both end up as filesystem paths, so a `..` in either is refused at
    /// load rather than caught by whatever fetches next.
    #[test]
    fn a_skill_id_or_path_that_escapes_its_directory_is_refused() {
        let mut c = load(tempfile::tempdir().unwrap().path()).unwrap();
        c.skill[0].id = "..".into();
        c.skill[1].path = "../x".into();
        let errs = validate(&c, &crate::agent::presets(), &|_| Some(String::new())).unwrap_err();
        let text: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
        assert!(
            text.iter()
                .any(|e| e.contains("skill id `..`") && e.contains("^[a-z0-9-]+$")),
            "{text:?}"
        );
        assert!(
            text.iter()
                .any(|e| e.contains(&c.skill[1].id) && e.contains("../x")),
            "{text:?}"
        );
    }

    #[test]
    fn a_migrated_skill_name_becomes_a_valid_id() {
        assert_eq!(skill_id("Ponytail"), "ponytail");
        assert_eq!(skill_id("b/../c"), "b----c");
        assert!(crate::skills::valid_id(&skill_id("Weird Name_v2")));

        let (t, _) = migrate_json(
            r#"{"skills":[{"name":"superpowers:test-driven-development","gate":"none","why":"w"}]}"#,
        )
        .unwrap();
        assert!(t.contains("id = \"test-driven-development\""), "{t}");
        assert!(
            t.contains("path = \"skills/test-driven-development\""),
            "{t}"
        );
    }

    #[test]
    fn subst_replaces_tokens() {
        let c = load(tempfile::tempdir().unwrap().path()).unwrap();
        assert_eq!(
            subst("run __CHECK__ in __HARNESS_DIR__", &c),
            "run bun run check in .harness"
        );
    }

    #[test]
    fn migrate_json_renames_keys() {
        let (t, renamed) = migrate_json(
            // r###"..."### because the payload contains `"##`.
            r###"{"check":"make","checkForce":"make -B","rowsHeading":"## 11. X","agentCommand":["claude","-p","{prompt}"]}"###,
        )
        .unwrap();
        assert!(t.contains("force = \"make -B\""));
        assert!(renamed.contains(&"checkForce -> check.force".to_string()));
        assert!(t.contains("preset = \"custom\""));
    }

    // --- gaps the brief's seven leave open ---

    /// The file everything else is merged over has to pass its own rules.
    #[test]
    fn the_defaults_validate() {
        let c = load(tempfile::tempdir().unwrap().path()).unwrap();
        let roles = |_: &str| Some(String::new());
        assert!(validate(&c, &crate::agent::presets(), &roles).is_ok());
        assert_eq!(c.skill.len(), 7);
        assert_eq!(c.layout.learnings_cap, 12);
        assert_eq!(c.layout.skills_dir, None);
        assert_eq!(c.stage[2].env["HARNESS_DRIVER"], "1");
        assert_eq!(c.pipeline[1].end_after_dry_rounds, 2);
    }

    /// Tables merge key by key; an array of tables replaces the list whole,
    /// so a user's one [[stage]] is the only stage.
    #[test]
    fn tables_merge_and_arrays_replace() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("harness.toml"),
            "[layout]\nspec = 'DESIGN.md'\n\n[[stage]]\nname = 'only'\ncommand = 'true'\n",
        )
        .unwrap();
        let c = load(d.path()).unwrap();
        assert_eq!(c.layout.spec, "DESIGN.md");
        assert_eq!(c.layout.harness_dir, ".harness", "untouched keys survive");
        assert_eq!(c.stage.len(), 1);
        assert_eq!(c.stage[0].name, "only");
        assert_eq!(c.stage[0].turns, 40, "the per-stage default");
    }

    #[test]
    fn pinned_force_is_not_overwritten_by_command() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("harness.toml"),
            "[check]\ncommand='make'\nforce='make -B'\n",
        )
        .unwrap();
        assert_eq!(load(d.path()).unwrap().check.force, "make -B");
    }

    #[test]
    fn timeouts_parse_by_unit() {
        let mut st = Stage {
            name: "s".into(),
            ..Stage::default()
        };
        assert_eq!(st.timeout_duration().unwrap(), None);
        for (text, secs) in [("45s", 45), ("30m", 1800), ("2h", 7200)] {
            st.timeout = Some(text.into());
            assert_eq!(
                st.timeout_duration().unwrap(),
                Some(Duration::from_secs(secs))
            );
        }
        // A multibyte last char must not split mid-scalar, and a product that
        // does not fit must not overflow. Both used to panic.
        for bad in [
            "30",
            "",
            "m",
            "1d",
            "-5s",
            "30м",
            "м",
            "18446744073709551615h",
        ] {
            st.timeout = Some(bad.into());
            assert!(st.timeout_duration().is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let mut c = load(tempfile::tempdir().unwrap().path()).unwrap();
        c.check.command = String::new();
        c.pipeline[0].when = "queue.takeable && check.red".into();
        c.pipeline[0].stages.push("ghost".into());
        c.stage[0].command = Some("true".into()); // both role and command
        let errs = validate(&c, &crate::agent::presets(), &|_| Some(String::new())).unwrap_err();
        let text = errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("check.command is empty"), "{text}");
        assert!(text.contains("is not a predicate"), "{text}");
        assert!(text.contains("ghost"), "{text}");
        assert!(text.contains("exactly one of role or command"), "{text}");
    }

    #[test]
    fn a_missing_role_file_is_refused() {
        let c = load(tempfile::tempdir().unwrap().path()).unwrap();
        let errs = validate(&c, &crate::agent::presets(), &|_| None).unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().contains("implementer")));
    }

    #[test]
    fn a_role_override_picks_its_own_preset() {
        let mut c = load(tempfile::tempdir().unwrap().path()).unwrap();
        c.agent.roles.insert(
            "verifier".into(),
            AgentOverride {
                preset: Some("aider".into()),
                ..AgentOverride::default()
            },
        );
        let errs = validate(&c, &crate::agent::presets(), &|_| Some(String::new())).unwrap_err();
        assert_eq!(errs.len(), 1, "only the verifier stage lacks a turn cap");
        assert!(errs[0].to_string().contains("verify"));

        // A timeout on that stage settles it.
        c.stage[1].timeout = Some("30m".into());
        assert!(validate(&c, &crate::agent::presets(), &|_| Some(String::new())).is_ok());
    }

    #[test]
    fn custom_without_a_command_is_refused() {
        let mut c = load(tempfile::tempdir().unwrap().path()).unwrap();
        c.agent.preset = "custom".into();
        let roles = |_: &str| Some(String::new());
        assert!(validate(&c, &crate::agent::presets(), &roles).is_err());
        c.agent.command = Some(vec!["mytool".into(), "{prompt}".into()]);
        assert!(validate(&c, &crate::agent::presets(), &roles).is_ok());
    }

    #[test]
    fn declared_skills_satisfy_their_tokens() {
        let c = load(tempfile::tempdir().unwrap().path()).unwrap();
        let roles = |_: &str| {
            Some("use {{skill:tdd}} then {{skill:tdd}} and {{skill:ponytail}}".to_string())
        };
        assert!(validate(&c, &crate::agent::presets(), &roles).is_ok());
        assert_eq!(
            crate::skills::required_ids("{{skill:tdd}} {{skill:tdd}} {{skill:x}} {{skill:"),
            vec!["tdd".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn subst_covers_every_token() {
        let mut c = load(tempfile::tempdir().unwrap().path()).unwrap();
        let text = "__CHECK__|__CHECK_FORCE__|__SPEC__|__HARNESS_DIR__|__SKILLS_DIR__|\
                    __CONTEXT_FILE__|__CONTRACT_FILE__|__SKILL_INVOCATION__|__DRIVER_COMMAND__|\
                    __ROWS_HEADING__|__ROWS_END_HEADING__|__SOURCE_ROOT__|__LEARNINGS_CAP__";
        let mut out = subst(text, &c);
        assert_eq!(
            out.matches("__").count(),
            2,
            "only __SKILLS_DIR__ is left standing: {out}"
        );
        assert!(out.contains("__SKILLS_DIR__"));
        assert!(out.contains("SPEC.md") && out.contains("## 12.") && out.contains("|12"));

        // Set, it substitutes like any other token.
        c.layout.skills_dir = Some(".codex/skills".into());
        out = subst(text, &c);
        assert!(!out.contains("__"), "{out}");
        assert!(out.contains(".codex/skills"));
    }

    /// The shipped sed expression has to produce the shipped regex.
    #[test]
    fn fail_name_sed_becomes_a_capturing_regex() {
        assert_eq!(
            fail_name_from_sed("s/.*(fail) //p").unwrap(),
            r".*\(fail\) (.+)$"
        );
        // An explicit BRE group is kept as the capture, and nothing is appended.
        assert_eq!(
            fail_name_from_sed(r"s/^FAIL \(.*\)$/\1/p").unwrap(),
            r"^FAIL (.*)$"
        );
        assert_eq!(fail_name_from_sed("not a sed"), None);

        let re = regex::Regex::new(&fail_name_from_sed("s/.*(fail) //p").unwrap()).unwrap();
        let caps = re.captures("  1 | (fail) parses a header").unwrap();
        assert_eq!(&caps[1], "parses a header");
    }

    /// The real harness.default.json, migrated, must load and validate.
    #[test]
    fn the_shipped_json_migrates_into_a_valid_config() {
        let json = include_str!("../../../harness.default.json");
        let (text, renamed) = migrate_json(json).unwrap();
        assert!(renamed.iter().any(|r| r == "skills -> skill"));
        assert!(renamed
            .iter()
            .any(|r| r == "learningsCap -> layout.learnings_cap"));
        assert!(!renamed.iter().any(|r| r.starts_with('_')), "{renamed:?}");
        // Layout has no row_count_file; with unknown keys refused, migration
        // has to drop it rather than write a file that will not load.
        assert!(renamed
            .iter()
            .any(|r| r == "rowCountFile -> dropped, no longer used"));

        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("harness.toml"), &text).unwrap();
        let c = load(d.path()).unwrap();
        assert_eq!(c.agent.preset, "custom");
        assert_eq!(c.agent.command.as_ref().unwrap()[0], "claude");
        assert_eq!(
            c.agent.usage.as_ref().unwrap().cost.as_deref(),
            Some("total_cost_usd")
        );
        assert_eq!(c.check.force, "bun run check -- --force");
        assert_eq!(c.check.fail_name, r".*\(fail\) (.+)$");
        assert_eq!(c.layout.skills_dir.as_deref(), Some(".claude/skills"));
        assert_eq!(c.layout.learnings_cap, 12);
        assert_eq!(c.skill.len(), 7);
        assert_eq!(c.skill[0].id, "test-driven-development");
        assert_eq!(c.skill[1].source, "github:thmsmtylr/ponytail");
        assert_eq!(c.skill[1].rev.as_deref(), Some("main"));
        assert_eq!(c.skill[6].gate, "queue-uncovered");
        // The [[stage]] and [[pipeline]] defaults survive the overlay.
        assert_eq!(c.stage.len(), 4);
        let roles = |_: &str| Some(String::new());
        assert!(validate(&c, &crate::agent::presets(), &roles).is_ok());
    }

    #[test]
    fn per_role_agent_commands_migrate_to_role_tables() {
        let (text, _) = migrate_json(
            r#"{"check":"make","agentCommand":{"default":["a","{prompt}"],"verifier":["b","{prompt}"]}}"#,
        )
        .unwrap();
        assert!(text.contains("[agent.verifier]"), "the spec form: {text}");
        assert!(!text.contains("[agent.roles."), "{text}");
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("harness.toml"), &text).unwrap();
        let c = load(d.path()).unwrap();
        assert_eq!(c.agent.command.as_ref().unwrap()[0], "a");
        assert_eq!(c.agent.roles["verifier"].command.as_ref().unwrap()[0], "b");
    }

    /// `[agent.<role>]` is the spec form and the only one; the named fields of
    /// `[agent]` stay named fields beside it.
    #[test]
    fn a_role_table_is_a_plain_agent_subtable() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("harness.toml"),
            "[agent]\npreset = 'goose'\n\n[agent.usage]\ncost = 'c'\n\n\
             [agent.verifier]\npreset = 'gemini'\nmodel = 'g'\n",
        )
        .unwrap();
        let c = load(d.path()).unwrap();
        assert_eq!(c.agent.preset, "goose");
        assert_eq!(c.agent.rate_limit_pattern, "hit your session limit");
        assert_eq!(c.agent.usage.as_ref().unwrap().cost.as_deref(), Some("c"));
        assert_eq!(c.agent.roles.len(), 1);
        assert_eq!(c.agent.roles["verifier"].preset.as_deref(), Some("gemini"));
        assert_eq!(c.agent.roles["verifier"].model.as_deref(), Some("g"));
        assert!(validate(&c, &crate::agent::presets(), &|_| Some(String::new())).is_ok());
    }

    #[test]
    fn a_misspelled_role_table_is_refused() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("harness.toml"),
            "[agent.verifer]\npreset = 'codex'\n",
        )
        .unwrap();
        let c = load(d.path()).unwrap();
        let errs = validate(&c, &crate::agent::presets(), &|_| Some(String::new())).unwrap_err();
        assert!(
            errs.iter().any(|e| e.to_string().contains("verifer")),
            "{errs:?}"
        );
    }

    /// A config that does not validate refuses to run, and a key in the wrong
    /// table is exactly the typo that would otherwise run with a stale value.
    #[test]
    fn an_unknown_key_is_refused() {
        let d = tempfile::tempdir().unwrap();
        for text in [
            "harness_dir = 'x'\n",                   // meant [layout]
            "[layout]\nharnesdir = 'x'\n",           // misspelled
            "[check]\ncomand = 'make'\n",            // misspelled
            "[[stage]]\nname = 'x'\nturn = 10\n",    // misspelled
            "[[skill]]\nid = 'x'\nsrc = 'y'\n",      // misspelled
            "[agent.verifier]\npresett = 'codex'\n", // inside a role table
            "[[pipeline]]\nname = 'x'\nwen = 'y'\n", // misspelled
        ] {
            std::fs::write(d.path().join("harness.toml"), text).unwrap();
            assert!(load(d.path()).is_err(), "should be refused: {text}");
        }
    }

    #[test]
    fn duplicate_stage_names_and_skill_ids_are_refused() {
        let mut c = load(tempfile::tempdir().unwrap().path()).unwrap();
        c.stage.push(c.stage[0].clone());
        c.skill.push(c.skill[0].clone());
        let errs = validate(&c, &crate::agent::presets(), &|_| Some(String::new())).unwrap_err();
        let text = errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("both named implement"), "{text}");
        assert!(text.contains("both have id tdd"), "{text}");
    }

    #[test]
    fn migrating_junk_is_an_error_not_a_panic() {
        assert!(migrate_json("{").is_err());
        assert!(migrate_json("[1,2]").is_err());
    }
}
