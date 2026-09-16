//! Renders a run from the event log, live or attached; `Model` folds events, `view` draws onto any `ratatui` backend.

mod stream;
mod view;

use std::panic::PanicHookInfo;
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;

use crate::events::{Event, Kind, Log};
use crate::queue::{self, Block};

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
    pub scroll: usize,
    pub stop_requested: bool,
    pub current_stage: Option<String>,
    pub output: Vec<String>,
    pub help: bool,
    // set once the event channel disconnects; the view then stays up (nothing more will change) until `q`
    pub finished: bool,
    // tracked for the queue's bold row and the output pane's title; not part of the shared interface, so private
    current_task: Option<String>,
    current_command: Option<String>,
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
            scroll: 0,
            stop_requested: false,
            current_stage: None,
            output: Vec::new(),
            help: false,
            finished: false,
            current_task: None,
            current_command: None,
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
            KeyCode::Up => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Char('?') => self.help = !self.help,
            _ => {}
        }
    }
}

impl Default for Model {
    fn default() -> Model {
        Model::new()
    }
}

fn read_queue(tasks: &Path) -> Vec<Block> {
    std::fs::read_to_string(tasks)
        .ok()
        .and_then(|text| queue::parse(&text).ok())
        .unwrap_or_default()
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
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        }
        if self.raw_mode {
            let _ = disable_raw_mode();
        }
    }
}

// unlike TerminalGuard, tries both steps unconditionally and ignores errors -- idempotent, safe to call more than once
fn restore_terminal() {
    let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
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
pub fn run_live(rx: Receiver<Event>, tasks: &Path, stop: &Path) -> anyhow::Result<()> {
    let (mut terminal, guard) = enter_terminal()?;
    let prev_hook = install_restore_hook();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_live_loop(&mut terminal, &rx, tasks, stop)
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
    tasks: &Path,
    stop: &Path,
) -> anyhow::Result<()> {
    let mut model = Model::new();
    model.queue = read_queue(tasks);
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
            model.queue = read_queue(tasks);
        }
        if event::poll(Duration::from_millis(500))? {
            if let CEvent::Key(key) = event::read()? {
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

// read-only: tails the log and TASKS.md rather than owning a channel; `q` never touches STOP since watch doesn't own the run
pub fn run_attached(harness_dir: &Path, tasks: &Path) -> anyhow::Result<()> {
    let (mut terminal, guard) = enter_terminal()?;
    let prev_hook = install_restore_hook();
    let log = Log::open(harness_dir);
    let pid_path = harness_dir.join("loop.pid");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_attached_loop(&mut terminal, &log, tasks, &pid_path)
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
    tasks: &Path,
    pid_path: &Path,
) -> anyhow::Result<()> {
    let mut model = Model::new();
    model.queue = read_queue(tasks);
    let mut last_seq = 0u64;
    loop {
        if let Ok(events) = log.read_since(last_seq) {
            for e in &events {
                last_seq = last_seq.max(e.seq);
                model.apply(e);
            }
        }
        model.queue = read_queue(tasks);
        model.loop_running = loop_running(pid_path);
        terminal
            .draw(|f| view(&model, f))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if event::poll(Duration::from_millis(500))? {
            if let CEvent::Key(key) = event::read()? {
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
    use crossterm::event::{KeyEventState, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn fixture_model() -> Model {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut w = Writer::new(EventLog::open(dir.path()));
        let events = vec![
            w.emit(Kind::RunStart {
                config_sha256: "deadbeef".into(),
                pipeline: None,
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
                turns: None,
            }),
            w.emit(Kind::Gate {
                gate: "verdict".into(),
                task: "T-001".into(),
                pass: true,
                reason: "ok".into(),
            }),
            w.emit(Kind::StageStart {
                stage: "verify".into(),
                role: Some("verifier".into()),
                command: Some("claude".into()),
                task: Some("T-001".into()),
            }),
        ];

        let tasks_md =
            "## [T-001] Do the thing\nstatus: review\n\n## [T-002] Another thing\nstatus: ready\n";

        let mut model = Model::new();
        model.queue = queue::parse(tasks_md).expect("parse queue");
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
    fn the_view_renders_a_fixture_run_headless() {
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
            "T-002  ready",
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
    fn q_writes_stop_and_no_key_touches_the_queue() {
        let mut model = fixture_model();
        let queue_before = format!("{:?}", model.queue);
        assert!(!model.stop_requested);

        // handle_key never mutates queue or writes TASKS.md -- checked for every recognised key, not just `q`
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
        ] {
            model.handle_key(key);
            assert_eq!(format!("{:?}", model.queue), queue_before);
        }
        assert!(model.stop_requested);
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
