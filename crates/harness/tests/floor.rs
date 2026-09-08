//! The integration floor: every `selftest.sh` assertion that Tasks 3-16 did
//! not already cover, one test per assertion and named after it.
//!
//! Three kinds live here. The ones about an installed tree reproduce the
//! selftest's setup through `fixture::Repo` + `init::install` + the binary.
//! The ones about shipped content -- the workflow, the role prompts, the
//! prose -- read the files directly. The ones about git history run
//! `docs/bootstrap.sh` against a history built here.
//!
//! Every scan asserts FIRING as well as passing: a grep that cannot report is
//! not a check, and "it found nothing" on a broken scanner reads the same as a
//! clean tree.

use harness::fixture::Repo;
use harness::init::{self, InitOpts};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The checkout this crate lives in.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("locate the repo root from CARGO_MANIFEST_DIR")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn re(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("compile pattern")
}

/// Every file under `dir`, relative to it, `.git` excluded.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(dir) {
                out.push(rel.to_path_buf());
            }
        }
    }
    out.sort();
    out
}

/// Every `.rs` under `crates/harness/src`, truncated at its `#[cfg(test)]`:
/// a fixture string in a test module is not the launcher parsing anything.
fn crate_sources() -> Vec<(PathBuf, String)> {
    let src = repo_root().join("crates/harness/src");
    walk(&src)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .map(|rel| {
            let text = read(&src.join(&rel));
            let cut = text.find("#[cfg(test)]").unwrap_or(text.len());
            (rel, text[..cut].to_string())
        })
        .collect()
}

/// Runs `program` with `cwd` and `HARNESS_BIN` pointed at the test binary, so
/// nothing here builds a release binary or reads one off PATH.
fn script(program: &Path, cwd: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env("HARNESS_BIN", env!("CARGO_BIN_EXE_harness"))
        .output()
        .unwrap_or_else(|e| panic!("run {}: {e}", program.display()));
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn harness(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run harness");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// A throwaway git repo with `git init`, one commit per subject, oldest first.
fn history(subjects: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    git(&["config", "commit.gpgsign", "false"]);
    for subject in subjects {
        fs::write(dir.path().join("f"), subject).expect("write");
        git(&["add", "f"]);
        git(&["commit", "-q", "-m", subject]);
    }
    dir
}

#[test]
fn no_installed_script_hardcodes_a_vendor_path_or_process() {
    // The core install names no vendor and no vendor directory. Prose may name
    // a vendor among others; an executable path or a process guard may not.
    let vendor = re(r"\.claude/(hooks|agents)|pgrep -f \.claude|spin [^|]*\bclaude\b");
    assert!(
        vendor.is_match(r#"pgrep -f .claude"#),
        "the scan cannot report"
    );

    let repo = Repo::new();
    init::install(
        &repo.root,
        &InitOpts {
            adapter: Some("claude".to_string()),
            ..InitOpts::default()
        },
    )
    .expect("install");

    let dir = repo.root.join(".harness");
    for rel in walk(&dir) {
        let Ok(text) = fs::read_to_string(dir.join(&rel)) else {
            continue;
        };
        assert!(
            !vendor.is_match(&text),
            ".harness/{}: {:?}",
            rel.display(),
            vendor.find(&text).map(|m| m.as_str())
        );
    }
}

#[test]
fn the_launcher_parses_no_task_blocks_itself() {
    // `selftest.sh` counted task-block parsing in `loop.sh` and required 0:
    // one parser, and the launcher holds none of its own. Here the parser is
    // `queue::match_heading`, and no other module may hold a second.
    let heading = re(r"## \[");
    assert!(
        heading.is_match("## [T-001] a task"),
        "the scan cannot report"
    );

    let holders: Vec<String> = crate_sources()
        .into_iter()
        .filter(|(_, text)| heading.is_match(text))
        .map(|(rel, _)| rel.display().to_string())
        .collect();
    assert_eq!(holders, vec!["queue.rs".to_string()]);
}

#[test]
fn the_write_path_gate_installs_where_the_rules_are_written() {
    // `selftest.sh` asserted `[ -x evals/run.sh ]` in the installed repo. The
    // runner is now the binary, so what has to land in the target repo is the
    // eval directory and a gate that is reachable from it.
    let repo = Repo::new();
    repo.init_harness("");
    let readme = read(&repo.root.join("evals/README.md"));
    assert!(readme.contains("harness eval --gate"), "{readme:.400}");

    // reachable, not merely documented: the gate refuses this repo's own
    // candidate rule for the right reason rather than 'no such command'
    let (code, _, stderr) = harness(&repo.root, &["eval", "--gate", "a-candidate-rule"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("no ablate.sh"), "{stderr}");
}

#[test]
fn the_driver_names_a_commit_on_the_round_that_no_task_claims() {
    let driver = repo_root().join("driver.sh");

    let fires = history(&[
        "feat: T-001 the work",
        "tidy the thing",
        "verify: T-001 verdict",
    ]);
    let (code, out) = script(&driver, fires.path(), &["--unlabelled"]);
    assert_eq!(code, 0, "{out}");
    let findings: Vec<&str> = out.lines().filter(|l| l.starts_with("FINDING ")).collect();
    // exactly one, and it names the commit by subject rather than merely counting
    assert_eq!(findings.len(), 1, "{out}");
    assert!(findings[0].ends_with(" tidy the thing"), "{}", findings[0]);

    // and a round whose every commit names a task is silent
    let quiet = history(&["feat: T-001 the work", "verify: T-001 verdict"]);
    let (code, out) = script(&driver, quiet.path(), &["--unlabelled"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out.lines().filter(|l| l.starts_with("FINDING ")).count(), 0);
}

/// The history `selftest.sh` builds: rounds, and a verifier that never refused.
const NO_REJECTION: [&str; 8] = [
    "init",
    "chore(dogfood): install the harness",
    "feat(x): T-001 a thing",
    "verify: T-001 VERIFIED",
    "chore: strip the dogfood instance",
    "chore(dogfood): install the harness",
    "fix(y): T-002 another thing",
    "chore: strip the round-2 dogfood instance",
];

#[test]
fn a_history_with_no_rejection_fails_the_bootstrap_check() {
    let bootstrap = repo_root().join("docs/bootstrap.sh");
    let fixture = history(&NO_REJECTION);
    let (code, out) = script(&bootstrap, fixture.path(), &["--check"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no REJECTED commit anywhere"), "{out}");
}

#[test]
fn the_bootstrap_record_is_derived_from_git() {
    let bootstrap = repo_root().join("docs/bootstrap.sh");
    let mut subjects: Vec<&str> = NO_REJECTION.to_vec();
    subjects.extend([
        "chore(dogfood): install the harness",
        "verify: T-003 REJECTED — a skipped assertion printed ok",
        "chore: strip the round-3 dogfood instance",
    ]);
    let fixture = history(&subjects);

    // three rounds, read out of a repository whose history was written a
    // moment ago: the record is derived rather than transcribed
    let (code, out) = script(&bootstrap, fixture.path(), &[]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out.lines().filter(|l| l.starts_with("round ")).count(),
        3,
        "{out}"
    );

    let (code, out) = script(&bootstrap, fixture.path(), &["--check"]);
    assert_eq!(code, 0, "{out}");

    // and --check passes on this package's own history
    let (code, out) = script(&bootstrap, &repo_root(), &["--check"]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn docs_demo_drives_one_loop_iteration_end_to_end_and_deletes_what_it_made() {
    let (code, out) = script(&repo_root().join("docs/demo.sh"), &repo_root(), &[]);
    assert_eq!(code, 0, "{out}");

    // The STAGE SEQUENCE, in order and unsorted: an iteration that printed a
    // verdict without an implement stage, or printed them the other way round,
    // is not the run README.md describes. Exit status alone would go green on
    // a demo that printed nothing (LEARNINGS.md, zero-as-pass).
    let seen: Vec<&str> = out
        .lines()
        .filter_map(|l| {
            if l.contains("stage.start") && l.contains("stage=implement") {
                Some("implement")
            } else if l.contains("stage.start") && l.contains("stage=verify") {
                Some("verify")
            } else if l.starts_with("T-001  status: ") {
                Some(l.trim_end())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        seen,
        vec!["implement", "verify", "T-001  status: done"],
        "{out}"
    );

    // "it must leave the machine as it found it": the demo prints the
    // directory it made, and an extraction that found no path must read as its
    // own absence rather than as a pass.
    let made = out
        .lines()
        .find_map(|l| l.strip_prefix("== install into a throwaway repo  ("))
        .and_then(|l| l.strip_suffix(')'))
        .unwrap_or_else(|| panic!("the demo printed no directory:\n{out}"));
    assert!(!Path::new(made).exists(), "{made} was left behind");
}

/// Every key of a `test-hashes.json` whose file no longer hashes to its
/// recorded digest.
fn hash_mismatches(root: &Path) -> Vec<String> {
    let path = root.join("test-hashes.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let covered: std::collections::BTreeMap<String, String> =
        serde_json::from_str(&text).expect("test-hashes.json is not a flat object");
    covered
        .into_iter()
        .filter(|(key, want)| {
            use sha2::Digest;
            let Ok(bytes) = fs::read(root.join(key)) else {
                return true;
            };
            let digest: String = sha2::Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            digest != *want
        })
        .map(|(key, _)| key)
        .collect()
}

#[test]
fn every_file_test_hashes_covers_still_hashes_to_its_recorded_digest() {
    // Asserted FIRING first: `test-hashes.json` is INSTANCE state and the
    // package tree carries none, so on this branch the scan below reads an
    // absent file. A check that cannot fail is not a check.
    let fixture = tempfile::tempdir().expect("tempdir");
    fs::write(fixture.path().join("covered.txt"), "the real bytes").expect("write");
    fs::write(
        fixture.path().join("test-hashes.json"),
        r#"{"covered.txt": "0000000000000000000000000000000000000000000000000000000000000000"}"#,
    )
    .expect("write");
    assert_eq!(hash_mismatches(fixture.path()), vec!["covered.txt"]);

    assert_eq!(hash_mismatches(&repo_root()), Vec::<String>::new());
}

/// One job's own block out of a workflow file: the lines after `  <key>:` up
/// to the next line at exactly two spaces of indent. An absent or renamed job
/// yields an empty block, so every token derived from it reads as its own
/// absence rather than as a silent pass.
fn job_block(workflow: &str, key: &str) -> String {
    let start = format!("  {key}:");
    let mut inside = false;
    let mut out = String::new();
    for line in workflow.lines() {
        if line == start {
            inside = true;
            continue;
        }
        if inside {
            // the block ends at the next job key, and at the comment that
            // introduces one: every comment inside a job is indented deeper
            if re(r"^  [a-z#]").is_match(line) {
                inside = false;
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The runners a job's `os:` matrix lists, sorted.
fn matrix_os(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in block.lines() {
        if re(r"^\s*os:").is_match(line) {
            inside = true;
            continue;
        }
        if inside {
            match line.trim().strip_prefix("- ") {
                Some(runner) => out.push(runner.trim().to_string()),
                None => inside = false,
            }
        }
    }
    out.sort();
    out
}

/// Does this block invoke the floor? `^[^#]*` so a line of the workflow's own
/// prose cannot stand in for the thing it describes, and not on a `name:`,
/// because a step label mentioning the floor is a label, not an invocation.
fn invokes_floor(block: &str) -> bool {
    block.lines().any(|l| {
        let before_comment = l.split('#').next().unwrap_or("");
        !before_comment.contains("name:")
            && (before_comment.contains("./selftest.sh") || before_comment.contains("cargo test"))
    })
}

/// `if:` and `continue-on-error:` at either depth: a job that never runs
/// asserts nothing, and one whose failure is a warning asserts nothing either.
fn switched_off(block: &str) -> usize {
    let off = re(r"^\s*(-\s+)?(if|continue-on-error):");
    block.lines().filter(|l| off.is_match(l)).count()
}

fn ci() -> String {
    read(&repo_root().join(".github/workflows/ci.yml"))
}

#[test]
fn ci_runs_the_floor_on_a_gnu_and_a_bsd_userland() {
    let block = job_block(&ci(), "floor");
    // macos-latest is BSD sed, ubuntu-latest is GNU sed, and both parser
    // defects this package has had were that difference.
    assert_eq!(matrix_os(&block), vec!["macos-latest", "ubuntu-latest"]);
    assert!(invokes_floor(&block), "{block}");
    assert_eq!(switched_off(&block), 0, "{block}");
    // HARNESS_EVALS asserted ABSENT, whole-file: the evals spawn a real agent,
    // and a CI job holding a model credential is the blast radius this package
    // argues against. Whole-file, because scoped to the job it would miss the
    // variable set at workflow top level.
    assert!(!re(r"(?m)^[^#]*HARNESS_EVALS").is_match(&ci()));

    // and the reading reports absence: a renamed job yields an empty block
    assert_eq!(
        matrix_os(&job_block(&ci(), "no-such-job")),
        Vec::<String>::new()
    );
    assert!(!invokes_floor(&job_block(&ci(), "no-such-job")));
}

#[test]
fn ci_runs_the_floor_with_the_driver_reaching_the_artifact() {
    // Until ci.yml carried a job that sets HARNESS_DRIVER, the only assertion
    // that touched the built artifact ran when the author remembered a flag
    // and never otherwise. Read by VALUE, not by presence: `HARNESS_DRIVER: ''`
    // is the token present and the feature off.
    let set = re(r#"(?m)^[^#]*HARNESS_DRIVER:\s*['"]?[^\s'"]"#);
    let block = job_block(&ci(), "driver");
    assert!(!block.is_empty(), "no driver job in ci.yml");
    assert!(set.is_match(&block), "{block}");
    assert!(invokes_floor(&block), "{block}");
    assert_eq!(switched_off(&block), 0, "{block}");

    // Asserted FIRING, not merely passing on a file that happens to be right:
    // the same readings over four fixtures, each an ablation the row cares
    // about. Every token has both of its readings somewhere here.
    let no_job = "jobs:\n  floor:\n    steps:\n      - run: ./selftest.sh\n";
    let no_var = "jobs:\n  driver:\n    steps:\n      - name: the floor, with the driver reaching the artifact\n        env:\n          HARNESS_DRIVER: ''\n        run: ./selftest.sh\n";
    let switched = "jobs:\n  driver:\n    if: false\n    steps:\n      - env:\n          HARNESS_DRIVER: '1'\n        run: ./selftest.sh\n";
    let soft = "jobs:\n  driver:\n    steps:\n      - env:\n          HARNESS_DRIVER: '1'\n        continue-on-error: true\n        run: ./selftest.sh\n";

    let reading = |yml: &str| {
        let b = job_block(yml, "driver");
        (
            !b.is_empty(),
            set.is_match(&b),
            invokes_floor(&b),
            switched_off(&b),
        )
    };
    assert_eq!(reading(no_job), (false, false, false, 0));
    assert_eq!(reading(no_var), (true, false, true, 0));
    assert_eq!(reading(switched), (true, true, true, 1));
    assert_eq!(reading(soft), (true, true, true, 1));
}

#[test]
fn every_github_action_is_pinned_to_a_commit_sha() {
    // A tag moves and a SHA does not, so a `uses:` pinned to a tag is an
    // unreviewed third party running with the workflow's token. Every one
    // carries its version in a trailing comment.
    let uses = re(r"(?m)^\s*(-\s+)?uses:");
    let pinned = re(r"uses:\s*[^@\s]+@[0-9a-f]{40}\s+#\s*\S");
    let unpinned = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| uses.is_match(l) && !pinned.is_match(l))
            .map(|l| l.trim().to_string())
            .collect()
    };

    let workflows = repo_root().join(".github/workflows");
    let files = walk(&workflows);
    assert!(!files.is_empty(), "no workflow files matched");
    for rel in &files {
        assert_eq!(
            unpinned(&read(&workflows.join(rel))),
            Vec::<String>::new(),
            "{}",
            rel.display()
        );
    }

    // Asserted FIRING: a tag pin and a bare SHA with no trailing comment,
    // beside one correctly pinned line.
    let fixture = "\
      - uses: actions/checkout@v7.0.1\n\
      - uses: ossf/scorecard-action@2d1146689b8cda280b9bc96326124645441f03bc\n\
      - uses: step-security/harden-runner@e14015d583714f6e62063499dc959a02595150a1 # v2.21.1\n";
    assert_eq!(unpinned(fixture).len(), 2, "{:?}", unpinned(fixture));
}

#[test]
fn the_skill_hook_fires_on_a_headless_lane() {
    // The role prompts name skills and a headless lane may ignore them; the
    // only thing that reaches one every turn is a hook the install writes into
    // the repo. `--adapter claude` writes it as `UserPromptSubmit`, not
    // `SessionStart`: SessionStart fires once and decays as the context grows.
    let repo = Repo::new();
    init::install(
        &repo.root,
        &InitOpts {
            adapter: Some("claude".to_string()),
            ..InitOpts::default()
        },
    )
    .expect("install");

    let settings: serde_json::Value =
        serde_json::from_str(&read(&repo.root.join(".claude/settings.json"))).expect("settings");
    let on_prompt = settings["hooks"]["UserPromptSubmit"].to_string();
    assert!(on_prompt.contains("harness hook skills"), "{on_prompt}");

    // It reports and never blocks -- refusal is the gates' job -- and its
    // stdout carries every declared skill. The COUNT is asserted, so a skill
    // list emptied to make this pass is the thing that fails it.
    let (code, stdout, stderr) = harness(&repo.root, &["hook", "skills"]);
    assert_eq!(code, 0, "{stderr}");
    let cfg = harness::config::load(&repo.root).expect("config");
    assert_eq!(cfg.skill.len(), 7);
    for skill in &cfg.skill {
        assert!(
            stdout.contains(&skill.id),
            "{} absent from {stdout}",
            skill.id
        );
    }
}

#[test]
fn no_role_prompt_carries_an_incident_narrative() {
    // A role prompt is an instruction, not a post-mortem. Dated incidents,
    // task ids and reproduction stories are evidence for a human reading the
    // repo; in a prompt they are tokens the model pays for on every stage.
    let narrative = re(
        r"(?i)[0-9]{4}-[0-9]{2}-[0-9]{2}|TASKS\.md T-[0-9]|reproduced (on |by )?[0-9]{4}|agentskills\.io",
    );
    assert!(
        narrative.is_match("reproduced on 2026-09-04"),
        "the scan cannot report"
    );

    let roles = repo_root().join("roles");
    for rel in walk(&roles) {
        let text = read(&roles.join(&rel));
        assert!(
            !narrative.is_match(&text),
            "roles/{}: {:?}",
            rel.display(),
            narrative.find(&text).map(|m| m.as_str())
        );
    }
}

#[test]
fn no_shipped_file_carries_rhetorical_filler() {
    // This is a public repository. The patterns below are rhetoric, not
    // information: antithesis, appeals to the point, and self-congratulation.
    // Every one of them can be replaced by the fact it was decorating.
    let filler = re(
        r"(?i)the whole point|that is the trick|is the whole |beautifully|elegantly|, it is one |extra steps|which is the point|the honest argument|is not a [a-z]+, it is",
    );
    assert!(
        filler.is_match("which is the point"),
        "the scan cannot report"
    );

    let root = repo_root();
    for rel in [
        "README.md",
        "roles",
        "templates",
        "skills",
        "docs/demo.sh",
        "docs/bootstrap.sh",
        "driver.sh",
        "crates/harness/src",
        "crates/harness/harness.default.toml",
    ] {
        let path = root.join(rel);
        let files: Vec<PathBuf> = if path.is_dir() {
            walk(&path).into_iter().map(|p| path.join(p)).collect()
        } else {
            vec![path]
        };
        for file in files {
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            assert!(
                !filler.is_match(&text),
                "{}: {:?}",
                file.display(),
                filler.find(&text).map(|m| m.as_str())
            );
        }
    }
}

#[test]
#[ignore = "shell package retires in Task 18"]
fn every_shipped_script_passes_shellcheck_at_full_severity() {}

#[test]
#[ignore = "shell package retires in Task 18"]
fn every_shellcheck_suppression_names_its_reason() {}

#[test]
#[ignore = "shell package retires in Task 18"]
fn every_shipped_script_is_shfmt_clean() {}

#[test]
#[ignore = "installs four repos and drives four iterations; run with --ignored"]
fn the_package_driver_reports_shortfalls_as_finding_lines() {
    // What is asserted is the INSTRUMENT: it reached the artifact (rc 0) and
    // everything it said was a finding -- never a verdict, a pass or a fail.
    // Deliberately NOT "it found at least one thing": that would assert the
    // harness stays broken.
    let (code, out) = script(&repo_root().join("driver.sh"), &repo_root(), &[]);
    assert_eq!(code, 0, "{out}");
    let stray: Vec<&str> = out
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with("FINDING "))
        .collect();
    assert_eq!(stray, Vec::<&str>::new());

    // One FINDING is one proposed block, so a finding the operator cannot open
    // is a task nobody can act on. Every sha the driver prints has to resolve
    // HERE: `drive()` works in a `mktemp -d` that is removed before it prints.
    let sha = re(r"^FINDING [^:]*: ([0-9a-f]+) ");
    for line in out.lines() {
        let Some(caps) = sha.captures(line) else {
            continue;
        };
        let rev = format!("{}^{{commit}}", &caps[1]);
        assert!(
            harness::git::git_ok(&repo_root(), &["rev-parse", "-q", "--verify", &rev]),
            "{line}"
        );
    }
}

#[test]
#[ignore = "spawns a real agent; run with HARNESS_EVALS and --ignored"]
fn the_trimmed_role_prompts_still_pass_their_evals() {
    let (code, stdout, stderr) = harness(&repo_root(), &["eval"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(stdout.lines().filter(|l| l.ends_with(" PASS")).count(), 3);
}
