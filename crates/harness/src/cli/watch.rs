use std::path::Path;

pub fn run() -> anyhow::Result<i32> {
    let dir = crate::config::load(Path::new("."))?.layout.harness_dir;
    crate::tui::run_attached(Path::new(&dir), Path::new("TASKS.md"))?;
    Ok(0)
}
