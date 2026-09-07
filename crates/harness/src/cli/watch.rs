use std::path::Path;

pub fn run() -> anyhow::Result<i32> {
    crate::tui::run_attached(Path::new(".harness"), Path::new("TASKS.md"))?;
    Ok(0)
}
