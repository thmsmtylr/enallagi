//! The probe assertions, one test per assertion and named after it.

use harness::config::{self, Config};
use harness::fixture::Repo;
use harness::probes::{self, CheckOutcome, ProbeCtx, ProbeResult};
use std::fs;

// install is the seed: docs, rails and roles all come from the binary, so a probe reads the tree an operator would get
fn seeded_with(overrides: &str) -> (Repo, Config) {
    let repo = Repo::new();
    repo.init_harness(overrides);
    harness::git::git(&repo.root, &["add", "-A"]).expect("add");
    harness::git::git(
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
    assert_eq!(results.len(), 21, "{}", render(&results));
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
    let (repo, cfg) = seeded();
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

#[test]
fn harness_immutable_has_no_hash_key() {
    // the rail names the file that is the gate; with the launcher a binary, that file is harness.toml
    let (repo, cfg) = seeded();
    let results = run(&repo, &cfg);
    let found = findings(&results, "hash-uncovered");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0]
            .message
            .contains("`harness-immutable` names harness.toml"),
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
fn a_rule_naming_a_real_eval_is_not() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "LEARNINGS.md",
        "- [2026-09-02] the check cache served a green nobody ran -> always run `./selftest.sh` uncached (evals/cache-green).\n",
    );
    fs::create_dir_all(repo.root.join("evals/cache-green")).expect("mkdir");
    assert_eq!(count(&run(&repo, &cfg), "learning-ungated"), Some(0));
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
        "\n## Rejected findings\n- [2026-09-13] the same friction is recorded 2 times: FIFTH sighting of a check firing on the prose that documents it - and the first where the \u{2014} refuted by `harness eval --gate prose-check`: `GATE prose-check REJECT the case passes with the rule ablated, so the rule changed no outcome`; PROGRESS.md keeps the evidence\n",
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

fn with_driver(body: &str) -> (Repo, Config) {
    let (repo, cfg) =
        seeded_with("[layout]\ndriver_command = \"$HARNESS_ROOT/src/fakedriver.sh\"\n");
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
            .contains("FINDING driver $HARNESS_ROOT/src/fakedriver.sh:0 the artifact answered but wrote nothing to the store"),
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
    assert_eq!(cfg.skill.len(), 7);
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
fn only_an_open_block_needs_a_scope_file() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        "TASKS.md",
        "\n## [T-002] the finished one, whose files a later cleanup deleted\nscope: src/gone.rs\nblockedBy: none\nstatus: done\n\n## [T-003] the open one nobody can take\nscope: src/missing.rs\nblockedBy: none\nstatus: ready\n",
    );
    let results = run(&repo, &cfg);
    let found = findings(&results, "queue-hygiene");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0].message.contains("src/missing.rs"),
        "{}",
        found[0].message
    );
}
