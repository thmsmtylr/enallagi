//! roles declared and fetched like skills: `[[role]]` in enallagi.toml, pinned in harness.lock

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
        ".enallagi/enallagi.toml",
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

// one implement stage on a stub agent; the run renders run/roles/implementer.md before spawning
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
    repo.write(".enallagi/enallagi.toml", &format!("{toml}{skills}"));
    repo.queue(
        ".enallagi",
        "## [T-001] do the thing\n\nscope: src/a.ts\nrows: none — harness\nstatus: ready-for-agent\ncriteria:\n  - it happens\n",
    );
    repo.write(".enallagi/PROGRESS.md", "# progress\n");
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
    let (code, msg) = hooks::immutable(&repo.root, &input(".enallagi/roles/implementer.md"), None);
    assert_eq!(code, 2, "{msg}");
    assert!(msg.contains("harness.lock"), "{msg}");
    assert!(msg.contains("locked role `implementer`"), "{msg}");

    let (code, msg) = hooks::immutable(&repo.root, &input(".enallagi/roles/verifier.md"), None);
    assert_eq!(code, 0, "{msg}");
}

// the arm lives on step 7 itself, so the assertion reads that line and not the whole prompt
#[test]
fn commit_step_arms() {
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
        "**Status:** review",
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

// each phrase is one reproduction rule; losing one lets an unreproduced claim reach done
#[test]
fn the_verifier_reproduces_each_claim() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../roles/verifier.md");
    let text = fs::read_to_string(&source).expect("read roles/verifier.md");
    for want in [
        "For each test the diff adds or changes",
        "run that test alone",
        "`git diff --quiet <file>` exits 0",
        "still passes with its logic removed is a rejection",
        "For each command, flag or probe the diff adds",
        "read-only against this repository",
        "For each fact about an outside tool",
        "A check that exits non-zero is a rejection",
        "never put down to the environment",
        "out of a probe's reach",
        "A claim with neither",
    ] {
        assert!(text.contains(want), "roles/verifier.md wants {want:?}");
    }
}

// the `claims` gate parses these two lines, so the rendered prompt must ask for both
#[test]
fn verifier_ran_exit_lines() {
    let repo = Repo::new();
    repo.write(".enallagi/enallagi.toml", "[check]\ncommand = \"true\"\n");
    let cfg = config::load(&repo.root).expect("load");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../roles/verifier.md");
    let text = config::subst(
        &fs::read_to_string(&source).expect("read roles/verifier.md"),
        &cfg,
    );
    for want in [
        "`ran: <command>`",
        "`exit: <n>`",
        "except a run against a mutant",
    ] {
        assert!(text.contains(want), "the rendered verifier wants {want:?}");
    }
}

// the loop tests seed the decision the adjudicator writes, so only this pins the rule it follows
#[test]
fn the_adjudicator_gates_each_proposed_learning() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../roles/adjudicator.md");
    let text = fs::read_to_string(&source).expect("read roles/adjudicator.md");
    for want in [
        "three or more citations",
        "`enallagi eval --gate <name>` passes",
        "`  killed: YYYY-MM-DD <the --gate output, or the instance count>`",
    ] {
        assert!(text.contains(want), "roles/adjudicator.md wants {want:?}");
    }
}

// the forms a role is told to write are the forms the tracker reads
#[test]
fn roles_name_verdict_forms() {
    for role in ["verifier", "implementer"] {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../roles/{role}.md"));
        let text = fs::read_to_string(&source).expect("read role");
        for form in enallagi::tracker::VERDICT_FORMS {
            assert!(
                text.contains(&format!("`verdict: {form}`")),
                "roles/{role}.md names no `verdict: {form}`"
            );
        }
    }
}

// the answers the `findings` gate reads, named the same in both roles
#[test]
fn roles_name_answer_forms() {
    for name in ["verifier", "implementer"] {
        for form in [
            "`answer: F-<n> accept <test>`",
            "`answer: F-<n> reject <concept>: <command>`",
        ] {
            assert!(role(name).contains(form), "roles/{name}.md names no {form}");
        }
    }
}

// each shipped role's {{skill:<id>}} tokens, rendered, that the config declares no skill for
fn unresolved(cfg: &config::Config) -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../roles");
    let mut names: Vec<_> = fs::read_dir(&dir)
        .expect("read roles/")
        .map(|e| e.expect("entry").path())
        .collect();
    names.sort();
    let mut out = Vec::new();
    for path in names {
        let text = config::subst(&fs::read_to_string(&path).expect("role"), cfg);
        for id in skills::required_ids(&text) {
            if !cfg.skill.iter().any(|s| s.id == id) {
                let role = path.file_name().expect("name").to_string_lossy();
                out.push(format!("{role}: {id}"));
            }
        }
    }
    out
}

#[test]
fn every_role_skill_resolves() {
    let repo = Repo::new();
    repo.write(".enallagi/enallagi.toml", "[check]\ncommand = \"true\"\n");
    let mut cfg = config::load(&repo.root).expect("load");
    assert_eq!(unresolved(&cfg), Vec::<String>::new());
    cfg.skill.retain(|s| s.id != "writing-for-agents");
    assert_eq!(unresolved(&cfg), ["implementer.md: writing-for-agents"]);
}

fn role(name: &str) -> String {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../roles/{name}.md"));
    fs::read_to_string(&source).expect("read role")
}

#[test]
fn implementer_loads_codebase_design() {
    assert!(role("implementer").contains("{{skill:codebase-design}}"));
}

#[test]
fn implementer_updates_stated_doc() {
    let text = role("implementer");
    for want in [
        "updates the doc that states it in the same commit",
        "a change no doc states touches no doc",
    ] {
        assert!(text.contains(want), "roles/implementer.md wants {want:?}");
    }
}

// a stub agent's result alone passed a writer the real run broke
#[test]
fn verifier_copies_writers() {
    let text = role("verifier");
    for want in [
        "writes files or calls a model",
        "cp -R __ENALLAGI_DIR__",
        "never the live __ENALLAGI_DIR__",
    ] {
        assert!(text.contains(want), "roles/verifier.md wants {want:?}");
    }
}

#[test]
fn adjudicator_takes_decomposed() {
    let text = role("adjudicator");
    for want in [
        "{{skill:triage}}",
        "A `probe: decompose` ticket is anchored by its spec",
        "set it to `needs-info` and halt",
        "`ready-for-human`",
        "__ENALLAGI_DIR__/.out-of-scope/<concept>.md",
    ] {
        assert!(text.contains(want), "roles/adjudicator.md wants {want:?}");
    }
    assert!(!text.contains("append the same line to `## Rejected findings`"));
}

// docs move with code only when the ticket's scope names the doc
#[test]
fn adjudicator_scopes_stated_doc() {
    let text = role("adjudicator");
    for want in [
        "makes a doc, skill or role file wrong, add that file to `scope:`",
        "name the line that states it under `## Comments`",
    ] {
        assert!(text.contains(want), "roles/adjudicator.md wants {want:?}");
    }
}

// every rule the decomposer stated is a gate, so the role is the skill plus the check
#[test]
fn decomposer_skill_only() {
    let text = role("decomposer");
    let body = text.split("\n---\n").nth(1).expect("frontmatter");
    assert!(body.contains("{{skill:to-tickets}}"), "{body}");
    assert!(body.contains("enallagi tasks check <spec>"), "{body}");
    assert!(!body.contains("Protocol:"), "{body}");
    assert!(body.lines().count() <= 3, "{body}");
}
