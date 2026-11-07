//! The one seam every spawn of lane-written code goes through (ADR 0004).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{Config, SandboxMode};
use crate::proxy::{Denial, Proxy};

pub mod bwrap;

// the security gate's scanners download from GitHub releases, which redirect to release-assets (ADR 0009)
pub const BASE_HOSTS: &[&str] = &[
    "github.com",
    "api.github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

const VAR: &str = "ENALLAGI_SANDBOX";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Off,
    Nested,
    Seatbelt,
    Bwrap,
}

pub struct Sandbox {
    kind: Kind,
    // dropped first, so no relay outlives its socket dir or its proxy
    _relays: Option<bwrap::Relays>,
    // ponytail: a proxy that cannot bind leaves None and lanes get port 0, so no egress
    proxy: Option<Proxy>,
    bwrap: PathBuf,
    ports: Vec<u16>,
    write: Vec<String>,
    dir: String,
    // removed on drop with every spawn's TMPDIR under it
    run: Option<tempfile::TempDir>,
    // the launcher's end of each bridged port, Linux only
    socks: Option<tempfile::TempDir>,
}

pub struct Wrapped {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

// what one spawn may write, and the paths inside that it may not
struct Policy {
    write: Vec<PathBuf>,
    deny: Vec<PathBuf>,
    // ancestors of a denied path inside a writable root: renaming one would carry the path out from under its rule
    pinned: Vec<PathBuf>,
}

impl Sandbox {
    pub fn start(root: &Path, cfg: &Config) -> Sandbox {
        Sandbox::with(root, cfg, std::env::var_os(VAR).is_some())
    }

    fn with(root: &Path, cfg: &Config, nested: bool) -> Sandbox {
        let kind = match cfg.sandbox.mode {
            SandboxMode::Off => Kind::Off,
            SandboxMode::Native if nested => Kind::Nested,
            SandboxMode::Native if cfg!(target_os = "macos") => Kind::Seatbelt,
            SandboxMode::Native => Kind::Bwrap,
        };
        let live = matches!(kind, Kind::Seatbelt | Kind::Bwrap);
        let cache = crate::config::cache_root();
        let dir_in = |sub: &str| {
            let parent = cache.join(sub);
            std::fs::create_dir_all(&parent).ok()?;
            tempfile::Builder::new()
                .prefix("run-")
                .tempdir_in(&parent)
                .ok()
        };
        let proxy = live.then(|| Proxy::start(allowlist(cfg)).ok()).flatten();
        let socks = (kind == Kind::Bwrap).then(|| dir_in("run")).flatten();
        let relays = socks.as_ref().map(|d| {
            let port = proxy.as_ref().map_or(0, |p| p.port);
            let ports: Vec<u16> = std::iter::once(port)
                .chain(cfg.sandbox.ports.clone())
                .collect();
            bwrap::Relays::start(d.path(), &ports)
        });
        Sandbox {
            kind,
            _relays: relays,
            proxy,
            bwrap: bwrap::find(root).unwrap_or_else(|| "bwrap".into()),
            ports: cfg.sandbox.ports.clone(),
            write: cfg.sandbox.write.clone(),
            dir: cfg.layout.harness_dir.clone(),
            run: live.then(|| dir_in("tmp")).flatten(),
            socks,
        }
    }

    pub fn name(&self) -> &'static str {
        match self.kind {
            Kind::Off => "off",
            Kind::Nested => "nested",
            Kind::Seatbelt => "seatbelt",
            Kind::Bwrap => "bwrap",
        }
    }

    /// The argv and env that spawn `argv` in `cwd` confined; `write` is the preset's own state paths.
    pub fn wrap<S: AsRef<str>>(&self, argv: &[S], cwd: &Path, write: &[String]) -> Wrapped {
        let argv: Vec<String> = argv.iter().map(|s| s.as_ref().to_string()).collect();
        let cwd = cwd.to_path_buf();
        if !matches!(self.kind, Kind::Seatbelt | Kind::Bwrap) {
            return Wrapped {
                argv,
                cwd,
                env: Vec::new(),
            };
        }
        let port = self.proxy.as_ref().map_or(0, |p| p.port);
        let url = format!("http://127.0.0.1:{port}");
        let mut env = Vec::new();
        for var in ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"] {
            env.push((var.to_string(), url.clone()));
            env.push((var.to_ascii_lowercase(), url.clone()));
        }
        for var in ["NO_PROXY", "no_proxy"] {
            env.push((var.to_string(), "localhost,127.0.0.1,::1".to_string()));
        }
        let tmp = self.run.as_ref().and_then(|run| {
            tempfile::Builder::new()
                .tempdir_in(run.path())
                .ok()
                .map(tempfile::TempDir::keep)
        });
        let extra = self.write.iter().chain(write).map(|w| home(w));
        let policy = policy(&cwd, &self.dir, extra.chain(tmp.clone()));
        if let Some(tmp) = tmp {
            env.push(("TMPDIR".to_string(), canon(&tmp).display().to_string()));
        }
        env.push((VAR.to_string(), self.name().to_string()));
        let argv = match self.kind {
            Kind::Bwrap => {
                // ponytail: no socket dir means no bridge to reach, so the lane has no egress
                let socks = self
                    .socks
                    .as_ref()
                    .map_or(Path::new("/nonexistent"), |d| d.path());
                let ports: Vec<u16> = std::iter::once(port).chain(self.ports.clone()).collect();
                bwrap::argv(&self.bwrap, &policy, socks, &ports, &cwd, argv)
            }
            _ => seatbelt(&policy, port, &self.ports, argv),
        };
        Wrapped { argv, cwd, env }
    }

    pub fn denials_since(&self, mark: usize) -> Vec<Denial> {
        let all = self.proxy.as_ref().map(Proxy::denials).unwrap_or_default();
        all.into_iter().skip(mark).collect()
    }
}

impl Wrapped {
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.argv[0]);
        c.args(&self.argv[1..])
            .current_dir(&self.cwd)
            .envs(self.env.iter().cloned());
        c
    }
}

fn home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(path),
    }
}

// Seatbelt matches real paths only, so `/var/..` must be written `/private/var/..`; a path not yet
// made is its canonical parent plus its name
fn canon(path: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(path) {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if parent != path => canon(parent).join(name),
        _ => path.to_path_buf(),
    }
}

fn policy(cwd: &Path, dir: &str, extra: impl Iterator<Item = PathBuf>) -> Policy {
    let mut write = vec![canon(cwd)];
    let mut deny = Vec::new();
    for repo in [cwd.to_path_buf(), cwd.join(dir)] {
        let Ok(common) = crate::git::git(
            &repo,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        ) else {
            continue;
        };
        let common = canon(Path::new(&common));
        // a missing hooks dir is one a lane could create
        let _ = std::fs::create_dir_all(common.join("hooks"));
        deny.extend([common.join("config"), common.join("hooks")]);
        for lane in std::fs::read_dir(common.join("worktrees"))
            .into_iter()
            .flatten()
            .flatten()
        {
            deny.extend([lane.path().join("commondir"), lane.path().join("gitdir")]);
        }
        let pointer = canon(&repo).join(".git");
        if pointer.is_file() {
            deny.push(pointer);
        }
        write.push(common);
    }
    write.extend(extra.map(|p| canon(&p)));
    write.sort();
    write.dedup();
    deny.sort();
    deny.dedup();
    let mut pinned: Vec<PathBuf> = deny
        .iter()
        .flat_map(|d| d.ancestors().skip(1))
        .filter(|a| write.iter().any(|w| a.starts_with(w)))
        .map(Path::to_path_buf)
        .collect();
    pinned.sort();
    pinned.dedup();
    Policy {
        write,
        deny,
        pinned,
    }
}

const SEATBELT_BASE: &str = include_str!("seatbelt_base.sbpl");
const SEATBELT_NETWORK: &str = include_str!("seatbelt_network.sbpl");

// paths ride in as -D params, so no path is ever quoted into the profile text
fn seatbelt(policy: &Policy, proxy: u16, ports: &[u16], argv: Vec<String>) -> Vec<String> {
    let mut params = Vec::new();
    let mut param = |path: &Path| {
        let key = format!("P{}", params.len());
        params.push(format!("-D{key}={}", path.display()));
        format!("(param \"{key}\")")
    };
    let mut profile = format!("{SEATBELT_BASE}\n{SEATBELT_NETWORK}\n(allow file-read*)\n");
    profile.push_str(
        "(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\") (subpath \"/private/tmp\"))\n",
    );
    for w in &policy.write {
        let p = param(w);
        profile.push_str(&format!(
            "(allow file-write* (subpath {p}))\n(allow network-outbound (remote unix-socket (subpath {p})))\n"
        ));
    }
    for port in std::iter::once(&proxy).chain(ports) {
        profile.push_str(&format!(
            "(allow network-outbound (remote ip \"localhost:{port}\"))\n"
        ));
    }
    profile.push_str("(allow network-bind network-inbound (local ip \"localhost:*\"))\n");
    for d in &policy.deny {
        profile.push_str(&format!("(deny file-write* (subpath {}))\n", param(d)));
    }
    for a in &policy.pinned {
        profile.push_str(&format!("(deny file-write* (literal {}))\n", param(a)));
    }
    let mut out = vec![
        "/usr/bin/sandbox-exec".to_string(),
        "-p".to_string(),
        profile,
    ];
    out.extend(params);
    out.push("--".to_string());
    out.extend(argv);
    out
}

/// Why `mode = "native"` cannot confine on this host, naming the fix; None when it can.
pub fn refusal(root: &Path, cfg: &Config) -> Option<String> {
    if cfg.sandbox.mode == SandboxMode::Off || std::env::var_os(VAR).is_some() {
        return None;
    }
    let off = "or set `[sandbox] mode = \"off\"` in enallagi.toml to run lanes unconfined";
    if cfg!(target_os = "macos") {
        return (!Path::new("/usr/bin/sandbox-exec").exists()).then(|| {
            format!(
                "/usr/bin/sandbox-exec is missing, so no lane can be confined; restore it, {off}"
            )
        });
    }
    if cfg!(target_os = "linux") {
        return bwrap::refusal(root, off);
    }
    Some(format!(
        "no native sandbox on {}; {off}",
        std::env::consts::OS
    ))
}

/// The base hosts, every configured preset's `hosts`, then `[sandbox] hosts`.
pub fn allowlist(cfg: &Config) -> Vec<String> {
    let presets = crate::agent::presets();
    let named = std::iter::once(&cfg.agent.preset)
        .chain(cfg.agent.roles.values().filter_map(|r| r.preset.as_ref()));
    let mut out: Vec<String> = BASE_HOSTS.iter().map(|h| h.to_string()).collect();
    for name in named {
        out.extend(presets.get(name).into_iter().flat_map(|p| p.hosts.clone()));
    }
    out.extend(cfg.sandbox.hosts.iter().cloned());
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_run_wraps_nothing() {
        let mut cfg = Config::default();
        cfg.sandbox.mode = SandboxMode::Native;
        let sb = Sandbox::with(Path::new("."), &cfg, true);
        assert_eq!(sb.name(), "nested");
        let w = sb.wrap(&["true"], Path::new("."), &[]);
        assert_eq!(w.argv, ["true"]);
        assert!(w.env.is_empty(), "{:?}", w.env);
    }

    #[test]
    fn release_hosts_allowed() {
        let list = allowlist(&Config::default());
        for host in [
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ] {
            assert!(
                list.iter().any(|e| crate::proxy::listed(e, host, 443)),
                "{host}: {list:?}"
            );
        }
    }
}
