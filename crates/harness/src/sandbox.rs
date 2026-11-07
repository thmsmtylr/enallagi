//! The one seam every spawn of lane-written code goes through (ADR 0004).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{Config, SandboxMode};
use crate::proxy::{Denial, Proxy};

// the security gate's scanners download from GitHub releases, which redirect to release-assets (ADR 0009)
pub const BASE_HOSTS: &[&str] = &[
    "github.com",
    "api.github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

pub struct Sandbox {
    mode: SandboxMode,
    // ponytail: a proxy that cannot bind leaves None and lanes get port 0, so no egress
    proxy: Option<Proxy>,
}

pub struct Wrapped {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

impl Sandbox {
    pub fn start(_root: &Path, cfg: &Config) -> Sandbox {
        let proxy = match cfg.sandbox.mode {
            SandboxMode::Off => None,
            SandboxMode::Native => Proxy::start(allowlist(cfg)).ok(),
        };
        Sandbox {
            mode: cfg.sandbox.mode,
            proxy,
        }
    }

    pub fn name(&self) -> &'static str {
        match self.mode {
            SandboxMode::Off => "off",
            SandboxMode::Native => "native",
        }
    }

    pub fn wrap<S: AsRef<str>>(&self, argv: &[S], cwd: &Path) -> Wrapped {
        let mut env = Vec::new();
        if self.mode != SandboxMode::Off {
            let url = format!(
                "http://127.0.0.1:{}",
                self.proxy.as_ref().map_or(0, |p| p.port)
            );
            for var in ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"] {
                env.push((var.to_string(), url.clone()));
                env.push((var.to_ascii_lowercase(), url.clone()));
            }
            for var in ["NO_PROXY", "no_proxy"] {
                env.push((var.to_string(), "localhost,127.0.0.1,::1".to_string()));
            }
        }
        Wrapped {
            argv: argv.iter().map(|s| s.as_ref().to_string()).collect(),
            cwd: cwd.to_path_buf(),
            env,
        }
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
