//! Five probes that read the event log and report findings.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::config::Config;
use crate::events::{Event, Kind, Log};

pub struct Finding {
    pub path: String,
    pub line: usize,
    pub message: String,
}

pub enum ProbeResult {
    Count(Vec<Finding>),
    Error(String),
}

fn load(log: &Log) -> Result<Vec<Event>, String> {
    match log.read_report() {
        Ok((events, _skipped)) if !events.is_empty() => Ok(events),
        _ => Err(no_events(log)),
    }
}

fn no_events(log: &Log) -> String {
    let dir = log
        .path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    format!("no events.jsonl under {dir}")
}

// idxs double as 1-based line numbers: true only while the log has no blank/unparseable lines ahead, which a run's own writer never emits
fn cite(log: &Log, events: &[Event], idxs: &[usize], message: &str) -> Finding {
    let run = &events[idxs[0]].run;
    let seqs: Vec<String> = idxs.iter().map(|&i| events[i].seq.to_string()).collect();
    Finding {
        path: log.path.to_string_lossy().into_owned(),
        line: idxs[0] + 1,
        message: format!("{message} (events {run}#{})", seqs.join(",")),
    }
}

fn normal(text: &str) -> String {
    let cleaned: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tokens(text: &str) -> HashSet<String> {
    normal(text)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f64 / union as f64
}

const FRICTION_OVERLAP: f64 = 0.5;

fn median_u64(values: &[u64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2] as f64
    } else {
        (v[n / 2 - 1] as f64 + v[n / 2] as f64) / 2.0
    }
}

fn median_f64(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

pub fn verdict_flip(log: &Log) -> ProbeResult {
    let events = match load(log) {
        Ok(e) => e,
        Err(msg) => return ProbeResult::Error(msg),
    };
    let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, e) in events.iter().enumerate() {
        if let Kind::TaskStatus {
            task, from, to, by, ..
        } = &e.kind
        {
            if from == "done" && to == "ready" && by == "verdict" {
                groups
                    .entry((e.run.clone(), task.clone()))
                    .or_default()
                    .push(i);
            }
        }
    }
    let mut keys: Vec<_> = groups.keys().cloned().collect();
    keys.sort();
    let mut findings = Vec::new();
    for key in keys {
        let idxs = &groups[&key];
        if idxs.len() >= 2 {
            findings.push(cite(
                log,
                &events,
                idxs,
                &format!(
                    "task {} was flipped done->ready by verdict {} times in one run",
                    key.1,
                    idxs.len()
                ),
            ));
        }
    }
    ProbeResult::Count(findings)
}

pub fn rejection_repeat(log: &Log) -> ProbeResult {
    let events = match load(log) {
        Ok(e) => e,
        Err(msg) => return ProbeResult::Error(msg),
    };
    // (first reason's tokens, event indices, tasks named, first reason's text)
    type Group = (HashSet<String>, Vec<usize>, Vec<String>, String);
    let mut groups: Vec<Group> = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if let Kind::TaskStatus {
            task,
            from,
            to,
            reason,
            ..
        } = &e.kind
        {
            if from == "review" && to == "ready" {
                let key = tokens(reason);
                if key.is_empty() {
                    continue;
                }
                match groups
                    .iter_mut()
                    .find(|(first, ..)| jaccard(first, &key) >= FRICTION_OVERLAP)
                {
                    Some((_, idxs, tasks, _)) => {
                        idxs.push(i);
                        tasks.push(task.clone());
                    }
                    None => groups.push((key, vec![i], vec![task.clone()], reason.clone())),
                }
            }
        }
    }
    let mut findings = Vec::new();
    for (_, idxs, tasks, reason) in groups {
        if idxs.len() >= 2 {
            findings.push(cite(
                log,
                &events,
                &idxs,
                &format!(
                    "rejection reason repeats across tasks {}: {}",
                    tasks.join(","),
                    reason
                ),
            ));
        }
    }
    ProbeResult::Count(findings)
}

struct StageEndInfo {
    idx: usize,
    stage: String,
    role_key: String,
    seconds: u64,
    cost: Option<f64>,
}

pub fn stage_outlier(log: &Log) -> ProbeResult {
    let events = match load(log) {
        Ok(e) => e,
        Err(msg) => return ProbeResult::Error(msg),
    };
    let mut role_by_run_stage: HashMap<(String, String), Option<String>> = HashMap::new();
    let mut ends: Vec<StageEndInfo> = Vec::new();
    for (i, e) in events.iter().enumerate() {
        match &e.kind {
            Kind::StageStart { stage, role, .. } => {
                role_by_run_stage.insert((e.run.clone(), stage.clone()), role.clone());
            }
            Kind::StageEnd {
                stage,
                seconds,
                cost,
                ..
            } => {
                let role = role_by_run_stage
                    .get(&(e.run.clone(), stage.clone()))
                    .cloned()
                    .flatten();
                ends.push(StageEndInfo {
                    idx: i,
                    stage: stage.clone(),
                    role_key: role.unwrap_or_else(|| stage.clone()),
                    seconds: *seconds,
                    cost: *cost,
                });
            }
            _ => {}
        }
    }
    let mut groups: HashMap<String, Vec<&StageEndInfo>> = HashMap::new();
    for end in &ends {
        groups.entry(end.role_key.clone()).or_default().push(end);
    }
    let mut keys: Vec<_> = groups.keys().cloned().collect();
    keys.sort();
    let mut findings = Vec::new();
    for key in keys {
        let members = &groups[&key];
        if members.len() < 3 {
            continue;
        }
        let secs: Vec<u64> = members.iter().map(|m| m.seconds).collect();
        let med_secs = median_u64(&secs);
        for m in members.iter() {
            if (m.seconds as f64) > 2.0 * med_secs {
                findings.push(cite(
                    log,
                    &events,
                    &[m.idx],
                    &format!(
                        "stage {} took {}s, over twice the {} median {:.1}s",
                        m.stage, m.seconds, key, med_secs
                    ),
                ));
            }
        }
        let costs: Vec<f64> = members.iter().filter_map(|m| m.cost).collect();
        if costs.len() >= 3 {
            let med_cost = median_f64(&costs);
            for m in members.iter() {
                if let Some(c) = m.cost {
                    if c > 2.0 * med_cost {
                        findings.push(cite(
                            log,
                            &events,
                            &[m.idx],
                            &format!(
                                "stage {} cost {:.2}, over twice the {} median {:.2}",
                                m.stage, c, key, med_cost
                            ),
                        ));
                    }
                }
            }
        }
    }
    ProbeResult::Count(findings)
}

pub fn turns_exhausted(log: &Log, cfg: &Config) -> ProbeResult {
    let events = match load(log) {
        Ok(e) => e,
        Err(msg) => return ProbeResult::Error(msg),
    };
    let mut findings = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if let Kind::StageEnd {
            stage,
            turns: Some(t),
            ..
        } = &e.kind
        {
            if let Some(s) = cfg.stage.iter().find(|s| &s.name == stage) {
                if u64::from(s.turns) == *t {
                    findings.push(cite(
                        log,
                        &events,
                        &[i],
                        &format!("stage {stage} exhausted its turn cap of {t}"),
                    ));
                }
            }
        }
    }
    ProbeResult::Count(findings)
}

fn stage_of_limit(k: &Kind) -> Option<&str> {
    match k {
        Kind::Limit { stage, .. } => Some(stage),
        _ => None,
    }
}

pub fn limit_repeat(log: &Log) -> ProbeResult {
    let events = match load(log) {
        Ok(e) => e,
        Err(msg) => return ProbeResult::Error(msg),
    };
    let mut starts_by_run: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in events.iter().enumerate() {
        if matches!(e.kind, Kind::StageStart { .. }) {
            starts_by_run.entry(e.run.clone()).or_default().push(i);
        }
    }
    let mut last_limit: HashMap<String, (usize, usize)> = HashMap::new(); // run -> (start position, event idx)
    let mut findings = Vec::new();
    for (i, e) in events.iter().enumerate() {
        let Some(stage) = stage_of_limit(&e.kind) else {
            continue;
        };
        let starts = match starts_by_run.get(&e.run) {
            Some(s) => s,
            None => continue,
        };
        let Some(pos) = starts.iter().rposition(|&si| si <= i) else {
            continue;
        };
        if let Some(&(prev_pos, prev_idx)) = last_limit.get(&e.run) {
            if pos == prev_pos + 1 {
                let prev_stage = stage_of_limit(&events[prev_idx].kind).unwrap_or_default();
                findings.push(cite(
                    log,
                    &events,
                    &[prev_idx, i],
                    &format!("limit repeats across consecutive stages {prev_stage} then {stage}"),
                ));
            }
        }
        last_limit.insert(e.run.clone(), (pos, i));
    }
    ProbeResult::Count(findings)
}

pub fn all(log: &Log, cfg: &Config) -> Vec<(&'static str, ProbeResult)> {
    vec![
        ("verdict-flip", verdict_flip(log)),
        ("rejection-repeat", rejection_repeat(log)),
        ("stage-outlier", stage_outlier(log)),
        ("turns-exhausted", turns_exhausted(log, cfg)),
        ("limit-repeat", limit_repeat(log)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Stage;
    use crate::events::{Log, Writer};

    fn writer() -> (tempfile::TempDir, Writer) {
        let dir = tempfile::tempdir().expect("tempdir");
        let w = Writer::new(Log::open(dir.path()));
        (dir, w)
    }

    fn status(task: &str, from: &str, to: &str, reason: &str, by: &str) -> Kind {
        Kind::TaskStatus {
            task: task.into(),
            from: from.into(),
            to: to.into(),
            reason: reason.into(),
            by: by.into(),
        }
    }

    fn start(stage: &str) -> Kind {
        Kind::StageStart {
            stage: stage.into(),
            role: None,
            command: None,
            task: None,
        }
    }

    fn end(stage: &str, seconds: u64, cost: Option<f64>, turns: Option<u64>) -> Kind {
        Kind::StageEnd {
            stage: stage.into(),
            task: None,
            seconds,
            exit: 0,
            cost,
            input_tokens: None,
            output_tokens: None,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            turns,
        }
    }

    #[test]
    fn verdict_flip_fires_on_two_forced_backs_in_one_run() {
        let (dir, mut w) = writer();
        w.emit(status("T-1", "done", "ready", "forced back", "verdict"));
        w.emit(status(
            "T-1",
            "done",
            "ready",
            "forced back again",
            "verdict",
        ));

        let log = Log::open(dir.path());
        match verdict_flip(&log) {
            ProbeResult::Count(findings) => {
                assert_eq!(findings.len(), 1);
                assert!(findings[0].message.contains("T-1"));
                assert!(findings[0].message.contains("#1,2"));
                assert_eq!(findings[0].line, 1);
            }
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn verdict_flip_needs_two_flips_on_the_same_task() {
        let (dir, mut w) = writer();
        w.emit(status("T-1", "done", "ready", "forced back", "verdict"));
        let log = Log::open(dir.path());
        match verdict_flip(&log) {
            ProbeResult::Count(findings) => assert_eq!(findings.len(), 0),
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }

        let (dir2, mut w2) = writer();
        w2.emit(status("T-1", "done", "ready", "forced back", "verdict"));
        w2.emit(status("T-2", "done", "ready", "forced back", "verdict"));
        let log2 = Log::open(dir2.path());
        match verdict_flip(&log2) {
            ProbeResult::Count(findings) => assert_eq!(findings.len(), 0),
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn rejection_repeat_uses_the_friction_measure() {
        let (dir, mut w) = writer();
        w.emit(status(
            "T-1",
            "review",
            "ready",
            "REJECTED: test weakened in journal.test.ts",
            "adjudicator",
        ));
        w.emit(status(
            "T-2",
            "review",
            "ready",
            "REJECTED: weakened test journal.test.ts",
            "adjudicator",
        ));
        w.emit(status(
            "T-3",
            "review",
            "ready",
            "REJECTED: out of scope",
            "adjudicator",
        ));

        let log = Log::open(dir.path());
        match rejection_repeat(&log) {
            ProbeResult::Count(findings) => {
                assert_eq!(findings.len(), 1);
                assert!(findings[0].message.contains("T-1"));
                assert!(findings[0].message.contains("T-2"));
                assert!(!findings[0].message.contains("T-3"));
            }
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn rejection_repeat_fires_at_the_boundary() {
        let (dir, mut w) = writer();
        w.emit(status(
            "T-1",
            "review",
            "ready",
            "SECOND occurrence of a probe firing on the prose that documents it \
             the first was this",
            "adjudicator",
        ));
        w.emit(status(
            "T-2",
            "review",
            "ready",
            "FIFTH sighting of a check firing on the prose that documents it \
             and the first where the",
            "adjudicator",
        ));

        let log = Log::open(dir.path());
        match rejection_repeat(&log) {
            ProbeResult::Count(findings) => assert_eq!(findings.len(), 1),
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn rejection_repeat_holds_under_the_boundary() {
        let (dir, mut w) = writer();
        w.emit(status(
            "T-1",
            "review",
            "ready",
            "none new. One thing worth the next lane's time, not a rule: \
             harness hooks probes.sh",
            "adjudicator",
        ));
        w.emit(status(
            "T-2",
            "review",
            "ready",
            "none new. One thing worth the next lane's time: the selftest \
             assertion deliberately does",
            "adjudicator",
        ));

        let log = Log::open(dir.path());
        match rejection_repeat(&log) {
            ProbeResult::Count(findings) => assert_eq!(findings.len(), 0),
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn stage_outlier_is_over_twice_the_role_median() {
        let (dir, mut w) = writer();
        for seconds in [100, 110, 120, 300] {
            w.emit(start("implement"));
            w.emit(end("implement", seconds, None, None));
        }

        let log = Log::open(dir.path());
        match stage_outlier(&log) {
            ProbeResult::Count(findings) => {
                assert_eq!(findings.len(), 1);
                assert!(findings[0].message.contains("300"));
            }
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn stage_outlier_flags_cost_independently_of_seconds() {
        let (dir, mut w) = writer();
        for cost in [1.0, 1.1, 1.2, 5.0] {
            w.emit(start("implement"));
            // seconds flat: no seconds-outlier should fire alongside this.
            w.emit(end("implement", 100, Some(cost), None));
        }

        let log = Log::open(dir.path());
        match stage_outlier(&log) {
            ProbeResult::Count(findings) => {
                assert_eq!(findings.len(), 1);
                assert!(findings[0].message.contains("cost"));
                assert!(findings[0].message.contains('5'));
            }
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn turns_exhausted_matches_the_stage_ceiling() {
        let (dir, mut w) = writer();
        w.emit(start("implement"));
        w.emit(end("implement", 60, None, Some(120)));

        let log = Log::open(dir.path());
        let cfg = Config {
            stage: vec![Stage {
                name: "implement".into(),
                turns: 120,
                ..Default::default()
            }],
            ..Default::default()
        };
        match turns_exhausted(&log, &cfg) {
            ProbeResult::Count(findings) => assert_eq!(findings.len(), 1),
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn turns_exhausted_does_not_fire_short_of_the_ceiling() {
        let (dir, mut w) = writer();
        w.emit(start("implement"));
        // one turn short of the configured cap: the agent chose to stop.
        w.emit(end("implement", 60, None, Some(119)));

        let log = Log::open(dir.path());
        let cfg = Config {
            stage: vec![Stage {
                name: "implement".into(),
                turns: 120,
                ..Default::default()
            }],
            ..Default::default()
        };
        match turns_exhausted(&log, &cfg) {
            ProbeResult::Count(findings) => assert_eq!(findings.len(), 0),
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn limit_repeat_needs_consecutive_stages() {
        let limit = |stage: &str| Kind::Limit {
            stage: stage.into(),
            matched: "rate limit".into(),
            sleep_seconds: 30,
            attempt: 1,
        };

        let (dir, mut w) = writer();
        w.emit(start("A"));
        w.emit(limit("A"));
        w.emit(start("B"));
        w.emit(limit("B"));
        let log = Log::open(dir.path());
        match limit_repeat(&log) {
            ProbeResult::Count(findings) => assert_eq!(findings.len(), 1),
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }

        let (dir2, mut w2) = writer();
        w2.emit(start("A"));
        w2.emit(limit("A"));
        w2.emit(start("B"));
        w2.emit(start("C"));
        w2.emit(limit("C"));
        let log2 = Log::open(dir2.path());
        match limit_repeat(&log2) {
            ProbeResult::Count(findings) => assert_eq!(findings.len(), 0),
            ProbeResult::Error(e) => panic!("expected findings, got error: {e}"),
        }
    }

    #[test]
    fn a_missing_log_is_error_not_zero() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = Log::open(dir.path());
        let cfg = Config::default();

        assert!(matches!(verdict_flip(&log), ProbeResult::Error(_)));
        assert!(matches!(rejection_repeat(&log), ProbeResult::Error(_)));
        assert!(matches!(stage_outlier(&log), ProbeResult::Error(_)));
        assert!(matches!(turns_exhausted(&log, &cfg), ProbeResult::Error(_)));
        assert!(matches!(limit_repeat(&log), ProbeResult::Error(_)));

        for (_, result) in all(&log, &cfg) {
            assert!(matches!(result, ProbeResult::Error(_)));
        }
    }
}
