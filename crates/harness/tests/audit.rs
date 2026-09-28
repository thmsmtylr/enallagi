//! `enallagi audit` over a seeded fixture: the signal it groups, the learnings it proposes, and the kill that silences one.

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

// the scripted auditor logs each prompt and cites every instance the prompt names
const AUDITOR: &str = "printf '%s\\n----\\n' \"$1\" >> src/.prompts
cites=$(printf '%s\\n' \"$1\" | sed -n 's/^- \\(`[^`]*`\\)$/\\1/p' | paste -sd, -)
printf 'BEGIN ENALLAGI LEARNING\\nthe instances repeat one shape → hold the rule\\ninstances: %s\\nEND ENALLAGI LEARNING\\n' \"$cites\"
";

fn seeded() -> (Repo, Config) {
    seeded_with(AUDITOR)
}

fn seeded_with(agent: &str) -> (Repo, Config) {
    let repo = Repo::new();
    let argv = repo.stub_agent(agent);
    repo.init_harness(&format!(
        "[agent]\npreset = \"custom\"\ncommand = {argv:?}\n\n[check]\ncommand = \"true\"\n"
    ));
    let cfg = config::load(&repo.root).expect("config");
    (repo, cfg)
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

#[test]
fn six_findings_of_two_classes_are_two_learnings() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let progress = ".enallagi/PROGRESS.md";
    let decisions_rel = ".enallagi/DECISIONS.md";
    let tasks = ".enallagi/TASKS.md";
    let mut lost = vec![
        at(&repo, progress, "merged true on a failed fetch"),
        at(&repo, decisions_rel, "## [T-010]"),
        at(&repo, decisions_rel, "REJECTED: the probe returns ok"),
    ];
    let mut vacuous = vec![
        at(&repo, progress, "passed with the filter deleted"),
        at(&repo, decisions_rel, "a mutant survived"),
        at(&repo, tasks, "## [T-012]"),
    ];
    lost.sort();
    vacuous.sort();
    let noise = at(&repo, progress, "a slow disk");
    let report = audit::run(&repo.root, &cfg).expect("audit");
    let classes: Vec<&str> = report.proposed.iter().map(|p| p.class).collect();
    assert_eq!(classes, ["lost-result", "vacuous-test"]);
    let mut got: Vec<Vec<String>> = report
        .proposed
        .iter()
        .map(|p| p.instances.clone())
        .collect();
    got.iter_mut().for_each(|i| i.sort());
    assert_eq!(got, [lost.clone(), vacuous.clone()]);

    let written = decisions(&repo);
    assert!(written.contains("## Proposed learnings"), "{written}");
    for cite in lost.iter().chain(&vacuous) {
        assert!(written.contains(&format!("`{cite}`")), "{cite}: {written}");
    }
    assert!(
        !written.contains(&format!("`{noise}`")),
        "a friction of no class was cited"
    );
}

#[test]
fn a_killed_learning_is_not_proposed_again() {
    let (repo, _) = seeded();
    with_signal(&repo);
    let (code, out) = audit_bin(&repo);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("proposed `lost-result`"), "{out}");

    // the operator kills one and deletes the other without a kill line
    let text = decisions(&repo);
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let entry = lines
        .iter()
        .position(|l| l.contains("[proposed] `lost-result`"))
        .expect(&text);
    let end = entry
        + 1
        + lines[entry + 1..]
            .iter()
            .take_while(|l| l.starts_with("  "))
            .count();
    lines.insert(
        end,
        "  killed: 2026-09-29 the fetch was a fixture".to_string(),
    );
    let gone = lines
        .iter()
        .position(|l| l.contains("[proposed] `vacuous-test`"))
        .expect(&text);
    while lines.get(gone + 1).is_some_and(|l| l.starts_with("  ")) {
        lines.remove(gone + 1);
    }
    lines.remove(gone);
    fs::write(
        repo.root.join(".enallagi/DECISIONS.md"),
        lines.join("\n") + "\n",
    )
    .expect("write");
    append(
        &repo,
        ".enallagi/PROGRESS.md",
        "friction: the hook swallowed the exit status\n",
    );

    let (code, out) = audit_bin(&repo);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("`lost-result` killed 2026-09-29"), "{out}");
    assert!(!out.contains("proposed `lost-result`"), "{out}");
    assert!(out.contains("proposed `vacuous-test`"), "{out}");
    let written = decisions(&repo);
    assert_eq!(written.matches("[proposed] `lost-result`").count(), 1);
    assert_eq!(written.matches("[proposed] `vacuous-test`").count(), 1);
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
fn one_instance_of_a_class_proposes_nothing() {
    let (repo, cfg) = seeded();
    append(
        &repo,
        ".enallagi/PROGRESS.md",
        "friction: the hook swallowed the exit status\n",
    );
    let before = decisions(&repo);
    let report = audit::run(&repo.root, &cfg).expect("audit");
    assert!(report.proposed.is_empty(), "{report:?}");
    assert_eq!(decisions(&repo), before);
}

#[test]
fn a_second_audit_repeats_no_proposal() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    assert_eq!(
        audit::run(&repo.root, &cfg).expect("audit").proposed.len(),
        2
    );
    let once = decisions(&repo);
    let again = audit::run(&repo.root, &cfg).expect("audit");
    assert!(again.proposed.is_empty(), "{again:?}");
    assert_eq!(again.settled.len(), 2, "{again:?}");
    assert_eq!(decisions(&repo), once);
}

#[test]
fn an_accepted_class_is_not_proposed() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let rel = ".enallagi/DECISIONS.md";
    let text = decisions(&repo).replacen(
        "\n## Rejected findings\n",
        "\n- [2026-09-29] `lost-result`: assert on what `src/agent.rs` returns\n\n## Rejected findings\n",
        1,
    );
    fs::write(repo.root.join(rel), text).expect("write");
    let report = audit::run(&repo.root, &cfg).expect("audit");
    let classes: Vec<&str> = report.proposed.iter().map(|p| p.class).collect();
    assert_eq!(classes, ["vacuous-test"]);
    assert!(report.settled[0].contains("accepted"), "{report:?}");
}

#[test]
fn each_class_is_handed_to_the_auditor_role() {
    let (repo, cfg) = seeded();
    with_signal(&repo);
    let report = audit::run(&repo.root, &cfg).expect("audit");
    assert_eq!(report.proposed.len(), 2, "{report:?}");
    let asked = prompts(&repo);
    assert_eq!(asked.len(), 2, "{asked:#?}");
    let role = include_str!("../../../roles/auditor.md");
    let protocol = role.lines().find(|l| l.starts_with("1. ")).expect("step 1");
    for (prompt, proposal) in asked.iter().zip(&report.proposed) {
        assert!(prompt.contains(protocol), "{prompt}");
        assert!(
            prompt.contains(&format!("`{}`", proposal.class)),
            "{prompt}"
        );
        for cite in &proposal.instances {
            assert!(prompt.contains(&format!("- `{cite}`")), "{cite}: {prompt}");
        }
    }
    let written = decisions(&repo);
    assert_eq!(
        written
            .matches("[proposed] `lost-result`: the instances repeat one shape → hold the rule")
            .count(),
        1,
        "{written}"
    );
}

#[test]
fn a_learning_citing_no_instance_is_refused() {
    let (repo, cfg) = seeded_with(
        "printf 'BEGIN ENALLAGI LEARNING\\na lesson → a rule\\ninstances: `src/elsewhere.rs:1`\\nEND ENALLAGI LEARNING\\n'\n",
    );
    with_signal(&repo);
    let before = decisions(&repo);
    let report = audit::run(&repo.root, &cfg).expect("audit");
    assert!(report.proposed.is_empty(), "{report:?}");
    assert_eq!(report.settled.len(), 2, "{report:?}");
    assert!(
        report.settled.iter().all(|s| s.contains("refused")),
        "{report:?}"
    );
    assert_eq!(decisions(&repo), before);
}
