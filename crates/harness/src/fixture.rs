//! fixture: test support, also used by `harness eval` and the driver.

use crate::git;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

    /// The fallible twin of `new`: a fixture that cannot be built (a git
    /// binary missing, a full disk, a tempdir the sandbox refuses) is an
    /// I/O error here, never a panic. `harness eval` builds every fixture
    /// through this path so a broken fixture is reported as `ERROR`, not a
    /// crash.
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

    /// The fallible twin of `write`.
    pub fn try_write(&self, rel: &str, content: &str) -> std::io::Result<()> {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
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

    /// `harness.toml` = the embedded default plus `toml_overrides`, then a full
    /// install: the roles, the rails, the skill and the documents.
    pub fn init_harness(&self, toml_overrides: &str) {
        self.write("harness.toml", toml_overrides);
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

fn run(root: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .expect("spawn git");
    assert!(status.success(), "git {args:?} failed");
}
