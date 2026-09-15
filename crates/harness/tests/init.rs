//! The install assertions, one test per assertion and named after it.

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

#[test]
fn init_exits_0_on_a_fresh_repo() {
    let repo = Repo::new();
    let report = install(&repo);
    assert!(repo.root.join(".enallagi/harness.toml").is_file());
    assert!(repo.root.join(".enallagi/RAILS.md").is_file());
    assert!(!repo.root.join(".harness").exists());
    assert!(report.wrote.contains(&".enallagi/harness.toml".to_string()));
    assert!(
        report.wrote.contains(&".enallagi/AGENTS.md".to_string()),
        "{:?}",
        report.wrote
    );
    assert!(!repo.root.join("AGENTS.md").exists());
    assert!(
        !report.track.contains(&".enallagi".to_string()),
        "the harness directory is its own repository: {:?}",
        report.track
    );
}

#[test]
fn init_makes_the_harness_directory_a_repository_the_product_ignores() {
    let repo = Repo::new();
    install(&repo);
    let state = repo.root.join(".enallagi");
    let git = |dir: &Path, args: &[&str]| harness::git::git(dir, args);
    let top = git(&state, &["rev-parse", "--show-toplevel"]).expect("a repository");
    assert_eq!(
        fs::canonicalize(top).expect("top"),
        fs::canonicalize(&state).expect("state")
    );
    for scratch in ["events.jsonl", "loop.pid", "run/roles/implementer.md"] {
        assert!(
            harness::git::git_ok(&state, &["check-ignore", "-q", scratch]),
            "{scratch} is not ignored by the harness directory's repository"
        );
    }
    let porcelain = git(
        &repo.root,
        &["status", "--porcelain", "--untracked-files=all"],
    )
    .expect("status");
    assert!(
        !porcelain.contains(".enallagi"),
        "the product repository sees the harness directory: {porcelain}"
    );

    install(&repo);
    let exclude = git(&repo.root, &["rev-parse", "--git-path", "info/exclude"]).expect("path");
    let exclude = read(&repo, &exclude);
    assert_eq!(
        exclude.lines().filter(|l| *l == "/.enallagi/").count(),
        1,
        "{exclude}"
    );
}

#[test]
fn init_with_each_preset_leaves_a_clean_fixture_clean_and_its_gitignore_unchanged() {
    for name in harness::agent::presets().keys() {
        let repo = Repo::new();
        repo.write(".gitignore", "node_modules/\n");
        repo.commit_all("ignore");
        with(&repo, &adapter(name));
        with(&repo, &adapter(name));
        let git = |args: &[&str]| harness::git::git(&repo.root, args).expect("git");
        assert_eq!(
            git(&["status", "--porcelain", "--untracked-files=all"]),
            "",
            "{name}"
        );
        assert_eq!(git(&["diff", "--", ".gitignore"]), "", "{name}");
    }
}

#[test]
fn init_excludes_the_harness_directory_and_every_entry_point_in_one_marked_block() {
    let repo = Repo::new();
    let report = with(&repo, &adapter("gemini"));
    with(&repo, &adapter("gemini"));
    let exclude =
        harness::git::git(&repo.root, &["rev-parse", "--git-path", "info/exclude"]).expect("path");
    let exclude = read(&repo, &exclude);
    let lines: Vec<&str> = exclude.lines().collect();
    let open = lines.iter().filter(|l| **l == "# >>> harness").count();
    let close = lines.iter().filter(|l| **l == "# <<< harness").count();
    assert_eq!((open, close), (1, 1), "{exclude}");
    let start = lines.iter().position(|l| *l == "# >>> harness").unwrap();
    let end = lines.iter().position(|l| *l == "# <<< harness").unwrap();
    let block = &lines[start + 1..end];
    assert!(block.contains(&"/.enallagi/"), "{exclude}");
    for entry in ["GEMINI.md", ".gemini/settings.json"] {
        assert!(
            report.wrote.contains(&entry.to_string()),
            "{:?}",
            report.wrote
        );
        assert!(
            block.contains(&format!("/{entry}").as_str()),
            "no {entry} in {exclude}"
        );
    }
    assert!(
        !report.track.contains(&"GEMINI.md".to_string()),
        "{:?}",
        report.track
    );
}

#[test]
fn a_root_layout_with_no_harness_toml_seeds_the_context_file_at_the_root() {
    let repo = Repo::new();
    repo.write("TASKS.md", "# TASKS\n");
    let report = install(&repo);
    let cfg = harness::config::load(&repo.root).expect("config");
    assert_eq!(cfg.layout.context_file, "AGENTS.md");
    assert!(
        report.wrote.contains(&"AGENTS.md".to_string()),
        "{:?}",
        report.wrote
    );
    assert!(!repo.root.join(".enallagi/AGENTS.md").exists());
}

#[test]
fn init_leaves_a_root_layout_queue_in_the_product_repository() {
    let repo = Repo::new();
    repo.write("TASKS.md", "# TASKS\n");
    repo.commit_all("queue");
    install(&repo);
    assert!(!repo.root.join(".enallagi/.git").exists());
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

// every seeded document must name the binary's own subcommands, never the shell scripts it replaced
#[test]
fn no_seeded_document_names_a_deleted_script() {
    let repo = Repo::new();
    with(&repo, &adapter("claude"));
    let dead = regex::Regex::new(
        r"loop\.sh|archive-done\.sh|probes\.sh|tasks\.py|install\.sh|selftest\.sh|harness\.json",
    )
    .expect("regex");
    for rel in walk(&repo.root) {
        let Ok(text) = fs::read_to_string(repo.root.join(&rel)) else {
            continue;
        };
        assert!(
            !dead.is_match(&text),
            "{}: {:?}",
            rel.display(),
            dead.find(&text).map(|m| m.as_str())
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

#[test]
fn the_context_file_is_seeded_and_the_pointers_point_at_it() {
    let repo = Repo::new();
    with(&repo, &adapter("codex"));
    assert!(!read(&repo, ".enallagi/AGENTS.md").is_empty());
    for pointer in ["CLAUDE.md", "GEMINI.md", ".github/copilot-instructions.md"] {
        assert!(
            read(&repo, pointer).contains(".enallagi/AGENTS.md"),
            "{pointer} does not point at .enallagi/AGENTS.md"
        );
    }
}

#[test]
fn the_context_file_stays_short() {
    let repo = Repo::new();
    install(&repo);
    let lines = read(&repo, ".enallagi/AGENTS.md").lines().count();
    assert!(lines < 80, "AGENTS.md is {lines} lines");
}

#[test]
fn the_project_skill_is_valid_agentskills_frontmatter() {
    let repo = Repo::new();
    install(&repo);
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
    let ignore = read(&repo, ".enallagi/.gitignore");
    let lines: Vec<&str> = ignore.lines().collect();
    assert_eq!(
        lines,
        vec![
            "events.jsonl",
            "*.log",
            "logs/",
            "worktrees/",
            "loop.pid",
            "run/",
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
    // a kept document is still tracked, or the first verdict fails on it
    assert!(report.track.contains(&"TASKS.md".to_string()));
}

#[test]
fn the_context_file_is_resynced_to_the_configured_check() {
    let repo = Repo::new();
    install(&repo);
    assert!(read(&repo, ".enallagi/AGENTS.md").contains("`bun run check`"));
    repo.write(
        ".enallagi/harness.toml",
        "[check]\ncommand = \"make check\"\n",
    );
    install(&repo);
    let context = read(&repo, ".enallagi/AGENTS.md");
    assert!(context.contains("`make check`"), "{context:.400}");
    assert!(!context.contains("`bun run check`"));
}

#[test]
fn an_installed_file_that_drifted_from_its_source_is_reported() {
    let repo = Repo::new();
    install(&repo);
    // zero on a freshly installed tree, or this is a permanent finding nobody reads
    assert_eq!(stale(&repo), Vec::new());

    repo.write(".enallagi/roles/scout.md", "# not what init wrote\n");
    fs::remove_file(repo.root.join(".enallagi/roles/verifier.md")).expect("rm");
    let found = stale(&repo);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found.iter().any(|f| f.path == ".enallagi/roles/scout.md"
            && f.message.contains("differs from the source")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.path == ".enallagi/roles/verifier.md"
                && f.message.contains("does not have it")),
        "{found:?}"
    );

    // a document the project has edited is not drift: seed never overwrites one
    repo.write(".enallagi/TASKS.md", "# my own queue\n");
    assert_eq!(stale(&repo).len(), 2);

    install(&repo);
    assert_eq!(stale(&repo), Vec::new());
}

#[test]
fn init_migrates_harness_json_and_prints_each_renamed_key() {
    let repo = Repo::new();
    repo.write(
        "harness.json",
        r#"{"check": "make check", "spec": "DESIGN.md", "harnessDir": ".enallagi"}"#,
    );
    let report = install(&repo);
    assert!(read(&repo, ".enallagi/harness.toml").contains("make check"));
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
    assert!(repo.root.join(".enallagi/DESIGN.md").is_file());
}

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
    assert!(
        text.contains("\"Monitor\""),
        "Monitor is not denied: {text}"
    );
    assert_eq!(settings["attribution"]["commit"], "", "{text}");
    assert_eq!(settings["attribution"]["pr"], "", "{text}");

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
    let repo = Repo::new();
    let report = with(&repo, &adapter("qwen"));
    assert!(read(&repo, "QWEN.md").contains("AGENTS.md"));
    assert_eq!(report.wrote.iter().filter(|p| *p == "QWEN.md").count(), 1);

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
fn each_preset_gets_a_root_pointer_only_when_its_argv_cannot_name_the_context_file() {
    let presets = harness::agent::presets();
    let claude = &presets["claude"].argv;
    let flag = claude
        .iter()
        .position(|w| w == "--append-system-prompt-file")
        .expect("claude passes the context file");
    assert_eq!(claude[flag + 1], "{context_file}");

    for (name, preset) in &presets {
        let repo = Repo::new();
        let report = with(&repo, &adapter(name));
        assert!(!read(&repo, ".enallagi/AGENTS.md").is_empty(), "{name}");
        let named = preset.argv.iter().any(|w| w.contains("{context_file}"));
        let mut entries = vec![
            "AGENTS.md".to_string(),
            "CLAUDE.md".to_string(),
            "GEMINI.md".to_string(),
            "QWEN.md".to_string(),
        ];
        entries.extend(preset.instruction_file.clone());
        for entry in &entries {
            // AGENTS.md is no pointer file; a flagless preset gets it only as its own instruction file
            let pointer = !named
                && (entry != "AGENTS.md"
                    || preset.instruction_file.as_deref() == Some("AGENTS.md"));
            if !pointer {
                assert!(!repo.root.join(entry).exists(), "{name} wrote {entry}");
                continue;
            }
            assert!(
                read(&repo, entry).contains(".enallagi/AGENTS.md"),
                "{name}: {entry} does not point at the context file"
            );
            assert!(
                report.excluded.contains(entry),
                "{name}: {entry} not excluded: {:?}",
                report.excluded
            );
        }
    }
}

#[test]
fn a_tracked_instruction_file_is_never_written() {
    let files = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "QWEN.md"];
    for name in harness::agent::presets().keys() {
        let repo = Repo::new();
        for file in files {
            repo.write(file, "");
        }
        repo.commit_all("empty instruction files");
        let report = with(&repo, &adapter(name));
        for file in files {
            assert_eq!(read(&repo, file), "", "{name} wrote {file}");
            assert!(
                !report.wrote.contains(&file.to_string()),
                "{name}: {:?}",
                report.wrote
            );
        }
    }
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
    assert!(!repo.root.join(".claude/agents").exists());
    assert!(!repo.root.join(".enallagi").exists());
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
    assert!(report.wrote.contains(&".enallagi/RAILS.md".to_string()));
    let planned = init::planned_files(&repo.root, &InitOpts::default()).expect("plan");
    assert!(planned.iter().any(|(p, c)| p == ".enallagi/roles/scout.md"
        && c.contains("harness probe")
        && !c.contains("__HARNESS_DIR__/hooks")));
}

#[test]
fn the_fixture_installs_the_harness() {
    let repo = Repo::new();
    repo.init_harness("[check]\ncommand = \"true\"\n");
    assert!(repo.root.join(".enallagi/RAILS.md").is_file());
    assert!(read(&repo, ".enallagi/RAILS.md").contains("`true`"));
}

#[test]
fn the_seeded_documents_state_rules_and_do_not_argue() {
    let repo = Repo::new();
    install(&repo);
    let argues = regex::Regex::new(
        r"\b(because|which is why|the reason|worth|deliberately|on purpose|it turns out|in practice)\b",
    )
    .expect("regex");
    let mut offences = Vec::new();
    for rel in walk(&repo.root) {
        let name = rel.to_string_lossy().to_string();
        if !name.ends_with(".md") {
            continue;
        }
        let text = read(&repo, &name);
        let (mut inside, mut run) = (false, 0usize);
        for (i, line) in text.lines().enumerate() {
            if argues.is_match(line) {
                offences.push(format!("{name}:{} {line}", i + 1));
            }
            let comment = inside || line.trim_start().starts_with("<!--");
            inside = comment && !line.contains("-->");
            run = if comment { run + 1 } else { 0 };
            if run == 3 {
                offences.push(format!("{name}:{} comment runs past two lines", i - 1));
            }
        }
    }
    assert!(offences.is_empty(), "{}", offences.join("\n"));
}

#[test]
fn init_plans_every_instance_file_under_the_harness_directory() {
    let repo = Repo::new();
    let planned = init::planned_files(&repo.root, &adapter("claude")).expect("plan");
    let entry_points = [
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        "QWEN.md",
        ".github/copilot-instructions.md",
        ".claude/settings.json",
    ];
    let paths: Vec<&str> = planned.iter().map(|(path, _)| path.as_str()).collect();
    for path in &paths {
        assert!(
            path.starts_with(".enallagi/")
                || entry_points.contains(path)
                || path.starts_with(".claude/agents/")
                || path.starts_with(".claude/skills/"),
            "{path} is planned outside the harness directory"
        );
    }
    for path in [
        ".enallagi/harness.toml",
        ".enallagi/TASKS.md",
        ".enallagi/SPEC.md",
        ".enallagi/evals/README.md",
    ] {
        assert!(paths.contains(&path), "{path} not planned: {paths:?}");
    }
}

const ROOT_LAYOUT: &[(&str, &str)] = &[
    ("TASKS.md", "# TASKS\n"),
    ("PROGRESS.md", "# PROGRESS\n"),
    ("SPEC.md", "# spec\n"),
    ("harness.toml", "[check]\ncommand = \"true\"\n"),
    (".harness/RAILS.md", "rails\n"),
    (".harness/roles/scout.md", "scout\n"),
];

fn root_layout() -> Repo {
    let repo = Repo::new();
    for (rel, text) in ROOT_LAYOUT {
        repo.write(rel, text);
    }
    repo
}

fn instance_mentions(planned: &[(String, String)]) -> Vec<(String, String)> {
    let mention = regex::Regex::new(
        r"([A-Za-z0-9_./-]*)(?:(?:TASKS|PROGRESS|DECISIONS|LEARNINGS)\.md|harness\.toml)",
    )
    .expect("regex");
    let mut out = Vec::new();
    for (path, text) in planned {
        if path.ends_with(".toml") || path.ends_with(".json") {
            continue;
        }
        for found in mention.captures_iter(text) {
            out.push((path.clone(), found[0].to_string()));
        }
    }
    assert!(!out.is_empty(), "no instance-file mention was rendered");
    out
}

#[test]
fn every_rendered_role_template_and_skill_names_instance_files_under_the_harness_directory() {
    let repo = Repo::new();
    let planned = init::planned_files(&repo.root, &adapter("claude")).expect("plan");
    let off: Vec<_> = instance_mentions(&planned)
        .into_iter()
        .filter(|(_, m)| {
            let name = m.rsplit('/').next().unwrap_or(m);
            *m != format!(".enallagi/{name}")
        })
        .collect();
    assert!(off.is_empty(), "{off:#?}");
}

#[test]
fn a_root_layout_renders_instance_files_at_the_root() {
    let repo = root_layout();
    let planned = init::planned_files(&repo.root, &adapter("claude")).expect("plan");
    let off: Vec<_> = instance_mentions(&planned)
        .into_iter()
        .filter(|(_, m)| m.contains('/'))
        .collect();
    assert!(off.is_empty(), "{off:#?}");
}

fn moved_to(rel: &str) -> String {
    format!(".enallagi/{}", rel.trim_start_matches(".harness/"))
}

fn harness_init(repo: &Repo, args: &[&str]) -> String {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_harness"))
        .arg("init")
        .args(args)
        .current_dir(&repo.root)
        .output()
        .expect("run harness init");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn the_git_add_line_init_prints_on_a_fresh_install_exits_0() {
    let repo = Repo::new();
    let stdout = harness_init(&repo, &[]);
    let line = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("4. git add"))
        .unwrap_or_else(|| panic!("no git add line in:\n{stdout}"));
    let paths: Vec<&str> = line.split_whitespace().collect();
    assert!(!paths.contains(&"AGENTS.md"), "{line}");
    let out = std::process::Command::new("git")
        .arg("add")
        .args(&paths)
        .current_dir(&repo.root)
        .output()
        .expect("run git add");
    assert!(out.status.success(), "git add {line}: {out:?}");
}

#[test]
fn init_lists_each_root_and_legacy_instance_file_with_its_new_path_and_moves_nothing() {
    let repo = root_layout();
    let stdout = harness_init(&repo, &[]);
    for (rel, _) in ROOT_LAYOUT {
        let line = format!("{rel} -> {}", moved_to(rel));
        assert!(stdout.contains(&line), "no `{line}` in:\n{stdout}");
        assert!(repo.root.join(rel).is_file(), "{rel} was moved");
    }
    assert!(!repo.root.join(".enallagi").exists());
}

#[test]
fn init_move_puts_every_listed_file_at_its_new_path_unchanged() {
    let repo = root_layout();
    let stdout = harness_init(&repo, &["--move"]);
    for (rel, text) in ROOT_LAYOUT {
        let new = moved_to(rel);
        assert!(
            stdout.contains(&format!("{rel} -> {new}")),
            "{rel} not listed:\n{stdout}"
        );
        assert_eq!(read(&repo, &new), *text, "{new}");
        assert!(!repo.root.join(rel).exists(), "{rel} remains");
    }
    assert!(!repo.root.join(".harness").exists());
}
