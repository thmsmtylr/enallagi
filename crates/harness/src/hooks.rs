//! The agent lifecycle hooks (PreToolUse, Stop, UserPromptSubmit), all fail-open.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config;
use crate::gates::{self, CheckReport};
use crate::queue;
use crate::skills;

// this can be bypassed via Bash sed -i or a heredoc: the real authority is the verifier reading the diff
pub fn immutable(root: &Path, input: &str) -> (i32, String) {
    let Some(target) = tool_input_path(root, input) else {
        return (0, String::new());
    };

    if let Some(hit) = hashed_hit(root, &target) {
        return (
            2,
            format!(
                "immutable: {hit} is covered by test-hashes.json (SPEC.md §0.2, tests-immutable \
                 and harness-immutable). This edit is refused. If the change is genuinely \
                 needed, stop the task and write it into the task notes."
            ),
        );
    }
    if let Some(hit) = locked_hit(root, &target) {
        return (
            2,
            format!(
                "immutable: {hit} is a locked skill, covered by harness.lock. This edit is \
                 refused. Re-resolve the lock deliberately (`harness skills sync`), not through \
                 the edit tool."
            ),
        );
    }
    (0, String::new())
}

fn tool_input_path(root: &Path, input: &str) -> Option<PathBuf> {
    let v: serde_json::Value = serde_json::from_str(input).ok()?;
    let path = v.get("tool_input")?.get("file_path")?.as_str()?;
    if path.is_empty() {
        return None;
    }
    Some(normalize(root, path))
}

fn normalize(root: &Path, rel: &str) -> PathBuf {
    let joined = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        root.join(rel)
    };
    realpath_like(&joined)
}

// resolves symlinks on the longest existing ancestor only: a nonexistent tail (a file about to be
// created) has nothing on disk to canonicalize, so it is appended to the resolved ancestor as-is
fn realpath_like(p: &Path) -> PathBuf {
    let normalized = lexical_normalize(p);
    let mut existing = normalized.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        tail.push(name.to_os_string());
        existing = existing.parent().map(Path::to_path_buf).unwrap_or_default();
    }
    let mut resolved = std::fs::canonicalize(&existing).unwrap_or(existing);
    for comp in tail.into_iter().rev() {
        resolved.push(comp);
    }
    resolved
}

fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn hashed_hit(root: &Path, target: &Path) -> Option<String> {
    let hashes_path = root.join("test-hashes.json");
    if normalize(root, "test-hashes.json") == *target {
        if hashes_path.is_file() {
            return Some("test-hashes.json (the reference itself)".to_string());
        }
        return None;
    }
    let text = std::fs::read_to_string(&hashes_path).ok()?;
    let keys: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).ok()?;

    if keys.contains_key("package.json#scripts") {
        let mut covered = vec![root.join("package.json")];
        if let Ok(pkg_text) = std::fs::read_to_string(root.join("package.json")) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&pkg_text) {
                if let Some(workspaces) = pkg.get("workspaces").and_then(|w| w.as_array()) {
                    for pattern in workspaces.iter().filter_map(|p| p.as_str()) {
                        covered.extend(glob_package_jsons(root, pattern));
                    }
                }
            }
        }
        for found in covered {
            if realpath_like(&found) == *target {
                let rel = found.strip_prefix(root).unwrap_or(&found);
                return Some(format!(
                    "{} (its scripts, via package.json#scripts)",
                    rel.display()
                ));
            }
        }
    }

    for key in keys.keys() {
        if key.contains('#') {
            continue;
        }
        if normalize(root, key) == *target {
            return Some(key.clone());
        }
    }
    None
}

fn glob_package_jsons(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let full = format!("{}/package.json", pattern.trim_end_matches('/'));
    let mut current = vec![root.to_path_buf()];
    for comp in full.split('/') {
        if comp.is_empty() {
            continue;
        }
        let mut next = Vec::new();
        let is_wild = comp.contains(['*', '?', '[']);
        for base in &current {
            if is_wild {
                let Ok(matcher) = globset::Glob::new(comp) else {
                    continue;
                };
                let matcher = matcher.compile_matcher();
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        if matcher.is_match(entry.file_name()) {
                            next.push(entry.path());
                        }
                    }
                }
            } else {
                let candidate = base.join(comp);
                if candidate.exists() {
                    next.push(candidate);
                }
            }
        }
        current = next;
    }
    current.sort();
    current
}

fn locked_hit(root: &Path, target: &Path) -> Option<String> {
    let lock_path = root.join("harness.lock");
    if normalize(root, "harness.lock") == *target {
        if lock_path.is_file() {
            return Some("harness.lock (the reference itself)".to_string());
        }
        return None;
    }
    if !lock_path.is_file() {
        return None;
    }
    let lock = skills::read_lock(root).ok()?;
    if lock.skill.is_empty() {
        return None;
    }
    let cfg = config::load(root).ok()?;
    let skills_dir = gates::skills_dir_for(&cfg);
    for entry in &lock.skill {
        let dir = normalize(root, &format!("{skills_dir}/{}", entry.id));
        if target.starts_with(&dir) {
            let rel = target.strip_prefix(root).unwrap_or(target);
            return Some(format!("{} (locked skill `{}`)", rel.display(), entry.id));
        }
    }
    None
}

pub fn one_writer(root: &Path, _input: &str) -> (i32, String) {
    let Ok(cfg) = config::load(root) else {
        return (0, String::new());
    };
    if crate::archive::loop_live(root, &cfg.layout.harness_dir).is_none() {
        return (0, String::new());
    }

    let text = queue::Queue {
        path: root.join("TASKS.md"),
    }
    .read()
    .unwrap_or_default();
    let task = queue::parse(&text).ok().and_then(|blocks| {
        queue::ids_at(&blocks, "review")
            .into_iter()
            .next()
            .or_else(|| queue::ready_unattended(&blocks))
    });
    let named = task.map(|t| format!(" It is on {t}.")).unwrap_or_default();
    (
        2,
        format!(
            "one-writer: a lane is live in this checkout and this session is not it.{named} One \
             checkout is one writer (LEARNINGS.md): two sessions editing the same file ship two \
             versions of it. This edit is refused. `touch STOP` halts the loop before its next \
             stage; when it is idle every write is allowed again."
        ),
    )
}

pub fn verify_done(root: &Path, input: &str) -> (i32, String) {
    if stop_hook_active(input) {
        return (0, String::new());
    }
    let Ok(cfg) = config::load(root) else {
        return (0, String::new());
    };
    let check_covered = root
        .join(&cfg.layout.harness_dir)
        .join("hooks")
        .join("check-covered.sh");
    if is_executable(&check_covered) {
        return run_check_covered(&check_covered, input);
    }
    render_check_report(&gates::check_delta(root, &cfg, false))
}

fn stop_hook_active(input: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(input)
        .ok()
        .and_then(|v| v.get("stop_hook_active").and_then(|b| b.as_bool()))
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn run_check_covered(path: &Path, input: &str) -> (i32, String) {
    use std::io::Write;
    let mut child = match Command::new("sh")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                2,
                format!("verify-done: could not run {}: {e}", path.display()),
            )
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    match child.wait_with_output() {
        Ok(out) => (
            out.status.code().unwrap_or(2),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ),
        Err(e) => (2, format!("verify-done: {e}")),
    }
}

fn render_check_report(report: &CheckReport) -> (i32, String) {
    if !report.red {
        return (0, String::new());
    }
    if report.unnamed {
        return (
            2,
            format!(
                "check RED, and no failure could be named — nothing to forgive:\n{}",
                tail_lines(&report.output, 30)
            ),
        );
    }
    let forgiven_block = (!report.forgiven.is_empty()).then(|| {
        format!(
            "check-gate: forgiven by .check-baseline:\n{}",
            indented(&report.forgiven)
        )
    });
    if report.unforgiven.is_empty() {
        return (0, forgiven_block.unwrap_or_default());
    }
    let mut msg = String::new();
    if let Some(block) = forgiven_block {
        msg.push_str(&block);
        msg.push('\n');
    }
    msg.push_str("check RED, not on the baseline:\n");
    msg.push_str(&indented(&report.unforgiven));
    msg.push('\n');
    msg.push_str(&tail_lines(&report.output, 30));
    (2, msg)
}

fn indented(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("  {n}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

pub fn skills_contract(root: &Path) -> (i32, String) {
    let skills = config::load(root).map(|c| c.skill).unwrap_or_default();
    if !skills.is_empty() {
        println!("The skills this harness relies on, and the gate that enforces each:");
        for s in &skills {
            println!("- {} — {} (gate: {})", s.id, s.why, s.gate);
        }
    }
    (0, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::fixture::Repo;
    use std::process::Command as StdCommand;

    fn cfg_or_default(root: &Path) -> Config {
        config::load(root).unwrap_or_default()
    }

    struct Reaper(std::process::Child);

    impl Drop for Reaper {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn input(path: &str) -> String {
        format!(r#"{{"tool_input":{{"file_path":"{path}"}}}}"#)
    }

    #[test]
    fn immutable_refuses_an_edit_to_a_hashed_file() {
        let r = Repo::new();
        r.write("src/a.ts", "export const a = 1\n");
        r.write("test-hashes.json", r#"{"src/a.ts":"deadbeef"}"#);
        let (code, msg) = immutable(&r.root, &input("src/a.ts"));
        assert_eq!(code, 2);
        assert!(msg.contains("covered by test-hashes.json"), "{msg}");
    }

    #[test]
    fn immutable_refuses_the_reference_itself() {
        let r = Repo::new();
        r.write("test-hashes.json", r#"{"src/a.ts":"deadbeef"}"#);
        let (code, msg) = immutable(&r.root, &input("test-hashes.json"));
        assert_eq!(code, 2);
        assert!(msg.contains("the reference itself"), "{msg}");
    }

    #[test]
    fn immutable_refuses_a_locked_skill_file() {
        let r = Repo::new();
        r.write(
            "harness.lock",
            "version = 1\n\n[[skill]]\nid = \"tdd\"\nsource = \"path:skills/tdd\"\nsha256 = \"ab\"\n",
        );
        let (code, msg) = immutable(&r.root, &input(".claude/skills/tdd/SKILL.md"));
        assert_eq!(code, 2, "{msg}");
        assert!(msg.contains("harness.lock"), "{msg}");
    }

    #[test]
    fn immutable_allows_an_uncovered_file() {
        let r = Repo::new();
        r.write("test-hashes.json", r#"{"src/a.ts":"deadbeef"}"#);
        let (code, msg) = immutable(&r.root, &input("src/b.ts"));
        assert_eq!(code, 0, "{msg}");
    }

    #[test]
    #[cfg(unix)]
    fn immutable_refuses_a_symlinked_alias_of_a_hashed_file() {
        let r = Repo::new();
        r.write("src/a.ts", "export const a = 1\n");
        r.write("test-hashes.json", r#"{"src/a.ts":"deadbeef"}"#);
        std::os::unix::fs::symlink(r.root.join("src/a.ts"), r.root.join("src/b.ts"))
            .expect("symlink");
        let (code, msg) = immutable(&r.root, &input("src/b.ts"));
        assert_eq!(code, 2, "{msg}");
        assert!(msg.contains("covered by test-hashes.json"), "{msg}");
    }

    #[test]
    #[cfg(unix)]
    fn immutable_refuses_a_symlinked_directory_containing_a_locked_skill() {
        let r = Repo::new();
        r.write(".claude/skills/tdd/SKILL.md", "# tdd\n");
        r.write(
            "harness.lock",
            "version = 1\n\n[[skill]]\nid = \"tdd\"\nsource = \"path:skills/tdd\"\nsha256 = \"ab\"\n",
        );
        std::os::unix::fs::symlink(r.root.join(".claude/skills/tdd"), r.root.join("alias"))
            .expect("symlink");
        let (code, msg) = immutable(&r.root, &input("alias/SKILL.md"));
        assert_eq!(code, 2, "{msg}");
        assert!(msg.contains("harness.lock"), "{msg}");
    }

    #[test]
    fn immutable_normalizes_a_nonexistent_target_under_an_existing_directory() {
        let r = Repo::new();
        r.write("test-hashes.json", r#"{"src/a.ts":"deadbeef"}"#);
        let (code, msg) = immutable(&r.root, &input("src/does-not-exist-yet.ts"));
        assert_eq!(code, 0, "{msg}");
    }

    #[test]
    fn immutable_refuses_a_workspace_package_json_when_scripts_are_hashed() {
        let r = Repo::new();
        r.write(
            "package.json",
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        );
        r.write("packages/foo/package.json", r#"{"name":"foo"}"#);
        r.write("test-hashes.json", r#"{"package.json#scripts":"deadbeef"}"#);
        let (code, msg) = immutable(&r.root, &input("packages/foo/package.json"));
        assert_eq!(code, 2, "{msg}");
        assert!(msg.contains("package.json#scripts"), "{msg}");

        let (code, _) = immutable(&r.root, &input("package.json"));
        assert_eq!(code, 2);

        r.write("packages/bar/package.json", r#"{"name":"bar"}"#);
        let (code, _) = immutable(&r.root, &input("packages/bar/other.ts"));
        assert_eq!(code, 0);
    }

    #[test]
    fn a_write_from_a_session_that_is_not_the_live_lane_is_refused() {
        let r = Repo::new();
        r.write(
            "TASKS.md",
            "## [T-001] x\nstatus: review\nblockedBy: none\n",
        );
        r.commit_all("seed");
        let child = StdCommand::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        r.write(".harness/loop.pid", &child.id().to_string());
        let _reaper = Reaper(child);

        let (code, msg) = one_writer(&r.root, &input("TASKS.md"));
        assert_eq!(code, 2, "{msg}");
        assert!(msg.contains("one-writer:"), "{msg}");
        assert!(msg.contains("T-001"), "{msg}");
    }

    #[test]
    fn a_write_from_under_the_loop_itself_is_allowed() {
        let r = Repo::new();
        r.write(".harness/loop.pid", &std::process::id().to_string());
        let (code, msg) = one_writer(&r.root, &input("TASKS.md"));
        assert_eq!(code, 0, "{msg}");
    }

    #[test]
    fn a_pid_file_left_by_a_loop_that_is_gone_is_not_a_live_loop() {
        let r = Repo::new();
        let child = StdCommand::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();
        drop(Reaper(child)); // kills and waits immediately: the pid file must name a dead loop
        r.write(".harness/loop.pid", &pid.to_string());

        let (code, msg) = one_writer(&r.root, &input("TASKS.md"));
        assert_eq!(code, 0, "{msg}");
    }

    #[test]
    fn verify_done_short_circuits_on_stop_hook_active() {
        let r = Repo::new();
        let (code, msg) = verify_done(&r.root, r#"{"stop_hook_active":true}"#);
        assert_eq!(code, 0);
        assert_eq!(msg, "");
    }

    #[test]
    fn verify_done_runs_check_delta_otherwise() {
        let r = Repo::new();
        let cmd = r.stub_check("echo '(fail) alpha'\nexit 1\n");
        r.write(
            "harness.toml",
            &format!("[check]\ncommand = \"{cmd}\"\nfail_name = '\\(fail\\) (.+)$'\n"),
        );
        let (code, msg) = verify_done(&r.root, "{}");
        assert_eq!(code, 2, "{msg}");
        assert!(msg.contains("check RED, not on the baseline:"), "{msg}");
        assert!(msg.contains("alpha"), "{msg}");

        let cmd = r.stub_check("exit 0\n");
        r.write(
            "harness.toml",
            &format!("[check]\ncommand = \"{cmd}\"\nfail_name = '\\(fail\\) (.+)$'\n"),
        );
        let (code, msg) = verify_done(&r.root, "{}");
        assert_eq!(code, 0, "{msg}");
        assert_eq!(msg, "");
    }

    #[test]
    fn the_skills_contract_lists_every_declared_skill() {
        let r = Repo::new();
        r.write(
            "harness.toml",
            "[[skill]]\nid = \"tdd\"\nsource = \"path:skills/tdd\"\npath = \"skills/tdd\"\ngate = \"verdict\"\nwhy = \"forces a failing test first\"\n",
        );
        let (code, _) = skills_contract(&r.root);
        assert_eq!(code, 0);
        let cfg = cfg_or_default(&r.root);
        assert_eq!(cfg.skill.len(), 1);
        assert_eq!(cfg.skill[0].id, "tdd");
        assert_eq!(cfg.skill[0].gate, "verdict");
    }

    #[test]
    fn skills_contract_prints_nothing_for_no_declared_skills() {
        let r = Repo::new();
        let (code, msg) = skills_contract(&r.root);
        assert_eq!(code, 0);
        assert_eq!(msg, "");
    }
}
