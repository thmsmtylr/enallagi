//! The eval runner and the write-path gate on a candidate rule, driven with a stub agent so no real agent is spawned.

use std::fs;
use std::path::{Path, PathBuf};

fn package() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("evals")).expect("mkdir evals");
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
    run_harness_eval_in(pkg, gate, agent, names, None)
}

fn run_harness_eval_in(
    pkg: &Path,
    gate: Option<&str>,
    agent: Option<&str>,
    names: &[&str],
    tmpdir: Option<&Path>,
) -> Eval {
    let mut cmd = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"));
    if let Some(t) = tmpdir {
        cmd.env("TMPDIR", t);
    }
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
    let out = cmd.output().expect("run enallagi eval");
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
    let pkg = package();
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
fn an_eval_script_calls_the_installing_binary() {
    let pkg = package();
    write_eval(
        pkg.path(),
        "case",
        "do the thing",
        "command -v enallagi >bin.txt",
        &format!(
            r#"[ "$(cat bin.txt)" -ef "{}" ]"#,
            env!("CARGO_BIN_EXE_enallagi")
        ),
        None,
    );

    let r = run_eval(pkg.path(), "true", &["case"]);
    assert_eq!(r.code, 0, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(last_line(&r.stdout), "EVAL case PASS");
}

#[test]
fn the_eval_runner_fails_a_role_that_breaks_its_rule() {
    let pkg = package();
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
fn an_eval_argv_carries_dir_and_context() {
    let pkg = package();
    write_eval(
        pkg.path(),
        "case",
        "do the thing",
        "true",
        r#"[ "$(cat args.txt)" = ".enallagi .enallagi/AGENTS.md" ]"#,
        None,
    );
    let stub = pkg.path().join("args.sh");
    write_exec(&stub, "printf '%s %s\\n' \"$1\" \"$2\" >args.txt\n");

    let r = run_eval(
        pkg.path(),
        &format!(
            "{} {{harness_dir}} {{context_file}} {{prompt}}",
            stub.display()
        ),
        &["case"],
    );
    assert_eq!(r.code, 0, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(last_line(&r.stdout), "EVAL case PASS");
}

#[test]
fn no_agent_configured_refuses_the_evals() {
    let pkg = package();
    write_eval(pkg.path(), "case", "do the thing", "true", "true", None);
    // a preset that resolves to nothing: agent resolution fails closed
    fs::write(
        pkg.path().join("enallagi.toml"),
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
    let pkg = package();
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
    let pkg = package();
    write_eval(pkg.path(), "case", "irrelevant", "true", "true", None);

    // TMPDIR names nothing, so the throwaway repo can never be created
    let r = run_harness_eval_in(
        pkg.path(),
        None,
        Some("/bin/true {prompt}"),
        &["case"],
        Some(&pkg.path().join("no-such-tmpdir")),
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        last_line(&r.stdout),
        "EVAL case ERROR (the fixture could not be built — nothing was measured)"
    );
}

// RULE is a real line of the verifier prompt enallagi init writes; ablate.sh deletes it
const RULE: &str = "git status --porcelain";

fn write_gate_fixture(pkg: &Path, other_assert: &str) -> PathBuf {
    write_eval(
        pkg,
        "rule",
        "RULE",
        "true",
        r#"[ -f outcome.txt ] && [ "$(cat outcome.txt)" = PASS ]"#,
        Some(&format!(
            "sed -i.bak '/{RULE}/d' .enallagi/roles/verifier.md\nrm -f .enallagi/roles/verifier.md.bak\n"
        )),
    );
    write_eval(pkg, "other", "OTHER", "true", other_assert, None);

    let stub = pkg.join("stub.sh");
    write_exec(
        &stub,
        &format!(
            "case \"$1\" in\n\
             RULE) grep -q '{RULE}' .enallagi/roles/verifier.md 2>/dev/null && echo PASS >outcome.txt ;;\n\
             OTHER) : ;;\n\
             esac\n\
             exit 0\n"
        ),
    );
    stub
}

#[test]
fn a_rule_that_misses_its_case_is_rejected() {
    let pkg = package();
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
fn a_rule_that_changes_nothing_is_rejected() {
    let pkg = package();
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
fn a_rule_that_regresses_an_eval_is_rejected() {
    let pkg = package();
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
fn a_rule_that_only_fixes_its_case_is_accepted() {
    let pkg = package();
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

#[test]
fn an_eval_with_no_prompt_needs_no_agent() {
    let pkg = package();
    let dir = pkg.path().join("evals").join("case");
    fs::create_dir_all(&dir).unwrap();
    write_exec(&dir.join("setup.sh"), "echo done >state.txt\n");
    write_exec(&dir.join("assert.sh"), r#"[ "$(cat state.txt)" = done ]"#);
    // a preset that resolves to nothing: an eval carrying a prompt would be refused here
    fs::write(
        pkg.path().join("enallagi.toml"),
        "[agent]\npreset = \"doesnotexist\"\n",
    )
    .unwrap();

    let r = run_harness_eval(pkg.path(), None, None, &["case"]);
    assert_eq!(r.code, 0, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(last_line(&r.stdout), "EVAL case PASS");
}

#[test]
fn a_prompt_that_is_not_a_file_is_a_fixture_error() {
    let pkg = package();
    let dir = pkg.path().join("evals").join("case");
    write_exec(&dir.join("setup.sh"), "true");
    write_exec(&dir.join("assert.sh"), "true");
    fs::create_dir_all(dir.join("prompt.txt")).unwrap();

    let r = run_eval(pkg.path(), "/bin/true {prompt}", &["case"]);
    assert_eq!(r.code, 1, "stdout={} stderr={}", r.stdout, r.stderr);
    assert_eq!(
        last_line(&r.stdout),
        "EVAL case ERROR (the fixture could not be built — nothing was measured)"
    );
}
