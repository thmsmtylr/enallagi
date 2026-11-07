//! The security gate's scanners: pinned per platform in harness.lock, fetched once into the user cache.

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

// from each release's checksums file; bumping one is an operator edit to harness.lock
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
        .ok_or_else(|| format!("harness.lock pins no {id} for {platform}"))?;
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
    let pins = crate::skills::read_lock(root, dir)
        .map_err(|e| e.to_string())?
        .tool;
    let bin = tool(&pins, "gitleaks", &platform(), cache)?;
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
    let findings = fs::read_to_string(report)
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).ok());
    match (out.status.code(), findings) {
        (Some(0 | 1), Some(findings)) => Ok(findings),
        (code, _) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
            Err(format!(
                "gitleaks exited {} with no readable report: {}",
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
    fn old_lock_round_trips() {
        let text = "version = 1\n\n[[skill]]\nid = \"tdd\"\nsource = \"path:vendor/tdd\"\nsha256 = \"ab\"\n";
        let repo = crate::fixture::Repo::new();
        repo.write(".enallagi/harness.lock", text);
        let lock = crate::skills::read_lock(&repo.root, ".enallagi").unwrap();
        crate::skills::write_lock(&repo.root, ".enallagi", &lock).unwrap();
        let again = fs::read_to_string(repo.root.join(".enallagi/harness.lock")).unwrap();
        assert_eq!(again, text);
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
