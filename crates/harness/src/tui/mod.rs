//! Renders a run from the event log, live or attached; `Model` folds events, `view` draws onto any `ratatui` backend.

mod stream;
mod view;

use std::cell::Cell;
use std::panic::PanicHookInfo;
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEvent,
    KeyEventKind, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;

use crate::block::Block;
use crate::events::{Event, Kind, Log};
use crate::tracker::Tracker;

pub use view::view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Queue,
    Stages,
    Output,
}

pub struct Model {
    pub events: Vec<Event>,
    pub queue: Vec<Block>,
    pub loop_running: bool,
    pub budget_usd: Option<f64>,
    pub focus: Pane,
    // lines back from each pane's tail, indexed by `Pane as usize`; 0 follows the tail
    pub scroll: [usize; 3],
    // the output pane shows the agent's stream in place of the event feed
    pub raw: bool,
    pub stop_requested: bool,
    pub current_stage: Option<String>,
    pub output: Vec<String>,
    pub help: bool,
    // set once the event channel disconnects; the view then stays up (nothing more will change) until `q`
    pub finished: bool,
    // tracked for the queue's bold row and the output pane's title; not part of the shared interface, so private
    current_task: Option<String>,
    current_command: Option<String>,
    // each pane's inner height and furthest offset, recorded by the last draw so keys page and stop at the top
    heights: Cell<[usize; 3]>,
    max_scroll: Cell<[usize; 3]>,
}

impl Model {
    pub fn new() -> Model {
        Model {
            events: Vec::new(),
            queue: Vec::new(),
            loop_running: false,
            budget_usd: std::env::var("BUDGET_USD")
                .ok()
                .and_then(|s| s.parse().ok()),
            focus: Pane::Queue,
            scroll: [0; 3],
            raw: false,
            stop_requested: false,
            current_stage: None,
            output: Vec::new(),
            help: false,
            finished: false,
            current_task: None,
            current_command: None,
            heights: Cell::new([10; 3]),
            max_scroll: Cell::new([usize::MAX; 3]),
        }
    }

    // output is capped at the last 200 chunks
    pub fn apply(&mut self, e: &Event) {
        match &e.kind {
            Kind::StageStart {
                stage,
                task,
                command,
                ..
            } => {
                self.current_stage = Some(stage.clone());
                self.current_task = task.clone();
                self.current_command = command.clone();
                self.output.clear();
            }
            Kind::StageOutput { stage, chunk }
                if self.current_stage.as_deref() == Some(stage.as_str()) =>
            {
                self.output.push(chunk.clone());
                if self.output.len() > 200 {
                    self.output.remove(0);
                }
            }
            _ => {}
        }
        self.events.push(e.clone());
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.stop_requested = true,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Pane::Queue => Pane::Stages,
                    Pane::Stages => Pane::Output,
                    Pane::Output => Pane::Queue,
                };
            }
            KeyCode::Up => self.scroll_by(1),
            KeyCode::Down => self.scroll_by(-1),
            KeyCode::PageUp => self.scroll_by(self.page(self.focus) as isize),
            KeyCode::PageDown => self.scroll_by(-(self.page(self.focus) as isize)),
            KeyCode::Home => {
                self.scroll[self.focus as usize] = self.max_scroll.get()[self.focus as usize]
            }
            KeyCode::End => self.scroll[self.focus as usize] = 0,
            KeyCode::Char('r') => self.raw = !self.raw,
            KeyCode::Char('?') => self.help = !self.help,
            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_by(3),
            MouseEventKind::ScrollDown => self.scroll_by(-3),
            _ => {}
        }
    }

    pub fn offset(&self, pane: Pane) -> usize {
        self.scroll[pane as usize]
    }

    pub fn page(&self, pane: Pane) -> usize {
        self.heights.get()[pane as usize].max(1)
    }

    fn scroll_by(&mut self, lines: isize) {
        let i = self.focus as usize;
        let s = self.scroll[i];
        self.scroll[i] = if lines >= 0 {
            s.saturating_add(lines as usize)
        } else {
            s.saturating_sub(lines.unsigned_abs())
        }
        .min(self.max_scroll.get()[i]);
    }

    fn record(&self, pane: Pane, height: usize, total: usize) {
        let i = pane as usize;
        let mut heights = self.heights.get();
        heights[i] = height;
        self.heights.set(heights);
        let mut max = self.max_scroll.get();
        max[i] = total.saturating_sub(height);
        self.max_scroll.set(max);
    }
}

impl Default for Model {
    fn default() -> Model {
        Model::new()
    }
}

fn read_queue(tracker: &Tracker) -> Vec<Block> {
    tracker.blocks().unwrap_or_default()
}

fn loop_running(pid_path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(pid_path) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// restores on drop only the steps that succeeded, including on panic (Drop runs during unwinding);
// run_live/run_attached also catch_unwind so the terminal is restored before the panic resumes
struct TerminalGuard {
    raw_mode: bool,
    alt_screen: bool,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.alt_screen {
            let _ = execute!(std::io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        }
        if self.raw_mode {
            let _ = disable_raw_mode();
        }
    }
}

// unlike TerminalGuard, tries both steps unconditionally and ignores errors -- idempotent, safe to call more than once
fn restore_terminal() {
    let _ = execute!(std::io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

fn enter_terminal() -> anyhow::Result<(Terminal<CrosstermBackend<std::io::Stdout>>, TerminalGuard)>
{
    enable_raw_mode()?;
    let mut guard = TerminalGuard {
        raw_mode: true,
        alt_screen: false,
    };
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    guard.alt_screen = true;
    execute!(std::io::stdout(), EnableMouseCapture)?;
    let terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    Ok((terminal, guard))
}

type PanicHook = dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static;

// pulled out of install_restore_hook so a test can supply a fake restore/prev without touching a real terminal
fn build_hook<R>(restore: R, prev: Arc<PanicHook>) -> Box<PanicHook>
where
    R: Fn() + Send + Sync + 'static,
{
    Box::new(move |info| {
        restore();
        (*prev)(info);
    })
}

// restores the terminal before forwarding to the previous hook, so a panic's message prints on the normal screen
fn install_restore_hook() -> Arc<PanicHook> {
    let prev: Arc<PanicHook> = Arc::from(std::panic::take_hook());
    std::panic::set_hook(build_hook(restore_terminal, Arc::clone(&prev)));
    prev
}

fn restore_hook(prev: Arc<PanicHook>) {
    std::panic::set_hook(Box::new(move |info| (*prev)(info)));
}

// `q` writes the STOP file and keeps drawing until rx disconnects -- the loop decides when to end, the TUI just asks
pub fn run_live(rx: Receiver<Event>, tracker: &Tracker, stop: &Path) -> anyhow::Result<()> {
    let (mut terminal, guard) = enter_terminal()?;
    let prev_hook = install_restore_hook();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_live_loop(&mut terminal, &rx, tracker, stop)
    }));
    restore_hook(prev_hook);
    drop(guard);
    match outcome {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

// `q` means STOP while the run is live; once it's finished nothing is left to stop, so the same
// key means leave instead -- distinguished here rather than in Model::handle_key, which has no
// notion of "leave" and stays a plain state fold.
fn is_leave_key(finished: bool, key: &KeyEvent) -> bool {
    finished && key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q')
}

fn run_live_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    rx: &Receiver<Event>,
    tracker: &Tracker,
    stop: &Path,
) -> anyhow::Result<()> {
    let mut model = Model::new();
    model.queue = read_queue(tracker);
    let mut stop_written = false;
    let mut leave = false;
    loop {
        if !model.finished {
            loop {
                match rx.try_recv() {
                    Ok(e) => model.apply(&e),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        model.finished = true;
                        break;
                    }
                }
            }
            model.queue = read_queue(tracker);
        }
        if event::poll(Duration::from_millis(500))? {
            let read = event::read()?;
            if let CEvent::Mouse(mouse) = read {
                model.handle_mouse(mouse);
            }
            if let CEvent::Key(key) = read {
                let leaving = is_leave_key(model.finished, &key);
                model.handle_key(key);
                if leaving {
                    leave = true;
                } else if model.stop_requested && !model.finished && !stop_written {
                    std::fs::write(stop, b"")?;
                    stop_written = true;
                }
            }
        }
        terminal
            .draw(|f| view(&model, f))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if leave {
            break;
        }
    }
    Ok(())
}

// read-only: tails the log and the queue rather than owning a channel; `q` never touches STOP since watch doesn't own the run
pub fn run_attached(harness_dir: &Path, tracker: &Tracker) -> anyhow::Result<()> {
    let (mut terminal, guard) = enter_terminal()?;
    let prev_hook = install_restore_hook();
    let log = Log::open(harness_dir);
    let pid_path = harness_dir.join("loop.pid");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_attached_loop(&mut terminal, &log, tracker, &pid_path)
    }));
    restore_hook(prev_hook);
    drop(guard);
    match outcome {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn run_attached_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    log: &Log,
    tracker: &Tracker,
    pid_path: &Path,
) -> anyhow::Result<()> {
    let mut model = Model::new();
    model.queue = read_queue(tracker);
    let mut last_seq = 0u64;
    loop {
        if let Ok(events) = log.read_since(last_seq) {
            for e in &events {
                last_seq = last_seq.max(e.seq);
                model.apply(e);
            }
        }
        model.queue = read_queue(tracker);
        model.loop_running = loop_running(pid_path);
        terminal
            .draw(|f| view(&model, f))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if event::poll(Duration::from_millis(500))? {
            let read = event::read()?;
            if let CEvent::Mouse(mouse) = read {
                model.handle_mouse(mouse);
            }
            if let CEvent::Key(key) = read {
                model.handle_key(key);
                if model.stop_requested {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Log as EventLog, Writer};
    use crossterm::event::{KeyEventState, KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn fixture_model() -> Model {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut w = Writer::new(EventLog::open(dir.path()));
        let events = vec![
            w.emit(Kind::RunStart {
                config_sha256: "deadbeef".into(),
                pipeline: None,
                sandbox: "off".into(),
                binary: None,
            }),
            w.emit(Kind::StageStart {
                stage: "implement".into(),
                role: Some("implementer".into()),
                command: Some("claude".into()),
                task: Some("T-001".into()),
            }),
            w.emit(Kind::StageEnd {
                stage: "implement".into(),
                task: Some("T-001".into()),
                seconds: 276,
                exit: 0,
                cost: Some(0.61),
                input_tokens: None,
                output_tokens: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                turns: None,
                turn_cap: None,
            }),
            w.emit(Kind::Gate {
                gate: "verdict".into(),
                task: "T-001".into(),
                pass: true,
                reason: "ok".into(),
                tally: None,
            }),
            w.emit(Kind::StageStart {
                stage: "verify".into(),
                role: Some("verifier".into()),
                command: Some("claude".into()),
                task: Some("T-001".into()),
            }),
        ];

        let tasks_md = "## [T-001] Do the thing\nstatus: review\n\n## [T-002] Another thing\nstatus: ready-for-agent\n\n## [T-003] Then this\nstatus: ready-for-agent\nblockedBy: T-001, T-002\n";
        let repo = crate::fixture::Repo::new();
        repo.queue(".enallagi", tasks_md);

        let mut model = Model::new();
        model.queue = read_queue(&Tracker::open(&repo.root, ".enallagi"));
        for e in &events {
            model.apply(e);
        }
        model
    }

    fn press(c: char) -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn buffer_text(backend: &TestBackend) -> String {
        backend
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
    }

    #[test]
    fn view_headless() {
        let model = fixture_model();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| view(&model, f)).expect("draw");
        let text = buffer_text(terminal.backend());
        for needle in [
            "implement",
            "T-001",
            "4m36s",
            "verdict \u{2713}",
            "T-002  ready-for-agent",
            "T-003  ready-for-agent  T-001, T-0",
            "running",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
        }
    }

    #[test]
    fn an_unfinished_run_shows_zero_spend() {
        let mut model = fixture_model();
        model
            .events
            .retain(|e| !matches!(e.kind, Kind::StageEnd { .. }));
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| view(&model, f)).expect("draw");
        let text = buffer_text(terminal.backend());
        assert!(text.contains("$0.00"), "{text}");
        assert!(!text.contains("$-0.00"), "{text}");
    }

    #[test]
    fn a_finished_run_renders_the_leave_prompt() {
        let mut model = fixture_model();
        model.finished = true;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| view(&model, f)).expect("draw");
        let text = buffer_text(terminal.backend());
        assert!(text.contains("finished \u{2014} q to leave"), "{text}");
        assert!(text.contains("halts:"), "{text}");
        assert!(text.contains("warnings:"), "{text}");
    }

    #[test]
    fn q_leaves_once_finished_instead_of_writing_stop() {
        assert!(is_leave_key(true, &press('q')));
        assert!(
            !is_leave_key(false, &press('q')),
            "still running: q means STOP, not leave"
        );
        assert!(!is_leave_key(true, &press('x')), "only q leaves");
    }

    #[test]
    fn q_writes_stop() {
        let mut model = fixture_model();
        let queue_before = format!("{:?}", model.queue);
        assert!(!model.stop_requested);

        // handle_key never mutates the queue -- checked for every recognised key, not just `q`
        for key in [
            press('q'),
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            },
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            },
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            },
            press('?'),
            press('r'),
            key(KeyCode::PageUp),
            key(KeyCode::PageDown),
            key(KeyCode::Home),
            key(KeyCode::End),
        ] {
            model.handle_key(key);
            assert_eq!(format!("{:?}", model.queue), queue_before);
        }
        assert!(model.stop_requested);
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn wheel(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn tool_chunk(name: &str) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"{name}","input":{{"path":"a.rs"}}}}]}}}}"#
        )
    }

    fn with_tool_calls(n: usize) -> Model {
        let mut model = fixture_model();
        let dir = tempfile::tempdir().expect("tempdir");
        let mut w = Writer::new(EventLog::open(dir.path()));
        for i in 0..n {
            model.apply(&w.emit(Kind::StageOutput {
                stage: "verify".into(),
                chunk: tool_chunk(&format!("Tool{i:02}")),
            }));
        }
        model
    }

    fn draw(model: &Model) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        terminal.draw(|f| view(model, f)).expect("draw");
        buffer_text(terminal.backend())
    }

    #[test]
    fn keys_scroll_focused_pane() {
        let mut model = with_tool_calls(100);
        model.focus = Pane::Output;
        draw(&model);
        let page = model.page(Pane::Output);
        assert!(page > 1, "{page}");
        model.handle_key(key(KeyCode::PageUp));
        assert_eq!(model.offset(Pane::Output), page);
        assert_eq!(model.offset(Pane::Queue), 0);
        model.handle_key(key(KeyCode::PageUp));
        model.handle_key(key(KeyCode::PageDown));
        assert_eq!(model.offset(Pane::Output), page);
        model.handle_key(key(KeyCode::Home));
        let top = model.offset(Pane::Output);
        assert_eq!(top, 107 - page, "four events of 7 rows and 100 tool calls");
        model.handle_key(key(KeyCode::PageUp));
        assert_eq!(model.offset(Pane::Output), top, "the top stops the scroll");
        model.handle_key(key(KeyCode::PageDown));
        assert_eq!(model.offset(Pane::Output), top - page);
        model.handle_key(key(KeyCode::End));
        assert_eq!(model.offset(Pane::Output), 0);

        model.handle_mouse(wheel(MouseEventKind::ScrollUp));
        assert_eq!(model.offset(Pane::Output), 3);
        assert_eq!(model.offset(Pane::Stages), 0);
        model.handle_mouse(wheel(MouseEventKind::ScrollDown));
        assert_eq!(model.offset(Pane::Output), 0);
    }

    #[test]
    fn home_end_show_ends() {
        let mut model = with_tool_calls(40);
        model.focus = Pane::Output;
        let tail = draw(&model);
        assert!(tail.contains("Tool39"), "{tail}");
        assert!(!tail.contains("Tool00"), "{tail}");
        model.handle_key(key(KeyCode::Home));
        let top = draw(&model);
        assert!(top.contains("stage.start"), "{top}");
        assert!(!top.contains("Tool39"), "{top}");
        model.handle_key(key(KeyCode::End));
        assert!(draw(&model).contains("Tool39"));
    }

    #[test]
    fn wrapped_tail_shows() {
        let mut model = fixture_model();
        let dir = tempfile::tempdir().expect("tempdir");
        let mut w = Writer::new(EventLog::open(dir.path()));
        for _ in 0..30 {
            model.apply(&w.emit(Kind::Gate {
                gate: "verdict".into(),
                task: "T-001".into(),
                pass: false,
                reason: "x".repeat(300),
                tally: None,
            }));
        }
        model.apply(&w.emit(Kind::StageOutput {
            stage: "verify".into(),
            chunk: tool_chunk("Newest"),
        }));
        model.focus = Pane::Output;
        assert!(draw(&model).contains("tool  Newest"));
        model.handle_key(key(KeyCode::Home));
        assert!(!draw(&model).contains("tool  Newest"));
        model.handle_key(key(KeyCode::End));
        assert!(draw(&model).contains("tool  Newest"));
    }

    #[test]
    fn output_default_feed() {
        let mut model = with_tool_calls(1);
        let dir = tempfile::tempdir().expect("tempdir");
        let mut w = Writer::new(EventLog::open(dir.path()));
        model.apply(&w.emit(Kind::StageOutput {
            stage: "verify".into(),
            chunk: r#"{"type":"other","n":1}"#.into(),
        }));
        let feed = draw(&model);
        assert!(feed.contains("stage.start"), "{feed}");
        assert!(feed.contains("gate"), "{feed}");
        assert!(feed.contains("tool  Tool00"), "{feed}");
        assert!(!feed.contains(r#"{"type""#), "{feed}");
        model.handle_key(press('r'));
        let raw = draw(&model);
        assert!(raw.contains(r#"{"type":"other","n":1}"#), "{raw}");
        assert!(!raw.contains("stage.start"), "{raw}");
    }

    #[test]
    fn help_lists_keys() {
        let mut model = fixture_model();
        model.handle_key(press('?'));
        let text = draw(&model);
        for needle in ["Tab", "PgUp PgDn", "Home", "End", "wheel", "r ", "q "] {
            assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
        }
    }

    #[test]
    fn a_zero_width_terminal_does_not_panic() {
        let model = fixture_model();
        let backend = TestBackend::new(0, 0);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| view(&model, f)).expect("draw");
    }

    #[test]
    fn restore_terminal_is_idempotent() {
        restore_terminal();
        restore_terminal();
    }

    #[test]
    fn panic_hook_restores_before_forwarding() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Mutex;

        let restored = Arc::new(AtomicBool::new(false));
        let order: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let restored_flag = Arc::clone(&restored);
        let order_for_restore = Arc::clone(&order);
        let fake_restore = move || {
            order_for_restore.lock().expect("lock").push("restored");
            restored_flag.store(true, Ordering::SeqCst);
        };

        let order_for_prev = Arc::clone(&order);
        let fake_prev: Arc<PanicHook> = Arc::new(move |_info: &PanicHookInfo<'_>| {
            order_for_prev.lock().expect("lock").push("forwarded");
        });

        let saved = std::panic::take_hook();
        std::panic::set_hook(build_hook(fake_restore, fake_prev));
        let result = std::panic::catch_unwind(|| panic!("boom"));
        std::panic::set_hook(saved);

        assert!(result.is_err());
        assert!(restored.load(Ordering::SeqCst));
        assert_eq!(*order.lock().expect("lock"), vec!["restored", "forwarded"]);
    }
}
