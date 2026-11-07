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
    for (key, _) in std::env::vars() {
        if key.starts_with(crate::config::ENV) {
            cmd.env_remove(key);
        }
    }
    cmd.env("XDG_CACHE_HOME", stub_cache());
    cmd
}

// writes the --report-path or --output it is handed an empty result, as a clean scan would
const STUB_SCANNER: &str = r#"#!/bin/sh
# stub scanner
while [ $# -gt 0 ]; do
  case "$1" in
    --report-path) echo '[]' > "$2" ;;
    --output) echo '{"results":[]}' > "$2" ;;
  esac
  shift
done
"#;

/// Stub `gitleaks` and `osv-scanner` at the default pins' cache paths, so no spawned binary fetches.
pub fn seed_scanners(cache: &Path) {
    let platform = crate::security::platform();
    for pin in crate::security::default_pins() {
        if pin.platform != platform {
            continue;
        }
        let bin = crate::security::tool_path(&pin, cache);
        fs::create_dir_all(bin.parent().expect("tool dir")).expect("create the tool dir");
        // renamed into place, since test processes share the cache
        let tmp = bin.with_extension(format!("{}", std::process::id()));
        fs::write(&tmp, STUB_SCANNER).expect("write the stub scanner");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).expect("chmod stub");
        }
        fs::rename(&tmp, &bin).expect("place the stub scanner");
    }
}

fn stub_cache() -> &'static Path {
    static CACHE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let cache = std::env::temp_dir().join("enallagi-fixture-cache");
        seed_scanners(&cache);
        cache
    })
}

/// `Repo::queue` for a fixture that builds its own repository.
pub fn queue(root: &Path, dir: &str, text: &str) {
    let t = crate::tracker::Tracker::open(root, dir);
    let spec = crate::tracker::repo_slug(root);
    // a rewrite replaces the queue, so a block it dropped must not linger as a file
    let issues = t.0.join(&spec).join("issues");
    let _ = fs::remove_dir_all(&issues);
    fs::create_dir_all(&issues).expect("create the issues dir");
    for b in crate::block::parse(text).expect("parse the fixture queue") {
        t.append(&spec, &crate::block::block_text(&b))
            .expect("write the fixture ticket");
    }
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

    /// Writes block text as ticket files under `dir/specs/`, replacing the queue.
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
        self.write(".enallagi/enallagi.toml", toml_overrides);
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
    fn stub_scanners_pinned() {
        let out = command("env").output().expect("env");
        let env = String::from_utf8_lossy(&out.stdout).into_owned();
        let cache = env
            .lines()
            .find_map(|l| l.strip_prefix("XDG_CACHE_HOME="))
            .expect("XDG_CACHE_HOME");
        let platform = crate::security::platform();
        for id in ["gitleaks", "osv-scanner"] {
            let bin = crate::security::tool(
                &crate::security::default_pins(),
                id,
                &platform,
                Path::new(cache),
            )
            .expect("seeded");
            assert!(fs::read_to_string(bin).expect("stub").contains("stub"));
        }
    }

    #[test]
    fn queue_reads_back() {
        let r = Repo::new();
        r.queue(
            ".enallagi",
            "## [T-001] one\nstatus: ready-for-agent\n\n## [T-002] two\nstatus: done\n",
        );
        r.queue(".enallagi", "## [T-001] one\nstatus: done\n");
        let blocks = Tracker::open(&r.root, ".enallagi").blocks().unwrap();
        let ids: Vec<(&str, Option<String>)> =
            blocks.iter().map(|b| (b.id.as_str(), state(b))).collect();
        assert_eq!(ids, vec![("T-001", Some("done".to_string()))]);
    }
}
