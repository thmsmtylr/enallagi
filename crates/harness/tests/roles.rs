//! Roles declared and fetched like skills: `[[role]]` in enallagi.toml, pinned in harness.lock.

use enallagi::agent::presets;
use enallagi::config;
use enallagi::events::{Kind, Log, Writer};
use enallagi::fixture::Repo;
use enallagi::hooks;
use enallagi::pipeline::{self, RunOpts};
use enallagi::roles;
use enallagi::skills::{self, ResolveOpts, SkillError};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const BODY: &str = "You implement ONE task.\n";

fn writer(root: &Path) -> Writer {
    Writer::new(Log::open(&root.join(".enallagi")))
}

fn opts(repo: &Repo, frozen: bool) -> ResolveOpts {
    ResolveOpts {
        frozen,
        cache_dir: repo.root.join("cache"),
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// the default `implement` stage names `implementer`, so a [[role]] under that name is used by a stage
fn declare(repo: &Repo, source: &str, rev: Option<&str>) -> config::Config {
    let rev = rev.map_or(String::new(), |r| format!("rev = \"{r}\"\n"));
    repo.write(
        "enallagi.toml",
        &format!(
            "[check]\ncommand = \"true\"\n\n[[role]]\nname = \"implementer\"\nsource = \"{source}\"\npath = \"roles\"\n{rev}"
        ),
    );
    let cfg = config::load(&repo.root).expect("load");
    config::validate(&cfg, &presets(), &|_| Some(String::new())).expect("valid");
    cfg
}

#[test]
fn a_declared_role_lands_before_its_stage() {
    let upstream = Repo::new();
    upstream.write("roles/implementer.md", BODY);
    upstream.commit_all("role");
    enallagi::git::git(&upstream.root, &["tag", "v1"]).expect("tag");
    let bare_home = tempfile::TempDir::new().expect("tempdir");
    let bare = bare_home.path().join("upstream.git");
    let from = upstream.root.to_string_lossy().to_string();
    let to = bare.to_string_lossy().to_string();
    enallagi::git::git(bare_home.path(), &["clone", "--bare", "-q", &from, &to])
        .expect("bare clone");

    let repo = Repo::new();
    let cfg = declare(&repo, &format!("git+file://{}", bare.display()), Some("v1"));
    let mut w = writer(&repo.root);
    let names = vec!["implementer".to_string()];

    let got =
        roles::resolve(&repo.root, &cfg, &names, &opts(&repo, false), &mut w).expect("resolve");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].name, "implementer");
    assert_eq!(got[0].result, "fetched");
    assert_eq!(got[0].body, BODY);
    let vendored = repo.root.join(".enallagi/roles/implementer.md");
    assert_eq!(got[0].path, vendored);
    assert_eq!(fs::read_to_string(&vendored).expect("vendored"), BODY);

    let tag_sha = enallagi::git::git(&upstream.root, &["rev-parse", "v1^{commit}"]).expect("sha");
    let lock = skills::read_lock(&repo.root, ".enallagi").expect("lock");
    assert!(lock.skill.is_empty());
    assert_eq!(lock.role.len(), 1);
    assert_eq!(lock.role[0].id, "implementer");
    assert_eq!(
        lock.role[0].source,
        format!("git+file://{}", bare.display())
    );
    assert_eq!(lock.role[0].rev.as_deref(), Some("v1"));
    assert_eq!(lock.role[0].commit.as_deref(), Some(tag_sha.as_str()));
    assert_eq!(lock.role[0].sha256, sha256(BODY.as_bytes()));
    let text = fs::read_to_string(repo.root.join(".enallagi/harness.lock")).expect("lock text");
    assert!(text.contains("[[role]]"), "{text}");

    let again =
        roles::resolve(&repo.root, &cfg, &names, &opts(&repo, false), &mut w).expect("again");
    assert_eq!(again[0].result, "cached");
    assert_eq!(
        fs::read_to_string(repo.root.join(".enallagi/harness.lock")).expect("lock text"),
        text
    );
    let events = Log::open(&repo.root.join(".enallagi"))
        .read()
        .expect("events");
    let results: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::SkillResolved { id, result, .. } if id == "implementer" => Some(result.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(results, ["fetched", "cached"]);
}

#[test]
fn a_drifted_role_is_refused_under_frozen() {
    let repo = Repo::new();
    repo.write("vendor/roles/implementer.md", BODY);
    let cfg = declare(&repo, "path:vendor", None);
    let mut w = writer(&repo.root);
    let names = vec!["implementer".to_string()];
    roles::resolve(&repo.root, &cfg, &names, &opts(&repo, false), &mut w).expect("first");

    let vendored = repo.root.join(".enallagi/roles/implementer.md");
    fs::write(&vendored, "tampered\n").expect("tamper");

    let err = roles::resolve(&repo.root, &cfg, &names, &opts(&repo, true), &mut w)
        .expect_err("frozen refuses");
    assert!(
        matches!(&err, SkillError::Unresolved { id, .. } if id == "implementer"),
        "{err}"
    );
    let events = Log::open(&repo.root.join(".enallagi"))
        .read()
        .expect("events");
    assert!(events.iter().any(|e| matches!(
        &e.kind,
        Kind::SkillResolved { id, result, .. } if id == "implementer" && result == "refused"
    )));
    assert_eq!(
        fs::read_to_string(&vendored).expect("untouched"),
        "tampered\n"
    );

    let got = roles::resolve(&repo.root, &cfg, &names, &opts(&repo, false), &mut w)
        .expect("unfrozen refetches");
    assert_eq!(got[0].result, "fetched");
    assert_eq!(fs::read_to_string(&vendored).expect("restored"), BODY);
}

// one implement stage on a stub agent; the run renders <harness_dir>/run/roles/implementer.md before spawning
fn pipeline_repo() -> Repo {
    let repo = Repo::new();
    repo.write(
        ".enallagi/.gitignore",
        "events.jsonl\n*.log\nlogs/\nloop.pid\nrun/\n",
    );
    let agent = repo.stub_agent("echo '{\"total_cost_usd\":0.1}'\n");
    let check = repo.stub_check("exit 0\n");
    let argv: Vec<String> = agent.iter().map(|a| format!("\"{a}\"")).collect();
    let toml = format!(
        "[agent]\npreset = \"custom\"\ncommand = [{}]\n\n[agent.usage]\ncost = \"total_cost_usd\"\n\n\
         [check]\ncommand = \"{check}\"\n\n\
         [[pipeline]]\nname = \"task\"\nwhen = \"queue.takeable\"\nstages = [\"implement\"]\n\n\
         [[stage]]\nname = \"implement\"\nrole = \"implementer\"\nturns = 5\n",
        argv.join(", ")
    );
    let skills = repo.local_skills(&toml);
    repo.write("enallagi.toml", &format!("{toml}{skills}"));
    repo.write(
        "TASKS.md",
        "## [T-001] do the thing\n\nscope: src/a.ts\nrows: none — harness\nstatus: ready\ncriteria:\n  - it happens\n",
    );
    repo.write("SPEC.md", "# spec\n");
    repo.write("PROGRESS.md", "# progress\n");
    repo.commit_all("harness");
    repo
}

fn run_once(repo: &Repo) {
    let opts = RunOpts {
        max_iter: 1,
        ..RunOpts::default()
    };
    pipeline::run(&repo.root, &opts, Box::new(|_| {})).expect("run");
}

#[test]
fn an_undeclared_role_falls_back() {
    let installed = pipeline_repo();
    installed.write(".enallagi/roles/implementer.md", BODY);
    installed.commit_all("installed role");
    run_once(&installed);
    let rendered = installed.root.join(".enallagi/run/roles/implementer.md");
    assert_eq!(fs::read_to_string(&rendered).expect("rendered"), BODY);
    assert!(skills::read_lock(&installed.root, ".enallagi")
        .expect("lock")
        .role
        .is_empty());

    let embedded = pipeline_repo();
    run_once(&embedded);
    let rendered = embedded.root.join(".enallagi/run/roles/implementer.md");
    let text = fs::read_to_string(&rendered).expect("rendered");
    assert!(text.contains("You implement ONE task"), "{text}");
    assert!(!embedded
        .root
        .join(".enallagi/roles/implementer.md")
        .exists());
    assert!(skills::read_lock(&embedded.root, ".enallagi")
        .expect("lock")
        .role
        .is_empty());
}

#[test]
fn immutable_refuses_an_edit_to_a_role() {
    let repo = Repo::new();
    repo.write(
        ".enallagi/harness.lock",
        "version = 1\n\n[[role]]\nid = \"implementer\"\nsource = \"path:vendor\"\nsha256 = \"ab\"\n",
    );
    let input = |path: &str| format!(r#"{{"tool_input":{{"file_path":"{path}"}}}}"#);
    let (code, msg) = hooks::immutable(&repo.root, &input(".enallagi/roles/implementer.md"));
    assert_eq!(code, 2, "{msg}");
    assert!(msg.contains("harness.lock"), "{msg}");
    assert!(msg.contains("locked role `implementer`"), "{msg}");

    let (code, msg) = hooks::immutable(&repo.root, &input(".enallagi/roles/verifier.md"));
    assert_eq!(code, 0, "{msg}");
}

// the arm lives on step 7 itself, so the assertion reads that line and not the whole prompt
#[test]
fn the_commit_step_names_both_arms() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../roles/implementer.md");
    let text = fs::read_to_string(&source).expect("read roles/implementer.md");
    let step = text
        .lines()
        .find(|line| line.starts_with("7. "))
        .expect("roles/implementer.md has a step 7");
    for want in [
        "feat(<scope>)",
        "git status --porcelain",
        "already committed at",
        "no commit",
        "status: review",
    ] {
        assert!(step.contains(want), "step 7 wants {want}: {step}");
    }
}

// plain_record.rs closes a rejection region only at an author marker, so step 6 must ask for one
#[test]
fn the_notes_step_requires_the_author_marker() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../roles/implementer.md");
    let text = fs::read_to_string(&source).expect("read roles/implementer.md");
    let step = text
        .lines()
        .find(|line| line.starts_with("6. "))
        .expect("roles/implementer.md has a step 6");
    assert!(
        step.contains("IMPLEMENTER"),
        "step 6 wants IMPLEMENTER: {step}"
    );
}
