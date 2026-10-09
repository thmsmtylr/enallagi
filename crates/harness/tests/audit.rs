//! `enallagi audit` over a seeded fixture and a scripted auditor: the signal it hands over, the citations it checks, and the entries it writes or merges.

use enallagi::audit;
use enallagi::config::{self, Config};
use enallagi::fixture::Repo;
use enallagi::probes::{self, CheckOutcome, ProbeCtx, ProbeResult};
use std::fs;

const PROGRESS: &str = "
## 2026-09-01 — T-001 — landed
friction: `enallagi worktree` reported merged true on a failed fetch, so the lane looked landed
next: T-002

## 2026-09-02 — T-002 — landed
friction: the new test passed with the filter deleted, so it guarded nothing
next: T-003

## 2026-09-03 — T-003 — landed
friction: the lane waited on a slow disk
next: none
";

const REJECTED: &str = "- [2026-09-03] a mutant survived: the citation floor scanned comments while the defect sat in a fixture — refuted by `cargo test -q floor`\n";

const ARCHIVED: &str = "
## [T-010] spawn loses the stage result
scope: src/agent.rs
status: done
notes: https://github.com/o/r/pull/3#discussion_r11
  > spawn returns Err after a full StageResult, discarding the stage's accounting

## [T-011] the reader
scope: src/read.rs
status: done
notes: VERIFIER 2026-09-03
  REJECTED: the probe returns ok on a failed read
";

const QUEUED: &str = "
## [T-012] the citation test
scope: src/floor.rs
status: proposed
notes: https://github.com/o/r/pull/4#discussion_r12
  > this test selects 0 tests, so it passes with the logic deleted
";

// stubbed green: the audit never runs the check, and neither do the two probes it is weighed against
const GREEN: CheckOutcome = CheckOutcome {
    ran: true,
    red: false,
    output: String::new(),
};

// the scripted auditor logs each prompt it is handed and prints the answer the test gave it
fn seeded() -> (Repo, Config) {
    let repo = Repo::new();
    let argv = answering(&repo, "");
    repo.init_harness(&format!(
        "[agent]\npreset = \"custom\"\ncommand = {argv:?}\n\n[check]\ncommand = \"true\"\n"
    ));
    let cfg = config::load(&repo.root).expect("config");
    (repo, cfg)
}

fn answering(repo: &Repo, answer: &str) -> Vec<String> {
    repo.stub_agent(&format!(
        "printf '%s\\n----\\n' \"$1\" >> src/.prompts\ncat <<'ANSWER'\n{answer}\nANSWER\n"
    ))
}

fn learning(class: &str, text: &str, cites: &[&str]) -> String {
    let cites: Vec<String> = cites.iter().map(|c| format!("`{c}`")).collect();
    format!(
        "BEGIN ENALLAGI LEARNING\nclass: {class}\n{text}\ninstances: {}\nEND ENALLAGI LEARNING\n",
        cites.join(", ")
    )
}

fn prompts(repo: &Repo) -> Vec<String> {
    fs::read_to_string(repo.root.join("src/.prompts"))
        .unwrap_or_default()
        .split("\n----\n")
        .filter(|p| !p.trim().is_empty())
        .map(String::from)
        .collect()
}

fn append(repo: &Repo, rel: &str, text: &str) {
    let path = repo.root.join(rel);
    let mut body = fs::read_to_string(&path).unwrap_or_default();
    body.push_str(text);
    fs::write(path, body).expect("append");
}

// the rejected entry sits under its heading, and the archived blocks after it, as `enallagi run` leaves them
fn with_signal(repo: &Repo) {
    append(repo, ".enallagi/PROGRESS.md", PROGRESS);
    let rel = ".enallagi/DECISIONS.md";
    let decisions = fs::read_to_string(repo.root.join(rel)).expect("decisions");
    let decisions = decisions.replacen(
        "\n## Rejected findings\n",
        &format!("\n## Rejected findings\n\n{REJECTED}"),
        1,
    );
    fs::write(repo.root.join(rel), format!("{decisions}{ARCHIVED}")).expect("write");
    append(repo, ".enallagi/TASKS.md", QUEUED);
}

fn at(repo: &Repo, rel: &str, needle: &str) -> String {
    let text = fs::read_to_string(repo.root.join(rel)).expect("read");
    let line = text
        .lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("{needle} not in {rel}"));
    format!("{rel}:{}", line + 1)
}

fn audit_bin(repo: &Repo) -> (i32, String) {
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("audit")
        .current_dir(&repo.root)
        .output()
        .expect("spawn");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

fn decisions(repo: &Repo) -> String {
    fs::read_to_string(repo.root.join(".enallagi/DECISIONS.md")).expect("decisions")
}

fn found(repo: &Repo, cfg: &Config, name: &str) -> Vec<probes::Finding> {
    let ctx = ProbeCtx {
        root: &repo.root,
        cfg,
        check: Some(&GREEN),
        driver: false,
    };
    match probes::run_all(&ctx, &[name.to_string()]).remove(0).1 {
        ProbeResult::Count(found) => found,
        other => panic!("{name}: {other:?}"),
    }
}

const LOST: &str =
    "a result is dropped on its way to the caller → assert on what the caller receives";
const VACUOUS: &str =
    "a test passes with its logic deleted → delete the branch and watch the test fail first";

fn two_classes(repo: &Repo) -> (Vec<String>, Vec<String>) {
    let progress = ".enallagi/PROGRESS.md";
    let decisions_rel = ".enallagi/DECISIONS.md";
    let lost = vec![
        at(repo, progress, "merged true on a failed fetch"),
        at(repo, decisions_rel, "## [T-010]"),
        at(repo, decisions_rel, "REJECTED: the probe returns ok"),
    ];
    let vacuous = vec![
        at(repo, progress, "passed with the filter deleted"),
        at(repo, decisions_rel, "a mutant survived"),
        at(repo, ".enallagi/TASKS.md", "## [T-012]"),
    ];
    (lost, vacuous)
}

fn refs(cites: &[String]) -> Vec<&str> {
    cites.iter().map(String::as_str).collect()
}

#[test]
fn six_findings_of_two_classes_are_two_learnings() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let (lost, vacuous) = two_classes(&repo);
    let noise = at(&repo, ".enallagi/PROGRESS.md", "a slow disk");
    answering(
        &repo,
        &format!(
            "{}{}",
            learning("lost result", LOST, &refs(&lost)),
            learning("vacuous test", VACUOUS, &refs(&vacuous))
        ),
    );
    let report = audit::run(&repo.root, &cfg).expect("audit");
    let classes: Vec<&str> = report.proposed.iter().map(|p| p.class.as_str()).collect();
    assert_eq!(classes, ["lost result", "vacuous test"], "{report:?}");
    let got: Vec<Vec<String>> = report
        .proposed
        .iter()
        .map(|p| p.instances.clone())
        .collect();
    assert_eq!(got, [lost.clone(), vacuous.clone()]);

    let asked = prompts(&repo);
    assert_eq!(asked.len(), 1, "{asked:#?}");
    let role = include_str!("../../../roles/auditor.md");
    let protocol = role.lines().find(|l| l.starts_with("1. ")).expect("step 1");
    assert!(asked[0].contains(protocol), "{}", asked[0]);
    for cite in lost.iter().chain(&vacuous).chain([&noise]) {
        assert!(
            asked[0].contains(&format!("- `{cite}`")),
            "{cite}: {}",
            asked[0]
        );
    }

    let written = decisions(&repo);
    assert!(
        written.contains(&format!("- [proposed] `lost result`: {LOST}")),
        "{written}"
    );
    assert!(
        written.contains(&format!("- [proposed] `vacuous test`: {VACUOUS}")),
        "{written}"
    );
    for cite in lost.iter().chain(&vacuous) {
        assert!(written.contains(&format!("`{cite}`")), "{cite}: {written}");
    }
    assert!(!written.contains(&format!("`{noise}`")), "{written}");
}

#[test]
fn a_learning_whose_citations_fail_is_refused() {
    let (repo, _) = seeded();
    with_signal(&repo);
    let (lost, _) = two_classes(&repo);
    let head = enallagi::git::git(&repo.root, &["rev-parse", "HEAD"]).expect("head");
    answering(
        &repo,
        &[
            learning(
                "elsewhere",
                "a lesson → a rule",
                &[&lost[0], "src/elsewhere.rs:1"],
            ),
            learning(
                "past the end",
                "a lesson → a rule",
                &[&lost[0], ".enallagi/PROGRESS.md:9999"],
            ),
            learning(
                "no such commit",
                "a lesson → a rule",
                &[&lost[0], "0123456789abcdef"],
            ),
            learning("alone", "a lesson → a rule", &[&lost[0]]),
            learning("lost result", LOST, &[&lost[1], &head]),
        ]
        .concat(),
    );
    let before = decisions(&repo);
    let (code, out) = audit_bin(&repo);
    assert_eq!(code, 0, "{out}");
    for (class, reason) in [
        ("elsewhere", "`src/elsewhere.rs:1` does not resolve"),
        (
            "past the end",
            "`.enallagi/PROGRESS.md:9999` does not resolve",
        ),
        ("no such commit", "`0123456789abcdef` does not resolve"),
        ("alone", "1 instance"),
    ] {
        assert!(
            out.lines()
                .any(|l| l.contains(&format!("`{class}` refused")) && l.contains(reason)),
            "{class}: {out}"
        );
    }
    assert!(
        out.contains("proposed `lost result` from 2 instances"),
        "{out}"
    );
    let written = decisions(&repo);
    assert_eq!(written.matches("- [proposed] ").count(), 1, "{written}");
    assert!(written.contains(&format!("`{head}`")), "{written}");
    assert_ne!(written, before);
}

#[test]
fn an_overlapping_learning_merges_into_its_entry() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let (lost, vacuous) = two_classes(&repo);
    let rel = ".enallagi/DECISIONS.md";
    let standing = format!(
        "- [proposed] `dropped result`: a result is dropped before the caller sees it → assert on what the caller receives\n  instances: `{}`, `{}`",
        lost[0], lost[1]
    );
    let text = decisions(&repo).replacen(
        "\n## Rejected findings\n",
        &format!("\n## Proposed learnings\n\n{standing}\n\n## Rejected findings\n"),
        1,
    );
    fs::write(repo.root.join(rel), text).expect("write");
    answering(
        &repo,
        &format!(
            "{}{}",
            learning("lost result", LOST, &[&lost[1], &lost[2]]),
            learning("vacuous test", VACUOUS, &refs(&vacuous))
        ),
    );
    let report = audit::run(&repo.root, &cfg).expect("audit");
    let classes: Vec<&str> = report.proposed.iter().map(|p| p.class.as_str()).collect();
    assert_eq!(classes, ["vacuous test"], "{report:?}");
    assert_eq!(report.merged.len(), 1, "{report:?}");
    let written = decisions(&repo);
    assert_eq!(written.matches("- [proposed] ").count(), 2, "{written}");
    assert!(!written.contains("`lost result`"), "{written}");
    assert!(
        written.contains(&format!(
            "  instances: `{}`, `{}`, `{}`\n",
            lost[0], lost[1], lost[2]
        )),
        "{written}"
    );
}

#[test]
fn an_earned_rule_takes_the_instances_of_its_class() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let (lost, _) = two_classes(&repo);
    let rel = ".enallagi/DECISIONS.md";
    let rule = "- [2026-09-29] a result dropped before its caller: assert on what the caller receives in `src/agent.rs`";
    let text = decisions(&repo).replacen(
        "\n## Rejected findings\n",
        &format!("\n{rule}\n\n## Rejected findings\n"),
        1,
    );
    fs::write(repo.root.join(rel), text).expect("write");
    answering(&repo, &learning("lost result", LOST, &refs(&lost)));
    let report = audit::run(&repo.root, &cfg).expect("audit");
    assert!(report.proposed.is_empty(), "{report:?}");
    let written = decisions(&repo);
    let cites: Vec<String> = lost.iter().map(|c| format!("`{c}`")).collect();
    assert!(
        written.contains(&format!("{rule}\n  instances: {}\n", cites.join(", "))),
        "{written}"
    );
    assert!(!written.contains("## Proposed learnings"), "{written}");
}

#[test]
fn a_killed_learning_is_not_proposed_again() {
    let (repo, _) = seeded();
    with_signal(&repo);
    let (lost, vacuous) = two_classes(&repo);
    let rel = ".enallagi/DECISIONS.md";
    let killed = format!(
        "- [proposed] `dropped result`: a result is dropped before the caller sees it → assert on what the caller receives\n  instances: `{}`, `{}`\n  killed: 2026-09-29 the fetch was a fixture",
        lost[0], lost[1]
    );
    let text = decisions(&repo).replacen(
        "\n## Rejected findings\n",
        &format!("\n## Proposed learnings\n\n{killed}\n\n## Rejected findings\n"),
        1,
    );
    fs::write(repo.root.join(rel), text).expect("write");
    answering(
        &repo,
        &format!(
            "{}{}",
            learning("lost result", LOST, &refs(&lost)),
            learning("vacuous test", VACUOUS, &refs(&vacuous))
        ),
    );
    let (code, out) = audit_bin(&repo);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("`lost result` killed 2026-09-29"), "{out}");
    assert!(!out.contains("proposed `lost result`"), "{out}");
    assert!(out.contains("proposed `vacuous test`"), "{out}");
    let written = decisions(&repo);
    assert!(written.contains(&killed), "{written}");
    assert!(!written.contains("`lost result`"), "{written}");
}

#[test]
fn a_proposed_learning_is_inert_until_accepted() {
    let (repo, mut cfg) = seeded();
    let learnings = fs::read_to_string(repo.root.join(".enallagi/LEARNINGS.md")).expect("read");
    cfg.layout.learnings_cap = learnings.lines().filter(|l| l.starts_with("- ")).count();
    let entry = "- [proposed] a test is watched red before it lands";
    let rel = ".enallagi/DECISIONS.md";
    let text = decisions(&repo);
    let proposed = text.replacen(
        "\n## Rejected findings\n",
        &format!("\n## Proposed learnings\n\n{entry}\n\n## Rejected findings\n"),
        1,
    );
    fs::write(repo.root.join(rel), &proposed).expect("write");
    assert!(found(&repo, &cfg, "learning-unenforced").is_empty());
    assert!(found(&repo, &cfg, "learning-ungated").is_empty());

    let accepted = text.replacen(
        "\n## Rejected findings\n",
        &format!("\n{entry}\n\n## Rejected findings\n"),
        1,
    );
    fs::write(repo.root.join(rel), &accepted).expect("write");
    let unenforced = found(&repo, &cfg, "learning-unenforced");
    assert_eq!(unenforced.len(), 1, "{unenforced:?}");
    assert_eq!(
        format!("{}:{}", unenforced[0].path, unenforced[0].line),
        at(&repo, rel, entry)
    );
    let ungated = found(&repo, &cfg, "learning-ungated");
    assert!(
        ungated.iter().any(|f| f.message.contains("cap of")),
        "{ungated:?}"
    );
}

#[test]
fn a_round_spawns_no_audit_stage() {
    let (repo, cfg) = seeded();
    let names: Vec<String> = cfg.pipeline.iter().map(|p| p.name.clone()).collect();
    let plan = enallagi::pipeline::plan(&repo.root, &cfg, &names).expect("plan");
    let spawned: Vec<&str> = plan
        .lines()
        .filter(|l| l.contains("would spawn:"))
        .collect();
    assert!(!spawned.is_empty(), "{plan}");
    assert!(spawned.iter().all(|l| !l.contains("audit")), "{spawned:#?}");
    assert!(cfg
        .pipeline
        .iter()
        .flat_map(|p| &p.stages)
        .all(|s| !s.contains("audit")));
}

#[test]
fn one_finding_asks_the_auditor_nothing() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        ".enallagi/PROGRESS.md",
        "friction: the hook swallowed the exit status\n",
    );
    let before = decisions(&repo);
    let report = audit::run(&repo.root, &cfg).expect("audit");
    assert!(report.proposed.is_empty(), "{report:?}");
    assert!(prompts(&repo).is_empty(), "{:#?}", prompts(&repo));
    assert_eq!(decisions(&repo), before);
}

#[test]
fn a_second_audit_repeats_no_proposal() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let (lost, vacuous) = two_classes(&repo);
    answering(
        &repo,
        &format!(
            "{}{}",
            learning("lost result", LOST, &refs(&lost)),
            learning("vacuous test", VACUOUS, &refs(&vacuous))
        ),
    );
    assert_eq!(
        audit::run(&repo.root, &cfg).expect("audit").proposed.len(),
        2
    );
    let once = decisions(&repo);
    let again = audit::run(&repo.root, &cfg).expect("audit");
    assert!(again.proposed.is_empty(), "{again:?}");
    assert_eq!(again.merged.len(), 2, "{again:?}");
    assert_eq!(decisions(&repo), once);
}

// the stub refuses a leading `-` the way an agent CLI's option parser does
#[test]
fn the_auditor_prompt_is_not_read_as_an_option() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let (lost, _) = two_classes(&repo);
    let answer = learning("lost result", LOST, &refs(&lost));
    repo.stub_agent(&format!(
        "case \"$1\" in -*) echo \"error: unknown option '$1'\"; exit 1;; esac\ncat <<'ANSWER'\n{answer}\nANSWER\n"
    ));
    let report = audit::run(&repo.root, &cfg).expect("audit");
    let classes: Vec<&str> = report.proposed.iter().map(|p| p.class.as_str()).collect();
    assert_eq!(classes, ["lost result"], "{report:?}");
}

#[test]
fn a_failed_auditor_writes_nothing() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let (lost, _) = two_classes(&repo);
    let before = decisions(&repo);
    let answer = learning("lost result", LOST, &refs(&lost));
    repo.stub_agent(&format!("cat <<'ANSWER'\n{answer}\nANSWER\nexit 1\n"));
    let err = audit::run(&repo.root, &cfg).expect_err("a failed auditor");
    assert!(err.to_string().contains("exited 1"), "{err}");
    assert_eq!(decisions(&repo), before);
}

fn commit_state(repo: &Repo, subject: &str) -> String {
    let state = repo.root.join(".enallagi");
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
            "--allow-empty",
            "-qm",
            subject,
        ],
    )
    .expect("commit");
    enallagi::git::git(&state, &["rev-parse", "--short", "HEAD"]).expect("head")
}

const RULE: &str =
    "- [2026-10-01] `lost result`: a result dropped before its caller → assert on what the caller receives";
const LATER: &str = "friction: spawn dropped the stage result again before the caller saw it\n";

fn earned(repo: &Repo, entry: &str) {
    let text = decisions(repo).replacen(
        "## Earned rules\n",
        &format!("## Earned rules\n\n{entry}\n"),
        1,
    );
    fs::write(repo.root.join(".enallagi/DECISIONS.md"), text).expect("write");
}

// a rule promoted over the fixture's signal, whose class then recurs in one later friction
fn recurred(repo: &Repo, revised: &[&str]) -> (String, String) {
    with_signal(repo);
    earned(repo, RULE);
    let sha = commit_state(repo, "a promoted rule");
    append(repo, ".enallagi/PROGRESS.md", LATER);
    let (lost, _) = two_classes(repo);
    let new = at(
        repo,
        ".enallagi/PROGRESS.md",
        "spawn dropped the stage result",
    );
    let mut lines = vec![
        format!("  promoted: {sha}"),
        format!("  instances: `{}`, `{}`, `{new}`", lost[0], lost[1]),
    ];
    lines.extend(revised.iter().map(|r| format!("  revised: {r}")));
    let text = decisions(repo).replacen(
        &format!("{RULE}\n"),
        &format!("{RULE}\n{}\n", lines.join("\n")),
        1,
    );
    fs::write(repo.root.join(".enallagi/DECISIONS.md"), text).expect("write");
    (sha, new)
}

fn revision(handle: &str, cites: &[&str]) -> String {
    learning("lost result", LOST, cites).replacen(
        "END ENALLAGI LEARNING",
        &format!("revises: `{handle}`\nEND ENALLAGI LEARNING"),
        1,
    )
}

#[test]
fn an_audit_stamps_each_rule_with_its_promotion() {
    let (repo, cfg) = seeded();
    earned(&repo, RULE);
    let sha = commit_state(&repo, "a promoted rule");
    commit_state(&repo, "a later round");
    audit::run(&repo.root, &cfg).expect("audit");
    assert!(
        decisions(&repo).contains(&format!("{RULE}\n  promoted: {sha}\n")),
        "{}",
        decisions(&repo)
    );
}

#[test]
fn an_ineffective_rule_goes_back_for_revision() {
    let (repo, cfg) = seeded();
    let (sha, new) = recurred(&repo, &[]);
    let handle = at(&repo, ".enallagi/DECISIONS.md", "`lost result`");
    let (lost, _) = two_classes(&repo);
    answering(&repo, &revision(&handle, &[&lost[2], &new]));
    let report = audit::run(&repo.root, &cfg).expect("audit");
    let asked = prompts(&repo);
    assert_eq!(asked.len(), 1, "{asked:#?}");
    assert!(asked[0].contains(&format!("- `{handle}`")), "{}", asked[0]);
    assert!(asked[0].contains(&format!("`{new}`")), "{}", asked[0]);
    assert!(asked[0].contains(&sha), "{}", asked[0]);
    let classes: Vec<&str> = report.proposed.iter().map(|p| p.class.as_str()).collect();
    assert_eq!(classes, ["lost result"], "{report:?}");
    let written = decisions(&repo);
    assert!(
        written.contains(&format!("- [proposed] `lost result`: {LOST}")),
        "{written}"
    );
    let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
    assert!(
        written.contains(&format!("  revised: {today} `lost result`\n")),
        "{written}"
    );

    fs::remove_file(repo.root.join("src/.prompts")).expect("clear");
    let again = audit::run(&repo.root, &cfg).expect("audit");
    assert!(again.proposed.is_empty(), "{again:?}");
    assert!(
        prompts(&repo)
            .iter()
            .all(|p| !p.contains(&format!("- `{handle}`"))),
        "{:#?}",
        prompts(&repo)
    );
}

#[test]
fn a_promoted_revision_replaces_its_rule() {
    let (repo, cfg) = seeded();
    let revised = "- [2026-10-05] `lost result v2`: spawn drops the result → return it whole";
    earned(
        &repo,
        &format!("{RULE}\n  revised: 2026-10-02 `lost result v2`\n{revised}"),
    );
    audit::run(&repo.root, &cfg).expect("audit");
    let written = decisions(&repo);
    assert!(!written.contains(RULE), "{written}");
    assert!(
        written.contains(&format!(
            "{revised}\n  revised: 2026-10-02 `lost result v2`\n"
        )),
        "{written}"
    );
}

#[test]
fn a_rule_recurring_after_two_revisions_is_removed() {
    let (repo, cfg) = seeded();
    let (_, new) = recurred(
        &repo,
        &["2026-10-02 `lost result`", "2026-10-04 `lost result`"],
    );
    let handle = at(&repo, ".enallagi/DECISIONS.md", "`lost result`");
    audit::run(&repo.root, &cfg).expect("audit");
    let written = decisions(&repo);
    assert!(!written.contains(RULE), "{written}");
    let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
    let expired = written
        .split("## Expired findings")
        .nth(1)
        .unwrap_or_else(|| panic!("{written}"));
    assert!(
        expired
            .lines()
            .any(|l| l.starts_with(&format!("- [{today}] "))
                && l.contains("`lost result`")
                && l.contains("2 revisions")
                && l.contains(&format!("`{new}`"))),
        "{written}"
    );
    assert!(
        prompts(&repo)
            .iter()
            .all(|p| !p.contains(&format!("- `{handle}`"))),
        "{:#?}",
        prompts(&repo)
    );
}

#[test]
fn a_rule_not_recurring_is_not_handed_back() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let (lost, _) = two_classes(&repo);
    earned(
        &repo,
        &format!("{RULE}\n  instances: `{}`, `{}`", lost[0], lost[1]),
    );
    let sha = commit_state(&repo, "a promoted rule");
    let handle = at(&repo, ".enallagi/DECISIONS.md", "`lost result`");
    audit::run(&repo.root, &cfg).expect("audit");
    let written = decisions(&repo);
    assert!(
        written.contains(&format!("  promoted: {sha}\n")),
        "{written}"
    );
    assert!(!written.contains("  revised: "), "{written}");
    assert!(
        prompts(&repo)
            .iter()
            .all(|p| !p.contains(&format!("- `{handle}`"))),
        "{:#?}",
        prompts(&repo)
    );
}
