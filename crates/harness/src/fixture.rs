//! fixture: test support, also used by `enallagi eval` and the driver.

use crate::git;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// a lane exports its own variables, and a spawned binary must read the fixture's tree, not the lane's
pub fn command(bin: &str) -> Command {
    let mut cmd = Command::new(bin);
    // CI=true freezes skill resolution, so a fixture run on a runner refuses every stage
    cmd.env_remove("CI");
    crate::config::drop_legacy_env(&mut cmd);
    for (key, _) in std::env::vars() {
        if key.starts_with(crate::config::ENV) {
            cmd.env_remove(key);
        }
    }
    cmd
}

/// `Repo::queue` for a fixture that builds its own repository.
pub fn queue(root: &Path, dir: &str, text: &str) {
    if std::env::var("ENALLAGI_TEST_LAYOUT").as_deref() != Ok("files") {
        let path = root.join(format!("{dir}/TASKS.md").trim_start_matches('/'));
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent dirs");
        fs::write(path, text).expect("write fixture file");
        return;
    }
    let specs = crate::config::instance_path(root, dir, "specs");
    let spec = crate::tracker::repo_slug(root);
    // a rewrite replaces the queue, so a block it dropped must not linger as a file
    let _ = fs::remove_dir_all(specs.join(&spec).join("issues"));
    crate::tracker::write_converted(&specs, &spec, text).expect("convert the fixture queue");
}

pub struct Repo {
    pub dir: tempfile::TempDir,
    pub root: PathBuf,
}

impl Repo {
    pub fn new() -> Repo {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let root = dir.path().to_path_buf();
        run(&root, &["init", "-q"]);
        run(&root, &["config", "user.email", "t@t"]);
        run(&root, &["config", "user.name", "t"]);
        let repo = Repo { dir, root };
        repo.write("src/schema.ts", "export const x = 1\n");
        repo.commit_all("init");
        repo
    }

    // a fixture that can't be built is an I/O error here, never a panic, so eval reports ERROR not a crash
    pub fn try_new() -> std::io::Result<Repo> {
        let dir = tempfile::TempDir::new()?;
        let root = dir.path().to_path_buf();
        let io = std::io::Error::other;
        git::git(&root, &["init", "-q"]).map_err(io)?;
        git::git(&root, &["config", "user.email", "t@t"]).map_err(io)?;
        git::git(&root, &["config", "user.name", "t"]).map_err(io)?;
        let repo = Repo { dir, root };
        repo.try_write("src/schema.ts", "export const x = 1\n")?;
        git::commit_paths(&repo.root, &["."], "init").map_err(io)?;
        Ok(repo)
    }

    pub fn write(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, content).expect("write fixture file");
    }

    pub fn try_write(&self, rel: &str, content: &str) -> std::io::Result<()> {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }

    /// Writes legacy queue text as `dir/TASKS.md`, or, under `ENALLAGI_TEST_LAYOUT=files`, as the ticket files `tasks migrate` makes of it.
    pub fn queue(&self, dir: &str, text: &str) {
        queue(&self.root, dir, text);
    }

    pub fn commit_all(&self, msg: &str) {
        run(&self.root, &["add", "-A"]);
        run(
            &self.root,
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", msg],
        );
    }

    pub fn stub_agent(&self, script_body: &str) -> Vec<String> {
        self.write_script("src/fakeagent.sh", script_body);
        vec![
            "./src/fakeagent.sh".to_string(),
            "{prompt}".to_string(),
            "{turns}".to_string(),
        ]
    }

    pub fn stub_check(&self, script_body: &str) -> String {
        self.write_script("src/fakecheck.sh", script_body);
        "./src/fakecheck.sh".to_string()
    }

    /// `[[skill]]` entries for every shipped skill id the given TOML does not declare, each a
    /// `path:` source under `vendor/`, so a fixture run never reaches the network.
    pub fn local_skills(&self, toml: &str) -> String {
        const IDS: [&str; 8] = [
            "tdd",
            "ponytail",
            "debugging",
            "review-received",
            "verify-before-done",
            "review-requested",
            "brainstorming",
            "caveman-commit",
        ];
        let mut out = String::new();
        for id in IDS {
            if toml.contains(&format!("id = \"{id}\"")) {
                continue;
            }
            self.write(&format!("vendor/{id}/SKILL.md"), &format!("# {id}\n"));
            out.push_str(&format!(
                "\n[[skill]]\nid = \"{id}\"\nsource = \"path:vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"fixture\"\n"
            ));
        }
        out
    }

    pub fn init_harness(&self, toml_overrides: &str) {
        self.write("enallagi.toml", toml_overrides);
        crate::init::install(&self.root, &crate::init::InitOpts::default()).expect("install");
    }

    fn write_script(&self, rel: &str, script_body: &str) {
        let content = format!("#!/usr/bin/env bash\n{script_body}");
        self.write(rel, &content);
        let path = self.root.join(rel);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod script");
        }
    }
}

impl Default for Repo {
    fn default() -> Self {
        Self::new()
    }
}

// through git::git, so a fixture's own commits drop the ambient identity the way the harness's do
fn run(root: &std::path::Path, args: &[&str]) {
    git::git(root, args).unwrap_or_else(|e| panic!("git {args:?} failed: {e}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::{state, Tracker};

    #[test]
    fn queue_reads_back_under_either_layout() {
        let r = Repo::new();
        r.queue(
            ".enallagi",
            "# TASKS\n\n## [T-001] one\nstatus: ready\n\n## [T-002] two\nstatus: done\n",
        );
        r.queue(".enallagi", "# TASKS\n\n## [T-001] one\nstatus: done\n");
        let blocks = Tracker::open(&r.root, ".enallagi").blocks().unwrap();
        let ids: Vec<(&str, Option<String>)> =
            blocks.iter().map(|b| (b.id.as_str(), state(b))).collect();
        assert_eq!(ids, vec![("T-001", Some("done".to_string()))]);
        let files = std::env::var("ENALLAGI_TEST_LAYOUT").as_deref() == Ok("files");
        assert_eq!(r.root.join(".enallagi/specs").is_dir(), files);
        assert_eq!(r.root.join(".enallagi/TASKS.md").is_file(), !files);
    }
}
