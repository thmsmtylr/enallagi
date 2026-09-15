//! Adapter presets: one static TOML file per supported coding agent, embedded into the binary and parsed on demand.

use crate::config::{AgentConfig, Layout};
use crate::events::{Kind, Writer};
use regex::Regex;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TurnCap {
    Flag,
    Time,
    Config,
    None,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct UsagePaths {
    pub cost: Option<String>,
    pub input_tokens: Option<String>,
    pub output_tokens: Option<String>,
    pub turns: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Preset {
    pub name: String,
    pub argv: Vec<String>,
    pub turn_cap: TurnCap,
    #[serde(default)]
    pub usage: UsagePaths,
    #[serde(default)]
    pub skills_dir: Option<String>,
    #[serde(default)]
    pub invocation: Option<String>,
    #[serde(default)]
    pub instruction_file: Option<String>,
    #[serde(default)]
    pub hooks_file: Option<String>,
    #[serde(default)]
    pub hook_events: BTreeMap<String, String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub model_flag: Option<String>,
}

pub type Presets = BTreeMap<String, Preset>;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("preset {0} is not known")]
    UnknownPreset(String),
    #[error("preset custom needs [agent] command")]
    CustomWithoutCommand,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("stopped: STOP file appeared during a rate-limit wait")]
    Stopped,
    #[error("the agent command is empty")]
    EmptyCommand,
}

const PRESET_FILES: &[(&str, &str)] = &[
    ("claude", include_str!("../adapters/presets/claude.toml")),
    ("codex", include_str!("../adapters/presets/codex.toml")),
    ("gemini", include_str!("../adapters/presets/gemini.toml")),
    (
        "opencode",
        include_str!("../adapters/presets/opencode.toml"),
    ),
    ("copilot", include_str!("../adapters/presets/copilot.toml")),
    ("goose", include_str!("../adapters/presets/goose.toml")),
    ("aider", include_str!("../adapters/presets/aider.toml")),
    ("amp", include_str!("../adapters/presets/amp.toml")),
    ("cursor", include_str!("../adapters/presets/cursor.toml")),
    ("kimi", include_str!("../adapters/presets/kimi.toml")),
    ("qwen", include_str!("../adapters/presets/qwen.toml")),
    ("omp", include_str!("../adapters/presets/omp.toml")),
    ("pi", include_str!("../adapters/presets/pi.toml")),
];

pub fn presets() -> Presets {
    PRESET_FILES
        .iter()
        .map(|(name, text)| {
            let preset: Preset = toml::from_str(text).expect(name);
            (name.to_string(), preset)
        })
        .collect()
}

pub struct Resolved {
    pub argv: Vec<String>,
    pub preset: Preset,
}

// a hand-written command's cap is read back off its argv, never inherited from a preset it no longer matches
fn cap_of(argv: &[String]) -> TurnCap {
    if argv.iter().any(|w| w.contains("{timeout}")) {
        TurnCap::Time
    } else if argv.iter().any(|w| w.contains("{turns}")) {
        TurnCap::Flag
    } else {
        TurnCap::None
    }
}

// removes the flag along with its value -- `--max-turns` with nothing after it is a usage error
fn drop_token(argv: &mut Vec<String>, token: &str) {
    if let Some(i) = argv.iter().position(|w| w.contains(token)) {
        argv.remove(i);
        if i > 0 {
            argv.remove(i - 1);
        }
    }
}

pub fn resolve(cfg: &AgentConfig, role: &str, presets: &Presets) -> Result<Resolved, AgentError> {
    let over = cfg.roles.get(role);
    let name = over
        .and_then(|o| o.preset.as_deref())
        .unwrap_or(&cfg.preset);
    let command = over
        .and_then(|o| o.command.as_ref())
        .or(cfg.command.as_ref());
    let model = over
        .and_then(|o| o.model.as_deref())
        .or(cfg.model.as_deref());
    let usage = over.and_then(|o| o.usage.as_ref()).or(cfg.usage.as_ref());

    let mut preset = if name == "custom" {
        let argv = command.ok_or(AgentError::CustomWithoutCommand)?.clone();
        Preset {
            name: "custom".to_string(),
            turn_cap: cap_of(&argv),
            argv,
            usage: UsagePaths::default(),
            skills_dir: None,
            invocation: None,
            instruction_file: None,
            hooks_file: None,
            hook_events: BTreeMap::new(),
            env: BTreeMap::new(),
            model_flag: None,
        }
    } else {
        let mut preset = presets
            .get(name)
            .cloned()
            .ok_or_else(|| AgentError::UnknownPreset(name.to_string()))?;
        if let Some(argv) = command {
            preset.argv.clone_from(argv);
            preset.turn_cap = cap_of(argv);
        }
        preset
    };

    if let Some(usage) = usage {
        preset.usage = usage.clone();
    }
    let mut argv = std::mem::take(&mut preset.argv);
    if matches!(preset.turn_cap, TurnCap::None | TurnCap::Config) {
        drop_token(&mut argv, "{turns}");
    }
    if !matches!(preset.turn_cap, TurnCap::Time) {
        drop_token(&mut argv, "{timeout}");
    }
    if let (Some(flag), Some(model)) = (&preset.model_flag, model) {
        argv.push(flag.clone());
        argv.push(model.to_string());
    }
    preset.argv.clone_from(&argv);
    Ok(Resolved { argv, preset })
}

// substituted before {prompt}, so a prompt that quotes either token reaches the agent verbatim
pub fn fill_layout(argv: &[String], layout: &Layout) -> Vec<String> {
    argv.iter()
        .map(|word| {
            word.replace("{harness_dir}", &layout.harness_dir)
                .replace("{context_file}", &layout.context_file)
        })
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageValues {
    pub cost: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub turns: Option<u64>,
}

// agents stream one JSON object per line with totals in the last one, so the last parseable line wins
pub fn extract_usage(output: &str, paths: &UsagePaths) -> UsageValues {
    let mut values = UsageValues::default();
    let Some(last) = output.lines().rev().find_map(|line| {
        serde_json::from_str::<serde_json::Value>(line.trim())
            .ok()
            .filter(serde_json::Value::is_object)
    }) else {
        return values;
    };
    let at = |path: &Option<String>| -> Option<&serde_json::Value> {
        path.as_deref()?
            .split('.')
            .try_fold(&last, |node, key| node.get(key))
    };
    values.cost = at(&paths.cost).and_then(serde_json::Value::as_f64);
    values.input_tokens = at(&paths.input_tokens).and_then(serde_json::Value::as_u64);
    values.output_tokens = at(&paths.output_tokens).and_then(serde_json::Value::as_u64);
    values.turns = at(&paths.turns).and_then(serde_json::Value::as_u64);
    values
}

// without this a session-limit notice reads as a finished iteration and burns the rest of the run doing nothing
pub fn seconds_until_reset(notice: &str, now: jiff::Zoned) -> Option<u64> {
    static RESET: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RESET.get_or_init(|| {
        Regex::new(r"(?i)resets\s+(\d{1,2}):(\d{2})\s*([ap])\.?m\.?\s*\(([^)]+)\)")
            .expect("the reset pattern is a literal")
    });
    let caps = re.captures(notice)?;
    let hour: i8 = caps.get(1)?.as_str().parse().ok()?;
    let minute: i8 = caps.get(2)?.as_str().parse().ok()?;
    let pm = caps.get(3)?.as_str().eq_ignore_ascii_case("p");
    let hour = hour % 12 + if pm { 12 } else { 0 };
    let zone = jiff::tz::TimeZone::get(caps.get(4)?.as_str()).ok()?;
    let now = now.with_time_zone(zone);
    let mut reset = now
        .with()
        .hour(hour)
        .minute(minute)
        .second(0)
        .subsec_nanosecond(0)
        .build()
        .ok()?;
    if reset <= now {
        reset = reset.checked_add(jiff::Span::new().days(1)).ok()?;
    }
    let seconds = reset.timestamp().as_second() - now.timestamp().as_second();
    Some(u64::try_from(seconds).ok()? + 60)
}

pub struct StageSpawn<'a> {
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: &'a Path,
    pub timeout: Option<Duration>,
    pub prompt: String,
    pub turns: u32,
    pub stage: String,
    pub task: Option<String>,
    pub preset: Preset,
}

#[derive(Debug)]
pub struct StageResult {
    pub exit: i32,
    pub seconds: u64,
    pub output: String,
    pub usage: UsageValues,
    pub timed_out: bool,
}

const POLL: Duration = Duration::from_millis(200);
// how long a SIGTERM gets to be honoured before the child is killed outright
const GRACE: Duration = Duration::from_secs(10);
// a stale reset rolls to the same time tomorrow, so a further-out one isn't worth a day-long sleep
const MAX_WAIT: u64 = 6 * 3600;

// retried at most twice: an agent that prints the limit notice forever would hold the run forever
pub fn spawn(
    s: &StageSpawn,
    events: &mut Writer,
    stop_file: &Path,
    rate_limit: &Regex,
) -> Result<StageResult, AgentError> {
    let mut attempt = 0u32;
    // one clock for the whole stage, not per attempt: retries and waits count against the seconds budget
    let started = Instant::now();
    loop {
        let (exit, output, timed_out) = run_once(s, events)?;
        let result = StageResult {
            exit,
            seconds: started.elapsed().as_secs(),
            usage: extract_usage(&output, &s.preset.usage),
            output,
            timed_out,
        };
        if attempt >= 2 {
            return Ok(result);
        }
        let Some(matched) = result.output.lines().find(|l| rate_limit.is_match(l)) else {
            return Ok(result);
        };
        let Some(sleep_seconds) =
            seconds_until_reset(matched, jiff::Zoned::now()).filter(|s| *s <= MAX_WAIT)
        else {
            return Ok(result);
        };
        events.emit(Kind::Limit {
            stage: s.stage.clone(),
            matched: matched.to_string(),
            sleep_seconds,
            attempt,
        });
        sleep_until(sleep_seconds, stop_file)?;
        attempt += 1;
    }
}

fn sleep_until(mut left: u64, stop_file: &Path) -> Result<(), AgentError> {
    while left > 0 {
        if stop_file.exists() {
            return Err(AgentError::Stopped);
        }
        let chunk = left.min(60);
        std::thread::sleep(Duration::from_secs(chunk));
        left -= chunk;
    }
    Ok(())
}

fn run_once(s: &StageSpawn, events: &mut Writer) -> Result<(i32, String, bool), AgentError> {
    let turns = s.turns.to_string();
    let timeout = s.timeout.map(|t| t.as_secs().to_string());
    let argv: Vec<String> = s
        .argv
        .iter()
        .map(|word| {
            // prompt substituted last, one pass each: a prompt that quotes {turns} or {prompt} must reach the agent verbatim
            let word = word.replace("{turns}", &turns);
            let word = match &timeout {
                Some(t) => word.replace("{timeout}", t),
                None => word,
            };
            word.replace("{prompt}", &s.prompt)
        })
        .collect();
    let (program, args) = argv.split_first().ok_or(AgentError::EmptyCommand)?;

    let mut child = Command::new(program)
        .args(args)
        .current_dir(s.cwd)
        .envs(&s.preset.env)
        .envs(&s.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // stdout and stderr interleave into one buffer: most presets put progress on stderr and gates read one stream
    let buffer = Arc::new(Mutex::new(String::new()));
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(drain(out, Arc::clone(&buffer)));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(drain(err, Arc::clone(&buffer)));
    }

    let started = Instant::now();
    let mut sent = 0usize;
    let mut last_emit = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if s.timeout.is_some_and(|t| started.elapsed() >= t) {
            timed_out = true;
            break terminate(&mut child)?;
        }
        if last_emit.elapsed() >= Duration::from_secs(1) {
            flush(&buffer, &mut sent, &s.stage, events);
            last_emit = Instant::now();
        }
        std::thread::sleep(POLL);
    };
    // ponytail: a timed-out child can leave a grandchild holding the pipe, so
    // the readers are joined only on a clean exit; upgrade to a process-group
    // kill if a preset turns out to orphan writers on a clean exit too.
    if !timed_out {
        for reader in readers {
            let _ = reader.join();
        }
    }
    flush(&buffer, &mut sent, &s.stage, events);

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
    Ok((exit, output, timed_out))
}

fn terminate(child: &mut std::process::Child) -> Result<std::process::ExitStatus, AgentError> {
    let pid = child.id().to_string();
    let _ = Command::new("kill").args(["-TERM", &pid]).status();
    let deadline = Instant::now() + GRACE;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            child.kill()?;
            return Ok(child.wait()?);
        }
        std::thread::sleep(POLL);
    }
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

// at most one stage.output a second, or a chatty agent writes more event lines than the TUI can redraw off
fn flush(buffer: &Mutex<String>, sent: &mut usize, stage: &str, events: &mut Writer) {
    let chunk = match buffer.lock() {
        Ok(text) => {
            let chunk = text[*sent..].to_string();
            *sent = text.len();
            chunk
        }
        Err(_) => return,
    };
    if !chunk.is_empty() {
        events.emit(Kind::StageOutput {
            stage: stage.to_string(),
            chunk,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Kind, Log, Writer};
    use regex::Regex;

    #[test]
    fn thirteen_presets_load() {
        let presets = presets();
        assert_eq!(presets.len(), 13);
        assert_eq!(presets["aider"].turn_cap, TurnCap::None);
        assert_eq!(
            presets["claude"].usage.cost,
            Some("total_cost_usd".to_string())
        );
        for (name, preset) in &presets {
            assert!(
                preset.argv.iter().any(|a| a.contains("{prompt}")),
                "{name} argv missing {{prompt}}"
            );
        }
    }

    #[test]
    fn role_override_replaces_preset() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.roles.insert(
            "verifier".into(),
            crate::config::AgentOverride {
                preset: Some("codex".into()),
                command: None,
                model: None,
                usage: None,
            },
        );
        let presets = presets();
        let r = resolve(&cfg, "verifier", &presets).unwrap();
        assert_eq!(r.argv[0], "codex");
        let d = resolve(&cfg, "implementer", &presets).unwrap();
        assert_eq!(d.argv[0], "claude");
    }

    #[test]
    fn custom_preset_needs_a_command() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.preset = "custom".into();
        assert!(resolve(&cfg, "scout", &presets()).is_err());
        cfg.command = Some(vec!["mytool".into(), "{prompt}".into()]);
        assert_eq!(
            resolve(&cfg, "scout", &presets()).unwrap().argv[0],
            "mytool"
        );
    }

    #[test]
    fn a_preset_without_a_turn_flag_loses_the_turns_word_and_its_flag() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.preset = "gemini".into();
        cfg.model = Some("g-9".into());
        let r = resolve(&cfg, "scout", &presets()).unwrap();
        assert!(!r.argv.iter().any(|w| w.contains("{turns}")));
        assert!(!r.argv.iter().any(|w| w.contains("{timeout}")));
        assert_eq!(
            &r.argv[r.argv.len() - 2..],
            &["--model".to_string(), "g-9".to_string()]
        );
    }

    #[test]
    fn the_time_capped_preset_keeps_its_timeout_word() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.preset = "omp".into();
        let r = resolve(&cfg, "scout", &presets()).unwrap();
        assert!(r.argv.iter().any(|w| w == "{timeout}"));
    }

    #[test]
    fn usage_is_read_from_the_last_json_object() {
        let out = "text\n{\"type\":\"assistant\"}\n{\"type\":\"result\",\"total_cost_usd\":0.42,\"usage\":{\"input_tokens\":10,\"output_tokens\":5},\"num_turns\":3}\n";
        let u = extract_usage(out, &presets()["claude"].usage);
        assert_eq!(u.cost, Some(0.42));
        assert_eq!(u.turns, Some(3));
        assert_eq!(u.input_tokens, Some(10));
        assert_eq!(u.output_tokens, Some(5));
        let none = extract_usage("no json here\n", &presets()["claude"].usage);
        assert_eq!(none.cost, None);
    }

    #[test]
    fn reset_time_parses_from_a_notice() {
        let now = jiff::civil::date(2026, 9, 7)
            .at(10, 0, 0, 0)
            .in_tz("Australia/Melbourne")
            .unwrap();
        let s = seconds_until_reset(
            "You've hit your session limit \u{b7} resets 12:40am (Australia/Melbourne)",
            now.clone(),
        )
        .unwrap();
        assert_eq!(s, 14 * 3600 + 40 * 60 + 60);
        assert!(seconds_until_reset("hit your session limit", now.clone()).is_none());
        assert!(seconds_until_reset("resets 12:40am (Mars/Olympus)", now).is_none());
    }

    fn stop_file(root: &std::path::Path) -> std::path::PathBuf {
        crate::config::instance_path(root, ".enallagi", "STOP")
    }

    fn spawner<'a>(argv: Vec<String>, root: &'a std::path::Path) -> StageSpawn<'a> {
        StageSpawn {
            argv,
            env: Default::default(),
            cwd: root,
            timeout: None,
            prompt: String::new(),
            turns: 1,
            stage: "x".into(),
            task: None,
            preset: presets()["claude"].clone(),
        }
    }

    #[test]
    fn spawn_substitutes_and_streams() {
        let r = crate::fixture::Repo::new();
        let argv = r.stub_agent("echo \"prompt=$1 turns=$2\"; echo '{\"total_cost_usd\":0.1}'");
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let mut s = spawner(argv, &r.root);
        s.prompt = "hi there".into();
        s.turns = 7;
        s.stage = "implement".into();
        let res = spawn(
            &s,
            &mut w,
            &stop_file(&r.root),
            &Regex::new("never").unwrap(),
        )
        .unwrap();
        assert_eq!(res.exit, 0);
        assert!(
            res.output.contains("prompt=hi there turns=7"),
            "{}",
            res.output
        );
        assert_eq!(res.usage.cost, Some(0.1));
        assert!(!res.timed_out);
        assert!(w
            .log
            .read()
            .unwrap()
            .iter()
            .any(|e| matches!(e.kind, Kind::StageOutput { .. })));
    }

    #[test]
    fn a_signalled_child_reports_128_plus_the_signal() {
        let r = crate::fixture::Repo::new();
        // KILL, not INT: a job started with `&` from a non-interactive shell inherits SIGINT ignored
        let argv = r.stub_agent("kill -KILL $$");
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let res = spawn(
            &spawner(argv, &r.root),
            &mut w,
            &stop_file(&r.root),
            &Regex::new("never").unwrap(),
        )
        .unwrap();
        assert_eq!(res.exit, 137);
    }

    #[test]
    fn timeout_kills_and_reports_124() {
        let r = crate::fixture::Repo::new();
        let argv = r.stub_agent("sleep 30");
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let mut s = spawner(argv, &r.root);
        s.timeout = Some(std::time::Duration::from_secs(1));
        let res = spawn(
            &s,
            &mut w,
            &stop_file(&r.root),
            &Regex::new("never").unwrap(),
        )
        .unwrap();
        assert!(res.timed_out);
        assert_eq!(res.exit, 124);
    }

    #[test]
    fn a_stop_file_during_the_limit_wait_stops_the_run() {
        let r = crate::fixture::Repo::new();
        std::fs::create_dir_all(stop_file(&r.root).parent().unwrap()).unwrap();
        std::fs::write(stop_file(&r.root), "").unwrap();
        let argv = r.stub_agent(&limit_notice("1M", "+1 minute"));
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let err = spawn(
            &spawner(argv, &r.root),
            &mut w,
            &stop_file(&r.root),
            &Regex::new("hit your session limit").unwrap(),
        )
        .unwrap_err();
        assert!(matches!(err, AgentError::Stopped));
        assert_eq!(
            w.log
                .read()
                .unwrap()
                .iter()
                .filter(|e| matches!(e.kind, Kind::Limit { .. }))
                .count(),
            1
        );
    }

    // reset computed at run time, not fixed: a fixed timestamp would be in the past by the second retry
    // BSD `date` first, GNU second, so this runs on both macOS and Linux
    fn limit_notice(bsd: &str, gnu: &str) -> String {
        const FMT: &str = "'+hit your session limit resets %I:%M%p (UTC)'";
        format!("date -u -v+{bsd} {FMT} 2>/dev/null || date -u -d '{gnu}' {FMT}")
    }

    #[test]
    #[ignore = "sleeps up to four minutes waiting out two synthetic session limits"]
    fn rate_limit_is_retried_at_most_twice() {
        let r = crate::fixture::Repo::new();
        let runs = r.root.join("runs");
        let notice = limit_notice("1M", "+1 minute");
        let argv = r.stub_agent(&format!("echo x >> {}; {notice}", runs.display()));
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let res = spawn(
            &spawner(argv, &r.root),
            &mut w,
            &stop_file(&r.root),
            &Regex::new("hit your session limit").unwrap(),
        )
        .unwrap();
        assert_eq!(res.exit, 0);
        assert_eq!(std::fs::read_to_string(&runs).unwrap().lines().count(), 3);
        assert!(
            res.seconds >= 120,
            "seconds covers both waits, not just the last attempt: {}",
            res.seconds
        );
        let limits: Vec<_> = w
            .log
            .read()
            .unwrap()
            .into_iter()
            .filter_map(|e| match e.kind {
                Kind::Limit { attempt, .. } => Some(attempt),
                _ => None,
            })
            .collect();
        assert_eq!(limits, vec![0, 1]);
    }

    #[test]
    fn the_prompt_is_substituted_last() {
        let r = crate::fixture::Repo::new();
        let argv = r.stub_agent("printf '%s|%s\\n' \"$1\" \"$2\"");
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let mut s = spawner(argv, &r.root);
        s.prompt = "see {turns} and {prompt}".into();
        s.turns = 7;
        let res = spawn(
            &s,
            &mut w,
            &stop_file(&r.root),
            &Regex::new("never").unwrap(),
        )
        .unwrap();
        assert_eq!(res.output.trim(), "see {turns} and {prompt}|7");
    }

    #[test]
    fn a_reset_more_than_six_hours_out_is_not_a_limit() {
        let r = crate::fixture::Repo::new();
        let argv = r.stub_agent(&limit_notice("10H", "+10 hours"));
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let res = spawn(
            &spawner(argv, &r.root),
            &mut w,
            &stop_file(&r.root),
            &Regex::new("hit your session limit").unwrap(),
        )
        .unwrap();
        assert_eq!(res.exit, 0);
        assert!(!w
            .log
            .read()
            .unwrap()
            .iter()
            .any(|e| matches!(e.kind, Kind::Limit { .. })));
    }

    #[test]
    fn an_override_command_keeps_the_tokens_it_spells_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.preset = "gemini".into();
        cfg.command = Some(vec![
            "mytool".into(),
            "--turns".into(),
            "{turns}".into(),
            "{prompt}".into(),
        ]);
        let r = resolve(&cfg, "scout", &presets()).unwrap();
        assert_eq!(r.argv[1..3], ["--turns".to_string(), "{turns}".to_string()]);
        assert_eq!(r.preset.turn_cap, TurnCap::Flag);

        cfg.command = Some(vec!["mytool".into(), "{prompt}".into()]);
        let r = resolve(&cfg, "scout", &presets()).unwrap();
        assert_eq!(r.argv, ["mytool".to_string(), "{prompt}".to_string()]);
        assert_eq!(r.preset.turn_cap, TurnCap::None);
    }
}
