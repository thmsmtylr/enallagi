//! events: the append-only JSONL event log written during a run and read
//! back by `harness watch`, `harness events`, and the probes.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: String,
    pub run: String,
    pub iter: u32,
    pub seq: u64,
    #[serde(flatten)]
    pub kind: Kind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Kind {
    #[serde(rename = "run.start")]
    RunStart {
        config_sha256: String,
        pipeline: Option<String>,
    },
    #[serde(rename = "run.end")]
    RunEnd {
        halts: Vec<String>,
        landed: Vec<String>,
        promoted: Vec<String>,
        killed: Vec<String>,
        warnings: Vec<String>,
    },
    #[serde(rename = "stage.start")]
    StageStart {
        stage: String,
        role: Option<String>,
        command: Option<String>,
        task: Option<String>,
    },
    #[serde(rename = "stage.output")]
    StageOutput { stage: String, chunk: String },
    #[serde(rename = "stage.end")]
    StageEnd {
        stage: String,
        task: Option<String>,
        seconds: u64,
        exit: i32,
        cost: Option<f64>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        turns: Option<u64>,
    },
    #[serde(rename = "gate")]
    Gate {
        gate: String,
        task: String,
        pass: bool,
        reason: String,
    },
    #[serde(rename = "task.status")]
    TaskStatus {
        task: String,
        from: String,
        to: String,
        reason: String,
        by: String,
    },
    #[serde(rename = "halt")]
    Halt { halt: String, reason: String },
    #[serde(rename = "limit")]
    Limit {
        stage: String,
        matched: String,
        sleep_seconds: u64,
        attempt: u32,
    },
    #[serde(rename = "skill.resolved")]
    SkillResolved {
        id: String,
        commit: Option<String>,
        result: String,
    },
    #[serde(rename = "probe")]
    Probe {
        name: String,
        count: Option<u64>,
        error: Option<String>,
    },
}

/// A stage-scoped field shared by several `Kind` variants, used by the CLI's
/// `--role` filter to track which events belong to a running stage.
pub fn stage_of(k: &Kind) -> Option<&str> {
    match k {
        Kind::StageStart { stage, .. }
        | Kind::StageOutput { stage, .. }
        | Kind::StageEnd { stage, .. }
        | Kind::Limit { stage, .. } => Some(stage),
        _ => None,
    }
}

/// The task a `Kind` variant is about, used by the CLI's `--task` filter.
pub fn task_of(k: &Kind) -> Option<&str> {
    match k {
        Kind::StageStart { task, .. } | Kind::StageEnd { task, .. } => task.as_deref(),
        Kind::Gate { task, .. } | Kind::TaskStatus { task, .. } => Some(task.as_str()),
        _ => None,
    }
}

pub struct Log {
    pub path: PathBuf,
}

impl Log {
    /// `<harness_dir>/events.jsonl`. Parent directories are created on the
    /// first `append`, not here.
    pub fn open(harness_dir: &Path) -> Log {
        Log {
            path: harness_dir.join("events.jsonl"),
        }
    }

    pub fn append(&self, e: &Event) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let line = serde_json::to_string(e)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{line}")
    }

    /// Parses every line, pairing each with the exact text it came from (so
    /// `--json` can echo the file's own line rather than a re-serialization
    /// of the parsed value) and reporting the ones that failed to parse
    /// rather than failing the whole read. A missing file reads as empty.
    pub fn read_lines(&self) -> io::Result<(Vec<(String, Event)>, usize)> {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
            Err(err) => return Err(err),
        };
        let mut pairs = Vec::new();
        let mut skipped = 0usize;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Event>(line) {
                Ok(e) => pairs.push((line.to_string(), e)),
                Err(_) => skipped += 1,
            }
        }
        Ok((pairs, skipped))
    }

    /// Like `read_lines`, but without the raw text alongside each event.
    pub fn read_report(&self) -> io::Result<(Vec<Event>, usize)> {
        let (pairs, skipped) = self.read_lines()?;
        Ok((pairs.into_iter().map(|(_, e)| e).collect(), skipped))
    }

    pub fn read(&self) -> io::Result<Vec<Event>> {
        self.read_report().map(|(events, _)| events)
    }

    /// Events with `seq` strictly greater than `seq`.
    pub fn read_since(&self, seq: u64) -> io::Result<Vec<Event>> {
        Ok(self.read()?.into_iter().filter(|e| e.seq > seq).collect())
    }
}

/// A live listener on the writer. It is called from inside `emit`, so a stage's
/// output reaches it while the agent is still running rather than after it.
pub type Sink = Box<dyn FnMut(&Event) + Send>;

pub struct Writer {
    pub log: Log,
    pub run: String,
    pub iter: u32,
    seq: u64,
    last_error: Option<String>,
    sink: Option<Sink>,
}

impl Writer {
    pub fn new(log: Log) -> Writer {
        let now = jiff::Timestamp::now();
        let ts = rfc3339_secs(now);
        let nanos = now.subsec_nanosecond() as u32;
        let hex = (std::process::id() ^ nanos) & 0xffff;
        Writer {
            log,
            run: format!("{ts}-{hex:04x}"),
            iter: 0,
            seq: 0,
            last_error: None,
            sink: None,
        }
    }

    /// Everything this writer emits from here on also goes to `sink`, in order,
    /// as it happens. One listener: the run has one caller.
    pub fn set_sink(&mut self, sink: Sink) {
        self.sink = Some(sink);
    }

    pub fn emit(&mut self, kind: Kind) -> Event {
        self.seq += 1;
        let event = Event {
            ts: rfc3339_secs(jiff::Timestamp::now()),
            run: self.run.clone(),
            iter: self.iter,
            seq: self.seq,
            kind,
        };
        // Best-effort: a write failure here has nowhere to report to since
        // `emit` returns the event, not a Result. The caller still gets the
        // in-memory `Event`; the failure is recorded in `last_error` for a
        // caller that wants to notice.
        self.last_error = self.log.append(&event).err().map(|err| err.to_string());
        if let Some(sink) = &mut self.sink {
            sink(&event);
        }
        event
    }

    /// The error from the most recent `emit`'s append, if it failed. `None`
    /// once a later `emit` succeeds.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn set_iter(&mut self, i: u32) {
        self.iter = i;
    }
}

fn rfc3339_secs(ts: jiff::Timestamp) -> String {
    ts.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Renders one event as `ts kind key=value ...` for `--no-tui` and
/// `harness events`. Fields are taken from the event's own JSON
/// serialization, in that object's key order, with `ts` and `kind` pulled
/// out to the front. A `null` value is omitted; a string containing
/// whitespace is quoted; arrays are joined with `,`.
pub fn render_line(e: &Event) -> String {
    let value = serde_json::to_value(e).unwrap_or(serde_json::Value::Null);
    let Some(obj) = value.as_object() else {
        return String::new();
    };
    let ts = obj.get("ts").and_then(|v| v.as_str()).unwrap_or("");
    let kind = obj.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let mut parts = vec![ts.to_string(), kind.to_string()];
    for (k, v) in obj {
        if k == "ts" || k == "kind" {
            continue;
        }
        if let Some(rendered) = render_value(v) {
            parts.push(format!("{k}={rendered}"));
        }
    }
    parts.join(" ")
}

fn render_value(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(if s.chars().any(char::is_whitespace) {
            format!("\"{s}\"")
        } else {
            s.clone()
        }),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(render_value).collect();
            Some(parts.join(","))
        }
        serde_json::Value::Object(_) => Some(v.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_then_read_round_trips_and_seq_is_monotonic() {
        let d = tempfile::tempdir().unwrap();
        let mut w = Writer::new(Log::open(d.path()));
        w.emit(Kind::RunStart {
            config_sha256: "a".into(),
            pipeline: None,
        });
        w.set_iter(1);
        w.emit(Kind::Halt {
            halt: "stop".into(),
            reason: "STOP".into(),
        });
        let all = w.log.read().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].seq, 2);
        assert_eq!(all[1].iter, 1);
        assert!(matches!(all[1].kind, Kind::Halt { .. }));
    }

    #[test]
    fn kind_serialises_with_dotted_names() {
        let mut w = Writer::new(Log::open(tempfile::tempdir().unwrap().path()));
        let e = w.emit(Kind::StageEnd {
            stage: "implement".into(),
            task: Some("T-1".into()),
            seconds: 3,
            exit: 0,
            cost: Some(0.5),
            input_tokens: None,
            output_tokens: None,
            turns: None,
        });
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("\"kind\":\"stage.end\""));
        assert!(j.contains("\"cost\":0.5"));
    }

    #[test]
    fn read_since_skips_earlier_seq() {
        let mut w = Writer::new(Log::open(tempfile::tempdir().unwrap().path()));
        for _ in 0..3 {
            w.emit(Kind::Halt {
                halt: "x".into(),
                reason: "y".into(),
            });
        }
        assert_eq!(w.log.read_since(2).unwrap().len(), 1);
    }

    #[test]
    fn render_line_is_one_line() {
        let mut w = Writer::new(Log::open(tempfile::tempdir().unwrap().path()));
        let e = w.emit(Kind::Gate {
            gate: "scope".into(),
            task: "T-1".into(),
            pass: false,
            reason: "touched x".into(),
        });
        let l = render_line(&e);
        assert!(l.contains("gate "));
        assert!(l.contains("pass=false"));
        assert!(!l.contains('\n'));
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let d = tempfile::tempdir().unwrap();
        let log = Log::open(d.path());
        let (events, skipped) = log.read_report().unwrap();
        assert!(events.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn unparseable_lines_are_counted_not_fatal() {
        let d = tempfile::tempdir().unwrap();
        let mut w = Writer::new(Log::open(d.path()));
        w.emit(Kind::Halt {
            halt: "x".into(),
            reason: "y".into(),
        });
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&w.log.path)
            .unwrap();
        writeln!(f, "not json").unwrap();
        let (events, skipped) = w.log.read_report().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn render_line_quotes_whitespace_and_omits_null() {
        let mut w = Writer::new(Log::open(tempfile::tempdir().unwrap().path()));
        let e = w.emit(Kind::StageStart {
            stage: "implement".into(),
            role: None,
            command: Some("do the thing".into()),
            task: Some("T-1".into()),
        });
        let l = render_line(&e);
        assert!(l.contains("command=\"do the thing\""));
        assert!(!l.contains("role="));
    }

    #[test]
    fn emit_records_append_failure_in_last_error() {
        let d = tempfile::tempdir().unwrap();
        let blocker = d.path().join("blocker");
        fs::write(&blocker, "not a directory").unwrap();
        // `Log::open` joins "events.jsonl" onto this, so `append` will try to
        // `create_dir_all` a path that already exists as a plain file.
        let mut w = Writer::new(Log::open(&blocker));
        assert!(w.last_error().is_none());
        w.emit(Kind::Halt {
            halt: "x".into(),
            reason: "y".into(),
        });
        assert!(w.last_error().is_some());
    }

    #[test]
    fn read_lines_pairs_the_raw_line_with_its_parsed_event() {
        let d = tempfile::tempdir().unwrap();
        let mut w = Writer::new(Log::open(d.path()));
        w.emit(Kind::Halt {
            halt: "x".into(),
            reason: "y".into(),
        });
        let (pairs, skipped) = w.log.read_lines().unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, serde_json::to_string(&pairs[0].1).unwrap());
    }

    #[test]
    fn run_id_is_ts_dash_four_hex() {
        let w = Writer::new(Log::open(tempfile::tempdir().unwrap().path()));
        let (ts, hex) = w.run.rsplit_once('-').unwrap();
        assert!(ts.ends_with('Z'));
        assert_eq!(hex.len(), 4);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }
}
