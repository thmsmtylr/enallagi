//! Ported from `selftest.sh:597-667`: the eval runner and the write-path
//! gate on a candidate rule, driven with a stub agent so no real agent is
//! spawned.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A copy of this package's own install surface -- the parts `install.sh`
/// reads -- so a test's `pkg` is a real installable package without
/// touching the checked-out repo.
fn package_copy() -> tempfile::TempDir {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("locate repo root from CARGO_MANIFEST_DIR");
    let dir = tempfile::tempdir().expect("tempdir");
    for name in [
        "install.sh",
        "harness",
        "templates",
        "roles",
        "skills",
        "harness.default.json",
    ] {
        let status = Command::new("cp")
            .arg("-R")
            .arg(repo_root.join(name))
            .arg(dir.path().join(name))
            .status()
            .unwrap_or_else(|e| panic!("spawn cp {name}: {e}"));
        assert!(status.success(), "cp -R {name} failed");
    }
    fs::create_dir_all(dir.path().join("evals")).expect("mkdir evals");
    fs::copy(
        repo_root.join("evals/README.md"),
        dir.path().join("evals/README.md"),
    )
    .expect("copy evals/README.md");
    dir
}

fn write_exec(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, format!("#!/usr/bin/env bash\n{body}")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Writes `<pkg>/evals/<name>/{prompt.txt,setup.sh,assert.sh}`, and
/// `ablate.sh` when given.
fn write_eval(
    pkg: &Path,
    name: &str,
    prompt: &str,
    setup: &str,
    assert: &str,
    ablate: Option<&str>,
) {
    let dir = pkg.join("evals").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("prompt.txt"), prompt).unwrap();
    write_exec(&dir.join("setup.sh"), setup);
    write_exec(&dir.join("assert.sh"), assert);
    if let Some(body) = ablate {
        write_exec(&dir.join("ablate.sh"), body);
    }
}

struct Eval {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run_harness_eval(pkg: &Path, gate: Option<&str>, agent: Option<&str>, names: &[&str]) -> Eval {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_harness"));
    cmd.arg("eval");
    if let Some(name) = gate {
        cmd.args(["--gate", name]);
    }
    cmd.args(names).current_dir(pkg);
    match agent {
        Some(a) => {
            cmd.env("EVAL_AGENT", a);
        }
        None => {
            cmd.env_remove("EVAL_AGENT");
        }
    }
    let out = cmd.output().expect("run harness eval");
    Eval {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn run_eval(pkg: &Path, agent: &str, names: &[&str]) -> Eval {
    run_harness_eval(pkg, None, Some(agent), names)
}

fn run_gate(pkg: &Path, agent: Option<&str>, name: &str) -> Eval {
    run_harness_eval(pkg, Some(name), agent, &[])
}

fn last_line(s: &str) -> &str {
    s.lines().next_back().unwrap_or("")
}

#[test]
fn the_eval_runner_passes_a_role_that_obeys_its_rule() {
    let pkg = package_copy();
    write_eval(
        pkg.path(),
        "case",
        "do the thing",
        "echo pending >state.txt",
        r#"[ "$(cat state.txt)" = done ]"#,
        None,
    );
    let stub = pkg.path().join("obeys.sh");
    write_exec(&stub, "echo done >state.txt\n");

    let r = run_eval(
        pkg.path(),
        &format!("{} {{prompt}}", stub.display()),
        &["case"],
    );
    assert_eq!(r.code, 0, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(last_line(&r.stdout), "EVAL case PASS");
}

#[test]
fn the_eval_runner_fails_a_role_that_breaks_its_rule() {
    let pkg = package_copy();
    write_eval(
        pkg.path(),
        "case",
        "do the thing",
        "echo pending >state.txt",
        r#"[ "$(cat state.txt)" = done ]"#,
        None,
    );
    let stub = pkg.path().join("breaks.sh");
    write_exec(&stub, "echo wrong >state.txt\n");

    let r = run_eval(
        pkg.path(),
        &format!("{} {{prompt}}", stub.display()),
        &["case"],
    );
    assert_eq!(r.code, 1);
    assert_eq!(last_line(&r.stdout), "EVAL case FAIL");
}

#[test]
fn with_no_agent_configured_the_evals_refuse_rather_than_report() {
    let pkg = package_copy();
    write_eval(pkg.path(), "case", "do the thing", "true", "true", None);
    // A preset that resolves to nothing: agent resolution fails closed, the
    // same shape as a fresh checkout with no agentCommand configured.
    fs::write(
        pkg.path().join("harness.toml"),
        "[agent]\npreset = \"doesnotexist\"\n",
    )
    .unwrap();

    let r = run_harness_eval(pkg.path(), None, None, &["case"]);
    assert_eq!(r.code, 2, "stdout={} stderr={}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("evals: no agent configured"),
        "stderr={}",
        r.stderr
    );
    assert!(
        r.stderr
            .contains("evals: refusing to report a result for something that was never run."),
        "stderr={}",
        r.stderr
    );
}

#[test]
fn a_directory_with_no_evals_is_refused() {
    let pkg = package_copy();
    // package_copy's evals/ holds only README.md: a fresh install, no eval directories.
    let r = run_eval(pkg.path(), "/bin/true {prompt}", &[]);
    assert_eq!(r.code, 2, "stdout={} stderr={}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("evals: nothing under") && r.stderr.contains("to run"),
        "stderr={}",
        r.stderr
    );
}

#[test]
fn a_fixture_that_cannot_be_built_is_error_never_fail() {
    let pkg = package_copy();
    // install.sh always fails: the fixture can never be built.
    write_exec(&pkg.path().join("install.sh"), "exit 1\n");
    write_eval(pkg.path(), "case", "irrelevant", "true", "true", None);

    let r = run_eval(pkg.path(), "/bin/true {prompt}", &["case"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        last_line(&r.stdout),
        "EVAL case ERROR (the fixture could not be built — nothing was measured)"
    );
}

/// Sets up `pkg` with a "rule" eval (gated) and an "other" eval (used to
/// check for regressions). The rule's presence is a marker line in a role
/// file `install.sh` copies into the fixture; `ablate.sh` removes it. The
/// single stub agent tells which eval it was called for from the prompt
/// text, and for "rule" it passes only when the marker is present.
fn write_gate_fixture(pkg: &Path, other_assert: &str) -> PathBuf {
    fs::write(pkg.join("roles/case.md"), "MARKER: the rule\n").unwrap();
    write_eval(
        pkg,
        "rule",
        "RULE",
        "true",
        r#"[ -f outcome.txt ] && [ "$(cat outcome.txt)" = PASS ]"#,
        Some(
            "sed -i.bak '/MARKER: the rule/d' .harness/roles/case.md\nrm -f .harness/roles/case.md.bak\n",
        ),
    );
    write_eval(pkg, "other", "OTHER", "true", other_assert, None);

    let stub = pkg.join("stub.sh");
    write_exec(
        &stub,
        "case \"$1\" in\n\
         RULE) grep -q 'MARKER: the rule' .harness/roles/case.md 2>/dev/null && echo PASS >outcome.txt ;;\n\
         OTHER) : ;;\n\
         esac\n\
         exit 0\n",
    );
    stub
}

#[test]
fn a_candidate_rule_that_does_not_fix_its_case_is_rejected() {
    let pkg = package_copy();
    write_eval(pkg.path(), "rule", "RULE", "true", "false", Some("true\n"));
    let stub = pkg.path().join("stub.sh");
    write_exec(&stub, ": # never passes\n");

    let r = run_gate(
        pkg.path(),
        Some(&format!("{} {{prompt}}", stub.display())),
        "rule",
    );
    assert_eq!(r.code, 1, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(
        last_line(&r.stdout),
        "GATE rule REJECT the rule does not fix the case it came from (its eval fails with the rule in place)"
    );
}

#[test]
fn a_candidate_rule_whose_case_passes_without_it_is_rejected() {
    let pkg = package_copy();
    write_eval(pkg.path(), "rule", "RULE", "true", "true", Some("true\n"));
    let stub = pkg.path().join("stub.sh");
    write_exec(&stub, ": # always passes, ablated or not\n");

    let r = run_gate(
        pkg.path(),
        Some(&format!("{} {{prompt}}", stub.display())),
        "rule",
    );
    assert_eq!(r.code, 1, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(
        last_line(&r.stdout),
        "GATE rule REJECT the case passes with the rule ablated, so the rule changed no outcome"
    );
}

#[test]
fn a_candidate_rule_that_regresses_another_eval_is_rejected() {
    let pkg = package_copy();
    let stub = write_gate_fixture(pkg.path(), "false");

    let r = run_gate(
        pkg.path(),
        Some(&format!("{} {{prompt}}", stub.display())),
        "rule",
    );
    assert_eq!(r.code, 1, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(
        last_line(&r.stdout),
        "GATE rule REJECT it regresses evals that were passing: other"
    );
}

#[test]
fn a_candidate_rule_that_fixes_its_case_and_regresses_nothing_is_accepted() {
    let pkg = package_copy();
    let stub = write_gate_fixture(pkg.path(), "true");

    let r = run_gate(
        pkg.path(),
        Some(&format!("{} {{prompt}}", stub.display())),
        "rule",
    );
    assert_eq!(r.code, 0, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(
        last_line(&r.stdout),
        "GATE rule ACCEPT fixes its case, fails without itself, regresses nothing"
    );
}
