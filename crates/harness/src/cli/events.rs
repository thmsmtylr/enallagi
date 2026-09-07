use std::path::Path;

use crate::events::{render_line, stage_of, task_of, Event, Kind, Log};

pub struct Args {
    pub role: Option<String>,
    pub task: Option<String>,
    pub since: Option<String>,
    pub json: bool,
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let log = Log::open(Path::new(".harness"));
    let events = match log.read() {
        Ok(events) => events,
        Err(err) => {
            eprintln!("harness: events: {err}");
            return Ok(2);
        }
    };

    let mut events = events;
    if let Some(role) = &args.role {
        events = filter_role(events, role);
    }
    if let Some(task) = &args.task {
        events.retain(|e| task_of(&e.kind) == Some(task.as_str()));
    }
    if let Some(since) = &args.since {
        events.retain(|e| e.ts.as_str() >= since.as_str());
    }

    for e in &events {
        if args.json {
            println!("{}", serde_json::to_string(e)?);
        } else {
            println!("{}", render_line(e));
        }
    }

    Ok(0)
}

/// Keeps `stage.start` events whose role matches, plus every event of that
/// stage (by stage name, scoped to the run it started in) up to and
/// including its `stage.end`.
fn filter_role(events: Vec<Event>, role: &str) -> Vec<Event> {
    let mut active: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in events {
        let starts_here =
            matches!(&e.kind, Kind::StageStart { role: r, .. } if r.as_deref() == Some(role));
        if starts_here {
            if let Kind::StageStart { stage, .. } = &e.kind {
                active.insert((e.run.clone(), stage.clone()));
            }
        }
        let in_active_stage = stage_of(&e.kind)
            .map(|stage| active.contains(&(e.run.clone(), stage.to_string())))
            .unwrap_or(false);
        let is_end = matches!(&e.kind, Kind::StageEnd { .. });

        if starts_here || in_active_stage {
            let end_stage = if is_end {
                match &e.kind {
                    Kind::StageEnd { stage, .. } => Some((e.run.clone(), stage.clone())),
                    _ => None,
                }
            } else {
                None
            };
            out.push(e);
            if let Some(key) = end_stage {
                active.remove(&key);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Writer;

    fn writer() -> Writer {
        Writer::new(Log::open(tempfile::tempdir().unwrap().path()))
    }

    #[test]
    fn role_filter_keeps_the_whole_stage() {
        let mut w = writer();
        w.emit(Kind::StageStart {
            stage: "implement".into(),
            role: Some("implementer".into()),
            command: None,
            task: Some("T-1".into()),
        });
        w.emit(Kind::StageOutput {
            stage: "implement".into(),
            chunk: "hi".into(),
        });
        w.emit(Kind::StageStart {
            stage: "verify".into(),
            role: Some("verifier".into()),
            command: None,
            task: Some("T-1".into()),
        });
        w.emit(Kind::StageEnd {
            stage: "implement".into(),
            task: Some("T-1".into()),
            seconds: 1,
            exit: 0,
            cost: None,
            input_tokens: None,
            output_tokens: None,
            turns: None,
        });
        let all = w.log.read().unwrap();
        let kept = filter_role(all, "implementer");
        assert_eq!(kept.len(), 3);
        assert!(kept.iter().all(|e| stage_of(&e.kind) != Some("verify")));
    }

    #[test]
    fn task_filter_matches_task_field() {
        let mut w = writer();
        w.emit(Kind::Gate {
            gate: "scope".into(),
            task: "T-1".into(),
            pass: true,
            reason: "ok".into(),
        });
        w.emit(Kind::Gate {
            gate: "scope".into(),
            task: "T-2".into(),
            pass: true,
            reason: "ok".into(),
        });
        let all = w.log.read().unwrap();
        let kept: Vec<_> = all
            .into_iter()
            .filter(|e| task_of(&e.kind) == Some("T-1"))
            .collect();
        assert_eq!(kept.len(), 1);
    }
}
