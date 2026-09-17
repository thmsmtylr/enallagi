use enallagi::fixture::Repo;
use enallagi::init::{self, InitOpts};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

// a locally excluded scratch file under a scanned directory is not a shipped file
fn tracked(root: &Path) -> std::collections::HashSet<PathBuf> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["ls-files"])
        .output()
        .expect("git ls-files");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(PathBuf::from)
        .collect()
}

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

// truncated at #[cfg(test)] so a fixture string in a test module isn't mistaken for real launcher code
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

// ENALLAGI_BIN points at the test binary so nothing here builds release or reads one off PATH
fn script(program: &Path, cwd: &Path, args: &[&str]) -> (i32, String) {
    let out = enallagi::fixture::command(&program.display().to_string())
        .args(args)
        .current_dir(cwd)
        .env("ENALLAGI_BIN", env!("CARGO_BIN_EXE_enallagi"))
        .output()
        .unwrap_or_else(|e| panic!("run {}: {e}", program.display()));
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn harness(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
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
fn no_script_hardcodes_a_vendor_path() {
    // prose may name a vendor; only an executable path or process guard may not
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

    let dir = repo.root.join(".enallagi");
    for rel in walk(&dir) {
        let Ok(text) = fs::read_to_string(dir.join(&rel)) else {
            continue;
        };
        assert!(
            !vendor.is_match(&text),
            ".enallagi/{}: {:?}",
            rel.display(),
            vendor.find(&text).map(|m| m.as_str())
        );
    }
}

#[test]
fn the_launcher_parses_no_task_blocks_itself() {
    // only queue::match_heading may parse `## [` headings; a second parser here would drift from it
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
fn the_write_path_gate_installs_with_the_rules() {
    let repo = Repo::new();
    repo.init_harness("");
    let readme = read(&repo.root.join(".enallagi/evals/README.md"));
    assert!(readme.contains("enallagi eval --gate"), "{readme:.400}");

    // reachable, not merely documented: the gate must refuse this repo's own candidate rule
    let (code, _, stderr) = harness(&repo.root, &["eval", "--gate", "a-candidate-rule"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("no ablate.sh"), "{stderr}");
}

#[test]
fn the_driver_names_an_unclaimed_commit() {
    let driver = repo_root().join("driver.sh");

    let fires = history(&[
        "feat: T-001 the work",
        "tidy the thing",
        "verify: T-001 verdict",
    ]);
    let (code, out) = script(&driver, fires.path(), &["--unlabelled"]);
    assert_eq!(code, 0, "{out}");
    let findings: Vec<&str> = out.lines().filter(|l| l.starts_with("FINDING ")).collect();
    assert_eq!(findings.len(), 1, "{out}");
    assert!(findings[0].ends_with(" tidy the thing"), "{}", findings[0]);

    let quiet = history(&["feat: T-001 the work", "verify: T-001 verdict"]);
    let (code, out) = script(&driver, quiet.path(), &["--unlabelled"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out.lines().filter(|l| l.starts_with("FINDING ")).count(), 0);
}

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
fn a_history_with_no_rejection_fails() {
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

    let (code, out) = script(&bootstrap, fixture.path(), &[]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out.lines().filter(|l| l.starts_with("round ")).count(),
        3,
        "{out}"
    );

    let (code, out) = script(&bootstrap, fixture.path(), &["--check"]);
    assert_eq!(code, 0, "{out}");

    let (code, out) = script(&bootstrap, &repo_root(), &["--check"]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn docs_demo_drives_one_iteration_and_cleans_up() {
    let (code, out) = script(&repo_root().join("docs/demo.sh"), &repo_root(), &[]);
    assert_eq!(code, 0, "{out}");

    // exit code alone would pass on a demo that printed nothing; check the stage sequence too
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

    let made = out
        .lines()
        .find_map(|l| l.strip_prefix("== install into a throwaway repo  ("))
        .and_then(|l| l.strip_suffix(')'))
        .unwrap_or_else(|| panic!("the demo printed no directory:\n{out}"));
    assert!(!Path::new(made).exists(), "{made} was left behind");
}

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
fn every_hashed_file_matches_its_digest() {
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
            // ends at the next job key or a top-level comment; comments inside a job are indented deeper
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

// `^[^#]*` excludes commented-out lines; skips `name:` so a step label isn't mistaken for an invocation
fn invokes_floor(block: &str) -> bool {
    block.lines().any(|l| {
        let before_comment = l.split('#').next().unwrap_or("");
        !before_comment.contains("name:")
            && (before_comment.contains("./selftest.sh") || before_comment.contains("cargo test"))
    })
}

fn switched_off(block: &str) -> usize {
    let off = re(r"^\s*(-\s+)?(if|continue-on-error):");
    block.lines().filter(|l| off.is_match(l)).count()
}

fn ci() -> String {
    read(&repo_root().join(".github/workflows/ci.yml"))
}

#[test]
fn ci_runs_the_floor_on_a_gnu_and_a_bsd_userland() {
    let block = job_block(&ci(), "rust");
    // macos-latest is BSD sed, ubuntu-latest is GNU sed; that difference has broken parsing before
    assert_eq!(matrix_os(&block), vec!["macos-latest", "ubuntu-latest"]);
    assert!(invokes_floor(&block), "{block}");
    assert_eq!(switched_off(&block), 0, "{block}");
    // whole-file, not job-scoped: ENALLAGI_EVALS set at workflow top level would be missed otherwise
    assert!(!re(r"(?m)^[^#]*ENALLAGI_EVALS").is_match(&ci()));

    // the driver job is the other half of the floor: it runs cargo test with ENALLAGI_DRIVER set
    let driver_block = job_block(&ci(), "driver");
    let driver_env = re(r#"(?m)^[^#]*ENALLAGI_DRIVER:\s*['"]?1['"]?"#);
    assert!(invokes_floor(&driver_block), "{driver_block}");
    assert!(driver_env.is_match(&driver_block), "{driver_block}");

    assert_eq!(
        matrix_os(&job_block(&ci(), "no-such-job")),
        Vec::<String>::new()
    );
    assert!(!invokes_floor(&job_block(&ci(), "no-such-job")));

    // ablation self-checks: a matrix with one OS must not read as both userlands, and an `if:`
    // on the job must be caught by switched_off -- the same checks the neighbouring test makes.
    let one_os = "jobs:\n  rust:\n    strategy:\n      matrix:\n        os:\n          - ubuntu-latest\n    steps:\n      - run: cargo test --workspace\n";
    assert_ne!(
        matrix_os(&job_block(one_os, "rust")),
        vec!["macos-latest".to_string(), "ubuntu-latest".to_string()]
    );

    let switched = "jobs:\n  rust:\n    if: false\n    strategy:\n      matrix:\n        os:\n          - ubuntu-latest\n          - macos-latest\n    steps:\n      - run: cargo test --workspace\n";
    assert_eq!(switched_off(&job_block(switched, "rust")), 1);
}

#[test]
fn ci_runs_the_driver_against_the_artifact() {
    // read by VALUE not presence: `ENALLAGI_DRIVER: ''` is present but the feature is off
    let set = re(r#"(?m)^[^#]*ENALLAGI_DRIVER:\s*['"]?[^\s'"]"#);
    let block = job_block(&ci(), "driver");
    assert!(!block.is_empty(), "no driver job in ci.yml");
    assert!(set.is_match(&block), "{block}");
    assert!(invokes_floor(&block), "{block}");
    assert_eq!(switched_off(&block), 0, "{block}");

    let no_job = "jobs:\n  floor:\n    steps:\n      - run: ./selftest.sh\n";
    let no_var = "jobs:\n  driver:\n    steps:\n      - name: the floor, with the driver reaching the artifact\n        env:\n          ENALLAGI_DRIVER: ''\n        run: ./selftest.sh\n";
    let switched = "jobs:\n  driver:\n    if: false\n    steps:\n      - env:\n          ENALLAGI_DRIVER: '1'\n        run: ./selftest.sh\n";
    let soft = "jobs:\n  driver:\n    steps:\n      - env:\n          ENALLAGI_DRIVER: '1'\n        continue-on-error: true\n        run: ./selftest.sh\n";

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

// A fixture that inherits the shipped `[[skill]]` table clones github from a test: it passed on a
// warm cache and raced itself in CI. Only harness.default.toml may name a remote source.
#[test]
fn no_fixture_declares_a_fetched_skill() {
    let root = repo_root();
    let needle = concat!("source = \"", "github:");
    let mut offences = Vec::new();
    for dir in ["crates/harness/tests", "crates/harness/src"] {
        let base = root.join(dir);
        for rel in walk(&base) {
            if rel.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = fs::read_to_string(base.join(&rel)).expect("read a source file");
            for (i, line) in text.lines().enumerate() {
                if line.contains(needle) {
                    offences.push(format!("{dir}/{}:{}", rel.display(), i + 1));
                }
            }
        }
    }
    assert!(offences.is_empty(), "{}", offences.join("\n"));
}

#[test]
fn every_github_action_is_pinned_to_a_commit_sha() {
    // a tag can move; only a full SHA pin is immutable, so every `uses:` must be SHA-pinned
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

    let fixture = "\
      - uses: actions/checkout@v7.0.1\n\
      - uses: ossf/scorecard-action@2d1146689b8cda280b9bc96326124645441f03bc\n\
      - uses: step-security/harden-runner@e14015d583714f6e62063499dc959a02595150a1 # v2.21.1\n";
    assert_eq!(unpinned(fixture).len(), 2, "{:?}", unpinned(fixture));
}

#[test]
fn the_skill_hook_fires_on_a_headless_lane() {
    // UserPromptSubmit, not SessionStart: SessionStart fires once and decays as context grows
    let repo = Repo::new();
    init::install(
        &repo.root,
        &InitOpts {
            adapter: Some("claude".to_string()),
            ..InitOpts::default()
        },
    )
    .expect("install");

    let hooks: serde_json::Value = serde_json::from_str(&read(
        &repo.root.join(".enallagi/adapters/claude/hooks/hooks.json"),
    ))
    .expect("hooks");
    let on_prompt = hooks["hooks"]["UserPromptSubmit"].to_string();
    assert!(on_prompt.contains("enallagi hook skills"), "{on_prompt}");

    // count is asserted so an emptied skill list can't trivially pass this
    let (code, stdout, stderr) = harness(&repo.root, &["hook", "skills"]);
    assert_eq!(code, 0, "{stderr}");
    let cfg = enallagi::config::load(&repo.root).expect("config");
    assert_eq!(cfg.skill.len(), 8);
    for skill in &cfg.skill {
        assert!(
            stdout.contains(&skill.id),
            "{} absent from {stdout}",
            skill.id
        );
    }
}

#[test]
fn the_commit_register_skill_reaches_both_lanes() {
    // found by its gate, not its id: plain-record is the probe that fails when a subject comments
    let dir = tempfile::TempDir::new().expect("tempdir");
    let cfg = enallagi::config::load(dir.path()).expect("the shipped defaults load");
    let decl = cfg
        .skill
        .iter()
        .find(|s| s.gate == "plain-record")
        .expect("no shipped skill names gate plain-record");
    assert!(decl.source.starts_with("github:"), "{}", decl.source);
    assert!(decl.path.ends_with(&decl.id), "{}", decl.path);
    assert!(decl.rev.is_some(), "{} is unpinned", decl.id);
    assert!(!decl.why.is_empty(), "{} says what it is for", decl.id);

    let token = format!("{{{{skill:{}}}}}", decl.id);
    for role in ["implementer", "verifier"] {
        let text = read(&repo_root().join(format!("roles/{role}.md")));
        assert!(text.contains(&token), "roles/{role}.md wants {token}");
    }
}

#[test]
fn no_role_prompt_carries_an_incident_narrative() {
    // role prompts are instructions, not post-mortems; dated incidents cost tokens on every stage
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
fn no_shipped_document_calls_the_binary_harness() {
    // `harness` survives as a common noun, a path the tree holds and a key the source reads; the
    // binary, the config and the prompt token are `enallagi`
    let old = re(
        r"harness (init|eject|run|watch|probe|pr|base|gate|hook|skills|tasks|eval|events|worktree)\b|harness\.toml|__HARNESS_DIR__|HARNESS_[A-Z]",
    );
    assert!(old.is_match("harness probe"), "the scan cannot report");

    let root = repo_root();
    let shipped = tracked(&root);
    for dir in ["roles", "templates", "skills", "evals", "adapters", "docs"] {
        let at = root.join(dir);
        for rel in walk(&at) {
            if !shipped.contains(&Path::new(dir).join(&rel)) {
                continue;
            }
            let Ok(text) = fs::read_to_string(at.join(&rel)) else {
                continue;
            };
            assert!(
                !old.is_match(&text),
                "{dir}/{}: {:?}",
                rel.display(),
                old.find(&text).map(|m| m.as_str())
            );
        }
    }
}

#[test]
fn no_identifier_runs_past_fifty_characters() {
    const CAP: usize = 50;
    let decl = re(r"\b(?:fn|struct|enum|trait|union|mod|const|static|type)\s+([A-Za-z_]\w*)");
    let field = re(r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?([a-z_]\w*)[ \t]*:");
    assert!(decl.is_match("fn a() {}"), "the scan cannot report");
    assert!(
        field.is_match("    name: String,"),
        "the scan cannot report"
    );

    let crates = repo_root().join("crates");
    let mut long = Vec::new();
    for rel in walk(&crates) {
        if rel.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = read(&crates.join(&rel));
        for name in [&decl, &field]
            .iter()
            .flat_map(|p| p.captures_iter(&text))
            .map(|c| c[1].to_string())
        {
            if name.len() > CAP {
                long.push(format!("{}: {name} ({})", rel.display(), name.len()));
            }
        }
    }
    long.sort();
    long.dedup();
    assert!(long.is_empty(), "{} over {CAP}: {long:#?}", long.len());
}

// main ships the package; what `enallagi init` writes lives only on dogfood/* branches
#[test]
#[ignore = "main only: ci.yml runs it on main and on pull requests into main"]
fn main_tracks_no_instance_file() {
    let mut instance = std::collections::BTreeSet::from(["test-hashes.json".to_string()]);
    for name in enallagi::agent::presets().keys() {
        let repo = Repo::new();
        let opts = InitOpts {
            adapter: Some(name.clone()),
            dry_run: true,
        };
        let planned = init::planned_files(&repo.root, &opts).expect("plan an install");
        instance.extend(planned.into_iter().map(|(path, _)| path));
    }
    // seeded by init, but its source is in the tree
    instance.remove(".enallagi/evals/README.md");
    // a root-layout install writes these names at the root, and main tracks none of them either
    instance.extend(
        [
            "TASKS.md",
            "PROGRESS.md",
            "PROGRESS.archive.md",
            "DECISIONS.md",
            "LEARNINGS.md",
            "SPEC.md",
            ".check-baseline",
            "enallagi.toml",
        ]
        .map(String::from),
    );
    assert!(instance.contains(".enallagi/TASKS.md"), "{instance:?}");

    let out = Command::new("git")
        .args(["ls-files"])
        .current_dir(repo_root())
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "{:?}", out);
    let tracked: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| instance.contains(*l))
        .map(String::from)
        .collect();
    assert_eq!(tracked, Vec::<String>::new());

    let (code, out) = script(&repo_root().join("docs/bootstrap.sh"), &repo_root(), &[]);
    assert_eq!(code, 0, "{out}");
    assert!(
        !out.contains("(in flight)"),
        "a dogfood round is open:\n{out}"
    );
}

#[test]
fn the_shell_package_is_gone() {
    let gone = [
        "harness/",
        "install.sh",
        "selftest.sh",
        "adapters/claude/skill-hook.sh",
        "evals/run.sh",
    ];
    let out = Command::new("git")
        .args(["ls-files"])
        .current_dir(repo_root())
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "{:?}", out);
    let tracked = String::from_utf8_lossy(&out.stdout);
    for path in gone {
        assert!(
            !tracked.lines().any(|l| l == path || l.starts_with(path)),
            "{path} is still tracked"
        );
    }

    let mentions: Vec<String> = crate_sources()
        .into_iter()
        .flat_map(|(rel, text)| {
            text.lines()
                .filter(|l| l.contains("install.sh") || l.contains("selftest.sh"))
                .map(move |l| format!("{}: {}", rel.display(), l.trim()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(mentions, Vec::<String>::new());
}

#[test]
#[ignore = "installs four repos and drives four iterations; run with --ignored"]
fn the_driver_reports_shortfalls_as_findings() {
    // deliberately not asserting findings.len() > 0 -- that would require the harness to stay broken
    let (code, out) = script(&repo_root().join("driver.sh"), &repo_root(), &[]);
    assert_eq!(code, 0, "{out}");
    let stray: Vec<&str> = out
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with("FINDING "))
        .collect();
    assert_eq!(stray, Vec::<&str>::new());

    // every sha printed must resolve in this repo -- drive() works in a mktemp dir removed before it prints
    let sha = re(r"^FINDING [^:]*: ([0-9a-f]+) ");
    for line in out.lines() {
        let Some(caps) = sha.captures(line) else {
            continue;
        };
        let rev = format!("{}^{{commit}}", &caps[1]);
        assert!(
            enallagi::git::git_ok(&repo_root(), &["rev-parse", "-q", "--verify", &rev]),
            "{line}"
        );
    }
}

#[test]
#[ignore = "spawns a real agent; run with ENALLAGI_EVALS and --ignored"]
fn the_trimmed_role_prompts_still_pass_their_evals() {
    let (code, stdout, stderr) = harness(&repo_root(), &["eval"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(stdout.lines().filter(|l| l.ends_with(" PASS")).count(), 3);
}

#[test]
fn the_licence_is_mit() {
    let root = repo_root();
    let licence = read(&root.join("LICENSE"));
    assert!(
        licence.starts_with("MIT License"),
        "{}",
        licence.lines().next().unwrap_or("")
    );
    let manifest = read(&root.join("crates/harness/Cargo.toml"));
    assert!(manifest.contains("license = \"MIT\""), "{manifest}");

    // the declared skills are other people's MIT work; NOTICE carries their notices, which is what
    // MIT asks of anyone who ships or builds on a copy
    let notice = read(&root.join("NOTICE"));
    for (source, holder) in [
        ("obra/superpowers", "Jesse Vincent"),
        ("DietrichGebert/ponytail", "DietrichGebert"),
    ] {
        assert!(notice.contains(source), "NOTICE omits {source}");
        assert!(
            notice.contains(holder),
            "NOTICE omits the {source} copyright holder"
        );
    }
}

fn release_yml() -> String {
    read(&repo_root().join(".github/workflows/release.yml"))
}

fn crate_version_in(manifest: &str) -> String {
    re(r#"(?m)^version = "([^"]+)""#)
        .captures(manifest)
        .map(|c| c[1].to_string())
        .expect("a version line in the manifest")
}

// a tag is the release; one that does not name the crate version ships a binary printing a version
// its own release page does not carry
fn tag_mismatch(tag: &str, version: &str) -> Option<String> {
    match tag.strip_prefix('v') {
        Some(rest) if rest == version => None,
        _ => Some(format!("tag {tag} against crate version {version}")),
    }
}

#[test]
fn the_release_procedure_is_stated_in_one_file() {
    let procedure = release_yml();
    for step in [
        "crates/harness/Cargo.toml",
        "CHANGELOG.md",
        "git tag -a v",
        "git push origin v",
    ] {
        assert!(
            procedure.contains(step),
            "release.yml states no {step} step"
        );
    }

    let readme = read(&repo_root().join("README.md"));
    assert!(
        readme.contains(".github/workflows/release.yml"),
        "README links no release procedure"
    );
    let commands = re(r"(?m)^[^#]*git (tag|push origin v)");
    assert!(
        commands.is_match("git tag -a v1.0.0"),
        "the scan cannot report"
    );
    assert!(
        !commands.is_match(&readme),
        "{:?}",
        commands.find(&readme).map(|m| m.as_str())
    );
}

#[test]
fn the_changelog_lists_unreleased_and_the_tag() {
    let text = read(&repo_root().join("CHANGELOG.md"));
    let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(
        headings.first().copied(),
        Some("## Unreleased"),
        "{headings:?}"
    );
    assert!(
        headings.iter().any(|h| h.starts_with("## v0.1.0")),
        "{headings:?}"
    );

    let entries: Vec<&str> = text
        .lines()
        .skip_while(|l| *l != "## Unreleased")
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(!entries.is_empty(), "{text}");
    for line in &entries {
        assert!(line.starts_with("- "), "{line}");
    }
}

#[test]
fn a_tag_that_disagrees_with_the_version_fails() {
    assert_eq!(tag_mismatch("v0.1.0", "0.1.0"), None);
    let reported = tag_mismatch("v0.2.0", "0.1.0").expect("a mismatch is reported");
    assert!(
        reported.contains("v0.2.0") && reported.contains("0.1.0"),
        "{reported}"
    );
    assert!(tag_mismatch("0.1.0", "0.1.0").is_some());
    assert!(tag_mismatch("v0.1.0-rc1", "0.1.0").is_some());
}

#[test]
fn the_binary_prints_the_crate_version() {
    let root = repo_root();
    let version = crate_version_in(&read(&root.join("crates/harness/Cargo.toml")));
    let (code, stdout, stderr) = harness(&root, &["--version"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(stdout.trim(), format!("enallagi {version}"));
    assert_eq!(
        crate_version_in("[package]\nversion = \"9.9.9\"\n"),
        "9.9.9"
    );
}

#[test]
fn the_release_job_checks_the_tag_version() {
    let block = job_block(&release_yml(), "release");
    let check = block
        .find("--exact the_tag_equals_the_crate_version")
        .unwrap_or_else(|| panic!("no tag-version step:\n{block}"));
    let build = block
        .find("cargo zigbuild")
        .unwrap_or_else(|| panic!("no build step:\n{block}"));
    assert!(check < build, "{block}");
    assert_eq!(switched_off(&block), 0, "{block}");
}

// GITHUB_REF_NAME is unset outside the release job, and a check that cannot read what it compares
// fails rather than passes
#[test]
#[ignore = "release only: release.yml runs it on a tag push"]
fn the_tag_equals_the_crate_version() {
    let tag = std::env::var("GITHUB_REF_NAME").unwrap_or_default();
    assert!(!tag.is_empty(), "GITHUB_REF_NAME is unset");
    let version = crate_version_in(&read(&repo_root().join("crates/harness/Cargo.toml")));
    assert_eq!(tag_mismatch(&tag, &version), None);
}
