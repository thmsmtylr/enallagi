//! test support, also used by `enallagi eval` and the driver

pub mod twin;

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

/// every shipped skill id, in the defaults' order
pub const SKILL_IDS: [&str; 13] = [
    "to-tickets",
    "triage",
    "tdd",
    "diagnosing-bugs",
    "codebase-design",
    "writing-for-agents",
    "domain-modeling",
    "code-review",
    "ponytail",
    "ponytail-review",
    "caveman-commit",
    "receiving-code-review",
    "verification-before-completion",
];

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

/// stub `gitleaks` and `osv-scanner` at the default pins' cache paths, so no spawned binary fetches
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

/// a gitleaks logging argv to `<cache>/gitleaks.argv` that emits `report`, or `[]` when baselined
pub fn stub_gitleaks(cache: &Path, report: &str, exit: i32) {
    seed_scanners(cache);
    fs::write(cache.join("gitleaks.json"), report).expect("write the stub report");
    let platform = crate::security::platform();
    let pin = crate::security::default_pins()
        .into_iter()
        .find(|p| p.id == "gitleaks" && p.platform == platform)
        .expect("a gitleaks pin for this platform");
    let c = cache.display();
    fs::write(
        crate::security::tool_path(&pin, cache),
        format!(
            r#"#!/bin/sh
printf '%s\n' "$@" > {c}/gitleaks.argv
out= base=
while [ $# -gt 0 ]; do
  case "$1" in
    --report-path) out=$2 ;;
    --baseline-path) base=$2 ;;
  esac
  shift
done
echo scanner-stderr >&2
if [ -n "$base" ] && cmp -s "$base" {c}/gitleaks.json; then echo '[]' > "$out"; exit 0; fi
cp {c}/gitleaks.json "$out"
exit {exit}
"#
        ),
    )
    .expect("write the stub gitleaks");
}

fn stub_cache() -> &'static Path {
    static CACHE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        // per uid: a root run would leave a shared path unwritable to the next user
        let uid = unsafe { libc::getuid() };
        let xdg = std::env::temp_dir().join(format!("enallagi-fixture-cache-{uid}"));
        seed_scanners(&xdg.join("enallagi"));
        xdg
    })
}

/// `git args` in `root`, panicking on failure
pub fn git(root: &Path, args: &[&str]) -> String {
    crate::git::git(root, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

/// writes an executable bash script under `set -u`; returns its path
pub fn exec(path: &Path, body: &str) -> String {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, format!("#!/usr/bin/env bash\nset -u\n{body}")).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path.display().to_string()
}

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

    // an unbuildable fixture is an I/O error, never a panic, so eval reports ERROR
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

    /// writes block text as ticket files under `dir/specs/`, replacing the queue
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

    /// `[[skill]]` entries, each a `path:` source under `vendor/`, for every shipped id the TOML lacks
    pub fn local_skills(&self, toml: &str) -> String {
        let mut out = String::new();
        for id in SKILL_IDS {
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

    /// the seeded spec, relative to the root
    pub fn spec(&self) -> String {
        format!(
            ".enallagi/specs/{}/spec.md",
            crate::tracker::repo_slug(&self.root)
        )
    }

    fn write_script(&self, rel: &str, script_body: &str) {
        exec(&self.root.join(rel), script_body);
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
        let xdg = env
            .lines()
            .find_map(|l| l.strip_prefix("XDG_CACHE_HOME="))
            .expect("XDG_CACHE_HOME");
        let cache = Path::new(xdg).join("enallagi");
        let platform = crate::security::platform();
        for id in ["gitleaks", "osv-scanner"] {
            let bin =
                crate::security::tool(&crate::security::default_pins(), id, &platform, &cache)
                    .expect("seeded");
            assert!(fs::read_to_string(bin).expect("stub").contains("stub"));
        }
    }

    #[test]
    fn bot_recordings_are_dated() {
        let bots = twin::recordings();
        let names: Vec<&str> = bots.iter().map(|b| b.bot.as_str()).collect();
        assert_eq!(
            names,
            ["kilo", "greptile", "bugbot", "coderabbit", "claude"]
        );
        for b in &bots {
            let template = if b.posts == "thread" {
                &b.thread
            } else {
                &b.summary
            };
            assert!(template.contains(&b.marker), "{} carries no marker", b.bot);
            assert!(b.source.starts_with("https://github.com/"), "{}", b.bot);
            let date = b.date.as_bytes();
            assert!(
                date.len() == 10
                    && date[4] == b'-'
                    && date[7] == b'-'
                    && date.iter().filter(|c| c.is_ascii_digit()).count() == 8,
                "{}: {}",
                b.bot,
                b.date
            );
        }
        let recorded: Vec<&str> = bots
            .iter()
            .filter(|b| b.recorded)
            .map(|b| b.bot.as_str())
            .collect();
        assert_eq!(recorded, ["kilo", "greptile"]);
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
