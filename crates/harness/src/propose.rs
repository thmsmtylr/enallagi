//! Asks the configured agent for the check when no runner preset matches the tree, and writes it only once the check proves it.

use crate::config::{self, Config};
use crate::runners::{self, Detected};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const BEGIN: &str = "BEGIN ENALLAGI CHECK";
const END: &str = "END ENALLAGI CHECK";
pub const PROBE_NAME: &str = "enallagi_probe_fails";
const TURNS: u32 = 30;
const TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub command: String,
    pub fail_name: String,
    pub test_file_suffix_re: String,
    pub test_decl_patterns: Vec<String>,
    pub probe_file: String,
    pub probe_body: String,
    #[serde(default)]
    pub origin: BTreeMap<String, String>,
}

impl Proposal {
    fn keys(&self) -> Vec<Detected> {
        let at = |key: &str, leaf: &str, value: toml::Value| Detected {
            key: key.to_string(),
            value,
            origin: self
                .origin
                .get(leaf)
                .cloned()
                .unwrap_or_else(|| "agent".to_string()),
        };
        vec![
            at("check.command", "command", self.command.clone().into()),
            at(
                "check.fail_name",
                "fail_name",
                self.fail_name.clone().into(),
            ),
            at(
                "layout.test_file_suffix_re",
                "test_file_suffix_re",
                self.test_file_suffix_re.clone().into(),
            ),
            at(
                "layout.test_decl_patterns",
                "test_decl_patterns",
                self.test_decl_patterns.clone().into(),
            ),
        ]
    }
}

pub enum Outcome {
    Written(Vec<Detected>),
    /// Each attempt's proposal and the step it failed.
    Refused(Vec<(Vec<Detected>, String)>),
}

/// Asks, verifies, and writes enallagi.toml only when every step passes; a refusal is sent back once.
pub fn run(root: &Path, cfg: &Config) -> anyhow::Result<Outcome> {
    crate::agent::catch_stop_signals();
    let mut refused: Vec<(Vec<Detected>, String)> = Vec::new();
    let first = ask(root, cfg, &prompt())?;
    let step = match verify(root, cfg, &first) {
        Ok(()) => return written(root, first.keys()),
        Err(step) => step,
    };
    refused.push((first.keys(), step.clone()));
    let again = format!(
        "{}\nA proposal was already refused at this step, so propose again with it fixed:\n{step}\n",
        prompt()
    );
    match ask(root, cfg, &again) {
        Ok(second) => match verify(root, cfg, &second) {
            Ok(()) => return written(root, second.keys()),
            Err(step) => refused.push((second.keys(), step)),
        },
        Err(err) => refused.push((Vec::new(), err.to_string())),
    }
    Ok(Outcome::Refused(refused))
}

fn written(root: &Path, keys: Vec<Detected>) -> anyhow::Result<Outcome> {
    write(root, &keys)?;
    Ok(Outcome::Written(keys))
}

fn prompt() -> String {
    format!(
        "This repository's test runner has no enallagi preset. Read the repository: its package \
manifest, build files, CI workflows and a few test files. Name the one command that runs the whole \
test suite, the way its CI or its test script does, and how that command's output names a failing \
test. Do not edit, create or delete any file, and install nothing: enallagi runs the command itself \
to verify the answer.

Every regex is read by the Rust `regex` crate: no look-around (`(?=`, `(?!`, `(?<=`, `(?<!`) and \
no backreferences.

End the reply with exactly one block in this form, TOML between the two marker lines:

{BEGIN}
command = \"the shell command, run from the repository root, that exits 0 only when every test passes\"
fail_name = 'a regex matched against each output line; capture group 1 is the failing test name'
test_file_suffix_re = 'a regex that matches the repository-relative path of every test file'
test_decl_patterns = [\"how a test is declared in source, with {{name}} where its name goes\"]
probe_file = \"a repository-relative path that does not exist, in a directory that does, which the runner would pick up as a test file\"
probe_body = \"\"\"the full source of that file: one test named {PROBE_NAME} that always fails\"\"\"

[origin]
command = \"file:line the command was read from\"
fail_name = \"file:line\"
test_file_suffix_re = \"file:line\"
test_decl_patterns = \"file:line\"
{END}
"
    )
}

fn ask(root: &Path, cfg: &Config, prompt: &str) -> anyhow::Result<Proposal> {
    let presets = crate::agent::presets();
    let resolved = crate::agent::resolve(&cfg.agent, "default", &presets)?;
    let dir = &cfg.layout.harness_dir;
    let spawn = crate::agent::StageSpawn {
        argv: crate::agent::fill_layout(&resolved.argv, &cfg.layout),
        env: BTreeMap::new(),
        cwd: root,
        timeout: Some(TIMEOUT),
        prompt: prompt.to_string(),
        turns: TURNS,
        stage: "propose-check".to_string(),
        task: None,
        preset: resolved.preset,
    };
    let rate_limit = regex::Regex::new(&format!("(?i){}", cfg.agent.rate_limit_pattern))?;
    let mut events = crate::events::Writer::new(crate::events::Log::open(&root.join(dir)));
    let stop_file = config::instance_path(root, dir, "STOP");
    let result = crate::agent::spawn(&spawn, &mut events, &stop_file, &rate_limit)?;
    parse(&result.output).ok_or_else(|| {
        anyhow::anyhow!(
            "the agent exited {} and printed no `{BEGIN}` block that parses",
            result.exit
        )
    })
}

/// The last marked block in the output that parses; a JSON event line is read for the strings it carries.
pub fn parse(output: &str) -> Option<Proposal> {
    let mut texts = vec![String::new()];
    for line in output.lines() {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) if value.is_object() || value.is_array() => strings(&value, &mut texts),
            _ => {
                texts[0].push_str(line);
                texts[0].push('\n');
            }
        }
    }
    texts
        .iter()
        .flat_map(|text| blocks(text))
        .rev()
        .find_map(|block| toml::from_str(&block).ok())
}

fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) if text.contains(BEGIN) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

// an agent that fences its answer in markdown still answers
fn blocks(text: &str) -> Vec<String> {
    text.split(BEGIN)
        .skip(1)
        .filter_map(|part| part.split_once(END))
        .map(|(body, _)| {
            body.lines()
                .filter(|line| !line.trim_start().starts_with("```"))
                .collect::<Vec<&str>>()
                .join("\n")
        })
        .collect()
}

// removed on drop, so a check that fails, times out or is stopped leaves no file behind
struct Planted(PathBuf);

impl Drop for Planted {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The first step the proposal fails, or nothing when the check proves every key.
pub fn verify(root: &Path, cfg: &Config, p: &Proposal) -> Result<(), String> {
    let fail_name = regex::Regex::new(&p.fail_name).map_err(|e| format!("fail_name: {e}"))?;
    if fail_name.captures_len() < 2 {
        return Err("fail_name has no capture group to name a test".to_string());
    }
    let suffix = regex::Regex::new(&p.test_file_suffix_re)
        .map_err(|e| format!("test_file_suffix_re: {e}"))?;
    if p.test_decl_patterns.is_empty() || p.test_decl_patterns.iter().any(|d| !d.contains("{name}"))
    {
        return Err("every test_decl_patterns entry needs a {name}".to_string());
    }
    if !p
        .test_decl_patterns
        .iter()
        .any(|d| p.probe_body.contains(&d.replace("{name}", PROBE_NAME)))
    {
        return Err(format!(
            "no test_decl_patterns entry declares {PROBE_NAME} in probe_body"
        ));
    }
    let rel = Path::new(&p.probe_file);
    if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(format!(
            "probe_file {} leaves the repository root",
            p.probe_file
        ));
    }
    let probe = root.join(rel);
    if probe.exists() {
        return Err(format!("probe_file {} already exists", p.probe_file));
    }
    if !probe.parent().is_some_and(Path::is_dir) {
        return Err(format!(
            "probe_file {} has no existing directory",
            p.probe_file
        ));
    }
    if !suffix.is_match(&p.probe_file) {
        return Err(format!(
            "test_file_suffix_re does not match {}",
            p.probe_file
        ));
    }
    let tracked = crate::git::git(root, &["ls-files"]).unwrap_or_default();
    if !tracked.lines().any(|path| suffix.is_match(path)) {
        return Err("test_file_suffix_re matches no tracked file".to_string());
    }

    let mut checked = cfg.clone();
    checked.check.command.clone_from(&p.command);
    checked.check.fail_name.clone_from(&p.fail_name);
    let clean = crate::gates::check_delta(root, &checked, false);
    if let Some(reason) = &clean.timed_out {
        return Err(reason.clone());
    }
    if clean.red {
        return Err(format!(
            "the check exits {} on the clean tree:\n{}",
            clean.exit,
            clean.tail(20).join("\n")
        ));
    }

    std::fs::write(&probe, &p.probe_body).map_err(|e| format!("{}: {e}", p.probe_file))?;
    let planted = Planted(probe);
    let report = crate::gates::check_delta(root, &checked, false);
    drop(planted);
    if let Some(reason) = &report.timed_out {
        return Err(reason.clone());
    }
    if !report.red {
        return Err(format!(
            "the check exits 0 with {} in place, so the runner does not pick it up",
            p.probe_file
        ));
    }
    let names: Vec<&String> = report.unforgiven.iter().chain(&report.forgiven).collect();
    if !names.iter().any(|n| n.contains(PROBE_NAME)) {
        return Err(format!(
            "with {} in place the check exits {} and fail_name names {names:?}, not {PROBE_NAME}:\n{}",
            p.probe_file,
            report.exit,
            report.tail(20).join("\n")
        ));
    }
    Ok(())
}

// the config hash is re-cut only when it still matches the text this write replaces
fn write(root: &Path, keys: &[Detected]) -> anyhow::Result<()> {
    let path = config::config_path(root);
    let before = std::fs::read_to_string(&path)?;
    let (after, _) = config::prune_defaults(&runners::apply(&before, keys))?;
    std::fs::write(&path, &after)?;
    let dir = config::harness_dir(root);
    let hashes_path = config::instance_path(root, &dir, "test-hashes.json");
    let Ok(text) = std::fs::read_to_string(&hashes_path) else {
        return Ok(());
    };
    let mut hashes: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)?;
    let key = path
        .strip_prefix(root)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/");
    let old = crate::skills::sha256(before.as_bytes());
    if hashes.get(&key).and_then(|v| v.as_str()) == Some(old.as_str()) {
        hashes.insert(key, crate::skills::sha256(after.as_bytes()).into());
        std::fs::write(&hashes_path, serde_json::to_string_pretty(&hashes)? + "\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "command = \"make test\"\nfail_name = '^FAIL (.+)$'\ntest_file_suffix_re = '_test\\.sh$'\ntest_decl_patterns = [\"test {name}\"]\nprobe_file = \"t/p_test.sh\"\nprobe_body = \"test enallagi_probe_fails\"\n";

    #[test]
    fn a_plain_block_parses() {
        let out = format!("thinking\n{BEGIN}\n```toml\n{BLOCK}```\n{END}\n");
        assert_eq!(parse(&out).unwrap().command, "make test");
    }

    #[test]
    fn a_block_inside_a_json_event_parses() {
        let text = format!("{BEGIN}\n{BLOCK}{END}");
        let line = serde_json::json!({"type": "result", "result": text}).to_string();
        assert_eq!(parse(&line).unwrap().probe_file, "t/p_test.sh");
    }

    #[test]
    fn the_last_block_that_parses_wins() {
        let out = format!("{BEGIN}\n{BLOCK}{END}\n{BEGIN}\nnot toml\n{END}\n");
        assert_eq!(parse(&out).unwrap().command, "make test");
        let later = BLOCK.replace("make test", "make check");
        let out = format!("{BEGIN}\n{BLOCK}{END}\n{BEGIN}\n{later}{END}\n");
        assert_eq!(parse(&out).unwrap().command, "make check");
    }

    #[test]
    fn no_block_parses_to_nothing() {
        assert!(parse("no answer\n").is_none());
    }
}
