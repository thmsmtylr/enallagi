use std::path::{Path, PathBuf};

pub fn run() -> anyhow::Result<i32> {
    let root = Path::new(".");
    let dir = crate::config::load(root)?.layout.harness_dir;
    let live = live_root(root, &dir);
    let tasks = crate::config::instance_path(&live, &dir, "TASKS.md");
    crate::tui::run_attached(&live.join(&dir), &tasks)?;
    Ok(0)
}

// a lane started by `enallagi worktree` writes loop.pid inside its own worktree, so the parent's log is a finished round
fn live_root(root: &Path, harness_dir: &str) -> PathBuf {
    if crate::archive::loop_live(root, harness_dir).is_some() {
        return root.to_path_buf();
    }
    // lane-<timestamp>-<pid>, so the last name sorted is the newest lane
    let mut lanes: Vec<PathBuf> = std::fs::read_dir(root.join(harness_dir).join("worktrees"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("lane-"))
        })
        .collect();
    lanes.sort();
    lanes
        .into_iter()
        .rev()
        .find(|lane| crate::archive::loop_live(lane, harness_dir).is_some())
        .unwrap_or_else(|| root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;
    use std::process::{Child, Command};

    struct Reaper(Child);
    impl Drop for Reaper {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn sleeper() -> (u32, Reaper) {
        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        (child.id(), Reaper(child))
    }

    fn lane_pid(r: &Repo, lane: &str, pid: u32) -> PathBuf {
        let rel = format!(".enallagi/worktrees/{lane}");
        r.write(&format!("{rel}/.enallagi/loop.pid"), &pid.to_string());
        r.root.join(rel)
    }

    #[test]
    fn a_live_lane_is_what_watch_attaches_to() {
        let r = Repo::new();
        let (pid, _reaper) = sleeper();
        let lane = lane_pid(&r, "lane-20260916-120000-1", pid);

        assert_eq!(live_root(&r.root, ".enallagi"), lane);
    }

    #[test]
    fn no_live_pid_attaches_to_the_cwd() {
        let r = Repo::new();
        let (pid, reaper) = sleeper();
        drop(reaper); // the pid file names a lane whose loop has exited
        lane_pid(&r, "lane-20260916-120000-1", pid);

        assert_eq!(live_root(&r.root, ".enallagi"), r.root);
    }

    #[test]
    fn only_the_lane_whose_pid_is_alive_is_taken() {
        let r = Repo::new();
        let (dead, reaper) = sleeper();
        drop(reaper);
        let (live, _keep) = sleeper();
        lane_pid(&r, "lane-20260916-130000-2", dead);
        let alive = lane_pid(&r, "lane-20260916-120000-1", live);

        assert_eq!(live_root(&r.root, ".enallagi"), alive);
    }

    #[test]
    fn the_parents_own_loop_wins_over_a_lane() {
        let r = Repo::new();
        let (parent, _keep) = sleeper();
        let (lane, _also) = sleeper();
        r.write(".enallagi/loop.pid", &parent.to_string());
        lane_pid(&r, "lane-20260916-120000-1", lane);

        assert_eq!(live_root(&r.root, ".enallagi"), r.root);
    }
}
