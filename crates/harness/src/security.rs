//! The security gate's scanners: pinned per platform in this binary, fetched once into the user cache.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ToolPin {
    pub id: String,
    pub version: String,
    pub platform: String,
    pub url: String,
    pub sha256: String,
}

// from each release's checksums file; in the binary, since a lane can write harness.lock
const PINS: &[(&str, &str, &str, &str, &str)] = &[
    ("gitleaks", "8.30.1", "darwin-arm64", "https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_darwin_arm64.tar.gz", "b40ab0ae55c505963e365f271a8d3846efbc170aa17f2607f13df610a9aeb6a5"),
    ("gitleaks", "8.30.1", "darwin-x64", "https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_darwin_x64.tar.gz", "dfe101a4db2255fc85120ac7f3d25e4342c3c20cf749f2c20a18081af1952709"),
    ("gitleaks", "8.30.1", "linux-arm64", "https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_arm64.tar.gz", "e4a487ee7ccd7d3a7f7ec08657610aa3606637dab924210b3aee62570fb4b080"),
    ("gitleaks", "8.30.1", "linux-x64", "https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_x64.tar.gz", "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb"),
    ("osv-scanner", "2.7.0", "darwin-arm64", "https://github.com/google/osv-scanner/releases/download/v2.7.0/osv-scanner_darwin_arm64", "d4926ef8da134332543be276dff157e4c170f99ad91fc48e89448a8816bdf70b"),
    ("osv-scanner", "2.7.0", "darwin-x64", "https://github.com/google/osv-scanner/releases/download/v2.7.0/osv-scanner_darwin_amd64", "e82c5df7ab0a42dff29a1cd0660711447b56b4c0483ff66ad8634df10fd81a45"),
    ("osv-scanner", "2.7.0", "linux-arm64", "https://github.com/google/osv-scanner/releases/download/v2.7.0/osv-scanner_linux_arm64", "4061ef4c021ed3c47f203b818f337e351336cc5cd67c0274fc36bd1dfc728dd7"),
    ("osv-scanner", "2.7.0", "linux-x64", "https://github.com/google/osv-scanner/releases/download/v2.7.0/osv-scanner_linux_amd64", "160c6253e2922e9e0a6a9771c5c4c40a4b3c6090b34a3cdd5e80c96cc2e4218b"),
];

pub const BASELINE: &str = "security/gitleaks-baseline.json";

pub fn default_pins() -> Vec<ToolPin> {
    PINS.iter()
        .map(|(id, version, platform, url, sha256)| ToolPin {
            id: id.to_string(),
            version: version.to_string(),
            platform: platform.to_string(),
            url: url.to_string(),
            sha256: sha256.to_string(),
        })
        .collect()
}

pub fn platform() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    format!("{os}-{arch}")
}

pub fn tool_path(pin: &ToolPin, cache: &Path) -> PathBuf {
    cache
        .join("tools")
        .join(format!("{}-{}", pin.id, pin.version))
        .join(&pin.id)
}

/// The pinned binary for `id`, fetched into `<cache>/tools/` on a miss.
pub fn tool(pins: &[ToolPin], id: &str, platform: &str, cache: &Path) -> Result<PathBuf, String> {
    let pin = pins
        .iter()
        .find(|p| p.id == id && p.platform == platform)
        .ok_or_else(|| format!("enallagi pins no {id} for {platform}"))?;
    let bin = tool_path(pin, cache);
    // trusted once fetched, as ~/.cargo/bin is
    if bin.is_file() {
        return Ok(bin);
    }
    let io = |e: std::io::Error| format!("{id}: {e}");
    let tools = cache.join("tools");
    fs::create_dir_all(&tools).map_err(io)?;
    // dropped on every error path, so a bad download leaves nothing behind
    let tmp = tempfile::Builder::new()
        .prefix(".fetch-")
        .tempdir_in(&tools)
        .map_err(io)?;
    let file = tmp.path().join("download");
    run(Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&file)
        .arg(&pin.url))?;
    let got = crate::skills::sha256(&fs::read(&file).map_err(io)?);
    if got != pin.sha256 {
        return Err(format!(
            "{id} {}: sha256 {got} does not match the pinned {}; the download was deleted",
            pin.url, pin.sha256
        ));
    }
    let unpacked = if pin.url.ends_with(".tar.gz") {
        run(Command::new("tar")
            .arg("-xzf")
            .arg(&file)
            .arg("-C")
            .arg(tmp.path())
            .arg(id))?;
        tmp.path().join(id)
    } else {
        file
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&unpacked, fs::Permissions::from_mode(0o755)).map_err(io)?;
    }
    // a rename, so a concurrent fetch never sees a partial binary
    fs::create_dir_all(bin.parent().unwrap_or(&tools)).map_err(io)?;
    fs::rename(&unpacked, &bin).map_err(io)?;
    Ok(bin)
}

/// The pinned gitleaks run from `root` with `--redact`, `args` after `git`, its JSON report at
/// `report`. Exit 0 or 1 with a readable report is a result; anything else is an error.
pub fn gitleaks(
    root: &Path,
    dir: &str,
    cache: &Path,
    args: &[String],
    report: &Path,
) -> Result<Vec<serde_json::Value>, String> {
    let bin = tool(&default_pins(), "gitleaks", &platform(), cache)?;
    let at = |name: &str| crate::config::instance_path(root, dir, name);
    let out = Command::new(&bin)
        .current_dir(root)
        .arg("git")
        .args(args)
        .args(["--redact", "--report-format", "json", "--report-path"])
        .arg(report)
        .arg("--config")
        .arg(at("gitleaks.toml"))
        .arg("--gitleaks-ignore-path")
        .arg(at("gitleaksignore"))
        .output()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    read_report("gitleaks", &out, report)
}

// ponytail: the common lockfiles and manifests by name; one osv-scanner reads that is missing here goes unscanned
pub const LOCKFILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "composer.lock",
    "Gemfile.lock",
    "go.mod",
    "gradle.lockfile",
    "pom.xml",
    "mix.lock",
    "Pipfile.lock",
    "poetry.lock",
    "pdm.lock",
    "uv.lock",
    "requirements.txt",
    "pubspec.lock",
    "renv.lock",
    "packages.lock.json",
    "conan.lock",
];

/// `(package, version, advisory id)` for each advisory the pinned osv-scanner reports on `lockfile`,
/// which it reads by file name.
pub fn osv(
    root: &Path,
    dir: &str,
    cache: &Path,
    lockfile: &Path,
    report: &Path,
) -> Result<Vec<(String, String, String)>, String> {
    let bin = tool(&default_pins(), "osv-scanner", &platform(), cache)?;
    let mut cmd = Command::new(&bin);
    cmd.current_dir(root)
        .args(["scan", "source", "--lockfile"])
        .arg(lockfile)
        .args(["--format", "json", "--output"])
        .arg(report);
    let config = crate::config::instance_path(root, dir, "osv-scanner.toml");
    if config.is_file() {
        cmd.arg("--config").arg(config);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    let report: serde_json::Value = read_report("osv-scanner", &out, report)?;
    let mut found = Vec::new();
    let none = Vec::new();
    for result in report["results"].as_array().unwrap_or(&none) {
        for pkg in result["packages"].as_array().unwrap_or(&none) {
            let s = |k: &str| pkg["package"][k].as_str().unwrap_or("?").to_string();
            for vuln in pkg["vulnerabilities"].as_array().unwrap_or(&none) {
                let id = vuln["id"].as_str().unwrap_or("?").to_string();
                found.push((s("name"), s("version"), id));
            }
        }
    }
    Ok(found)
}

// exit 0 or 1 with a report that parses is a result; anything else names the exit and the stderr tail
fn read_report<T: serde::de::DeserializeOwned>(
    name: &str,
    out: &std::process::Output,
    report: &Path,
) -> Result<T, String> {
    let parsed = fs::read_to_string(report)
        .ok()
        .and_then(|t| serde_json::from_str::<T>(&t).ok());
    match (out.status.code(), parsed) {
        (Some(0 | 1), Some(parsed)) => Ok(parsed),
        (code, _) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
            Err(format!(
                "{name} exited {} with no readable report: {}",
                code.map_or("on a signal".to_string(), |c| c.to_string()),
                tail.into_iter().rev().collect::<Vec<_>>().join(" / ")
            ))
        }
    }
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("{cmd:?}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "{cmd:?} exited {}: {}",
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(url: &str, sha256: &str) -> ToolPin {
        ToolPin {
            id: "gitleaks".into(),
            version: "1.0.0".into(),
            platform: "test-os".into(),
            url: url.into(),
            sha256: sha256.into(),
        }
    }

    // a release-shaped tarball holding `gitleaks` and a README beside it
    fn tarball(dir: &Path) -> PathBuf {
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("gitleaks"), "#!/bin/sh\necho stub\n").unwrap();
        fs::write(src.join("README.md"), "readme\n").unwrap();
        let tar = dir.join("gitleaks.tar.gz");
        let ok = Command::new("tar")
            .arg("-czf")
            .arg(&tar)
            .arg("-C")
            .arg(&src)
            .args(["gitleaks", "README.md"])
            .status()
            .unwrap()
            .success();
        assert!(ok);
        tar
    }

    #[test]
    fn tool_fetch_checks_sha256() {
        let tmp = tempfile::tempdir().unwrap();
        let tar = tarball(tmp.path());
        let sha = crate::skills::sha256(&fs::read(&tar).unwrap());
        let pins = [pin(&format!("file://{}", tar.display()), &sha)];
        let cache = tmp.path().join("cache");
        let bin = tool(&pins, "gitleaks", "test-os", &cache).expect("fetch");
        assert_eq!(bin, cache.join("tools/gitleaks-1.0.0/gitleaks"));
        assert_eq!(fs::read_to_string(&bin).unwrap(), "#!/bin/sh\necho stub\n");
        // a hit never fetches again
        fs::remove_file(&tar).unwrap();
        assert_eq!(tool(&pins, "gitleaks", "test-os", &cache), Ok(bin));
    }

    #[test]
    fn tool_sha_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let tar = tarball(tmp.path());
        let pins = [pin(&format!("file://{}", tar.display()), &"0".repeat(64))];
        let cache = tmp.path().join("cache");
        let err = tool(&pins, "gitleaks", "test-os", &cache).unwrap_err();
        assert!(err.contains("sha256"), "{err}");
        let left: Vec<_> = fs::read_dir(cache.join("tools"))
            .map(|d| d.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn no_pin_names_platform() {
        let tmp = tempfile::tempdir().unwrap();
        let err = tool(&default_pins(), "gitleaks", "plan9-mips", tmp.path()).unwrap_err();
        assert!(err.contains("plan9-mips"), "{err}");
    }

    #[test]
    fn lock_pin_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let tar = tarball(tmp.path());
        let sha = crate::skills::sha256(&fs::read(&tar).unwrap());
        let repo = crate::fixture::Repo::new();
        let mut planted = pin(&format!("file://{}", tar.display()), &sha);
        planted.platform = platform();
        repo.write(
            ".enallagi/harness.lock",
            &format!(
                "version = 1\n\n[[tool]]\nid = \"gitleaks\"\nversion = \"1.0.0\"\nplatform = \"{}\"\nurl = \"{}\"\nsha256 = \"{}\"\n",
                planted.platform, planted.url, planted.sha256
            ),
        );
        let cache = tmp.path().join("cache");
        crate::fixture::stub_gitleaks(&cache, "[]", 0);
        let report = tmp.path().join("report.json");
        gitleaks(&repo.root, ".enallagi", &cache, &[], &report).expect("scan");
        assert!(!tool_path(&planted, &cache).exists());
        assert!(cache.join("gitleaks.argv").exists());
    }

    #[test]
    fn old_lock_round_trips() {
        let text = "version = 1\n\n[[skill]]\nid = \"tdd\"\nsource = \"path:vendor/tdd\"\nsha256 = \"ab\"\n";
        let repo = crate::fixture::Repo::new();
        repo.write(".enallagi/harness.lock", text);
        let lock = crate::skills::read_lock(&repo.root, ".enallagi").unwrap();
        crate::skills::write_lock(&repo.root, ".enallagi", &lock).unwrap();
        let again = fs::read_to_string(repo.root.join(".enallagi/harness.lock")).unwrap();
        assert_eq!(again, text);
    }

    // run by hand: the real scanners on a planted token and a known-vulnerable smallvec
    #[test]
    #[ignore = "downloads gitleaks and osv-scanner"]
    fn live_scan_planted_key() {
        use crate::events::{Log, Writer};
        let repo = crate::fixture::Repo::new();
        let cache = tempfile::tempdir().unwrap();
        let lock = |v: &str| {
            format!("version = 3\n\n[[package]]\nname = \"smallvec\"\nversion = \"{v}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n")
        };
        repo.write(".enallagi/.gitignore", "events.jsonl\n");
        repo.write(".enallagi/enallagi.toml", "[check]\ncommand = \"true\"\n");
        repo.write(".enallagi/gitleaks.toml", "[extend]\nuseDefault = true\n");
        repo.write("Cargo.lock", &lock("1.13.2"));
        let done = "## [T-001] bump\nstatus: done\nverdict: verified by verifier\n";
        repo.queue(".enallagi", done);
        repo.commit_all("setup");
        let baseline = repo.root.join(".enallagi").join(BASELINE);
        fs::create_dir_all(baseline.parent().unwrap()).unwrap();
        let old =
            gitleaks(&repo.root, ".enallagi", cache.path(), &[], &baseline).expect("baseline");
        println!("baseline: {} finding(s)", old.len());
        repo.commit_all("baseline");

        let cfg = crate::config::load(&repo.root).unwrap();
        let mut writer = Writer::new(Log::open(&repo.root.join(".enallagi")));
        let (mut warnings, mut halts, mut dry) = (Vec::new(), Vec::new(), 0);
        let mut gate = |base: &str| {
            let out = crate::gates::run(
                "security",
                &mut crate::gates::GateCtx {
                    root: &repo.root,
                    cfg: &cfg,
                    task: Some("T-001".into()),
                    iter_base: Some(base.into()),
                    state_base: Some(base.into()),
                    stage_output: String::new(),
                    events: &mut writer,
                    dry_run: false,
                    warnings: &mut warnings,
                    halts: &mut halts,
                    dry_rounds: &mut dry,
                    cache: cache.path().to_path_buf(),
                    sandbox: &crate::sandbox::Sandbox::start(&repo.root, &cfg),
                },
            );
            println!("pass={} halt={} {}", out.pass, out.halt, out.reason);
            out
        };

        let base = crate::git::head(&repo.root).unwrap();
        repo.write("Cargo.lock", &lock("1.6.0"));
        repo.commit_all("T-001 bump smallvec");
        let out = gate(&base);
        assert!(!out.pass && !out.halt && out.reason.contains("smallvec 1.6.0"));

        repo.queue(".enallagi", done);
        repo.commit_all("verdict");
        let base = crate::git::head(&repo.root).unwrap();
        // split so this file never holds a token-shaped string
        let token = format!("{}{}", "gh", "p_R8xQ2mZ7vK4pL9wN3tY6uH1jB5cF0dG2sA8eX");
        repo.write("src/token.txt", &format!("GITHUB_TOKEN={token}\n"));
        repo.commit_all("T-001 token");
        let out = gate(&base);
        assert!(out.halt && !out.reason.contains(&token), "{}", out.reason);
        let ticket = crate::tracker::Tracker::open(&repo.root, ".enallagi")
            .get("T-001")
            .unwrap()
            .unwrap();
        assert_eq!(
            crate::block::field(&ticket, "status").as_deref(),
            Some("ready-for-human")
        );

        // a redacted baseline holding the leak lets the same range pass
        let all = gitleaks(&repo.root, ".enallagi", cache.path(), &[], &baseline).expect("rescan");
        println!("rebaselined: {} finding(s)", all.len());
        repo.queue(".enallagi", done);
        assert!(gate(&base).pass);
    }

    #[test]
    fn default_pins_cover_platforms() {
        for p in ["darwin-arm64", "darwin-x64", "linux-x64", "linux-arm64"] {
            for id in ["gitleaks", "osv-scanner"] {
                assert!(
                    default_pins().iter().any(|t| t.id == id && t.platform == p),
                    "{id} {p}"
                );
            }
        }
    }
}
