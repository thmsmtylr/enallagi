//! The install assertions, one test per assertion and named after it.

use enallagi::fixture::Repo;
use enallagi::init::{self, InitOpts, InitReport};
use enallagi::probes::{self, CheckOutcome, Finding, ProbeCtx, ProbeResult};
use enallagi::runners;
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
    let cfg = enallagi::config::load(&repo.root).expect("config");
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
    assert!(repo.root.join(".enallagi/enallagi.toml").is_file());
    assert!(repo.root.join(".enallagi/RAILS.md").is_file());
    assert!(!repo.root.join(".harness").exists());
    assert!(report
        .wrote
        .contains(&".enallagi/enallagi.toml".to_string()));
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
fn init_makes_the_dir_an_ignored_repository() {
    let repo = Repo::new();
    install(&repo);
    let state = repo.root.join(".enallagi");
    let git = |dir: &Path, args: &[&str]| enallagi::git::git(dir, args);
    let top = git(&state, &["rev-parse", "--show-toplevel"]).expect("a repository");
    assert_eq!(
        fs::canonicalize(top).expect("top"),
        fs::canonicalize(&state).expect("state")
    );
    for scratch in ["events.jsonl", "loop.pid", "run/roles/implementer.md"] {
        assert!(
            enallagi::git::git_ok(&state, &["check-ignore", "-q", scratch]),
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
fn init_leaves_a_clean_fixture_clean() {
    for name in enallagi::agent::presets().keys() {
        let repo = Repo::new();
        repo.write(".gitignore", "node_modules/\n");
        repo.commit_all("ignore");
        with(&repo, &adapter(name));
        with(&repo, &adapter(name));
        let git = |args: &[&str]| enallagi::git::git(&repo.root, args).expect("git");
        assert_eq!(
            git(&["status", "--porcelain", "--untracked-files=all"]),
            "",
            "{name}"
        );
        assert_eq!(git(&["diff", "--", ".gitignore"]), "", "{name}");
    }
}

#[test]
fn init_excludes_the_dir_in_one_block() {
    let repo = Repo::new();
    let report = with(&repo, &adapter("gemini"));
    with(&repo, &adapter("gemini"));
    let exclude =
        enallagi::git::git(&repo.root, &["rev-parse", "--git-path", "info/exclude"]).expect("path");
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
fn a_root_layout_seeds_the_context_file() {
    let repo = Repo::new();
    repo.write("TASKS.md", "# TASKS\n");
    let report = install(&repo);
    let cfg = enallagi::config::load(&repo.root).expect("config");
    assert_eq!(cfg.layout.context_file, "AGENTS.md");
    assert!(
        report.wrote.contains(&"AGENTS.md".to_string()),
        "{:?}",
        report.wrote
    );
    assert!(!repo.root.join(".enallagi/AGENTS.md").exists());
}

#[test]
fn init_leaves_a_root_queue_in_the_product() {
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
fn the_context_file_is_seeded_and_pointed_at() {
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
    let skill = read(
        &repo,
        ".enallagi/adapters/claude/skills/running-the-loop/SKILL.md",
    );
    assert!(skill.starts_with("---\n"), "{skill:.40}");
    assert!(
        skill.lines().any(|l| l == "name: running-the-loop"),
        "{skill:.200}"
    );
    assert!(repo
        .root
        .join(".enallagi/adapters/claude/skills/running-the-loop/references/task-block.md")
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
fn an_appended_learning_survives_a_reinstall() {
    let repo = Repo::new();
    install(&repo);
    let rule = "- [2026-09-20] a rule this loop paid for\n";
    let seeded = read(&repo, ".enallagi/LEARNINGS.md");
    fs::write(repo.root.join(".enallagi/LEARNINGS.md"), seeded + rule).expect("append");
    let report = install(&repo);
    assert!(read(&repo, ".enallagi/LEARNINGS.md").ends_with(rule));
    assert!(
        report.kept.contains(&".enallagi/LEARNINGS.md".to_string()),
        "{report:?}"
    );
}

#[test]
fn the_context_file_resyncs_the_check() {
    let repo = Repo::new();
    install(&repo);
    let unset = format!("`{}`", enallagi::config::UNSET_CHECK);
    assert!(read(&repo, ".enallagi/AGENTS.md").contains(&unset));
    repo.write(
        ".enallagi/enallagi.toml",
        "[check]\ncommand = \"make check\"\n",
    );
    install(&repo);
    let context = read(&repo, ".enallagi/AGENTS.md");
    assert!(context.contains("`make check`"), "{context:.400}");
    assert!(!context.contains(&unset));
}

#[test]
fn init_names_the_keys_left_unset() {
    let repo = Repo::new();
    let report = install(&repo);
    let note = report
        .notes
        .iter()
        .find(|n| n.contains("before a lane runs"))
        .unwrap_or_else(|| panic!("{:?}", report.notes));
    for key in [
        "check.command",
        "check.force",
        "check.fail_name",
        "layout.source_root",
        "layout.test_file_suffix_re",
        "layout.test_decl_patterns",
    ] {
        assert!(note.contains(key), "{note}");
    }
    assert!(!read(&repo, ".enallagi/AGENTS.md").contains("``"));

    repo.write(
        ".enallagi/enallagi.toml",
        "[check]\ncommand = \"make check\"\n",
    );
    let note = install(&repo)
        .notes
        .into_iter()
        .find(|n| n.contains("before a lane runs"))
        .expect("a note");
    assert!(!note.contains("check.command"), "{note}");
    assert!(note.contains("check.fail_name"), "{note}");
}

#[test]
fn the_spec_resyncs_the_check() {
    let repo = Repo::new();
    install(&repo);
    let unset = format!("`{}`", enallagi::config::UNSET_CHECK);
    assert!(read(&repo, ".enallagi/SPEC.md").contains(&unset));
    repo.write(
        ".enallagi/enallagi.toml",
        "[check]\ncommand = \"make check\"\n",
    );
    install(&repo);
    let spec = read(&repo, ".enallagi/SPEC.md");
    let at = spec.find("### 0.4").expect("no 0.4 heading");
    let section = &spec[at..];
    assert!(section.contains("`make check`"), "{section:.400}");
    assert!(!spec.contains(&unset));
}

// the seeded spec must not name a check stage the configured check never runs
#[test]
fn the_seeded_spec_names_no_stage_the_check_lacks() {
    let repo = Repo::new();
    install(&repo);
    let spec = read(&repo, ".enallagi/SPEC.md");
    for claim in ["precheck", "hash-verify", "trace", "The check greps"] {
        assert!(!spec.contains(claim), "{claim} in {spec}");
    }
}

#[test]
fn a_drifted_install_is_reported() {
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

fn seeded(repo: &Repo, text: &str) {
    repo.write(".enallagi/enallagi.toml", text);
}

fn config(repo: &Repo) -> toml::Value {
    toml::from_str(&read(repo, ".enallagi/enallagi.toml")).expect("toml")
}

#[test]
fn a_seeded_config_writes_no_key_equal_to_its_default() {
    let repo = Repo::new();
    install(&repo);
    let text = read(&repo, ".enallagi/enallagi.toml");
    assert!(text.contains("harness.default.toml"), "{text}");
    assert!(
        config(&repo).as_table().expect("table").is_empty(),
        "{text}"
    );

    let migrated = Repo::new();
    migrated.write(
        "harness.json",
        r#"{"check": "make check", "harnessDir": ".enallagi"}"#,
    );
    install(&migrated);
    let default: toml::Value =
        toml::from_str(enallagi::config::DEFAULT_TOML).expect("harness.default.toml");
    let written = config(&migrated);
    for (table, keys) in written.as_table().expect("table") {
        for (key, value) in keys.as_table().expect("table") {
            assert_ne!(
                Some(value),
                default.get(table).and_then(|t| t.get(key)),
                "{table}.{key} equals its default"
            );
        }
    }
    assert_eq!(written["check"]["command"].as_str(), Some("make check"));
    assert!(written.get("layout").is_none(), "{written}");
}

#[test]
fn a_fully_seeded_config_reports_its_default_keys() {
    let repo = Repo::new();
    seeded(&repo, enallagi::config::DEFAULT_TOML);
    let report = install(&repo);
    let note = report
        .notes
        .iter()
        .find(|n| n.contains("equal their default"))
        .unwrap_or_else(|| panic!("{:?}", report.notes));
    assert!(note.contains("check.command"), "{note}");
    assert_eq!(
        read(&repo, ".enallagi/enallagi.toml"),
        enallagi::config::DEFAULT_TOML
    );
}

#[test]
fn a_stale_force_is_reported_against_the_command() {
    let repo = Repo::new();
    seeded(
        &repo,
        &enallagi::config::DEFAULT_TOML.replace("\ncommand = \"\"", "\ncommand = \"make check\""),
    );
    let report = install(&repo);
    let note = report
        .notes
        .iter()
        .find(|n| n.contains("follows command"))
        .unwrap_or_else(|| panic!("{:?}", report.notes));
    assert!(note.contains("check.force is the default"), "{note}");
}

#[test]
fn prune_defaults_keeps_only_the_changed_key() {
    let repo = Repo::new();
    seeded(
        &repo,
        &enallagi::config::DEFAULT_TOML.replace("\ncommand = \"\"", "\ncommand = \"make check\""),
    );
    let dropped = init::prune(&repo.root, false).expect("prune");
    assert!(dropped.contains(&"check.force".to_string()), "{dropped:?}");
    assert_eq!(
        config(&repo),
        toml::from_str::<toml::Value>("[check]\ncommand = \"make check\"\n").expect("toml")
    );
    let cfg = enallagi::config::load(&repo.root).expect("config");
    assert_eq!(cfg.check.force, "make check");
}

#[test]
fn prune_keeps_the_comments_of_kept_keys() {
    let repo = Repo::new();
    seeded(
        &repo,
        "# my own note: this command is set by CI\n[check]\ncommand = \"make check\"\n# the default timeout is fine, keep it explicit\ntimeout = \"30m\"\n",
    );
    let dropped = init::prune(&repo.root, false).expect("prune");
    assert_eq!(dropped, ["check.timeout"]);
    let text = read(&repo, ".enallagi/enallagi.toml");
    assert!(!text.contains("timeout"), "{text}");
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.contains(&"# my own note: this command is set by CI"),
        "{text}"
    );
    assert!(lines.contains(&"command = \"make check\""), "{text}");
    assert!(!text.contains("keep it explicit"), "{text}");
}

#[test]
fn prune_drops_every_line_of_a_multiline_array() {
    let default = enallagi::config::DEFAULT_TOML;
    let start = default.find("harness_files = [").expect("harness_files");
    let end = start + default[start..].find("\n]\n").expect("array end") + 2;
    let array = &default[start..end];
    assert!(array.lines().count() > 2, "{array}");
    let repo = Repo::new();
    seeded(
        &repo,
        &format!("[layout]\n{array}\nsource_root = \"lib/\"\n"),
    );
    let dropped = init::prune(&repo.root, false).expect("prune");
    assert_eq!(dropped, ["layout.harness_files"]);
    let text = read(&repo, ".enallagi/enallagi.toml");
    for line in array.lines() {
        assert!(!text.lines().any(|l| l == line), "{line:?} in {text}");
    }
    assert!(
        text.lines().any(|l| l == "source_root = \"lib/\""),
        "{text}"
    );
}

#[test]
fn prune_twice_writes_the_defaults_note_once() {
    let repo = Repo::new();
    seeded(
        &repo,
        "[check]\ncommand = \"make check\"\ntimeout = \"30m\"\nfail_name = \"\"\n",
    );
    init::prune(&repo.root, false).expect("prune");
    seeded(
        &repo,
        &format!(
            "{}timeout = \"30m\"\n",
            read(&repo, ".enallagi/enallagi.toml")
        ),
    );
    init::prune(&repo.root, false).expect("prune");
    let text = read(&repo, ".enallagi/enallagi.toml");
    let note = enallagi::config::DEFAULTS_NOTE.trim_end();
    assert_eq!(text.lines().filter(|l| *l == note).count(), 1, "{text}");
}

#[test]
fn init_migrates_harness_json_keys() {
    let repo = Repo::new();
    repo.write(
        "harness.json",
        r#"{"check": "make check", "spec": "DESIGN.md", "harnessDir": ".enallagi"}"#,
    );
    let report = install(&repo);
    assert!(read(&repo, ".enallagi/enallagi.toml").contains("make check"));
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
fn the_claude_adapter_writes_a_plugin() {
    let repo = Repo::new();
    let theirs = "{ \"model\": \"opus\" }\n";
    repo.write(".claude/settings.json", theirs);
    repo.write(
        ".enallagi/adapters/claude/hooks/hooks.json",
        r#"{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Edit", "hooks": [ { "type": "command", "command": "./theirs.sh" } ] }
    ]
  }
}
"#,
    );
    let planned = init::planned_files(&repo.root, &adapter("claude")).expect("plan");
    let under: Vec<&String> = planned
        .iter()
        .map(|(path, _)| path)
        .filter(|path| path.starts_with(".claude/"))
        .collect();
    assert!(under.is_empty(), "{under:?}");
    with(&repo, &adapter("claude"));

    let plugin = ".enallagi/adapters/claude";
    let manifest: serde_json::Value = serde_json::from_str(&read(
        &repo,
        &format!("{plugin}/.claude-plugin/plugin.json"),
    ))
    .expect("plugin json");
    assert_eq!(manifest["name"], "harness");
    for role in [
        "scout",
        "adjudicator",
        "implementer",
        "verifier",
        "researcher",
    ] {
        assert!(
            repo.root
                .join(format!("{plugin}/agents/{role}.md"))
                .is_file(),
            "no agent file for {role}"
        );
    }
    assert!(repo
        .root
        .join(format!("{plugin}/skills/running-the-loop/SKILL.md"))
        .is_file());
    let hooks = read(&repo, &format!("{plugin}/hooks/hooks.json"));
    assert!(
        hooks.contains("./theirs.sh"),
        "the foreign hook was dropped"
    );
    for command in [
        "enallagi hook immutable",
        "enallagi hook one-writer",
        "enallagi hook verify-done",
        "enallagi hook skills",
    ] {
        assert!(hooks.contains(command), "no {command} in {hooks}");
    }
    assert_eq!(read(&repo, ".claude/settings.json"), theirs);

    with(&repo, &adapter("claude"));
    assert_eq!(hooks, read(&repo, &format!("{plugin}/hooks/hooks.json")));
}

#[test]
fn the_claude_preset_loads_the_plugin() {
    let claude = &enallagi::agent::presets()["claude"];
    let flag = claude
        .argv
        .iter()
        .position(|w| w == "--plugin-dir")
        .expect("claude loads the plugin");
    assert_eq!(claude.argv[flag + 1], "{harness_dir}/adapters/claude");
    let settings = claude
        .argv
        .iter()
        .position(|w| w == "--settings")
        .expect("claude carries the deny rules");
    let json = &claude.argv[settings + 1];
    assert!(json.contains("Bash(git push:*)"));
    // a lane that keeps Monitor or a default attribution puts a co-author trailer on every commit
    assert!(json.contains("\"Monitor\""), "{json}");
    assert!(
        json.contains("\"attribution\":{\"commit\":\"\",\"pr\":\"\"}"),
        "{json}"
    );
    assert_eq!(
        claude.skills_dir.as_deref(),
        Some("{harness_dir}/adapters/claude/skills")
    );
    assert!(
        claude
            .invocation
            .as_deref()
            .is_some_and(|i| i.contains("harness:<id>")),
        "{:?}",
        claude.invocation
    );
}

#[test]
fn the_default_skills_plugin_is_harness() {
    let repo = Repo::new();
    install(&repo);
    let manifest: serde_json::Value = serde_json::from_str(&read(
        &repo,
        ".enallagi/adapters/claude/.claude-plugin/plugin.json",
    ))
    .expect("plugin json");
    assert_eq!(manifest["name"], "harness");
}

#[test]
fn a_root_layout_keeps_dot_claude_skills() {
    let repo = root_layout();
    let planned = init::planned_files(&repo.root, &adapter("claude")).expect("plan");
    let paths: Vec<&str> = planned.iter().map(|(path, _)| path.as_str()).collect();
    assert!(
        paths.contains(&".claude/skills/running-the-loop/SKILL.md"),
        "{paths:?}"
    );
    let cfg = enallagi::config::load(&repo.root).expect("config");
    assert_eq!(cfg.layout.skills_dir.as_deref(), Some(".claude/skills"));

    let unconfigured = Repo::new();
    unconfigured.write("TASKS.md", "# TASKS\n");
    let planned = init::planned_files(&unconfigured.root, &adapter("claude")).expect("plan");
    assert!(
        planned
            .iter()
            .any(|(path, _)| path == ".claude/skills/running-the-loop/SKILL.md"),
        "{planned:?}"
    );
}

#[test]
fn the_codex_adapter_writes_hooks_json() {
    let repo = Repo::new();
    with(&repo, &adapter("codex"));
    let hooks: serde_json::Value =
        serde_json::from_str(&read(&repo, "hooks.json")).expect("hooks json");
    assert_eq!(
        hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "enallagi hook immutable"
    );
    assert_eq!(
        hooks["hooks"]["Stop"][0]["hooks"][0]["command"],
        "enallagi hook verify-done"
    );
}

#[test]
fn the_adapter_points_at_the_context_file() {
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
fn a_root_pointer_only_without_an_argv_token() {
    let presets = enallagi::agent::presets();
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
    for name in enallagi::agent::presets().keys() {
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
    let hooks = ".enallagi/adapters/claude/hooks/hooks.json";
    repo.write(hooks, "{ not json at all\n");
    let err = init::install(&repo.root, &adapter("claude")).expect_err("invalid json");
    assert!(
        matches!(err, init::InitError::InvalidJson { ref path, .. } if path == hooks),
        "{err}"
    );
    assert!(!repo.root.join(".enallagi/adapters/claude/agents").exists());
    assert!(!repo.root.join(".enallagi/TASKS.md").exists());
}

#[test]
fn a_preset_with_no_hooks_still_points() {
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
fn dry_run_writes_nothing_and_says_so() {
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
        && c.contains("enallagi probe")
        && !c.contains("__ENALLAGI_DIR__/hooks")));
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
fn init_plans_every_instance_file() {
    let repo = Repo::new();
    let planned = init::planned_files(&repo.root, &adapter("claude")).expect("plan");
    let entry_points = [
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        "QWEN.md",
        ".github/copilot-instructions.md",
    ];
    let paths: Vec<&str> = planned.iter().map(|(path, _)| path.as_str()).collect();
    for path in &paths {
        assert!(
            path.starts_with(".enallagi/") || entry_points.contains(path),
            "{path} is planned outside the harness directory"
        );
    }
    for path in [
        ".enallagi/enallagi.toml",
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
    ("enallagi.toml", "[check]\ncommand = \"true\"\n"),
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
fn rendered_files_name_the_harness_dir() {
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
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("init")
        .args(args)
        .current_dir(&repo.root)
        .output()
        .expect("run enallagi init");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn the_printed_git_add_line_exits_0() {
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
fn init_lists_new_paths_and_moves_nothing() {
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
fn init_move_puts_files_at_their_new_paths() {
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

// the failing-test line each preset's fail_name is written against, captured from the runner itself
const CAPTURED: &[(&str, &str)] = &[
    ("bun", "a sum is wrong"),
    ("cargo", "tests::a_sum_is_wrong"),
    ("go", "TestSumIsWrong"),
    ("jest", "a sum is wrong"),
    ("node", "a sum is wrong"),
    ("pytest", "test_a_sum_is_wrong"),
    ("vitest", "a sum is wrong"),
];

fn fixture(runner: &str, name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runners")
        .join(runner)
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn named_by(pattern: &str, output: &str) -> Vec<String> {
    let re = regex::Regex::new(pattern).unwrap_or_else(|e| panic!("{pattern}: {e}"));
    let mut names: Vec<String> = output
        .lines()
        .filter_map(|line| re.captures(line))
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    names.dedup();
    names
}

#[test]
fn every_runner_preset_names_its_failing_test() {
    let presets = runners::presets();
    let shipped: Vec<&str> = presets.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        shipped,
        CAPTURED.iter().map(|(n, _)| *n).collect::<Vec<&str>>()
    );
    for (runner, want) in CAPTURED {
        let preset = presets.iter().find(|r| r.name == *runner).expect(runner);
        assert_eq!(
            named_by(&preset.fail_name, &fixture(runner, "fail.txt")),
            vec![want.to_string()],
            "{runner}"
        );
        let origin = fixture(runner, "origin.txt");
        assert!(origin.contains("command: "), "{runner}: {origin}");
        assert!(origin.contains("version: "), "{runner}: {origin}");
    }
}

// package.json's test script, a CI workflow that runs it, and tests under tests/
fn node_repo() -> Repo {
    let repo = Repo::new();
    repo.write(
        "package.json",
        "{\n  \"name\": \"fixture\",\n  \"scripts\": {\n    \"typecheck\": \"tsc --noEmit\",\n    \"test\": \"node --import tsx --test tests/*.test.ts\"\n  }\n}\n",
    );
    repo.write(
        ".github/workflows/ci.yml",
        "name: ci\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: npm ci\n      - run: npm run typecheck\n      - run: npm test\n",
    );
    repo.write("tests/date.test.ts", "test('a date parses', () => {})\n");
    repo.commit_all("a node tree");
    repo
}

fn detected(report: &InitReport, key: &str) -> String {
    report
        .notes
        .iter()
        .find(|n| n.starts_with(&format!("detected: {key} =")))
        .unwrap_or_else(|| panic!("{key} not detected: {:?}", report.notes))
        .clone()
}

#[test]
fn init_writes_the_keys_the_runner_decides() {
    let repo = node_repo();
    install(&repo);
    let written: toml::Value =
        toml::from_str(&read(&repo, ".enallagi/enallagi.toml")).expect("parse");
    let check = &written["check"];
    assert_eq!(
        check["command"].as_str(),
        Some("npm run typecheck && npm test")
    );
    let node = runners::presets()
        .into_iter()
        .find(|r| r.name == "node")
        .expect("node preset");
    assert_eq!(check["fail_name"].as_str(), Some(node.fail_name.as_str()));
    let layout = &written["layout"];
    assert_eq!(
        layout["test_file_suffix_re"].as_str(),
        Some(node.test_file_suffix_re.as_str())
    );
    assert!(layout.get("test_decl_patterns").is_some(), "{layout:?}");
    // src/ equals the default, so T-021's pruning leaves the key out and the load resolves it
    let loaded = enallagi::config::load(&repo.root).expect("load");
    assert_eq!(loaded.layout.source_root, "src");
    let prefixes = layout["allowed_prefixes"].as_array().expect("array");
    assert!(
        prefixes.iter().any(|p| p.as_str() == Some("tests/")),
        "{prefixes:?}"
    );
    assert!(
        prefixes.iter().any(|p| p.as_str() == Some(".enallagi/")),
        "{prefixes:?}"
    );
}

#[test]
fn a_second_job_is_not_part_of_the_check() {
    let repo = node_repo();
    repo.write(
        ".github/workflows/ci.yml",
        "name: ci\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: npm ci\n      - run: npm run typecheck\n      - run: npm test\n  deploy:\n    runs-on: ubuntu-latest\n    steps:\n      - run: npm run build\n      - run: ./deploy.sh production\n",
    );
    repo.commit_all("a workflow that also deploys");
    install(&repo);
    let written: toml::Value =
        toml::from_str(&read(&repo, ".enallagi/enallagi.toml")).expect("parse");
    assert_eq!(
        written["check"]["command"].as_str(),
        Some("npm run typecheck && npm test")
    );
}

#[test]
fn init_names_the_file_a_detected_value_came_from() {
    let repo = node_repo();
    let report = install(&repo);
    assert!(
        detected(&report, "check.command").contains("(.github/workflows/ci.yml:8)"),
        "{}",
        detected(&report, "check.command")
    );
    assert!(
        detected(&report, "check.fail_name").contains("(package.json:5)"),
        "{}",
        detected(&report, "check.fail_name")
    );
    for key in [
        "layout.test_file_suffix_re",
        "layout.test_decl_patterns",
        "layout.source_root",
        "layout.allowed_prefixes",
    ] {
        let line = detected(&report, key);
        assert!(
            line.contains(".ts:") || line.contains("package.json:"),
            "{line}"
        );
    }
}

#[test]
fn two_runners_in_one_tree_write_no_keys() {
    let repo = node_repo();
    repo.write("Cargo.toml", "[package]\nname = \"fixture\"\n");
    repo.commit_all("two runners");
    let report = install(&repo);
    let written = read(&repo, ".enallagi/enallagi.toml");
    assert!(!written.contains("fail_name"), "{written}");
    assert!(!written.contains("source_root"), "{written}");
    assert!(
        !report.notes.iter().any(|n| n.starts_with("detected: ")),
        "{:?}",
        report.notes
    );
    for name in ["cargo (Cargo.toml:1)", "node (package.json:5)"] {
        assert!(
            report
                .notes
                .iter()
                .any(|n| n == &format!("candidate: {name}")),
            "{name} not named: {:?}",
            report.notes
        );
    }
}

#[test]
fn a_tree_with_only_tests_writes_no_source_root() {
    let repo = node_repo();
    fs::remove_file(repo.root.join("src/schema.ts")).expect("remove src");
    repo.commit_all("no src");
    let report = install(&repo);
    let written = read(&repo, ".enallagi/enallagi.toml");
    assert!(!written.contains("source_root"), "{written}");
    assert!(
        !report
            .notes
            .iter()
            .any(|n| n.starts_with("detected: layout.source_root")),
        "{:?}",
        report.notes
    );
    assert!(
        detected(&report, "layout.allowed_prefixes").contains("\"tests/\""),
        "{:?}",
        report.notes
    );
    for key in [
        "check.command",
        "check.fail_name",
        "layout.test_file_suffix_re",
        "layout.test_decl_patterns",
    ] {
        detected(&report, key);
    }
}

#[test]
fn a_tree_with_no_runner_keeps_the_defaults() {
    let repo = Repo::new();
    let report = install(&repo);
    let written = read(&repo, ".enallagi/enallagi.toml");
    assert!(!written.contains("fail_name"), "{written}");
    assert!(
        !report.notes.iter().any(|n| n.starts_with("candidate: ")),
        "{:?}",
        report.notes
    );
    assert!(
        report.notes.iter().any(|n| n.contains("no test runner")),
        "{:?}",
        report.notes
    );
}
