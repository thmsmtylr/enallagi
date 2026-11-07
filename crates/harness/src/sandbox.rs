//! The one seam every spawn of lane-written code goes through (ADR 0004).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{Config, SandboxMode};

pub struct Sandbox {
    mode: SandboxMode,
}

pub struct Wrapped {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
}

impl Sandbox {
    pub fn start(_root: &Path, cfg: &Config) -> Sandbox {
        Sandbox {
            mode: cfg.sandbox.mode,
        }
    }

    pub fn name(&self) -> &'static str {
        match self.mode {
            SandboxMode::Off => "off",
            SandboxMode::Native => "native",
        }
    }

    pub fn wrap<S: AsRef<str>>(&self, argv: &[S], cwd: &Path) -> Wrapped {
        Wrapped {
            argv: argv.iter().map(|s| s.as_ref().to_string()).collect(),
            cwd: cwd.to_path_buf(),
        }
    }
}

impl Wrapped {
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.argv[0]);
        c.args(&self.argv[1..]).current_dir(&self.cwd);
        c
    }
}
