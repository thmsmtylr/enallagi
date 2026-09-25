//! `enallagi events` -- replays the log through the filters, keeping a matched stage's surrounding events and not the matching lines alone.

use std::path::Path;

use crate::events::{render_line, stage_of, task_of, Event, Kind, Log};

pub struct Args {
    pub role: Option<String>,
    pub task: Option<String>,
    pub since: Option<String>,
    pub json: bool,
}

// --role isn't here: it needs the whole stream (a stage's surrounding events, not just matches), so it's a separate pass in run
pub struct Filter {
    pub task: Option<String>,
    pub since: Option<String>,
}

pub(crate) fn keep(e: &Event, opts: &Filter) -> bool {
    if let Some(task) = &opts.task {
        if task_of(&e.kind) != Some(task.as_str()) {
            return false;
        }
    }
    if let Some(since) = &opts.since {
        if e.ts.as_str() < since.as_str() {
            return false;
        }
    }
    true
}

pub fn run(args: &Args) -> anyhow::Result<i32> {
    let log = Log::open(Path::new(
        &crate::config::load(Path::new("."))?.layout.harness_dir,
    ));
    let (pairs, _skipped) = match log.read_lines() {
        Ok(pairs) => pairs,
        Err(err) => {
            eprintln!("harness: events: {err}");
            return Ok(2);
        }
    };

    let mut pairs = pairs;
    if let Some(role) = &args.role {
        pairs = filter_role(pairs, role);
    }
    let filter = Filter {
        task: args.task.clone(),
        since: args.since.clone(),
    };
    pairs.retain(|(_, e)| keep(e, &filter));

    for (raw, e) in &pairs {
        if args.json {
            println!("{raw}");
        } else {
            println!("{}", render_line(e));
        }
    }

    Ok(0)
}

fn filter_role(items: Vec<(String, Event)>, role: &str) -> Vec<(String, Event)> {
    let mut active: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (raw, e) in items {
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
            out.push((raw, e));
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
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            turns: None,
            turn_cap: None,
        });
        let (pairs, _) = w.log.read_lines().unwrap();
        let kept = filter_role(pairs, "implementer");
        assert_eq!(kept.len(), 3);
        assert!(kept
            .iter()
            .all(|(_, e)| stage_of(&e.kind) != Some("verify")));
    }

    #[test]
    fn task_filter_matches_task_field() {
        let mut w = writer();
        w.emit(Kind::Gate {
            gate: "scope".into(),
            task: "T-1".into(),
            pass: true,
            reason: "ok".into(),
            tally: None,
        });
        w.emit(Kind::Gate {
            gate: "scope".into(),
            task: "T-2".into(),
            pass: true,
            reason: "ok".into(),
            tally: None,
        });
        let all = w.log.read().unwrap();
        let filter = Filter {
            task: Some("T-1".into()),
            since: None,
        };
        let kept: Vec<_> = all.into_iter().filter(|e| keep(e, &filter)).collect();
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn since_filter_keeps_ts_on_or_after() {
        let event_at = |ts: &str, reason: &str| Event {
            ts: ts.to_string(),
            run: "r".into(),
            iter: 0,
            seq: 1,
            kind: Kind::Halt {
                halt: "x".into(),
                reason: reason.to_string(),
            },
        };
        let early = event_at("2026-09-07T00:00:00Z", "early");
        let late = event_at("2026-09-07T00:00:01Z", "late");
        let filter = Filter {
            task: None,
            since: Some("2026-09-07T00:00:01Z".into()),
        };
        assert!(!keep(&early, &filter), "before `since` should be dropped");
        assert!(keep(&late, &filter), "exactly `since` should be kept");
    }
}
