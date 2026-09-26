//! `enallagi events` -- replays the log through the filters, keeping a matched stage's surrounding events and not the matching lines alone.

use std::path::Path;

use crate::events::{render_line, stage_of, task_of, Event, Kind, Log};

pub struct Args {
    pub role: Option<String>,
    pub task: Option<String>,
    pub since: Option<String>,
    pub json: bool,
    pub summary: bool,
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

    if args.summary {
        let rows = summarize(pairs.iter().map(|(_, e)| e));
        print_summary(&rows, args.json);
        return Ok(0);
    }

    for (raw, e) in &pairs {
        if args.json {
            println!("{raw}");
        } else {
            println!("{}", render_line(e));
        }
    }

    Ok(0)
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct TaskRow {
    pub task: String,
    pub stages: u64,
    pub verify_rounds: u64,
    pub done_verdicts: u64,
    pub rejections: u64,
    pub overturns: u64,
    pub seconds: u64,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

impl TaskRow {
    fn add(&mut self, o: &TaskRow) {
        self.stages += o.stages;
        self.verify_rounds += o.verify_rounds;
        self.done_verdicts += o.done_verdicts;
        self.rejections += o.rejections;
        self.overturns += o.overturns;
        self.seconds += o.seconds;
        self.cost += o.cost;
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cache_creation_input_tokens += o.cache_creation_input_tokens;
        self.cache_read_input_tokens += o.cache_read_input_tokens;
    }
}

pub fn summarize<'a>(events: impl Iterator<Item = &'a Event>) -> Vec<TaskRow> {
    let mut rows: std::collections::BTreeMap<String, TaskRow> = Default::default();
    for e in events {
        let Some(task) = task_of(&e.kind).filter(|t| !t.is_empty()) else {
            continue;
        };
        let row = rows.entry(task.to_string()).or_insert_with(|| TaskRow {
            task: task.to_string(),
            ..Default::default()
        });
        match &e.kind {
            Kind::StageEnd {
                stage,
                seconds,
                cost,
                input_tokens,
                output_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                ..
            } => {
                row.stages += 1;
                row.verify_rounds += u64::from(stage == "verify");
                row.seconds += seconds;
                row.cost += cost.unwrap_or(0.0);
                row.input_tokens += input_tokens.unwrap_or(0);
                row.output_tokens += output_tokens.unwrap_or(0);
                row.cache_creation_input_tokens += cache_creation_input_tokens.unwrap_or(0);
                row.cache_read_input_tokens += cache_read_input_tokens.unwrap_or(0);
            }
            // the verdict gate passes a not-done verdict without running the check, so only it lacks a tally
            Kind::Gate {
                gate, pass, tally, ..
            } if gate == "verdict" => {
                if *pass && tally.is_none() {
                    row.rejections += 1;
                } else {
                    row.done_verdicts += 1;
                }
            }
            Kind::TaskStatus { from, .. } if from == "done" => row.overturns += 1,
            _ => {}
        }
    }
    rows.into_values().collect()
}

fn print_summary(rows: &[TaskRow], json: bool) {
    let mut total = TaskRow {
        task: "total".into(),
        ..Default::default()
    };
    for row in rows {
        total.add(row);
    }
    let rate =
        (total.done_verdicts > 0).then(|| total.overturns as f64 / total.done_verdicts as f64);
    if json {
        for row in rows {
            println!("{}", serde_json::json!(row));
        }
        let mut footer = serde_json::json!(total);
        footer["false_completion_rate"] = serde_json::json!(rate);
        println!("{footer}");
        return;
    }
    println!(
        "{:<10} {:>6} {:>6} {:>6} {:>8} {:>10} {:>8} {:>8} {:>10} {:>10} {:>12} {:>12}",
        "task",
        "stages",
        "verify",
        "done",
        "rejected",
        "overturned",
        "seconds",
        "cost",
        "input",
        "output",
        "cache_create",
        "cache_read"
    );
    for row in rows.iter().chain(std::iter::once(&total)) {
        println!(
            "{:<10} {:>6} {:>6} {:>6} {:>8} {:>10} {:>8} {:>8.2} {:>10} {:>10} {:>12} {:>12}",
            row.task,
            row.stages,
            row.verify_rounds,
            row.done_verdicts,
            row.rejections,
            row.overturns,
            row.seconds,
            row.cost,
            row.input_tokens,
            row.output_tokens,
            row.cache_creation_input_tokens,
            row.cache_read_input_tokens
        );
    }
    match rate {
        Some(r) => println!(
            "false-completion rate: {}/{} = {:.3}",
            total.overturns, total.done_verdicts, r
        ),
        None => println!("false-completion rate: no done verdicts"),
    }
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
            sha: None,
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
