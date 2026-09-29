//! `enallagi report` over a seeded fixture: the document it writes, the evidence on every claim, and the skill it writes beside it.

use enallagi::fixture::Repo;
use enallagi::report;
use std::fs;

const ARCHIVED: &str = "
## [T-001] the parser drops a trailing comma
scope: src/parse.rs
rows: parses a trailing comma
status: done
notes: VERIFIER 2026-09-01 DONE

## [T-002] the lane loses its log when a stage times out
scope: crates/harness/src/pipeline.rs
rows: none — harness
status: done
notes: VERIFIER 2026-09-02 DONE
";

// two limits in consecutive stages, so limit-repeat reports one harness finding
const EVENTS: &str = r#"{"ts":"2026-09-01T00:00:00Z","run":"r-one","iter":0,"seq":1,"sha":"1111111111111111111111111111111111111111","kind":"run.start","config_sha256":"c","pipeline":null}
{"ts":"2026-09-01T00:00:01Z","run":"r-one","iter":1,"seq":2,"sha":"1111111111111111111111111111111111111111","kind":"stage.start","stage":"implement","role":"implementer","command":null,"task":"T-001"}
{"ts":"2026-09-01T00:00:02Z","run":"r-one","iter":1,"seq":3,"sha":"1111111111111111111111111111111111111111","kind":"limit","stage":"implement","matched":"hit your session limit","sleep_seconds":1,"attempt":1}
{"ts":"2026-09-01T00:00:03Z","run":"r-one","iter":1,"seq":4,"sha":"2222222222222222222222222222222222222222","kind":"stage.end","stage":"implement","task":"T-001","seconds":30,"exit":0,"cost":0.5,"input_tokens":10,"output_tokens":20,"turns":3}
{"ts":"2026-09-01T00:00:04Z","run":"r-one","iter":1,"seq":5,"sha":"2222222222222222222222222222222222222222","kind":"stage.start","stage":"verify","role":"verifier","command":null,"task":"T-002"}
{"ts":"2026-09-01T00:00:05Z","run":"r-one","iter":1,"seq":6,"sha":"2222222222222222222222222222222222222222","kind":"limit","stage":"verify","matched":"hit your session limit","sleep_seconds":1,"attempt":1}
{"ts":"2026-09-01T00:00:06Z","run":"r-one","iter":1,"seq":7,"sha":"2222222222222222222222222222222222222222","kind":"stage.end","stage":"verify","task":"T-002","seconds":20,"exit":0,"cost":0.25,"input_tokens":5,"output_tokens":5,"turns":2}
{"ts":"2026-09-02T00:00:00Z","run":"r-two","iter":0,"seq":1,"sha":"2222222222222222222222222222222222222222","kind":"run.start","config_sha256":"c","pipeline":null}
"#;

fn seeded() -> Repo {
    let repo = Repo::new();
    repo.init_harness("[check]\ncommand = \"true\"\n");
    let rel = repo.root.join(".enallagi/DECISIONS.md");
    let decisions = fs::read_to_string(&rel).expect("decisions");
    fs::write(&rel, format!("{decisions}{ARCHIVED}")).expect("write");
    repo.write(".enallagi/events.jsonl", EVENTS);
    repo
}

// a `claude` on PATH that leaves a mark, so a spawned agent is seen
fn report_bin(repo: &Repo, args: &[&str]) -> (i32, String, String) {
    let bin = repo.root.join("stubbin");
    fs::create_dir_all(&bin).expect("stub dir");
    let stub = bin.join("claude");
    fs::write(&stub, "#!/bin/sh\ntouch \"$(dirname \"$0\")/spawned\"\n").expect("stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .arg("report")
        .args(args)
        .env("PATH", path)
        .current_dir(&repo.root)
        .output()
        .expect("spawn");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn section<'a>(doc: &'a str, heading: &str) -> &'a str {
    let from = doc
        .find(heading)
        .unwrap_or_else(|| panic!("{heading} not in\n{doc}"));
    let rest = &doc[from + heading.len()..];
    &rest[..rest.find("\n## ").unwrap_or(rest.len())]
}

#[test]
fn report_spawns_no_agent_and_is_repeatable() {
    let repo = seeded();
    let (code, first, err) = report_bin(&repo, &[]);
    assert_eq!(code, 0, "{err}");
    let (_, second, _) = report_bin(&repo, &[]);
    assert_eq!(first, second, "two runs differ");
    assert!(
        !repo.root.join("stubbin/spawned").exists(),
        "the report spawned an agent"
    );
}

#[test]
fn every_report_line_carries_evidence() {
    let repo = seeded();
    let (code, doc, err) = report_bin(&repo, &[]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(report::unevidenced(&doc), Vec::<String>::new(), "{doc}");
    assert!(doc.contains("`.enallagi/DECISIONS.md:"), "{doc}");
}

#[test]
fn a_bare_claim_is_unevidenced() {
    let doc = "# report\n\n## This repository\n\n- T-001 landed — `DECISIONS.md:4`\n- the loop is fast\n- measured 3 stages at 1111111\n- `enallagi events --summary` counts 2\n";
    assert_eq!(report::unevidenced(doc), ["- the loop is fast"]);
}

#[test]
fn harness_findings_stay_out_of_the_repo() {
    let repo = seeded();
    let (code, doc, err) = report_bin(&repo, &[]);
    assert_eq!(code, 0, "{err}");
    let product = section(&doc, report::REPOSITORY);
    let harness = section(&doc, report::HARNESS);
    assert!(product.contains("T-001"), "{doc}");
    assert!(!product.contains("T-002"), "{doc}");
    assert!(!product.contains("limit-repeat"), "{doc}");
    assert!(harness.contains("T-002"), "{doc}");
    assert!(harness.contains("limit repeats"), "{doc}");
}

#[test]
fn report_carries_task_metrics_at_their_sha() {
    let repo = seeded();
    let (_, doc, _) = report_bin(&repo, &[]);
    let product = section(&doc, report::REPOSITORY);
    assert!(product.contains("stages 1"), "{doc}");
    assert!(product.contains("$0.50"), "{doc}");
    assert!(product.contains("at 2222222"), "{doc}");
}

#[test]
fn report_names_its_own_staleness() {
    let repo = seeded();
    let (_, doc, _) = report_bin(&repo, &[]);
    let head = enallagi::git::git(&repo.root, &["rev-parse", "HEAD"]).expect("head");
    let stamp = section(&doc, report::STALENESS);
    assert!(stamp.contains(&head), "{doc}");
    assert!(stamp.contains(enallagi::events::COMMIT), "{doc}");
    assert!(stamp.contains("r-one, r-two"), "{doc}");
}

#[test]
fn skill_flag_writes_the_report_as_a_skill() {
    let repo = seeded();
    let (_, doc, _) = report_bin(&repo, &[]);
    let (code, _, err) = report_bin(&repo, &["--skill"]);
    assert_eq!(code, 0, "{err}");
    let cfg = enallagi::config::load(&repo.root).expect("config");
    let presets = enallagi::agent::presets();
    let dir = enallagi::skills::skills_dir(&cfg, presets.get(&cfg.agent.preset));
    let path = repo.root.join(dir).join("enallagi-report/SKILL.md");
    let skill = fs::read_to_string(&path).expect("SKILL.md");
    let rest = skill.strip_prefix("---\n").expect("frontmatter opens");
    let (front, body) = rest.split_once("\n---\n").expect("frontmatter closes");
    let keys: Vec<&str> = front
        .lines()
        .map(|l| l.split_once(": ").expect("key: value").0)
        .collect();
    assert_eq!(keys, ["name", "description"], "{front}");
    assert!(front.contains("name: enallagi-report"), "{front}");
    assert_eq!(body.trim_start_matches('\n'), doc);
}
