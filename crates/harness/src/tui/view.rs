//! draws one `Model` onto one `ratatui::Frame`; pure, so tested headless with `TestBackend`

use std::collections::HashMap;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use super::{stream, Model, Pane};
use crate::events::Kind;
use crate::{block, tracker};

pub fn view(model: &Model, frame: &mut Frame) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    render_header(model, frame, rows[0]);

    let mid =
        Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)]).split(rows[1]);
    render_queue(model, frame, mid[0]);
    render_stages(model, frame, mid[1]);

    render_output(model, frame, rows[2]);
    render_footer(model, frame, rows[3]);

    if model.help {
        render_help(frame, area);
    }
}

fn render_header(model: &Model, frame: &mut Frame, area: Rect) {
    let run = model.events.first().map(|e| e.run.as_str()).unwrap_or("-");
    let iter = model.events.last().map(|e| e.iter).unwrap_or(0);
    let spend: f64 = model
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::StageEnd { cost, .. } => *cost,
            _ => None,
        })
        .fold(0.0, |a, c| a + c);
    let budget = match model.budget_usd {
        Some(b) => format!(" / ${b:.2}"),
        None => String::new(),
    };
    let loop_state = if model.loop_running {
        " · running"
    } else {
        ""
    };
    let text =
        format!("harness · run {run} · iter {iter} · ${spend:.2}{budget}{loop_state}    q: STOP");
    let p = Paragraph::new(Line::styled(
        text,
        Style::default().add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(p, area);
}

fn render_queue(model: &Model, frame: &mut Frame, area: Rect) {
    let done: Vec<String> = tracker::ids_in(&model.queue, &["done"]);
    let lines: Vec<Line> = model
        .queue
        .iter()
        .map(|b| {
            let status = tracker::state(b).unwrap_or_else(|| "?".to_string());
            let blockers: Vec<String> = block::blockers(b)
                .into_iter()
                .filter(|id| !done.contains(id))
                .collect();
            let mut text = format!("{}  {}", b.id, status);
            if status != "done" && !blockers.is_empty() {
                text.push_str("  ");
                text.push_str(&blockers.join(", "));
            }
            if let Some((_, lane)) = model.claims.iter().find(|(id, _)| *id == b.id) {
                text.push_str("  ");
                text.push_str(lane);
            }
            if model.focused().and_then(|l| l.task.as_deref()) == Some(b.id.as_str()) {
                Line::styled(text, Style::default().add_modifier(Modifier::BOLD))
            } else {
                Line::from(text)
            }
        })
        .collect();
    let p = Paragraph::new(window(model, Pane::Queue, lines, area))
        .block(Block::bordered().title("queue"));
    frame.render_widget(p, area);
}

struct StageRow {
    iter: u32,
    stage: String,
    task: Option<String>,
    running: bool,
    seconds: u64,
    exit: i32,
    cost: Option<f64>,
    gates: Vec<(String, bool)>,
}

fn stage_rows(model: &Model) -> Vec<StageRow> {
    let mut rows: Vec<StageRow> = Vec::new();
    let mut open: HashMap<(String, Option<String>), usize> = HashMap::new();
    for e in model.events.iter().filter(|e| model.shown(e)) {
        match &e.kind {
            Kind::StageStart { stage, task, .. } => {
                rows.push(StageRow {
                    iter: e.iter,
                    stage: stage.clone(),
                    task: task.clone(),
                    running: true,
                    seconds: 0,
                    exit: 0,
                    cost: None,
                    gates: Vec::new(),
                });
                open.insert((stage.clone(), task.clone()), rows.len() - 1);
            }
            Kind::StageEnd {
                stage,
                task,
                seconds,
                exit,
                cost,
                ..
            } => {
                if let Some(&i) = open.get(&(stage.clone(), task.clone())) {
                    rows[i].running = false;
                    rows[i].seconds = *seconds;
                    rows[i].exit = *exit;
                    rows[i].cost = *cost;
                }
            }
            Kind::Gate {
                gate, task, pass, ..
            } => {
                if let Some(row) = rows
                    .iter_mut()
                    .rev()
                    .find(|r| r.task.as_deref() == Some(task.as_str()))
                {
                    row.gates.push((gate.clone(), *pass));
                }
            }
            _ => {}
        }
    }
    rows
}

fn fmt_duration(seconds: u64) -> String {
    format!("{}m{:02}s", seconds / 60, seconds % 60)
}

fn render_stage_row(r: &StageRow) -> String {
    let mut s = format!(
        "{}  {:<10} {:<7}",
        r.iter,
        r.stage,
        r.task.as_deref().unwrap_or("-")
    );
    if r.running {
        s.push_str("  running");
    } else {
        s.push_str(&format!("  {}  exit {}", fmt_duration(r.seconds), r.exit));
        if let Some(c) = r.cost {
            s.push_str(&format!("  ${c:.2}"));
        }
    }
    for (gate, pass) in &r.gates {
        s.push_str(&format!(
            "  {gate} {}",
            if *pass { "\u{2713}" } else { "\u{2717}" }
        ));
    }
    s
}

fn render_stages(model: &Model, frame: &mut Frame, area: Rect) {
    let lines: Vec<Line> = stage_rows(model)
        .iter()
        .map(|r| Line::from(render_stage_row(r)))
        .collect();
    let p = Paragraph::new(window(model, Pane::Stages, lines, area))
        .block(Block::bordered().title("stages"));
    frame.render_widget(p, area);
}

fn output_title(model: &Model) -> String {
    let Some((lane, stage)) = model.focused().and_then(|l| Some((l, l.stage.as_ref()?))) else {
        return "output".to_string();
    };
    let mut t = stage.clone();
    if let Some(task) = &lane.task {
        t.push(' ');
        t.push_str(task);
    }
    if let Some(cmd) = &lane.command {
        t.push_str(" \u{b7} ");
        t.push_str(cmd);
    }
    t.push_str(&lane_tag(model));
    let turns = lane
        .output
        .iter()
        .filter(|c| stream::is_assistant_chunk(c))
        .count();
    if turns > 0 {
        t.push_str(&format!(" \u{b7} turn {turns}"));
    }
    t
}

fn lane_tag(model: &Model) -> String {
    match model.focused().filter(|_| model.lanes.len() > 1) {
        Some(lane) => format!(" \u{b7} lane {} \u{b7} l: next", lane.run),
        None => String::new(),
    }
}

// the lines of `lines` that fit `area`, ending `model.offset(pane)` lines before the tail
fn window<'a>(model: &Model, pane: Pane, lines: Vec<Line<'a>>, area: Rect) -> Vec<Line<'a>> {
    let height = area.height.saturating_sub(2) as usize;
    let total = lines.len();
    model.record(pane, height, total);
    let end = total - model.offset(pane).min(total.saturating_sub(height));
    let start = end.saturating_sub(height);
    lines.into_iter().skip(start).take(end - start).collect()
}

fn feed(model: &Model) -> Vec<String> {
    model
        .feed
        .iter()
        .filter(|(run, _)| model.shows(run))
        .map(|(_, line)| line.clone())
        .collect()
}

fn render_output(model: &Model, frame: &mut Frame, area: Rect) {
    let (all_lines, title) = if model.raw {
        let lines = model
            .focused()
            .map(|l| l.output.as_slice())
            .unwrap_or_default()
            .iter()
            .flat_map(|c| stream::parse_chunk(c))
            .collect();
        (
            lines,
            format!("{} \u{b7} raw \u{b7} r: events", output_title(model)),
        )
    } else {
        (
            feed(model),
            format!("events{} \u{b7} r: raw", lane_tag(model)),
        )
    };
    let width = area.width.saturating_sub(2).max(1) as usize;
    let lines = all_lines
        .iter()
        .flat_map(|l| wrap(l, width))
        .map(Line::from)
        .collect();
    let p = Paragraph::new(window(model, Pane::Output, lines, area))
        .block(Block::bordered().title(title));
    frame.render_widget(p, area);
}

// wrapped before windowing, so the window counts rows on screen and the tail is never cut
fn wrap(line: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars.chunks(width).map(|c| c.iter().collect()).collect()
}

fn render_footer(model: &Model, frame: &mut Frame, area: Rect) {
    let halts: Vec<String> = model
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Halt { halt, .. } => Some(halt.clone()),
            _ => None,
        })
        .collect();
    let warnings: Vec<String> = model
        .events
        .iter()
        .rev()
        .find_map(|e| match &e.kind {
            Kind::RunEnd { warnings, .. } => Some(warnings.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let halts_s = if halts.is_empty() {
        "none".to_string()
    } else {
        halts.join(", ")
    };
    let warnings_s = if warnings.is_empty() {
        "none".to_string()
    } else {
        warnings.join("; ")
    };
    let prefix = if model.finished {
        "finished \u{2014} q to leave    "
    } else {
        ""
    };
    let p = Paragraph::new(format!(
        "{prefix}halts: {halts_s}    warnings: {warnings_s}"
    ));
    frame.render_widget(p, area);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let width = area.width.min(48);
    let height = area.height.min(11);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let help_area = Rect::new(x, y, width, height);
    let p = Paragraph::new(
        "q          STOP (write) / quit (watch)\n\
         Tab        cycle focus\n\
         ↑ ↓ wheel  scroll the focused pane\n\
         PgUp PgDn  scroll a page\n\
         Home End   top / follow the tail\n\
         r          events / raw agent output\n\
         l          next lane\n\
         ?          toggle this help",
    )
    .block(Block::bordered().title("help"));
    frame.render_widget(Clear, help_area);
    frame.render_widget(p, help_area);
}
