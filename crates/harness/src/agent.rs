//! Adapter presets: one static TOML file per supported coding agent, embedded into the binary and parsed on demand.

use crate::config::{AgentConfig, Layout};
use crate::events::{Kind, Writer};
use regex::Regex;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
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
    pub cache_creation_input_tokens: Option<String>,
    pub cache_read_input_tokens: Option<String>,
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
    #[serde(default)]
    pub effort_flag: Option<String>,
    // kept out of argv so a lane runs with the agent's permission checks unless the operator asks otherwise
    #[serde(default)]
    pub bypass_flag: Option<String>,
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
    #[error("stopped: a STOP file or a signal arrived during a rate-limit wait")]
    Stopped,
    #[error("the agent command is empty")]
    EmptyCommand,
    #[error("preset {0} declares no bypass flag, so dangerously_skip_permissions cannot apply")]
    NoBypassFlag(String),
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

/// A model and an effort as one block reads them; the first level `resolve_task` looks at.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Levels {
    pub model: Option<String>,
    pub effort: Option<String>,
}

pub struct Resolved {
    pub argv: Vec<String>,
    pub preset: Preset,
    pub levels: Levels,
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

// the name init.rs gives the plugin it writes for `--plugin-dir`; disabling it would take the harness's own hooks with it
const ADAPTER_PLUGIN: &str = "harness@inline";

// `--settings` layers over the operator's own file rather than displacing it, so a plugin enabled there loads into every lane unless it is named false here
fn plugins_off(file: &str) -> String {
    let path = match file.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => Path::new(&home).join(rest),
            None => return "{}".to_string(),
        },
        None => Path::new(file).to_path_buf(),
    };
    let enabled = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("enabledPlugins").cloned());
    let Some(serde_json::Value::Object(enabled)) = enabled else {
        return "{}".to_string();
    };
    let off: serde_json::Map<String, serde_json::Value> = enabled
        .keys()
        .filter(|id| id.as_str() != ADAPTER_PLUGIN)
        .map(|id| (id.clone(), serde_json::Value::Bool(false)))
        .collect();
    serde_json::Value::Object(off).to_string()
}

// the settings path rides in the token rather than a preset key, so only a preset that asks for the override names a vendor's file
fn fill_plugins_off(argv: &[String]) -> Vec<String> {
    static TOKEN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = TOKEN.get_or_init(|| {
        Regex::new(r"\{plugins_off:([^{}]*)\}").expect("the token pattern is a literal")
    });
    argv.iter()
        .map(|word| {
            re.replace_all(word, |caps: &regex::Captures| plugins_off(&caps[1]))
                .into_owned()
        })
        .collect()
}

pub fn resolve(cfg: &AgentConfig, role: &str, presets: &Presets) -> Result<Resolved, AgentError> {
    resolve_task(cfg, role, presets, &Levels::default())
}

/// `resolve` with the task's own levels, which outrank `[agent.<role>]` and `[agent]` in that order.
pub fn resolve_task(
    cfg: &AgentConfig,
    role: &str,
    presets: &Presets,
    task: &Levels,
) -> Result<Resolved, AgentError> {
    let over = cfg.roles.get(role);
    let name = over
        .and_then(|o| o.preset.as_deref())
        .unwrap_or(&cfg.preset);
    let command = over
        .and_then(|o| o.command.as_ref())
        .or(cfg.command.as_ref());
    let model = task
        .model
        .as_deref()
        .or(over.and_then(|o| o.model.as_deref()))
        .or(cfg.model.as_deref());
    let effort = task
        .effort
        .as_deref()
        .or(over.and_then(|o| o.effort.as_deref()))
        .or(cfg.effort.as_deref());
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
            effort_flag: None,
            bypass_flag: None,
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
    if cfg.dangerously_skip_permissions {
        let flag = preset
            .bypass_flag
            .clone()
            .ok_or_else(|| AgentError::NoBypassFlag(preset.name.clone()))?;
        argv.push(flag);
    }
    if let (Some(flag), Some(model)) = (&preset.model_flag, model) {
        argv.push(flag.clone());
        argv.push(model.to_string());
    }
    // a preset with no effort_flag has no way to carry the setting, so it is dropped rather than refused
    if let (Some(flag), Some(effort)) = (&preset.effort_flag, effort) {
        argv.push(flag.clone());
        argv.push(effort.to_string());
    }
    let argv = fill_plugins_off(&argv);
    preset.argv.clone_from(&argv);
    let levels = Levels {
        model: model.map(str::to_string),
        effort: effort.map(str::to_string),
    };
    Ok(Resolved {
        argv,
        preset,
        levels,
    })
}

fn fill_word(word: &str, layout: &Layout) -> String {
    word.replace("{harness_dir}", &layout.harness_dir)
        .replace("{context_file}", &layout.context_file)
}

// substituted before {prompt}, so a prompt that quotes either token reaches the agent verbatim
pub fn fill_layout(argv: &[String], layout: &Layout) -> Vec<String> {
    argv.iter().map(|w| fill_word(w, layout)).collect()
}

/// A preset's `env` with the same layout tokens its `argv` takes.
pub fn fill_env(env: &BTreeMap<String, String>, layout: &Layout) -> BTreeMap<String, String> {
    env.iter()
        .map(|(k, v)| (k.clone(), fill_word(v, layout)))
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageValues {
    pub cost: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
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
    values.cache_creation_input_tokens =
        at(&paths.cache_creation_input_tokens).and_then(serde_json::Value::as_u64);
    values.cache_read_input_tokens =
        at(&paths.cache_read_input_tokens).and_then(serde_json::Value::as_u64);
    values.turns = at(&paths.turns).and_then(serde_json::Value::as_u64);
    values
}

// the date is optional: a weekly notice read `resets Sep 20 at 4am` and a session one `resets 2am`
fn reset_pattern() -> &'static Regex {
    static RESET: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RESET.get_or_init(|| {
        // the minutes are optional too: a live notice read `resets 2am` and the hour-only form must still wait
        Regex::new(
            r"(?i)resets\s+(?:([a-z]{3,9})\s+(\d{1,2})\s+at\s+)?(\d{1,2})(?::(\d{2}))?\s*([ap]\.?m\.?)\s*\(([^)]+)\)",
        )
        .expect("the reset pattern is a literal")
    })
}

fn month_number(name: &str) -> Option<i8> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let head = name.get(..3)?.to_ascii_lowercase();
    let index = MONTHS.iter().position(|m| *m == head)?;
    i8::try_from(index + 1).ok()
}

/// The dated part of a reset notice, `Sep 20 at 4am (Australia/Melbourne)`, in the notice's own words.
pub fn dated_reset(notice: &str) -> Option<String> {
    let caps = reset_pattern().captures(notice)?;
    let month = caps.get(1)?;
    month_number(month.as_str())?;
    // to the end of the whole match, not the meridiem: without the zone the time it names is ambiguous
    Some(notice[month.start()..caps.get(0)?.end()].to_string())
}

// without this a session-limit notice reads as a finished iteration and burns the rest of the run doing nothing
pub fn seconds_until_reset(notice: &str, now: jiff::Zoned) -> Option<u64> {
    let caps = reset_pattern().captures(notice)?;
    let hour: i8 = caps.get(3)?.as_str().parse().ok()?;
    if !(1..=12).contains(&hour) {
        return None;
    }
    let minute: i8 = match caps.get(4) {
        Some(m) => m.as_str().parse().ok()?,
        None => 0,
    };
    let pm = caps.get(5)?.as_str().starts_with(['p', 'P']);
    let hour = hour % 12 + if pm { 12 } else { 0 };
    let zone = jiff::tz::TimeZone::get(caps.get(6)?.as_str()).ok()?;
    let now = now.with_time_zone(zone);
    let reset = match (caps.get(1), caps.get(2)) {
        (Some(month), Some(day)) => {
            let month = month_number(month.as_str())?;
            let day: i8 = day.as_str().parse().ok()?;
            let at = |year: i16| -> Option<jiff::Zoned> {
                jiff::civil::Date::new(year, month, day)
                    .ok()?
                    .at(hour, minute, 0, 0)
                    .to_zoned(now.time_zone().clone())
                    .ok()
            };
            // the notice carries no year, so a date already past is next year's
            let this_year = at(now.year())?;
            if this_year > now {
                this_year
            } else {
                at(now.year() + 1)?
            }
        }
        _ => {
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
            reset
        }
    };
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
    /// The dated reset of a notice further out than the wait ceiling, once the stage has run.
    pub reset_beyond_wait: Option<String>,
}

const POLL: Duration = Duration::from_millis(200);
// a stale reset rolls to the same time tomorrow, so a further-out one isn't worth a day-long sleep
const MAX_WAIT: u64 = 6 * 3600;

// the two numbers POSIX fixes, so no binding crate is needed to name them
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;
const SIG_DFL: usize = 0;

static SIGNALLED: AtomicI32 = AtomicI32::new(0);

extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

extern "C" fn note_signal(signum: i32) {
    SIGNALLED.store(signum, Ordering::Relaxed);
    // an operator who signals twice is not made to wait: the second one gets the default disposition
    unsafe { signal(signum, SIG_DFL) };
}

/// Take SIGINT and SIGTERM, so a stop reaches the agent the launcher spawned instead of orphaning it.
pub fn catch_stop_signals() {
    let handler = note_signal as extern "C" fn(i32) as usize;
    unsafe {
        signal(SIGINT, handler);
        signal(SIGTERM, handler);
    }
}

/// The name of the signal a stop arrived on, once one has.
pub fn stop_signal() -> Option<&'static str> {
    match SIGNALLED.load(Ordering::Relaxed) {
        SIGINT => Some("SIGINT"),
        SIGTERM => Some("SIGTERM"),
        _ => None,
    }
}

#[cfg(not(test))]
fn stage_now() -> jiff::Zoned {
    jiff::Zoned::now()
}

// a test fixes the instant a notice is read against, so a reset time is a verdict and not a race
#[cfg(test)]
thread_local! {
    static FIXED_NOW: std::cell::RefCell<Option<jiff::Zoned>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn stage_now() -> jiff::Zoned {
    FIXED_NOW
        .with(|n| n.borrow().clone())
        .unwrap_or_else(jiff::Zoned::now)
}

// only the run's own verdict line, never a file the agent read or a command it wrote: both quote the
// notice while working on this scan, and each cost a round a multi-hour sleep on 2026-09-16
fn notice_of(line: &str) -> Option<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        // a preset that prints plain text has no envelope to read, so the line stands as itself
        return Some(line.to_string());
    };
    if value.get("type").and_then(|t| t.as_str()) != Some("result") {
        return None;
    }
    Some(
        value
            .get("result")
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .to_string(),
    )
}

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
        let mut result = StageResult {
            exit,
            seconds: started.elapsed().as_secs(),
            usage: extract_usage(&output, &s.preset.usage),
            output,
            timed_out,
            reset_beyond_wait: None,
        };
        if attempt >= 2 {
            return Ok(result);
        }
        let Some(matched) = result
            .output
            .lines()
            .filter_map(notice_of)
            .find(|text| rate_limit.is_match(text))
        else {
            return Ok(result);
        };
        let waited = seconds_until_reset(&matched, stage_now());
        let Some(sleep_seconds) = waited.filter(|s| *s <= MAX_WAIT) else {
            // the stage ran, so its seconds, usage and StageEnd are owed before the halt reads the reset
            result.reset_beyond_wait = dated_reset(&matched).filter(|_| waited.is_some());
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

fn sleep_until(seconds: u64, stop_file: &Path) -> Result<(), AgentError> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        // the same predicate the halt reads: every writer of the marker writes a file, never a directory
        if stop_file.is_file() || stop_signal().is_some() {
            return Err(AgentError::Stopped);
        }
        std::thread::sleep(left.min(POLL));
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

    // the layout is reloaded here because StageSpawn carries the filled argv, not the layout that filled it
    let preset_env = match crate::config::load(s.cwd) {
        Ok(cfg) => fill_env(&s.preset.env, &cfg.layout),
        Err(_) => s.preset.env.clone(),
    };

    use std::os::unix::process::CommandExt;
    let mut command = Command::new(program);
    crate::config::drop_legacy_env(&mut command);
    // a lane commits as the repository's configured identity; left to itself it invents one from
    // whatever the agent's own session knows, and that identity lands in the operator's history
    for (var, key) in [
        ("GIT_AUTHOR_NAME", "user.name"),
        ("GIT_COMMITTER_NAME", "user.name"),
        ("GIT_AUTHOR_EMAIL", "user.email"),
        ("GIT_COMMITTER_EMAIL", "user.email"),
    ] {
        if let Ok(value) = crate::git::git(s.cwd, &["config", "--get", key]) {
            if !value.is_empty() {
                command.env(var, value);
            }
        }
    }
    // the agent gets its own process group, so a stop reaches the lanes and shells it spawned too
    let mut child = command
        .args(args)
        .current_dir(s.cwd)
        .envs(&preset_env)
        .envs(&s.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let pgid = child.id();

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
    let mut stopped = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if stop_signal().is_some() {
            stopped = true;
            break crate::gates::kill_group(&mut child, pgid)?;
        }
        if s.timeout.is_some_and(|t| started.elapsed() >= t) {
            timed_out = true;
            break crate::gates::kill_group(&mut child, pgid)?;
        }
        if last_emit.elapsed() >= Duration::from_secs(1) {
            flush(&buffer, &mut sent, &s.stage, events);
            last_emit = Instant::now();
        }
        std::thread::sleep(POLL);
    };
    // a killed child can leave a grandchild holding the pipe, so the readers are joined only on a clean exit
    if !timed_out && !stopped {
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
    fn no_preset_argv_carries_its_bypass_flag() {
        let presets = presets();
        let expected = [
            ("claude", "--dangerously-skip-permissions"),
            ("codex", "--dangerously-bypass-approvals-and-sandbox"),
            ("gemini", "--yolo"),
            ("qwen", "--yolo"),
            ("kimi", "--yolo"),
            ("omp", "--yolo"),
        ];
        for (name, flag) in expected {
            assert_eq!(presets[name].bypass_flag.as_deref(), Some(flag), "{name}");
        }
        for (name, preset) in &presets {
            if let Some(flag) = &preset.bypass_flag {
                assert!(!preset.argv.contains(flag), "{name} argv carries {flag}");
            }
        }
    }

    #[test]
    fn the_bypass_flag_rides_only_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.preset = "claude".into();
        let off = resolve(&cfg, "scout", &presets()).unwrap();
        assert!(!off
            .argv
            .iter()
            .any(|w| w == "--dangerously-skip-permissions"));
        cfg.dangerously_skip_permissions = true;
        let on = resolve(&cfg, "scout", &presets()).unwrap();
        assert!(on
            .argv
            .iter()
            .any(|w| w == "--dangerously-skip-permissions"));
    }

    #[test]
    fn no_bypass_flag_refuses_the_skip() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.preset = "pi".into();
        cfg.dangerously_skip_permissions = true;
        let err = resolve(&cfg, "scout", &presets()).err().expect("refused");
        assert!(err.to_string().contains("pi"), "{err}");
    }

    #[test]
    fn role_override_replaces_preset() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.roles.insert(
            "verifier".into(),
            crate::config::AgentOverride {
                preset: Some("codex".into()),
                ..crate::config::AgentOverride::default()
            },
        );
        let presets = presets();
        let r = resolve(&cfg, "verifier", &presets).unwrap();
        assert_eq!(r.argv[0], "codex");
        let d = resolve(&cfg, "implementer", &presets).unwrap();
        assert_eq!(d.argv[0], "claude");
    }

    fn claude_with_plugins_file(path: &str) -> Vec<String> {
        let dir = tempfile::tempdir().unwrap();
        let cfg = crate::config::load(dir.path()).unwrap().agent;
        let mut presets = presets();
        let claude = presets.get_mut("claude").unwrap();
        claude.argv = claude
            .argv
            .iter()
            .map(|w| w.replace("~/.claude/settings.json", path))
            .collect();
        resolve(&cfg, "scout", &presets).unwrap().argv
    }

    fn settings_word(argv: &[String]) -> serde_json::Value {
        let i = argv
            .iter()
            .position(|w| w == "--settings")
            .expect("--settings");
        serde_json::from_str(&argv[i + 1]).expect("the settings word is JSON")
    }

    #[test]
    fn a_plugin_the_operator_enables_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(
            &file,
            r#"{"enabledPlugins":{"caveman@caveman":true,"never-seen@shop":true,"harness@inline":true}}"#,
        )
        .unwrap();
        let argv = claude_with_plugins_file(&file.to_string_lossy());
        let plugins = settings_word(&argv)["enabledPlugins"].clone();
        assert_eq!(plugins["caveman@caveman"], serde_json::json!(false));
        // a plugin no preset names still reaches the lane unless the list is read from the operator's own file
        assert_eq!(plugins["never-seen@shop"], serde_json::json!(false));
        // the adapter arrives by --plugin-dir and must survive the override
        assert_eq!(plugins.get("harness@inline"), None);
        assert!(argv.iter().any(|w| w == "--plugin-dir"));
    }

    #[test]
    fn no_plugins_file_leaves_the_settings_json() {
        let argv = claude_with_plugins_file("/nonexistent/settings.json");
        assert_eq!(
            settings_word(&argv)["enabledPlugins"],
            serde_json::json!({})
        );
    }

    #[test]
    fn a_task_level_wins_over_role_then_agent() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::load(dir.path()).unwrap().agent;
        cfg.model = Some("m-agent".into());
        cfg.effort = Some("e-agent".into());
        let presets = presets();
        let bare = resolve_task(&cfg, "scout", &presets, &Levels::default()).unwrap();
        assert_eq!(bare.levels.model.as_deref(), Some("m-agent"));
        assert_eq!(bare.levels.effort.as_deref(), Some("e-agent"));

        cfg.roles.insert(
            "scout".into(),
            crate::config::AgentOverride {
                model: Some("m-role".into()),
                effort: Some("e-role".into()),
                ..crate::config::AgentOverride::default()
            },
        );
        let role = resolve_task(&cfg, "scout", &presets, &Levels::default()).unwrap();
        assert_eq!(role.levels.model.as_deref(), Some("m-role"));
        assert_eq!(role.levels.effort.as_deref(), Some("e-role"));

        let task = Levels {
            model: Some("m-task".into()),
            effort: Some("e-task".into()),
        };
        let top = resolve_task(&cfg, "scout", &presets, &task).unwrap();
        assert_eq!(top.levels.model.as_deref(), Some("m-task"));
        assert_eq!(top.levels.effort.as_deref(), Some("e-task"));

        let empty = crate::config::AgentConfig {
            preset: "claude".into(),
            ..crate::config::AgentConfig::default()
        };
        let none = resolve_task(&empty, "scout", &presets, &Levels::default()).unwrap();
        assert_eq!(none.levels, Levels::default());
        assert!(!none.argv.iter().any(|w| w == "--model" || w == "--effort"));
    }

    #[test]
    fn effort_rides_the_preset_flag_or_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = crate::config::load(dir.path()).unwrap().agent;
        let levels = Levels {
            model: None,
            effort: Some("xhigh".into()),
        };
        let claude = resolve_task(&cfg, "scout", &presets(), &levels).unwrap();
        assert_eq!(
            &claude.argv[claude.argv.len() - 2..],
            &["--effort".to_string(), "xhigh".to_string()]
        );

        let mut without = cfg.clone();
        without.preset = "codex".into();
        let codex = resolve_task(&without, "scout", &presets(), &levels).unwrap();
        assert!(presets()["codex"].effort_flag.is_none());
        assert!(!codex.argv.iter().any(|w| w == "xhigh"));
        assert_eq!(codex.levels.effort.as_deref(), Some("xhigh"));
    }

    #[test]
    fn an_unknown_model_and_effort_reach_argv() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = crate::config::load(dir.path()).unwrap().agent;
        let levels = Levels {
            model: Some("no-such-model".into()),
            effort: Some("ludicrous".into()),
        };
        let r = resolve_task(&cfg, "scout", &presets(), &levels).unwrap();
        assert_eq!(
            &r.argv[r.argv.len() - 4..],
            &[
                "--model".to_string(),
                "no-such-model".to_string(),
                "--effort".to_string(),
                "ludicrous".to_string()
            ]
        );
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
    fn a_preset_without_a_turn_flag_drops_both() {
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
    fn usage_carries_four_disjoint_token_lanes() {
        let out = "{\"type\":\"result\",\"usage\":{\"input_tokens\":22,\"cache_creation_input_tokens\":54825,\"cache_read_input_tokens\":505740,\"output_tokens\":6233}}\n";
        let u = extract_usage(out, &presets()["claude"].usage);
        assert_eq!(u.input_tokens, Some(22));
        assert_eq!(u.cache_creation_input_tokens, Some(54825));
        assert_eq!(u.cache_read_input_tokens, Some(505740));
        assert_eq!(u.output_tokens, Some(6233));
    }

    #[test]
    fn seconds_until_reset_reads_a_clock_time() {
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
        let hour_only = seconds_until_reset(
            "You've hit your session limit \u{b7} resets 2am (Australia/Melbourne)",
            now.clone(),
        )
        .unwrap();
        assert_eq!(hour_only, 16 * 3600 + 60);
        assert!(seconds_until_reset("hit your session limit", now.clone()).is_none());
        assert!(seconds_until_reset("resets 12:40am (Mars/Olympus)", now.clone()).is_none());
        assert!(seconds_until_reset("resets 13am (UTC)", now.clone()).is_none());
        assert!(seconds_until_reset("resets 0am (UTC)", now).is_none());
    }

    #[test]
    fn seconds_until_reset_reads_a_dated_notice() {
        let now = jiff::civil::date(2026, 9, 18)
            .at(17, 57, 25, 0)
            .in_tz("UTC")
            .unwrap();
        // Sep 20 4am in Melbourne is 2026-09-19T18:00:00Z, and the function adds its minute of slack
        let s = seconds_until_reset(
            "You've hit your weekly limit \u{b7} resets Sep 20 at 4am (Australia/Melbourne)",
            now.clone(),
        )
        .unwrap();
        assert_eq!(s, 86555 + 60);
        let with_minutes =
            seconds_until_reset("resets Sep 20 at 4:30am (Australia/Melbourne)", now.clone())
                .unwrap();
        assert_eq!(with_minutes, 86555 + 30 * 60 + 60);
        // a January notice read in December is next year's, not a date 360 days behind
        let over_new_year = seconds_until_reset(
            "resets Jan 2 at 4am (UTC)",
            jiff::civil::date(2026, 12, 28)
                .at(0, 0, 0, 0)
                .in_tz("UTC")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(over_new_year, 5 * 86400 + 4 * 3600 + 60);
        assert!(seconds_until_reset("resets Hax 20 at 4am (UTC)", now.clone()).is_none());
        assert!(seconds_until_reset("resets Feb 30 at 4am (UTC)", now.clone()).is_none());
        assert!(seconds_until_reset("resets Sep 20 at 4am (Mars/Olympus)", now).is_none());
    }

    #[test]
    fn dated_reset_keeps_the_zone() {
        assert_eq!(
            dated_reset(
                "You've hit your weekly limit \u{b7} resets Sep 20 at 4am (Australia/Melbourne)"
            )
            .as_deref(),
            Some("Sep 20 at 4am (Australia/Melbourne)")
        );
        assert!(dated_reset("resets 4am (UTC)").is_none());
    }

    fn fix_now(zoned: jiff::Zoned) {
        FIXED_NOW.with(|n| *n.borrow_mut() = Some(zoned));
    }

    fn stop_file(root: &std::path::Path) -> std::path::PathBuf {
        crate::config::instance_path(root, ".enallagi", "STOP")
    }

    #[test]
    fn a_stop_directory_ends_no_wait() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stop = dir.path().join("STOP");
        std::fs::create_dir(&stop).expect("mkdir");
        assert!(sleep_until(1, &stop).is_ok());
    }

    #[test]
    fn a_stop_file_ends_the_wait_at_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stop = dir.path().join("STOP");
        std::fs::write(&stop, b"").expect("write");
        let started = Instant::now();
        assert!(matches!(sleep_until(60, &stop), Err(AgentError::Stopped)));
        assert!(started.elapsed() < Duration::from_secs(5));
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
    fn a_preset_env_value_takes_the_layout_tokens() {
        let r = crate::fixture::Repo::new();
        let argv = r.stub_agent("echo \"config=$CLAUDE_CONFIG_DIR\"");
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let mut s = spawner(argv, &r.root);
        s.preset.env.insert(
            "CLAUDE_CONFIG_DIR".to_string(),
            "{harness_dir}/run/claude".to_string(),
        );
        let res = spawn(
            &s,
            &mut w,
            &stop_file(&r.root),
            &Regex::new("never").unwrap(),
        )
        .unwrap();
        assert!(
            res.output.contains("config=.enallagi/run/claude"),
            "{}",
            res.output
        );
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
        fix_now(
            jiff::civil::date(2026, 9, 7)
                .at(10, 0, 0, 0)
                .in_tz("Australia/Melbourne")
                .unwrap(),
        );
        let argv =
            r.stub_agent("echo 'hit your session limit resets 10:01am (Australia/Melbourne)'");
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

    #[test]
    fn a_result_line_notice_sleeps_the_stage() {
        let r = crate::fixture::Repo::new();
        std::fs::create_dir_all(stop_file(&r.root).parent().unwrap()).unwrap();
        std::fs::write(stop_file(&r.root), "").unwrap();
        fix_now(
            jiff::civil::date(2026, 9, 7)
                .at(22, 0, 0, 0)
                .in_tz("UTC")
                .unwrap(),
        );
        let line = r#"{"type":"result","subtype":"success","result":"hit your session limit · resets 2am (UTC)"}"#;
        let argv = r.stub_agent(&format!("printf '%s\\n' '{line}'"));
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

    #[test]
    fn an_out_of_range_reset_hour_does_not_sleep() {
        let r = crate::fixture::Repo::new();
        // the stop file and the fixed instant together: an hour read as 1am would be inside MAX_WAIT and sleep
        std::fs::create_dir_all(stop_file(&r.root).parent().unwrap()).unwrap();
        std::fs::write(stop_file(&r.root), "").unwrap();
        fix_now(
            jiff::civil::date(2026, 9, 7)
                .at(22, 0, 0, 0)
                .in_tz("UTC")
                .unwrap(),
        );
        let argv = r.stub_agent("echo 'hit your session limit resets 13am (UTC)'");
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
    fn a_limit_notice_the_agent_quoted_is_not_a_limit() {
        let r = crate::fixture::Repo::new();
        let runs = r.root.join("runs");
        // both shapes seen on 2026-09-16: a file the agent read, and a command the agent wrote
        let read = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"assert!(seconds_until_reset(\"You've hit your session limit · resets 12:40am (Australia/Melbourne)\").is_some());"}]}}"#;
        let wrote = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"printf '%s' \"You've hit your session limit · resets 12:40am (Australia/Melbourne)\" > stub"}}]}}"#;
        // the notice carries an apostrophe, and an unescaped one ends the shell string and hangs the stage on the syntax error it prints
        let argv = r.stub_agent(&format!(
            "echo x >> {}; printf '%s\\n%s\\n' '{}' '{}'",
            runs.display(),
            read.replace('\'', "'\\''"),
            wrote.replace('\'', "'\\''")
        ));
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let res = spawn(
            &spawner(argv, &r.root),
            &mut w,
            &stop_file(&r.root),
            &Regex::new("hit your session limit").unwrap(),
        )
        .unwrap();
        assert_eq!(res.exit, 0);
        assert_eq!(std::fs::read_to_string(&runs).unwrap().lines().count(), 1);
        assert!(
            !w.log
                .read()
                .unwrap()
                .iter()
                .any(|e| matches!(e.kind, Kind::Limit { .. })),
            "a notice the agent read or wrote, rather than reported, must not sleep the stage"
        );
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
    fn a_dated_reset_past_the_ceiling_halts() {
        let r = crate::fixture::Repo::new();
        fix_now(
            jiff::civil::date(2026, 9, 18)
                .at(17, 57, 25, 0)
                .in_tz("UTC")
                .unwrap(),
        );
        let argv = r.stub_agent(
            "echo 'hit your weekly limit resets Sep 20 at 4am (Australia/Melbourne)'; exit 1",
        );
        let mut w = Writer::new(Log::open(&r.root.join(".enallagi")));
        let res = spawn(
            &spawner(argv, &r.root),
            &mut w,
            &stop_file(&r.root),
            &Regex::new("hit your weekly limit").unwrap(),
        )
        .unwrap();
        assert_eq!(
            res.reset_beyond_wait.as_deref(),
            Some("Sep 20 at 4am (Australia/Melbourne)")
        );
        assert_eq!(res.exit, 1);
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
