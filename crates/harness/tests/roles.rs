//! Roles declared and fetched like skills: `[[role]]` in harness.toml, pinned in harness.lock.

use harness::agent::presets;
use harness::config;
use harness::events::{Kind, Log, Writer};
use harness::fixture::Repo;
use harness::roles;
use harness::skills::{self, ResolveOpts, SkillError};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const BODY: &str = "You implement ONE task.\n";

fn writer(root: &Path) -> Writer {
    Writer::new(Log::open(&root.join(".harness")))
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
        "harness.toml",
        &format!(
            "[[role]]\nname = \"implementer\"\nsource = \"{source}\"\npath = \"roles\"\n{rev}"
        ),
    );
    let cfg = config::load(&repo.root).expect("load");
    config::validate(&cfg, &presets(), &|_| Some(String::new())).expect("valid");
    cfg
}

#[test]
fn a_declared_role_is_fetched_vendored_and_committed_before_its_stage() {
    let upstream = Repo::new();
    upstream.write("roles/implementer.md", BODY);
    upstream.commit_all("role");
    harness::git::git(&upstream.root, &["tag", "v1"]).expect("tag");
    let bare_home = tempfile::TempDir::new().expect("tempdir");
    let bare = bare_home.path().join("upstream.git");
    let from = upstream.root.to_string_lossy().to_string();
    let to = bare.to_string_lossy().to_string();
    harness::git::git(bare_home.path(), &["clone", "--bare", "-q", &from, &to])
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
    let vendored = repo.root.join(".harness/roles/implementer.md");
    assert_eq!(got[0].path, vendored);
    assert_eq!(fs::read_to_string(&vendored).expect("vendored"), BODY);

    let tag_sha = harness::git::git(&upstream.root, &["rev-parse", "v1^{commit}"]).expect("sha");
    let lock = skills::read_lock(&repo.root).expect("lock");
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
    let text = fs::read_to_string(repo.root.join("harness.lock")).expect("lock text");
    assert!(text.contains("[[role]]"), "{text}");

    let again =
        roles::resolve(&repo.root, &cfg, &names, &opts(&repo, false), &mut w).expect("again");
    assert_eq!(again[0].result, "cached");
    assert_eq!(
        fs::read_to_string(repo.root.join("harness.lock")).expect("lock text"),
        text
    );
    let events = Log::open(&repo.root.join(".harness"))
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
fn a_role_whose_vendored_file_drifted_is_refused_under_frozen() {
    let repo = Repo::new();
    repo.write("vendor/roles/implementer.md", BODY);
    let cfg = declare(&repo, "path:vendor", None);
    let mut w = writer(&repo.root);
    let names = vec!["implementer".to_string()];
    roles::resolve(&repo.root, &cfg, &names, &opts(&repo, false), &mut w).expect("first");

    let vendored = repo.root.join(".harness/roles/implementer.md");
    fs::write(&vendored, "tampered\n").expect("tamper");

    let err = roles::resolve(&repo.root, &cfg, &names, &opts(&repo, true), &mut w)
        .expect_err("frozen refuses");
    assert!(
        matches!(&err, SkillError::Unresolved { id, .. } if id == "implementer"),
        "{err}"
    );
    let events = Log::open(&repo.root.join(".harness"))
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
