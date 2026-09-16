use std::path::Path;

pub fn run() -> anyhow::Result<i32> {
    let dir = crate::config::load(Path::new("."))?.layout.harness_dir;
    let tasks = crate::config::instance_path(Path::new("."), &dir, "TASKS.md");
    crate::tui::run_attached(Path::new(&dir), &tasks)?;
    Ok(0)
}
