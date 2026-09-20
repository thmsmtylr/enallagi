//! The probe assertions, one test per assertion and named after it.

use enallagi::config::{self, Config};
use enallagi::fixture::Repo;
use enallagi::probes::{self, CheckOutcome, ProbeCtx, ProbeResult};
use std::fs;

// install is the seed: docs, rails and roles all come from the binary, so a probe reads the tree an operator would get
fn seeded_with(overrides: &str) -> (Repo, Config) {
    let repo = Repo::new();
    repo.init_harness(overrides);
    enallagi::git::git(&repo.root, &["add", "-A"]).expect("add");
    enallagi::git::git(
        &repo.root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "harness",
        ],
    )
    .expect("commit");
    let cfg = config::load(&repo.root).expect("config");
    (repo, cfg)
}

fn seeded() -> (Repo, Config) {
    seeded_with("")
}

// stubbed green: running the real check here would only measure the host
const GREEN: CheckOutcome = CheckOutcome {
    ran: true,
    red: false,
    output: String::new(),
};

fn run(repo: &Repo, cfg: &Config) -> Vec<(String, ProbeResult)> {
    run_with(repo, cfg, false)
}

fn run_with(repo: &Repo, cfg: &Config, driver: bool) -> Vec<(String, ProbeResult)> {
    let ctx = ProbeCtx {
        root: &repo.root,
        cfg,
        check: Some(&GREEN),
        driver,
    };
    probes::run_all(&ctx, &[])
}

fn render(results: &[(String, ProbeResult)]) -> String {
    probes::render(results)
}

fn count(results: &[(String, ProbeResult)], name: &str) -> Option<usize> {
    results
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, r)| match r {
            ProbeResult::Count(found) => Some(found.len()),
            _ => None,
        })
}

fn findings<'a>(results: &'a [(String, ProbeResult)], name: &str) -> &'a [probes::Finding] {
    match results.iter().find(|(n, _)| n == name) {
        Some((_, ProbeResult::Count(found))) => found,
        _ => &[],
    }
}

fn errors(results: &[(String, ProbeResult)]) -> Vec<&str> {
    results
        .iter()
        .filter(|(_, r)| matches!(r, ProbeResult::Error(_)))
        .map(|(n, _)| n.as_str())
        .collect()
}

/// Never the literal, or the fixture reports itself when this repo is probed.
fn marker() -> String {
    format!("{}{}", "ponytail", ":")
}

fn append(repo: &Repo, name: &str, text: &str) {
    let path = config::instance_path(&repo.root, &config::harness_dir(&repo.root), name);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    fs::write(&path, format!("{existing}{text}")).expect("append");
}

#[test]
fn probes_exit_0_every_probe_ran() {
    let (repo, cfg) = seeded();
    let results = run(&repo, &cfg);
    assert_eq!(errors(&results), Vec::<&str>::new());
    assert_eq!(results.len(), 24, "{}", render(&results));
}

#[test]
fn no_probe_errored() {
    let (repo, cfg) = seeded();
    let out = render(&run(&repo, &cfg));
    assert_eq!(
        out.lines().filter(|l| l.contains(" ERROR ")).count(),
        0,
        "{out}"
    );
}

#[test]
fn check_unnamed_reads_the_nested_context_file() {
    let (repo, cfg) = seeded_with("[check]\ncommand = \"true\"\n");
    assert_eq!(cfg.layout.context_file, ".enallagi/AGENTS.md");
    assert!(!repo.root.join("AGENTS.md").exists());
    assert_eq!(count(&run(&repo, &cfg), "check-unnamed"), Some(0));

    fs::remove_file(repo.root.join(".enallagi/AGENTS.md")).expect("rm");
    let results = run(&repo, &cfg);
    let found = findings(&results, "check-unnamed");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert_eq!(found[0].path, ".enallagi/AGENTS.md");
}

#[test]
fn check_unnamed_reports_an_unset_check() {
    let (repo, cfg) = seeded();
    assert_eq!(cfg.check.command, "");
    let context = fs::read_to_string(repo.root.join(".enallagi/AGENTS.md")).expect("read");
    assert!(context.contains(config::UNSET_CHECK), "{context:.400}");
    let results = run(&repo, &cfg);
    let found = findings(&results, "check-unnamed");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert_eq!(found[0].path, ".enallagi/AGENTS.md");
    assert!(
        found[0].message.contains("check.command"),
        "{}",
        found[0].message
    );
}

#[test]
fn the_row_parser_reads_the_seeded_criteria_table() {
    let (repo, cfg) = seeded();
    assert_eq!(count(&run(&repo, &cfg), "spec-untested"), Some(1));
}

#[test]
fn a_fresh_install_has_two_unenforced_rails() {
    let (repo, cfg) = seeded();
    let results = run(&repo, &cfg);
    let found = findings(&results, "rail-unenforced");
    assert_eq!(found.len(), 2, "{}", render(&results));
    for f in found {
        assert!(f.message.contains("test-hashes.json"), "{}", f.message);
    }
}

#[test]
fn the_seeded_criterion_is_untested() {
    let (repo, cfg) = seeded();
    assert_eq!(count(&run(&repo, &cfg), "queue-uncovered"), Some(1));
}

const COMMA_ROWS: &str = "# SPEC\n\n## 11. Exit criteria\n\n| Behaviour | Test |\n| --- | --- |\n| a | `tests/x.test.ts::the frames FRAMES held, in order` |\n| b | `tests/x.test.ts::the packs a core pack holds, named` |\n\n## 12. Notes\n";

fn claiming(repo: &Repo, rows: &str) {
    repo.write(".enallagi/SPEC.md", COMMA_ROWS);
    append(
        repo,
        "TASKS.md",
        &format!("\n## [T-002] the block that claims them\nscope: src/tests/x.test.ts\nblockedBy: none\nstatus: ready\nrows: {rows}\n"),
    );
}

#[test]
fn a_claimed_row_name_keeps_its_comma() {
    let (repo, cfg) = seeded();
    claiming(
        &repo,
        "tests/x.test.ts::the frames FRAMES held, in order, tests/x.test.ts::the packs a core pack holds, named",
    );
    let results = run(&repo, &cfg);
    assert_eq!(
        count(&results, "queue-uncovered"),
        Some(0),
        "{}",
        render(&results)
    );
}

#[test]
fn a_claimed_row_no_criteria_row_defines_is_found() {
    let (repo, cfg) = seeded();
    claiming(
        &repo,
        "tests/x.test.ts::the frames FRAMES held, in order, tests/x.test.ts::the packs a core pack holds, named, tests/x.test.ts::no row defines this",
    );
    let results = run(&repo, &cfg);
    let found = findings(&results, "queue-uncovered");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0].message.contains(
            "T-002 claims row tests/x.test.ts::no row defines this and the exit criteria define no such row"
        ),
        "{}",
        found[0].message
    );
}

const PIECE_ROWS: &str = "# SPEC\n\n## 11. Exit criteria\n\n| Behaviour | Test |\n| --- | --- |\n| a | `tests/x.test.ts::one` |\n| b | `tests/x.test.ts::two` |\n\n## 12. Notes\n";

fn claiming_pieces(repo: &Repo, rows: &str) {
    claiming(repo, rows);
    repo.write(".enallagi/SPEC.md", PIECE_ROWS);
}

#[test]
fn a_second_row_without_its_file_is_claimed() {
    let (repo, cfg) = seeded();
    claiming_pieces(&repo, "tests/x.test.ts::one, two");
    let results = run(&repo, &cfg);
    assert_eq!(
        count(&results, "queue-uncovered"),
        Some(0),
        "{}",
        render(&results)
    );
}

#[test]
fn an_undefined_piece_reports_the_whole_claim() {
    let (repo, cfg) = seeded();
    claiming_pieces(&repo, "tests/x.test.ts::one, nope");
    let results = run(&repo, &cfg);
    let found = findings(&results, "queue-uncovered");
    let claim =
        "T-002 claims row tests/x.test.ts::one, nope and the exit criteria define no such row";
    assert!(
        found.iter().any(|f| f.message.contains(claim)),
        "{}",
        render(&results)
    );
}

#[test]
fn harness_immutable_has_no_hash_key() {
    // the rail names the file that is the gate; with the launcher a binary, that file is enallagi.toml
    let (repo, cfg) = seeded();
    let results = run(&repo, &cfg);
    let found = findings(&results, "hash-uncovered");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0]
            .message
            .contains("`harness-immutable` names enallagi.toml"),
        "{}",
        found[0].message
    );
}

#[test]
fn the_seeded_progress_repeats_no_friction() {
    let (repo, cfg) = seeded();
    assert_eq!(count(&run(&repo, &cfg), "friction-repeat"), Some(0));
}

#[test]
fn no_driver_command_is_not_a_zero() {
    let (repo, cfg) = seeded();
    let out = render(&run(&repo, &cfg));
    assert!(out.contains("PROBE driver OFF -- "), "{out}");
}

#[test]
fn the_seeded_queue_is_clean() {
    let (repo, cfg) = seeded();
    assert_eq!(count(&run(&repo, &cfg), "queue-hygiene"), Some(0));
}

#[test]
fn every_seeded_learning_names_a_file_command_or_hook() {
    let (repo, cfg) = seeded();
    assert_eq!(count(&run(&repo, &cfg), "learning-unenforced"), Some(0));
}

#[test]
fn the_tree_has_no_litter() {
    let (repo, cfg) = seeded();
    let results = run(&repo, &cfg);
    assert_eq!(count(&results, "litter"), Some(0), "{}", render(&results));
}

// `skills sync` refuses a role whose skill token has no entry, so a fixture declares all eight
const IDS: [&str; 8] = [
    "tdd",
    "ponytail",
    "debugging",
    "review-received",
    "verify-before-done",
    "review-requested",
    "brainstorming",
    "caveman-commit",
];

// a path: source under src/ keeps the fixture off the network and off the litter list
fn vendored_skills(repo: &Repo) -> String {
    let mut toml = String::new();
    for id in IDS {
        repo.write(&format!("src/vendor/{id}/SKILL.md"), "body\n");
        toml.push_str(&format!(
            "\n[[skill]]\nid = \"{id}\"\nsource = \"path:src/vendor/{id}\"\npath = \"\"\ngate = \"none\"\nwhy = \"fixture\"\n"
        ));
    }
    toml
}

fn skills_sync(repo: &Repo) {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["skills", "sync"])
        .current_dir(&repo.root)
        .output()
        .expect("run enallagi skills sync");
    assert!(out.status.success(), "{out:?}");
}

// the conventions a repository already carries, then the install, then what `skills sync` writes
fn conventional() -> (Repo, Config) {
    let repo = Repo::new();
    for name in [
        "docs/guide.md",
        "bench/run.ts",
        "package-lock.json",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "CODE_OF_CONDUCT.md",
        "CHANGELOG.md",
    ] {
        repo.write(name, "x\n");
    }

    // docs/ and bench/ are this repository's own, so they are named per repository and not by the defaults
    let mut toml = String::from(
        "[check]\ncommand = \"true\"\n\n[layout]\nallowed_prefixes = [\"src/\", \"docs/\", \"bench/\", \".enallagi/\", \".claude/\", \".github/\"]\n",
    );
    toml.push_str(&vendored_skills(&repo));
    // a tracked harness directory keeps the install in the product's history, so what it writes is tracked
    repo.write(".enallagi/enallagi.toml", &toml);
    repo.commit_all("conventions");

    enallagi::init::install(&repo.root, &enallagi::init::InitOpts::default()).expect("install");
    repo.commit_all("harness");

    skills_sync(&repo);
    repo.commit_all("skills");
    let tracked =
        enallagi::git::git(&repo.root, &["ls-files", "--", ".enallagi/harness.lock"]).expect("ls");
    assert!(!tracked.is_empty(), "harness.lock is untracked");

    let cfg = config::load(&repo.root).expect("config");
    (repo, cfg)
}

#[test]
fn a_repository_of_conventions_has_no_litter() {
    let (repo, cfg) = conventional();
    let results = run(&repo, &cfg);
    assert_eq!(count(&results, "litter"), Some(0), "{}", render(&results));
}

#[test]
fn a_root_layout_lock_is_not_litter() {
    let repo = Repo::new();
    // a tracked TASKS.md at the root and none under the harness directory is the root layout
    repo.write("src/main.rs", "fn main() {}\n");
    repo.write("TASKS.md", "# TASKS\n");
    let toml = format!(
        "[check]\ncommand = \"true\"\n\n[layout]\nallowed_prefixes = [\"src/\", \".enallagi/\", \".claude/\"]\n{}",
        vendored_skills(&repo)
    );
    repo.write("enallagi.toml", &toml);
    repo.commit_all("product");

    skills_sync(&repo);
    repo.commit_all("skills");
    let tracked = enallagi::git::git(&repo.root, &["ls-files", "--", "harness.lock"]).expect("ls");
    assert!(!tracked.is_empty(), "harness.lock is untracked");

    let cfg = config::load(&repo.root).expect("config");
    let results = run(&repo, &cfg);
    assert_eq!(count(&results, "litter"), Some(0), "{}", render(&results));
}

#[test]
fn a_file_added_after_the_install_is_litter() {
    let (repo, cfg) = conventional();
    repo.write("scratch.md", "x\n");
    repo.commit_all("scratch");
    let results = run(&repo, &cfg);
    let found = findings(&results, "litter");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert_eq!(found[0].path, "scratch.md");
}

#[test]
fn no_adapter_install_reports_its_own_files() {
    for name in enallagi::agent::presets().keys() {
        let repo = Repo::new();
        // tracked before the install, so everything the install writes lands in the product's history
        repo.write(".enallagi/enallagi.toml", "");
        repo.commit_all("harness dir");
        enallagi::init::install(
            &repo.root,
            &enallagi::init::InitOpts {
                adapter: Some(name.clone()),
                dry_run: false,
            },
        )
        .expect("install");
        repo.commit_all("harness");
        let cfg = config::load(&repo.root).expect("config");
        let results = run(&repo, &cfg);
        assert_eq!(
            count(&results, "litter"),
            Some(0),
            "{name}\n{}",
            render(&results)
        );
    }
}

#[test]
fn a_traceless_install_leaves_no_litter() {
    for name in enallagi::agent::presets().keys() {
        let repo = Repo::new();
        // no harness directory in the product's history, so the install is excluded and read as untracked or ignored
        repo.write("src/main.rs", "fn main() {}\n");
        repo.commit_all("product");
        enallagi::init::install(
            &repo.root,
            &enallagi::init::InitOpts {
                adapter: Some(name.clone()),
                dry_run: false,
            },
        )
        .expect("install");
        let cfg = config::load(&repo.root).expect("config");
        let results = run(&repo, &cfg);
        assert_eq!(
            count(&results, "litter"),
            Some(0),
            "{name}\n{}",
            render(&results)
        );
    }
}

#[test]
fn the_seeded_learnings_all_predate_the_gate() {
    let (repo, cfg) = seeded();
    assert_eq!(count(&run(&repo, &cfg), "learning-ungated"), Some(0));
}

#[test]
fn a_dated_learning_with_no_eval_is_reported() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "LEARNINGS.md",
        "- [2026-09-02] the check cache served a green nobody ran -> always run `./selftest.sh` uncached.\n",
    );
    assert_eq!(count(&run(&repo, &cfg), "learning-ungated"), Some(1));
}

#[test]
fn a_dated_learning_is_told_where_to_move() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "LEARNINGS.md",
        "- [2026-09-02] the check cache served a green nobody ran -> always run `./selftest.sh` uncached (evals/cache-green).\n",
    );
    fs::create_dir_all(repo.root.join("evals/cache-green")).expect("mkdir");
    let results = run(&repo, &cfg);
    let found = findings(&results, "learning-ungated");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0].message.contains("## Earned rules")
            && found[0].message.contains(".enallagi/DECISIONS.md"),
        "{}",
        found[0].message
    );
}

#[test]
fn an_earned_rule_counts_against_the_cap() {
    let (repo, cfg) = seeded();
    let path = config::instance_path(&repo.root, &config::harness_dir(&repo.root), "DECISIONS.md");
    let rules: String = (1..=8)
        .map(|n| {
            format!("- [2026-09-0{n}] a rule that cost a run -> do the other thing (`git log`).\n")
        })
        .collect();
    let text = fs::read_to_string(&path).expect("decisions");
    let under = format!("## Earned rules\n\n{rules}");
    fs::write(&path, text.replacen("## Earned rules\n", &under, 1)).expect("write");
    let out = render(&run(&repo, &cfg));
    assert_eq!(
        out.lines()
            .filter(|l| l.contains("against a cap of 12"))
            .count(),
        1,
        "{out}"
    );
}

#[test]
fn a_learnings_file_over_its_cap_is_reported() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "LEARNINGS.md",
        "- [2026-09-02] the check cache served a green nobody ran -> always run `./selftest.sh` uncached (evals/cache-green).\n",
    );
    fs::create_dir_all(repo.root.join("evals/cache-green")).expect("mkdir");
    for n in 1..=8 {
        append(
            &repo,
            "LEARNINGS.md",
            &format!("- [2026-09-0{n}] a rule that cost a run -> do the other thing (evals/cache-green).\n"),
        );
    }
    let out = render(&run(&repo, &cfg));
    assert_eq!(
        out.lines()
            .filter(|l| l.contains("against a cap of 12"))
            .count(),
        1,
        "{out}"
    );
}

// two rewordings of one friction, plus a near-miss pair that opens with the same words but isn't a repeat; both are asserted because either alone passes a broken probe

const FRICTION_FIXTURE: &str = "
## fixture — a friction
friction: FOURTH sighting of a check firing on the prose that documents it, and the first where the
next: nothing

## fixture — the same one, reworded
friction: FIFTH sighting of a check firing on the prose that documents it - and the first where the
next: nothing

## fixture — a different friction that shares an opening
friction: none new. One thing worth the next lane's time, not a rule: `.enallagi/hooks/probes.sh`
next: nothing

## fixture — and another, sharing the same opening
friction: none new. One thing worth the next lane's time: the selftest assertion deliberately does
next: nothing
";

#[test]
fn a_reworded_repeat_of_one_friction_is_reported() {
    let (repo, cfg) = seeded();
    append(&repo, "PROGRESS.md", FRICTION_FIXTURE);
    assert_eq!(count(&run(&repo, &cfg), "friction-repeat"), Some(1));
}

#[test]
fn and_a_repeat_a_dated_kill_line_names_is_covered() {
    let (repo, cfg) = seeded();
    append(&repo, "PROGRESS.md", FRICTION_FIXTURE);
    append(
        &repo,
        "DECISIONS.md",
        "\n## Rejected findings\n- [2026-09-13] the same friction is recorded 2 times: FIFTH sighting of a check firing on the prose that documents it - and the first where the \u{2014} refuted by `enallagi eval --gate prose-check`: `GATE prose-check REJECT the case passes with the rule ablated, so the rule changed no outcome`; PROGRESS.md keeps the evidence\n",
    );
    assert_eq!(count(&run(&repo, &cfg), "friction-repeat"), Some(0));
}

#[test]
fn frictions_sharing_words_do_not_collapse() {
    let (repo, cfg) = seeded();
    append(&repo, "PROGRESS.md", FRICTION_FIXTURE);
    let out = render(&run(&repo, &cfg));
    assert_eq!(
        out.lines()
            .filter(
                |l| l.starts_with("FINDING friction-repeat .enallagi/PROGRESS.md:")
                    && l.contains("recorded 2 times")
                    && l.contains("FIFTH sighting")
            )
            .count(),
        1,
        "{out}"
    );
}

// one trap written twice, the second far longer: over the whole line's words the pair overlaps 0.2048
const LONG_REPEAT_FIXTURE: &str = "
## fixture — a stale test binary
friction: `sed -i.bak` then `mv` back restores the original mtime, so cargo reused the test binary built from the mutation and the suite failed on an assertion the source no longer carried. Nothing tells a stale test binary from a real red; `touch` on the file fixed it.
next: nothing

## fixture — the same trap, explained at length
friction: restoring the mutated source with `cp` left an older mtime than the test binary, the exact trap T-007's entry recorded, and `touch` on the file was needed before the green re-run meant anything. That is the second occurrence of a stale test binary reading as a real verdict and it is now owed a .enallagi/LEARNINGS.md line, which an implementer cannot write: nothing in the check distinguishes a rebuilt test binary from a reused one.
next: nothing
";

#[test]
fn a_longer_rewrite_of_one_friction_is_reported() {
    let (repo, cfg) = seeded();
    let path = config::instance_path(&repo.root, &config::harness_dir(&repo.root), "PROGRESS.md");
    let before = fs::read_to_string(&path).expect("progress").lines().count();
    append(&repo, "PROGRESS.md", LONG_REPEAT_FIXTURE);
    let results = run(&repo, &cfg);
    let found = findings(&results, "friction-repeat");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert_eq!(found[0].path, ".enallagi/PROGRESS.md");
    assert_eq!(found[0].line, before + 7);
    assert!(
        found[0].message.contains("recorded 2 times"),
        "{}",
        found[0].message
    );
}

fn with_driver(body: &str) -> (Repo, Config) {
    let (repo, cfg) =
        seeded_with("[layout]\ndriver_command = \"$ENALLAGI_ROOT/src/fakedriver.sh\"\n");
    repo.write("src/fakedriver.sh", &format!("#!/usr/bin/env bash\n{body}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            repo.root.join("src/fakedriver.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
    }
    (repo, cfg)
}

const SHORTFALL: &str = "echo \"FINDING the artifact answered but wrote nothing to the store\"\n";

#[test]
fn a_configured_driver_runs_and_exits_0() {
    let (repo, cfg) = with_driver(SHORTFALL);
    let results = run_with(&repo, &cfg, true);
    assert_eq!(errors(&results), Vec::<&str>::new(), "{}", render(&results));
}

#[test]
fn the_drivers_shortfall_is_one_finding() {
    let (repo, cfg) = with_driver(SHORTFALL);
    let results = run_with(&repo, &cfg, true);
    assert_eq!(count(&results, "driver"), Some(1), "{}", render(&results));
    assert!(
        render(&results)
            .contains("FINDING driver $ENALLAGI_ROOT/src/fakedriver.sh:0 the artifact answered but wrote nothing to the store"),
        "{}",
        render(&results)
    );
}

#[test]
fn configured_but_harness_driver_unset_is_still_off() {
    let (repo, cfg) = with_driver(SHORTFALL);
    let out = render(&run_with(&repo, &cfg, false));
    assert!(out.contains("PROBE driver OFF -- "), "{out}");
}

#[test]
fn an_unreachable_driver_fails_the_run() {
    let (repo, cfg) = with_driver("echo \"connection refused\" >&2\nexit 7\n");
    let results = run_with(&repo, &cfg, true);
    assert_eq!(errors(&results), vec!["driver"], "{}", render(&results));
}

#[test]
fn an_unreachable_artifact_is_an_error() {
    let (repo, cfg) = with_driver("echo \"connection refused\" >&2\nexit 7\n");
    let out = render(&run_with(&repo, &cfg, true));
    assert!(out.contains("PROBE driver ERROR "), "{out}");
    assert!(
        out.contains("the driver exited 7 without reaching the artifact"),
        "{out}"
    );
}

#[test]
fn every_declared_skill_names_its_enforcing_gate() {
    // asserts the COUNT, not just emptiness: an entry deleted to make this pass is the thing that fails it
    let (repo, cfg) = seeded();
    assert_eq!(cfg.skill.len(), 8);
    let results = run(&repo, &cfg);
    let found = findings(&results, "skill-ungated");
    let undefined: Vec<&str> = found
        .iter()
        .filter(|f| f.message.contains("names gate "))
        .map(|f| f.message.as_str())
        .collect();
    assert_eq!(undefined, Vec::<&str>::new());
}

#[test]
fn a_skill_with_no_enforcement_is_reported() {
    let gated = "[[skill]]\nid = \"fixture-gated\"\nsource = \"s\"\npath = \"p\"\ngate = \"ponytail-ceiling\"\nwhy = \"w\"\n";
    let hope = "[[skill]]\nid = \"fixture-hope\"\nsource = \"s\"\npath = \"p\"\ngate = \"none\"\nwhy = \"w\"\n";

    let (repo, cfg) = seeded_with(&format!("{gated}{hope}"));
    let results = run(&repo, &cfg);
    assert_eq!(count(&results, "skill-ungated"), Some(1));
    assert_eq!(
        render(&results)
            .lines()
            .filter(|l| l.starts_with("FINDING skill-ungated ")
                && l.contains("fixture-hope is declared with gate: none"))
            .count(),
        1
    );

    let (repo, cfg) = seeded_with(gated);
    assert_eq!(count(&run(&repo, &cfg), "skill-ungated"), Some(0));
}

// three markers: killed in place, killed at a moved line, and killed by neither of two near-miss shapes
fn ceilings() -> (Repo, Config) {
    let (repo, cfg) = seeded();
    let m = marker();
    repo.write(
        "src/a.sh",
        &format!("#!/usr/bin/env bash\n# {m} the first marker, killed where it stands\n"),
    );
    repo.write(
        "src/b.sh",
        &format!("#!/usr/bin/env bash\necho moved\n# {m} the second marker, killed at a line it has moved off\n"),
    );
    repo.write(
        "src/c.sh",
        &format!("#!/usr/bin/env bash\necho live\n# {m} the third marker, which nobody killed\n"),
    );
    repo.commit_all("markers");
    (repo, cfg)
}

#[test]
fn a_written_down_ceiling_is_not_reproposed() {
    let (repo, cfg) = ceilings();
    let before = count(&run(&repo, &cfg), "ponytail-ceiling").expect("count");
    assert_eq!(before, 3);
    let moved = fs::read_to_string(repo.root.join("src/b.sh")).expect("read");
    let moved_text = moved
        .lines()
        .nth(2)
        .expect("marker line")
        .trim()
        .to_string();
    append(
        &repo,
        "DECISIONS.md",
        &format!(
            "- [2026-09-04] the marker at `src/a.sh:2` — refuted by a re-run: settled.\n\
             - [2026-09-04] the marker at `src/b.sh:1`, since moved — refuted by a re-run: `{moved_text}`.\n\
             - the marker at `src/c.sh:3` — undated, so nothing decided it and it is not a kill.\n\
             \n## [T-000] an archived block, below the section\n\
             - [2026-09-04] the marker at `src/c.sh:3` — dated, but out of the section.\n"
        ),
    );
    let results = run(&repo, &cfg);
    assert_eq!(
        count(&results, "ponytail-ceiling"),
        Some(before - 2),
        "{}",
        render(&results)
    );
    let out = render(&results);
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("FINDING ponytail-ceiling src/a.sh:")
                || l.starts_with("FINDING ponytail-ceiling src/b.sh:"))
            .count(),
        0,
        "{out}"
    );
}

#[test]
fn a_ceiling_with_no_kill_line_is_still_reported() {
    let (repo, cfg) = ceilings();
    let live = |results: &[(String, ProbeResult)]| {
        render(results)
            .lines()
            .filter(|l| l.starts_with("FINDING ponytail-ceiling src/c.sh:3 "))
            .count()
    };
    assert_eq!(live(&run(&repo, &cfg)), 1);
    let moved = fs::read_to_string(repo.root.join("src/b.sh")).expect("read");
    let moved_text = moved
        .lines()
        .nth(2)
        .expect("marker line")
        .trim()
        .to_string();
    append(
        &repo,
        "DECISIONS.md",
        &format!(
            "- [2026-09-04] the marker at `src/a.sh:2` — refuted by a re-run: settled.\n\
             - [2026-09-04] the marker at `src/b.sh:1`, since moved — refuted by a re-run: `{moved_text}`.\n\
             - the marker at `src/c.sh:3` — undated, so nothing decided it and it is not a kill.\n\
             \n## [T-000] an archived block, below the section\n\
             - [2026-09-04] the marker at `src/c.sh:3` — dated, but out of the section.\n"
        ),
    );
    assert_eq!(live(&run(&repo, &cfg)), 1);
}

#[test]
fn a_slashed_row_resolves_under_source_root() {
    let (repo, cfg) = seeded_with(
        "[layout]\nsource_root = \"crate\"\ntest_file_suffix_re = '\\.rs'\ntest_decl_patterns = [\"fn {name}(\"]\n",
    );
    repo.write(
        ".enallagi/SPEC.md",
        "# SPEC\n\n## 11. Exit criteria\n\n| Criterion | Test |\n| --- | --- |\n| c | `tests/x.rs::t` |\n\n## 12. Notes\n",
    );
    let untested = |results: &[(String, ProbeResult)]| {
        render(results)
            .lines()
            .filter(|l| l.starts_with("FINDING spec-untested .enallagi/SPEC.md:7 "))
            .count()
    };
    assert_eq!(untested(&run(&repo, &cfg)), 1);
    repo.write("crate/tests/x.rs", "#[test]\nfn t() {}\n");
    let results = run(&repo, &cfg);
    assert_eq!(untested(&results), 0, "{}", render(&results));
}

#[test]
fn only_a_review_block_needs_a_scope_file() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the finished one, whose files a later cleanup deleted\nscope: src/gone.rs\nblockedBy: none\nstatus: done\n\n## [T-003] the reviewed one whose file never landed\nscope: src/missing.rs\nblockedBy: none\nstatus: review\n",
    );
    repo.write("src/other.rs", "");
    repo.commit_all("feat: T-003 touches another file");
    let results = run(&repo, &cfg);
    let found = findings(&results, "queue-hygiene");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0].message.contains("src/missing.rs"),
        "{}",
        found[0].message
    );
}

#[test]
fn a_ready_block_may_name_a_file_it_creates() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the one that adds two files\nscope: src/packs/core.ts, tests/cli.test.ts\nblockedBy: none\nstatus: ready\n",
    );
    let results = run(&repo, &cfg);
    assert_eq!(
        count(&results, "queue-hygiene"),
        Some(0),
        "{}",
        render(&results)
    );
}

#[test]
fn a_scope_directory_is_reported_as_matching_none() {
    let (repo, cfg) = seeded();
    repo.write("evals/verifier/setup.sh", "");
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the eval that adds files under a directory\nscope: evals/verifier, evals/other/new.sh\nblockedBy: none\nstatus: ready\n",
    );
    let results = run(&repo, &cfg);
    let found = findings(&results, "queue-hygiene");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0]
            .message
            .contains("evals/verifier names a directory")
            && found[0].message.contains("`evals/verifier/**`"),
        "{}",
        found[0].message
    );
}

#[test]
fn a_killed_id_reused_is_reported() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "DECISIONS.md",
        "\n- [2026-09-01] T-002 claimed a thing — refuted by `x`: y, and T-004 already carries it\n",
    );
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] a later finding under the killed number\nscope: src/a.rs\nblockedBy: none\nstatus: ready\n\n## [T-003] done under a killed number before the rule\nscope: src/b.rs\nblockedBy: none\nstatus: done\n\n## [T-004] the block the kill line names as the duplicate\nscope: src/c.rs\nblockedBy: none\nstatus: ready\n",
    );
    append(
        &repo,
        "DECISIONS.md",
        "- [2026-09-02] T-003 claimed another thing — refuted by `x`: y\n",
    );
    let results = run(&repo, &cfg);
    let found = findings(&results, "queue-hygiene");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0]
            .message
            .contains("T-002 reuses the id of a killed finding"),
        "{}",
        found[0].message
    );
}

// the two subjects the criteria name, taken from this repository's own history
const COMMENTARY_SUBJECT: &str = "fix(ci): four failures, four causes, none of them the same";
const RECORD_SUBJECT: &str = "queue: T-036 ready, docs/demo.sh joins the scope";

fn commit_subject(repo: &Repo, subject: &str) {
    repo.write("src/moved.ts", subject);
    repo.commit_all(subject);
}

fn plain_record(repo: &Repo, cfg: &Config) -> Vec<String> {
    render(&run(repo, cfg))
        .lines()
        .filter(|l| l.starts_with("FINDING plain-record "))
        .map(String::from)
        .collect()
}

#[test]
fn a_fresh_install_records_plainly() {
    let (repo, cfg) = seeded();
    let results = run(&repo, &cfg);
    assert_eq!(
        count(&results, "plain-record"),
        Some(0),
        "{}",
        render(&results)
    );
}

#[test]
fn a_subject_counting_the_change_is_reported() {
    let (repo, cfg) = seeded();
    commit_subject(&repo, COMMENTARY_SUBJECT);
    let found = plain_record(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`four causes`"), "{}", found[0]);
}

#[test]
fn a_subject_naming_what_changed_is_not() {
    let (repo, cfg) = seeded();
    commit_subject(&repo, RECORD_SUBJECT);
    assert_eq!(plain_record(&repo, &cfg), Vec::<String>::new());
}

#[test]
fn a_subject_with_an_em_dash_aside_fails() {
    let (repo, cfg) = seeded();
    commit_subject(
        &repo,
        "queue: T-014 to T-016 \u{2014} prose out of the shipped documents, and MIT",
    );
    let found = plain_record(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("em dash"), "{}", found[0]);
}

#[test]
fn a_subject_saying_why_it_matters_fails() {
    let (repo, cfg) = seeded();
    commit_subject(
        &repo,
        "fix(tests): no fixture reaches the network, which is why CI raced itself",
    );
    let found = plain_record(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("which is why"), "{}", found[0]);
}

#[test]
fn the_state_repository_log_is_read_too() {
    let (repo, cfg) = seeded();
    let state = repo.root.join(".enallagi");
    assert!(
        state.join(".git").exists(),
        "the fixture is a nested install"
    );
    for (key, value) in [("user.email", "t@t"), ("user.name", "t")] {
        enallagi::git::git(&state, &["config", key, value]).expect("identity");
    }
    enallagi::git::git(&state, &["add", "-A"]).expect("add");
    enallagi::git::git(
        &state,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            COMMENTARY_SUBJECT,
        ],
    )
    .expect("commit");
    let found = plain_record(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`four causes`"), "{}", found[0]);
}

#[test]
fn a_note_saying_what_the_work_meant_fails() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the block\nscope: src/schema.ts\nblockedBy: none\nstatus: ready\nnotes: the fixture was moved, which is why the run went green.\n",
    );
    let found = plain_record(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains(".enallagi/TASKS.md:"), "{}", found[0]);
}

#[test]
fn a_fenced_note_is_never_reported() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the block\nscope: src/schema.ts\nblockedBy: none\nstatus: ready\nnotes: the run is below.\n\n```\nfour failures, four causes, none of them the same, which is why the check was red \u{2014} and it is green now.\n```\n",
    );
    assert_eq!(plain_record(&repo, &cfg), Vec::<String>::new());
}

#[test]
fn a_pasted_command_in_a_note_is_exempt() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the block\nscope: src/schema.ts\nblockedBy: none\nstatus: ready\nnotes: `cargo test` prints `four failures, four causes, none of them the same`.\n",
    );
    assert_eq!(plain_record(&repo, &cfg), Vec::<String>::new());
}

#[test]
fn a_printed_em_dash_aside_is_reported() {
    let (repo, cfg) = seeded();
    repo.write(
        "src/report.ts",
        "console.log(\"EVAL ERROR (the fixture could not be built \u{2014} nothing was measured)\")\n",
    );
    repo.commit_all("report");
    let found = plain_record(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("src/report.ts:1"), "{}", found[0]);
}

// a verifier writes its verdict into notes:, and an implementer cannot clear someone else's words
#[test]
fn a_rejection_verdict_in_a_note_is_exempt() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the block\nscope: src/schema.ts\nblockedBy: none\nstatus: ready\nnotes: the review record.\n  REJECTED: nothing in the paste reproduced (verifier, 2026-09-17).\n  the heading list was rewritten, three numbers, and the fences are stripped.\n",
    );
    assert_eq!(plain_record(&repo, &cfg), Vec::<String>::new());
}

#[test]
fn a_note_counting_outside_a_verdict_is_reported() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the block\nscope: src/schema.ts\nblockedBy: none\nstatus: ready\nnotes: the review record.\n  REJECTED: nothing in the paste reproduced (verifier, 2026-09-17).\n  IMPLEMENTER 2026-09-17: the heading list was rewritten, three numbers, and the fences are stripped.\n",
    );
    let found = plain_record(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`three numbers`"), "{}", found[0]);
}

#[test]
fn a_verified_note_after_a_rejection_is_read() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the block\nscope: src/schema.ts\nblockedBy: none\nstatus: ready\nnotes: the review record.\n  REJECTED: nothing in the paste reproduced (verifier, 2026-09-17).\n  VERIFIED (verifier, 2026-09-17). the heading list was rewritten, three numbers, and the fences are stripped.\n",
    );
    let found = plain_record(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`three numbers`"), "{}", found[0]);
}

fn rejection_stale(repo: &Repo, cfg: &Config) -> Vec<String> {
    render(&run(repo, cfg))
        .lines()
        .filter(|l| l.starts_with("FINDING rejection-stale "))
        .map(String::from)
        .collect()
}

fn reviewed_block(id: &str, status: &str, verdicts: &str) -> String {
    format!("\n## [{id}] the block\nscope: src/schema.ts\nblockedBy: none\nstatus: {status}\nnotes: the review record.\n{verdicts}")
}

const REJECTION: &str =
    "  REJECTED: the quoted figure does not reproduce (verifier, 2026-09-17).\n";

// the shape T-064 carried: a verdict quoted in a later paragraph is a reference, not a verdict
const QUOTED: &str = "  Named and not a rejection point: two citations sit inside this block's `REJECTED:` records.\n";

#[test]
fn an_unanswered_rejection_is_reported() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        &reviewed_block(
            "T-002",
            "review",
            &format!("  IMPLEMENTER 2026-09-17: the change is committed.\n{REJECTION}"),
        ),
    );
    let found = rejection_stale(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("T-002"), "{}", found[0]);
}

#[test]
fn an_answered_rejection_is_quiet_at_review() {
    let (repo, cfg) = seeded();
    for (id, answer) in [
        ("T-002", "VERIFIED"),
        ("T-003", "Passed"),
        ("T-004", "IMPLEMENTER"),
    ] {
        append(
            &repo,
            "TASKS.md",
            &reviewed_block(
                id,
                "review",
                &format!(
                    "{REJECTION}  {answer} 2026-09-17: the rejection point is answered.\n{QUOTED}"
                ),
            ),
        );
    }
    assert_eq!(rejection_stale(&repo, &cfg), Vec::<String>::new());
}

#[test]
fn an_answered_rejection_is_quiet_at_done() {
    let (repo, cfg) = seeded();
    for (id, answer) in [
        ("T-002", "VERIFIED"),
        ("T-003", "Passed"),
        ("T-004", "IMPLEMENTER"),
    ] {
        append(
            &repo,
            "TASKS.md",
            &reviewed_block(
                id,
                "done",
                &format!(
                    "{REJECTION}  {answer} 2026-09-17: the rejection point is answered.\n{QUOTED}"
                ),
            ),
        );
    }
    assert_eq!(rejection_stale(&repo, &cfg), Vec::<String>::new());
}

#[test]
fn a_verdict_word_inside_a_rejection_is_not_read() {
    let (repo, cfg) = seeded();
    for (id, rejection) in [
        (
            "T-002",
            "  REJECTED: only three of the five criteria Passed (verifier, 2026-09-17).\n",
        ),
        (
            "T-003",
            "  REJECTED: the IMPLEMENTER left the tree dirty (verifier, 2026-09-17).\n",
        ),
    ] {
        append(&repo, "TASKS.md", &reviewed_block(id, "review", rejection));
    }
    let found = rejection_stale(&repo, &cfg);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].contains("T-002"), "{}", found[0]);
    assert!(found[1].contains("T-003"), "{}", found[1]);
}

#[test]
fn a_verdict_word_later_in_a_rejection_is_not_read() {
    let (repo, cfg) = seeded();
    let rejection =
        "  REJECTED: the diff is out of scope. Only three criteria Passed (verifier, 2026-09-18).\n";
    append(
        &repo,
        "TASKS.md",
        &reviewed_block("T-002", "review", rejection),
    );
    let found = rejection_stale(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("T-002"), "{}", found[0]);
}

#[test]
fn a_mid_line_answer_silences_a_rejection() {
    let (repo, cfg) = seeded();
    let block = format!(
        "\n## [T-002] the block\nscope: src/schema.ts\nblockedBy: none\nstatus: review\nnotes: {}\n",
        "Adjudicated 2026-09-17. REJECTED: the quoted figure does not reproduce (verifier, 2026-09-17). IMPLEMENTER 2026-09-17: the figure is re-run and stamped. VERIFIER 2026-09-17: VERIFIED."
    );
    append(&repo, "TASKS.md", &block);
    assert_eq!(rejection_stale(&repo, &cfg), Vec::<String>::new());
}

#[test]
fn check_red_says_nothing_for_a_timed_out_check() {
    let (repo, mut cfg) = seeded();
    cfg.check.force = "sleep 10".to_string();
    cfg.check.timeout = "2s".to_string();
    let report = enallagi::gates::check_delta(&repo.root, &cfg, true);
    assert!(report.timed_out.is_some(), "{report:?}");
    let outcome = report.outcome();
    assert!(
        !outcome.ran && !outcome.red,
        "a hang read as a check that ran"
    );
    let ctx = ProbeCtx {
        root: &repo.root,
        cfg: &cfg,
        check: Some(&outcome),
        driver: false,
    };
    let results = probes::run_all(&ctx, &["check-red".to_string()]);
    assert_eq!(count(&results, "check-red"), None);
    assert_eq!(errors(&results), vec!["check-red"]);
}

const GUIDE: &str = "# Contributing\n\nOpen an issue first. Run the tests before you push.\nWe use AI to label new issues.\n\nWe do not accept pull requests written by AI\ntools or LLMs. Such pull requests will be closed.\n";

fn policy(repo: &Repo, cfg: &Config) -> Vec<probes::Finding> {
    let ctx = ProbeCtx {
        root: &repo.root,
        cfg,
        check: Some(&GREEN),
        driver: false,
    };
    let results = probes::run_all(&ctx, &["contribution-policy".to_string()]);
    findings(&results, "contribution-policy").to_vec()
}

#[test]
fn a_guide_refusing_generated_work_is_reported() {
    let (repo, cfg) = seeded();
    assert_eq!(policy(&repo, &cfg), Vec::new());

    repo.write("CONTRIBUTING.md", GUIDE);
    let found = policy(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, "CONTRIBUTING.md");
    assert_eq!(found[0].line, 6);
    assert!(
        found[0]
            .message
            .contains("We do not accept pull requests written by AI tools or LLMs."),
        "{}",
        found[0].message
    );
}

#[test]
fn a_guide_without_a_refusal_reports_nothing() {
    let (repo, cfg) = seeded();
    repo.write(
        ".github/CONTRIBUTING.md",
        "# Contributing\n\nWe use AI to label new issues. Run the tests.\n",
    );
    repo.write(
        ".github/PULL_REQUEST_TEMPLATE.md",
        "Describe the change. Do not skip the tests.\n",
    );
    assert_eq!(policy(&repo, &cfg), Vec::new());
}

#[test]
fn every_policy_file_is_read() {
    let (repo, cfg) = seeded();
    for file in [
        "CONTRIBUTING.md",
        ".github/CONTRIBUTING.md",
        "AI_POLICY.md",
        ".github/PULL_REQUEST_TEMPLATE.md",
    ] {
        repo.write(file, "LLM-generated changes must be disclosed.\n");
    }
    let found = policy(&repo, &cfg);
    let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "CONTRIBUTING.md",
            ".github/CONTRIBUTING.md",
            "AI_POLICY.md",
            ".github/PULL_REQUEST_TEMPLATE.md",
        ]
    );
}

#[test]
fn a_refusal_in_a_heading_is_reported() {
    let (repo, cfg) = seeded();
    repo.write(
        "CONTRIBUTING.md",
        "# Contributing\n\n## No AI-generated pull requests\n\nThey will be closed.\n",
    );
    let found = policy(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 3);
    assert!(
        !found[0].message.contains("Contributing") && !found[0].message.contains("closed"),
        "{}",
        found[0].message
    );
}

#[test]
fn a_heading_lends_no_terms_to_its_body() {
    let (repo, cfg) = seeded();
    repo.write("CONTRIBUTING.md", "# AI\nWe will close it.\n");
    assert_eq!(policy(&repo, &cfg), Vec::new());
}

const TEST_GLOB: &str = "[layout]\ntest_glob = [\"tests/*.rs\"]\n";

fn unsubstituted(repo: &Repo, cfg: &Config) -> Vec<probes::Finding> {
    findings(&run(repo, cfg), "prompt-unsubstituted").to_vec()
}

#[test]
fn a_matched_test_glob_is_substituted_cleanly() {
    let (repo, cfg) = seeded_with(TEST_GLOB);
    repo.write("tests/a.rs", "#[test]\nfn a() {}\n");
    repo.commit_all("a test");
    assert_eq!(unsubstituted(&repo, &cfg), Vec::new());
}

#[test]
fn a_token_left_in_a_role_is_reported() {
    let (repo, cfg) = seeded_with(TEST_GLOB);
    repo.write("tests/a.rs", "#[test]\nfn a() {}\n");
    repo.commit_all("a test");
    // built, never written literally: this repository's own probe must not see the fixture
    let token = format!("__{}__", "NOT_A_KEY");
    let path = repo.root.join(".enallagi/roles/scout.md");
    let text = fs::read_to_string(&path).expect("scout.md");
    fs::write(&path, format!("{text}run {token}\n")).expect("write");
    let found = unsubstituted(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, ".enallagi/roles/scout.md");
    assert_eq!(found[0].line, text.lines().count() + 1);
    assert!(found[0].message.contains(&token), "{found:?}");
}

#[test]
fn a_test_glob_matching_nothing_is_reported() {
    let (repo, cfg) = seeded_with(TEST_GLOB);
    let found = unsubstituted(&repo, &cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, ".enallagi/roles/verifier.md");
    assert!(found[0].message.contains("'tests/*.rs'"), "{found:?}");
    let line = fs::read_to_string(repo.root.join(&found[0].path)).expect("verifier.md");
    let line = line.lines().nth(found[0].line - 1).expect("the line");
    assert!(line.contains("git diff $BASE -- 'tests/*.rs'"), "{line}");
}

#[test]
fn an_unprecomputed_check_honours_timeout() {
    let (repo, mut cfg) = seeded();
    cfg.check.force = "sleep 10".to_string();
    cfg.check.timeout = "2s".to_string();
    let ctx = ProbeCtx {
        root: &repo.root,
        cfg: &cfg,
        check: None,
        driver: false,
    };
    let started = std::time::Instant::now();
    let out = render(&probes::run_all(&ctx, &["check-red".to_string()]));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "{out}"
    );
    assert!(out.starts_with("PROBE check-red ERROR "), "{out}");
    assert!(out.contains("ran past 2s"), "{out}");
}

#[test]
fn a_check_not_on_path_could_not_be_run() {
    let (repo, mut cfg) = seeded();
    cfg.check.force = "no-such-check-on-path".to_string();
    let ctx = ProbeCtx {
        root: &repo.root,
        cfg: &cfg,
        check: None,
        driver: false,
    };
    let out = render(&probes::run_all(&ctx, &["check-red".to_string()]));
    assert!(
        out.starts_with("PROBE check-red ERROR the check could not be run:"),
        "{out}"
    );
}

fn criterion(repo: &Repo, status: &str, tokens: &str) {
    append(
        repo,
        "TASKS.md",
        &format!("\n## [T-002] the block whose criteria name tests\nscope: src/a.rs\nblockedBy: none\nstatus: {status}\nrows: none — harness\ncriteria:\n  - `{tokens}` passes\n  - the check exits 0\nnotes: none\n"),
    );
}

fn renamed(repo: &Repo) {
    repo.write("src/a.rs", "fn the_old_test_name() {}\n");
    repo.commit_all("old");
    repo.write("src/a.rs", "fn the_new_test_name() {}\n");
    repo.commit_all("new");
}

#[test]
fn a_criterion_naming_a_removed_test_is_found() {
    let (repo, cfg) = seeded_with("[layout]\nsource_root = \"src\"\n");
    renamed(&repo);
    criterion(&repo, "ready", "the_old_test_name");
    let results = run(&repo, &cfg);
    let found: Vec<_> = findings(&results, "queue-uncovered")
        .iter()
        .filter(|f| f.message.contains("T-002"))
        .collect();
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0].message.contains("the_old_test_name"),
        "{}",
        found[0].message
    );
    assert_eq!(found[0].path, ".enallagi/TASKS.md");
}

#[test]
fn a_criterion_naming_a_live_or_new_test_is_quiet() {
    let (repo, cfg) = seeded_with("[layout]\nsource_root = \"src\"\n");
    renamed(&repo);
    criterion(&repo, "ready", "the_new_test_name` and `the_unwritten_test");
    let results = run(&repo, &cfg);
    let found = findings(&results, "queue-uncovered");
    assert!(
        found.iter().all(|f| !f.message.contains("T-002")),
        "{}",
        render(&results)
    );
}

#[test]
fn a_done_block_naming_a_removed_test_is_quiet() {
    let (repo, cfg) = seeded_with("[layout]\nsource_root = \"src\"\n");
    renamed(&repo);
    criterion(&repo, "done", "the_old_test_name");
    let results = run(&repo, &cfg);
    let found = findings(&results, "queue-uncovered");
    assert!(
        found.iter().all(|f| !f.message.contains("T-002")),
        "{}",
        render(&results)
    );
}
