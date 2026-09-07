//! The probe assertions from `selftest.sh:107-231`, `:301-365` and
//! `:1034-1133`, one test per assertion and named after it.

use harness::config::{self, Config};
use harness::fixture::Repo;
use harness::probes::{self, CheckOutcome, ProbeCtx, ProbeResult};
use std::fs;
use std::path::PathBuf;

// ------------------------------------------------------------------ the seed

fn templates() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates")
}

fn template(name: &str, cfg: &Config) -> String {
    let text = fs::read_to_string(templates().join(name)).expect("read template");
    config::subst(&text, cfg)
}

/// What `harness init` (Task 13) seeds. Until it lands the tests write the
/// documents themselves, from the same templates through the same `subst`.
///
/// The three placeholders under the harness directory stand in for the
/// installed launcher and its hooks: the seeded RAILS.md names them as the
/// enforcement the check runs, and `rail-unenforced` asks whether they exist
/// and whether anything runs them. Without them a fresh tree reports every
/// process rail, which is the install being absent rather than a rail being
/// unenforced.
fn seed(repo: &Repo, cfg: &Config) {
    let dir = &cfg.layout.harness_dir;
    for (name, target) in [
        ("TASKS.md", "TASKS.md".to_string()),
        ("PROGRESS.md", "PROGRESS.md".to_string()),
        ("LEARNINGS.md", "LEARNINGS.md".to_string()),
        ("DECISIONS.md", "DECISIONS.md".to_string()),
        ("dot.check-baseline", ".check-baseline".to_string()),
        ("AGENTS.md", cfg.layout.context_file.clone()),
        ("SPEC.section.md", cfg.layout.spec.clone()),
        ("RAILS.md", format!("{dir}/RAILS.md")),
    ] {
        repo.write(&target, &template(name, cfg));
    }
    repo.write(
        &format!("{dir}/loop.sh"),
        &format!(
            "#!/usr/bin/env bash\n\
             # Stands in for the installed launcher until `harness init` writes one. The rails\n\
             # name these as the enforcement the check runs, and the probe reads this file to\n\
             # find out: {dir}/loop.sh {dir}/hooks/check-gate.sh {dir}/hooks/probes.sh\n"
        ),
    );
    repo.write(
        &format!("{dir}/hooks/check-gate.sh"),
        "#!/usr/bin/env bash\n",
    );
    repo.write(&format!("{dir}/hooks/probes.sh"), "#!/usr/bin/env bash\n");
    repo.write(&format!("{dir}/roles/verifier.md"), "# verifier\n");
    repo.commit_all("harness");
}

/// A repo with the documents seeded and the default configuration.
fn seeded() -> (Repo, Config) {
    let repo = Repo::new();
    let cfg = config::load(&repo.root).expect("default config");
    seed(&repo, &cfg);
    (repo, cfg)
}

/// A repo whose `harness.toml` carries `overrides`, seeded from the merged
/// configuration and with that file tracked.
fn seeded_with(overrides: &str) -> (Repo, Config) {
    let repo = Repo::new();
    repo.write("harness.toml", overrides);
    let cfg = config::load(&repo.root).expect("config");
    seed(&repo, &cfg);
    (repo, cfg)
}

/// The check is stubbed green: `check-red` is asserted by Task 8's gate tests,
/// and running `bun run check` here would only measure the host.
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

/// `sed -n 's/^PROBE <name> //p'` -- the count, or None when the probe did not
/// report one.
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

fn append(repo: &Repo, rel: &str, text: &str) {
    let path = repo.root.join(rel);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    fs::write(&path, format!("{existing}{text}")).expect("append");
}

// --------------------------------------------------- selftest.sh:107-231

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
fn the_row_parser_reads_the_seeded_criteria_table() {
    let (repo, cfg) = seeded();
    assert_eq!(count(&run(&repo, &cfg), "spec-untested"), Some(1));
}

#[test]
fn a_fresh_install_has_two_unenforced_rails_both_wanting_test_hashes() {
    let (repo, cfg) = seeded();
    let results = run(&repo, &cfg);
    let found = findings(&results, "rail-unenforced");
    assert_eq!(found.len(), 2, "{}", render(&results));
    for f in found {
        assert!(f.message.contains("test-hashes.json"), "{}", f.message);
    }
}

#[test]
fn the_seeded_criterion_is_untested_and_no_task_in_flight_names_it() {
    let (repo, cfg) = seeded();
    assert_eq!(count(&run(&repo, &cfg), "queue-uncovered"), Some(1));
}

#[test]
fn harness_immutable_names_loop_and_no_test_hashes_covers_it() {
    let (repo, cfg) = seeded();
    let results = run(&repo, &cfg);
    let found = findings(&results, "hash-uncovered");
    assert_eq!(found.len(), 1, "{}", render(&results));
    assert!(
        found[0]
            .message
            .contains("`harness-immutable` names .harness/loop.sh"),
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
fn with_no_driver_command_the_driver_says_so_rather_than_scoring_zero() {
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
fn the_seeded_learnings_are_all_seed_entries_which_predate_the_gate() {
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
fn and_the_same_rule_naming_an_eval_that_exists_is_not() {
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

// ---- a reworded friction is the same friction -------------------------------
// The exact-match key this replaced never collided, so five sightings of one
// friction sat in the record and the probe read 0 (TASKS.md [T-045]). The
// firing pair below is two of those five, taken verbatim from this package's
// own PROGRESS.archive.md:428 and :477 -- a FOURTH and a FIFTH sighting of the
// same thing, worded differently. The pair under it is the guard, and it is the
// closest measured NON-repeat in the same record (:1253 and :1351): two
// different frictions that open with the same eleven words. Both are asserted
// because either alone passes on a broken probe.

const FRICTION_FIXTURE: &str = "
## fixture — a friction
friction: FOURTH sighting of a check firing on the prose that documents it, and the first where the
next: nothing

## fixture — the same one, reworded
friction: FIFTH sighting of a check firing on the prose that documents it - and the first where the
next: nothing

## fixture — a different friction that shares an opening
friction: none new. One thing worth the next lane's time, not a rule: `.harness/hooks/probes.sh`
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
fn and_two_frictions_that_merely_share_words_are_not_collapsed_into_it() {
    let (repo, cfg) = seeded();
    append(&repo, "PROGRESS.md", FRICTION_FIXTURE);
    let out = render(&run(&repo, &cfg));
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("FINDING friction-repeat PROGRESS.md:")
                && l.contains("recorded 2 times")
                && l.contains("FIFTH sighting"))
            .count(),
        1,
        "{out}"
    );
}

// --------------------------------------------------- selftest.sh:301-365

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
    // the FINDING line reaches the scout verbatim
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
fn a_driver_that_cannot_reach_the_artifact_fails_the_whole_probe_run() {
    let (repo, cfg) = with_driver("echo \"connection refused\" >&2\nexit 7\n");
    let results = run_with(&repo, &cfg, true);
    assert_eq!(errors(&results), vec!["driver"], "{}", render(&results));
}

#[test]
fn an_unreachable_artifact_is_error_never_a_count_of_zero() {
    let (repo, cfg) = with_driver("echo \"connection refused\" >&2\nexit 7\n");
    let out = render(&run_with(&repo, &cfg, true));
    assert!(out.contains("PROBE driver ERROR "), "{out}");
    assert!(
        out.contains("the driver exited 7 without reaching the artifact"),
        "{out}"
    );
}

// --------------------------------------------------- selftest.sh:1034-1133

#[test]
fn every_declared_skill_names_its_enforcing_gate() {
    // The package default, checked against the two things a gate may name: the
    // built-in gate and probe names, and the installed rails. The expected
    // value carries the COUNT, so an entry deleted to make this pass is the
    // thing that fails it (LEARNINGS.md, zero-as-pass).
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

/// Three markers: one killed where it stands, one killed at a line it has since
/// moved off, and one nobody killed. The third is named twice more in the two
/// shapes that are NOT kills -- an undated line inside the section, and a dated
/// line under a block below it -- so a probe that takes either for a kill fails.
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
fn a_ceiling_whose_kill_is_already_written_down_is_not_re_proposed() {
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
