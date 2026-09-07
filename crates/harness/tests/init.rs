//! The install assertions from `selftest.sh:40-64` and `:107-120`, one test
//! per assertion and named after it.

use harness::fixture::Repo;
use harness::init::{self, InitOpts, InitReport};
use harness::probes::{self, CheckOutcome, Finding, ProbeCtx, ProbeResult};
use std::fs;
use std::path::{Path, PathBuf};

fn install(repo: &Repo) -> InitReport {
    with(repo, &InitOpts::default())
}

fn with(repo: &Repo, opts: &InitOpts) -> InitReport {
    init::install(&repo.root, opts).expect("install")
}

fn adapter(name: &str) -> InitOpts {
    InitOpts {
        adapter: Some(name.to_string()),
        ..InitOpts::default()
    }
}

fn read(repo: &Repo, rel: &str) -> String {
    fs::read_to_string(repo.root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// What `install-stale` says about this tree.
fn stale(repo: &Repo) -> Vec<Finding> {
    let cfg = harness::config::load(&repo.root).expect("config");
    let check = CheckOutcome {
        ran: true,
        red: false,
        output: String::new(),
    };
    let ctx = ProbeCtx {
        root: &repo.root,
        cfg: &cfg,
        check: Some(&check),
        driver: false,
    };
    match probes::run_all(&ctx, &["install-stale".to_string()])
        .into_iter()
        .find(|(name, _)| name == "install-stale")
    {
        Some((_, ProbeResult::Count(found))) => found,
        other => panic!("install-stale did not report a count: {other:?}"),
    }
}

/// Every file under `root` except git's own, relative to it.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_path_buf());
            }
        }
    }
    out.sort();
    out
}

// ------------------------------------------------------ selftest.sh:40-64

#[test]
fn init_exits_0_on_a_fresh_repo() {
    let repo = Repo::new();
    let report = install(&repo);
    assert!(repo.root.join("harness.toml").is_file());
    assert!(repo.root.join(".harness/RAILS.md").is_file());
    assert!(report.wrote.contains(&"harness.toml".to_string()));
    // the `git add` line names every top-level path the run touched
    for path in ["harness.toml", ".harness", "TASKS.md", "evals"] {
        assert!(
            report.track.contains(&path.to_string()),
            "{:?}",
            report.track
        );
    }
}

#[test]
fn no_token_survives_substitution() {
    let repo = Repo::new();
    with(&repo, &adapter("claude"));
    let token = regex::Regex::new(r"__[A-Z][A-Z_]+__").expect("regex");
    for rel in walk(&repo.root) {
        let Ok(text) = fs::read_to_string(repo.root.join(&rel)) else {
            continue;
        };
        assert!(
            !token.is_match(&text),
            "{}: {:?}",
            rel.display(),
            token.find(&text).map(|m| m.as_str())
        );
    }
}

#[test]
fn re_init_is_idempotent() {
    let repo = Repo::new();
    install(&repo);
    let before = walk(&repo.root)
        .iter()
        .map(|p| (p.clone(), fs::read(repo.root.join(p)).unwrap_or_default()))
        .collect::<Vec<_>>();
    install(&repo);
    let after = walk(&repo.root)
        .iter()
        .map(|p| (p.clone(), fs::read(repo.root.join(p)).unwrap_or_default()))
        .collect::<Vec<_>>();
    assert_eq!(before, after);
}

// ----------------------------------------------------- selftest.sh:107-120

#[test]
fn the_context_file_is_seeded_and_the_pointers_point_at_it() {
    let repo = Repo::new();
    install(&repo);
    assert!(!read(&repo, "AGENTS.md").is_empty());
    for pointer in ["CLAUDE.md", "GEMINI.md", ".github/copilot-instructions.md"] {
        assert!(
            read(&repo, pointer).contains("AGENTS.md"),
            "{pointer} does not point at AGENTS.md"
        );
    }
}

#[test]
fn the_context_file_stays_short() {
    let repo = Repo::new();
    install(&repo);
    let lines = read(&repo, "AGENTS.md").lines().count();
    assert!(lines < 80, "AGENTS.md is {lines} lines");
}

#[test]
fn the_project_skill_is_valid_agentskills_frontmatter() {
    let repo = Repo::new();
    install(&repo);
    // the default preset is claude, so the skill lands in its skills directory
    let skill = read(&repo, ".claude/skills/running-the-loop/SKILL.md");
    assert!(skill.starts_with("---\n"), "{skill:.40}");
    assert!(
        skill.lines().any(|l| l == "name: running-the-loop"),
        "{skill:.200}"
    );
    assert!(repo
        .root
        .join(".claude/skills/running-the-loop/references/task-block.md")
        .is_file());
}

#[test]
fn init_writes_the_harness_gitignore() {
    let repo = Repo::new();
    install(&repo);
    let ignore = read(&repo, ".harness/.gitignore");
    let lines: Vec<&str> = ignore.lines().collect();
    assert_eq!(
        lines,
        vec![
            "events.jsonl",
            "*.log",
            "logs/",
            "worktrees/",
            "loop.pid",
            "skills/",
            "__pycache__/",
        ]
    );
}

#[test]
fn documents_with_content_are_kept() {
    let repo = Repo::new();
    repo.write("TASKS.md", "# my own queue\n");
    repo.write("SPEC.md", "# my own spec\n");
    let report = install(&repo);
    assert_eq!(read(&repo, "TASKS.md"), "# my own queue\n");
    assert_eq!(read(&repo, "SPEC.md"), "# my own spec\n");
    assert!(report.kept.contains(&"TASKS.md".to_string()), "{report:?}");
    assert!(report.kept.contains(&"SPEC.md".to_string()), "{report:?}");
    // and a kept document is still tracked, or the first verdict fails on it
    assert!(report.track.contains(&"TASKS.md".to_string()));
}

#[test]
fn the_context_file_is_resynced_to_the_configured_check() {
    let repo = Repo::new();
    install(&repo);
    assert!(read(&repo, "AGENTS.md").contains("`bun run check`"));
    repo.write("harness.toml", "[check]\ncommand = \"make check\"\n");
    install(&repo);
    let context = read(&repo, "AGENTS.md");
    assert!(context.contains("`make check`"), "{context:.400}");
    assert!(!context.contains("`bun run check`"));
}

// ----------------------------------------------------- selftest.sh:1179-1225

#[test]
fn an_installed_file_that_drifted_from_its_source_is_reported() {
    let repo = Repo::new();
    install(&repo);
    // 0 on the freshly installed tree is what stops the probe being a permanent
    // finding nobody reads
    assert_eq!(stale(&repo), Vec::new());

    // one installed file edited and another deleted; the two FINDINGs are what
    // prove it reports the file that drifted rather than a count
    repo.write(".harness/roles/scout.md", "# not what init wrote\n");
    fs::remove_file(repo.root.join(".harness/roles/verifier.md")).expect("rm");
    let found = stale(&repo);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found.iter().any(|f| f.path == ".harness/roles/scout.md"
            && f.message.contains("differs from the source")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.path == ".harness/roles/verifier.md"
                && f.message.contains("does not have it")),
        "{found:?}"
    );

    // a document the project has edited is not drift: seed never overwrites one
    repo.write("TASKS.md", "# my own queue\n");
    assert_eq!(stale(&repo).len(), 2);

    install(&repo);
    assert_eq!(stale(&repo), Vec::new());
}

// ------------------------------------------------------------- migration

#[test]
fn init_migrates_harness_json_and_prints_each_renamed_key() {
    let repo = Repo::new();
    repo.write(
        "harness.json",
        r#"{"check": "make check", "spec": "DESIGN.md", "harnessDir": ".harness"}"#,
    );
    let report = install(&repo);
    assert!(read(&repo, "harness.toml").contains("make check"));
    assert!(repo.root.join("harness.json.migrated").is_file());
    assert!(!repo.root.join("harness.json").exists());
    assert!(
        report
            .migrated_keys
            .contains(&"check -> check.command".to_string()),
        "{:?}",
        report.migrated_keys
    );
    assert!(
        report
            .migrated_keys
            .contains(&"spec -> layout.spec".to_string()),
        "{:?}",
        report.migrated_keys
    );
    // the migrated answers are what the install is substituted from
    assert!(repo.root.join("DESIGN.md").is_file());
}

// -------------------------------------------------------------- adapters

#[test]
fn the_claude_adapter_writes_agents_and_merges_settings() {
    let repo = Repo::new();
    repo.write(
        ".claude/settings.json",
        r#"{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Edit", "hooks": [ { "type": "command", "command": "./theirs.sh" } ] }
    ]
  },
  "permissions": { "deny": [ "Bash(sudo:*)" ] },
  "model": "opus"
}
"#,
    );
    with(&repo, &adapter("claude"));

    for role in [
        "scout",
        "adjudicator",
        "implementer",
        "verifier",
        "researcher",
    ] {
        assert!(
            repo.root
                .join(format!(".claude/agents/{role}.md"))
                .is_file(),
            "no agent file for {role}"
        );
    }
    let settings: serde_json::Value =
        serde_json::from_str(&read(&repo, ".claude/settings.json")).expect("settings json");
    let text = settings.to_string();
    assert!(text.contains("./theirs.sh"), "the foreign hook was dropped");
    assert_eq!(
        settings["model"], "opus",
        "the rest of the file was dropped"
    );
    for command in [
        "harness hook immutable",
        "harness hook one-writer",
        "harness hook verify-done",
        "harness hook skills",
    ] {
        assert!(text.contains(command), "no {command} in {text}");
    }
    assert!(
        text.contains("Bash(git push:*)"),
        "the deny rules were not added"
    );
    assert!(
        text.contains("Bash(sudo:*)"),
        "a deny rule of theirs was dropped"
    );

    // and merging twice adds nothing twice
    let before = read(&repo, ".claude/settings.json");
    with(&repo, &adapter("claude"));
    assert_eq!(before, read(&repo, ".claude/settings.json"));
}

#[test]
fn the_codex_adapter_writes_hooks_json() {
    let repo = Repo::new();
    with(&repo, &adapter("codex"));
    let hooks: serde_json::Value =
        serde_json::from_str(&read(&repo, "hooks.json")).expect("hooks json");
    assert_eq!(
        hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "harness hook immutable"
    );
    assert_eq!(
        hooks["hooks"]["Stop"][0]["hooks"][0]["command"],
        "harness hook verify-done"
    );
}

#[test]
fn the_adapter_points_its_own_instruction_file_at_the_context_file() {
    // QWEN.md is not in the default pointer_files, so the adapter adds it
    let repo = Repo::new();
    let report = with(&repo, &adapter("qwen"));
    assert!(read(&repo, "QWEN.md").contains("AGENTS.md"));
    assert_eq!(report.wrote.iter().filter(|p| *p == "QWEN.md").count(), 1);

    // GEMINI.md is one, and the adapter must not plan it a second time
    let repo = Repo::new();
    let report = with(&repo, &adapter("gemini"));
    assert_eq!(
        report.wrote.iter().filter(|p| *p == "GEMINI.md").count(),
        1,
        "{:?}",
        report.wrote
    );
}

#[test]
fn a_settings_file_that_is_not_json_is_refused() {
    let repo = Repo::new();
    repo.write(".claude/settings.json", "{ not json at all\n");
    let err = init::install(&repo.root, &adapter("claude")).expect_err("invalid json");
    assert!(
        matches!(err, init::InitError::InvalidJson { ref path, .. } if path == ".claude/settings.json"),
        "{err}"
    );
    // and it refused before writing anything
    assert!(!repo.root.join(".claude/agents").exists());
    assert!(!repo.root.join(".harness").exists());
}

#[test]
fn a_preset_with_no_hooks_file_still_points_its_instruction_file_at_the_context_file() {
    let repo = Repo::new();
    let report = with(&repo, &adapter("omp"));
    assert!(read(&repo, ".omp/AGENTS.md").contains("AGENTS.md"));
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.starts_with("adapter omp: no hooks file")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_preset_with_no_hooks_file_says_so() {
    let repo = Repo::new();
    let report = with(&repo, &adapter("opencode"));
    assert!(
        report
            .notes
            .iter()
            .any(|n| n == "adapter opencode: no hooks file; hooks are code or absent for this tool"),
        "{:?}",
        report.notes
    );
    assert!(!repo.root.join(".opencode").exists());
}

#[test]
fn an_unknown_adapter_is_refused() {
    let repo = Repo::new();
    let err = init::install(&repo.root, &adapter("emacs")).expect_err("unknown adapter");
    assert!(matches!(err, init::InitError::UnknownAdapter(ref a) if a == "emacs"));
}

// --------------------------------------------------------------- dry run

#[test]
fn dry_run_writes_nothing_and_says_what_it_would_write() {
    let repo = Repo::new();
    let before = walk(&repo.root);
    let report = with(
        &repo,
        &InitOpts {
            dry_run: true,
            ..InitOpts::default()
        },
    );
    assert_eq!(walk(&repo.root), before);
    assert!(report.wrote.contains(&".harness/RAILS.md".to_string()));
    let planned = init::planned_files(&repo.root, &InitOpts::default()).expect("plan");
    assert!(planned.iter().any(|(p, c)| p == ".harness/roles/scout.md"
        && c.contains("harness probe")
        && !c.contains("__HARNESS_DIR__/hooks")));
}

#[test]
fn the_fixture_installs_the_harness() {
    let repo = Repo::new();
    repo.init_harness("[check]\ncommand = \"true\"\n");
    assert!(repo.root.join(".harness/RAILS.md").is_file());
    assert!(read(&repo, ".harness/RAILS.md").contains("`true`"));
}
