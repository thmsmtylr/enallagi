//! skills: the declared skills of `harness.toml`, fetched from git or a path,
//! vendored into the repository, pinned in `harness.lock`, and rendered into
//! the role prompts that name them with `{{skill:<id>}}`.

use crate::agent::Preset;
use crate::config::{Config, SkillDecl};
use crate::events::{Kind, Writer};
use crate::git;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("skill {id}: source {spec} is not github:<owner>/<repo>, git+<url> or path:<dir>")]
    BadSource { id: String, spec: String },
    #[error("skill {id} is not declared in harness.toml")]
    Undeclared { id: String },
    #[error("skill {id} is unresolved: {why}")]
    Unresolved { id: String, why: String },
    #[error("harness.lock: {0}")]
    Lock(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] git::GitError),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LockEntry {
    pub id: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub sha256: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Lock {
    pub version: u32,
    #[serde(default)]
    pub skill: Vec<LockEntry>,
}

impl Default for Lock {
    fn default() -> Self {
        Lock {
            version: 1,
            skill: Vec::new(),
        }
    }
}

pub fn lock_path(root: &Path) -> PathBuf {
    root.join("harness.lock")
}

/// A missing lock reads as an empty one; an unparseable lock is an error,
/// because silently re-fetching over a corrupt pin defeats the pin.
pub fn read_lock(root: &Path) -> Result<Lock, SkillError> {
    let text = match fs::read_to_string(lock_path(root)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Lock::default()),
        Err(e) => return Err(e.into()),
    };
    toml::from_str(&text).map_err(|e| SkillError::Lock(e.to_string()))
}

pub fn write_lock(root: &Path, lock: &Lock) -> Result<(), SkillError> {
    let mut lock = lock.clone();
    lock.skill.sort_by(|a, b| a.id.cmp(&b.id));
    let text = toml::to_string(&lock).map_err(|e| SkillError::Lock(e.to_string()))?;
    fs::write(lock_path(root), text)?;
    Ok(())
}

/// The `{{skill:<id>}}` tokens of a role prompt, in order, deduped.
pub fn required_ids(role_text: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for tail in role_text.split("{{skill:").skip(1) {
        let Some((id, _)) = tail.split_once("}}") else {
            continue;
        };
        let id = id.trim();
        if !id.is_empty() && !ids.iter().any(|seen| seen == id) {
            ids.push(id.to_string());
        }
    }
    ids
}

/// Where vendored skills live, relative to the repository root: the configured
/// directory, else the preset's own, else `<harness_dir>/skills` for a preset
/// that has no skills mechanism and reads the body inline instead.
pub fn skills_dir(cfg: &Config, preset: &Preset) -> PathBuf {
    if let Some(dir) = cfg.layout.skills_dir.as_deref() {
        return PathBuf::from(dir);
    }
    if let Some(dir) = preset.skills_dir.as_deref() {
        return PathBuf::from(dir);
    }
    Path::new(&cfg.layout.harness_dir).join("skills")
}

pub struct ResolveOpts {
    pub frozen: bool,
    pub cache_dir: PathBuf,
}

impl Default for ResolveOpts {
    fn default() -> Self {
        ResolveOpts {
            frozen: false,
            cache_dir: default_cache_dir(),
        }
    }
}

fn default_cache_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(x).join("harness");
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".cache").join("harness")
}

#[derive(Debug, Clone)]
pub struct ResolvedSkill {
    pub id: String,
    pub dir: PathBuf,
    /// `cached` when the vendored copy already matched the lock, `fetched`
    /// when it was copied in during this call.
    pub result: String,
    pub body: String,
}

/// Resolves each id to a vendored skill directory. Under `frozen` a skill that
/// does not already match its lock entry is refused rather than fetched, so a
/// pipeline cannot silently pull new instructions mid-run.
pub fn resolve(
    root: &Path,
    cfg: &Config,
    preset: &Preset,
    ids: &[String],
    opts: &ResolveOpts,
    events: &mut Writer,
) -> Result<Vec<ResolvedSkill>, SkillError> {
    let base = root.join(skills_dir(cfg, preset));
    let mut lock = read_lock(root)?;
    let mut out = Vec::new();
    let mut dirty = false;

    for id in ids {
        let decl = cfg
            .skill
            .iter()
            .find(|s| &s.id == id)
            .ok_or_else(|| SkillError::Undeclared { id: id.clone() })?;
        let dir = base.join(id);
        let entry = lock.skill.iter().position(|e| &e.id == id);

        match cached(&lock, entry, decl, &dir) {
            Ok(body) => {
                let commit = entry.and_then(|i| lock.skill[i].commit.clone());
                events.emit(Kind::SkillResolved {
                    id: id.clone(),
                    commit,
                    result: "cached".into(),
                });
                out.push(ResolvedSkill {
                    id: id.clone(),
                    dir,
                    result: "cached".into(),
                    body,
                });
            }
            Err(why) => {
                if opts.frozen {
                    events.emit(Kind::SkillResolved {
                        id: id.clone(),
                        commit: entry.and_then(|i| lock.skill[i].commit.clone()),
                        result: "refused".into(),
                    });
                    return Err(SkillError::Unresolved {
                        id: id.clone(),
                        why,
                    });
                }
                let (source_root, commit) = fetch(root, decl, opts)?;
                let body = vendor(&source_root.join(&decl.path), &dir)?;
                let new = LockEntry {
                    id: id.clone(),
                    source: decl.source.clone(),
                    rev: decl.rev.clone(),
                    commit: commit.clone(),
                    sha256: sha256(body.as_bytes()),
                };
                match entry {
                    Some(i) => lock.skill[i] = new,
                    None => lock.skill.push(new),
                }
                dirty = true;
                events.emit(Kind::SkillResolved {
                    id: id.clone(),
                    commit,
                    result: "fetched".into(),
                });
                out.push(ResolvedSkill {
                    id: id.clone(),
                    dir,
                    result: "fetched".into(),
                    body,
                });
            }
        }
    }

    if dirty {
        write_lock(root, &lock)?;
    }
    Ok(out)
}

/// `Ok(body)` when the lock, the declaration and the vendored `SKILL.md` all
/// agree; `Err(why)` names the first thing that did not.
fn cached(
    lock: &Lock,
    entry: Option<usize>,
    decl: &SkillDecl,
    dir: &Path,
) -> Result<String, String> {
    let Some(e) = entry.map(|i| &lock.skill[i]) else {
        return Err("no lock entry".into());
    };
    if e.source != decl.source || e.rev != decl.rev {
        return Err("the declaration moved away from the lock".into());
    }
    let body = fs::read_to_string(dir.join("SKILL.md")).map_err(|_| "not vendored".to_string())?;
    if sha256(body.as_bytes()) != e.sha256 {
        return Err("the vendored SKILL.md does not match its locked sha256".into());
    }
    Ok(body)
}

/// The directory the skill's `path` is relative to, plus the commit it is at
/// for a git source.
fn fetch(
    root: &Path,
    decl: &SkillDecl,
    opts: &ResolveOpts,
) -> Result<(PathBuf, Option<String>), SkillError> {
    let bad = || SkillError::BadSource {
        id: decl.id.clone(),
        spec: decl.source.clone(),
    };
    let source = decl.source.trim();

    if let Some(rest) = source.strip_prefix("path:") {
        let p = Path::new(rest);
        if rest.is_empty() {
            return Err(bad());
        }
        return Ok((
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                root.join(p)
            },
            None,
        ));
    }

    let url = if let Some(rest) = source.strip_prefix("github:") {
        let (owner, repo) = rest.split_once('/').ok_or_else(bad)?;
        if owner.is_empty() || repo.is_empty() || repo.contains('/') {
            return Err(bad());
        }
        format!("https://github.com/{owner}/{repo}.git")
    } else if let Some(rest) = source.strip_prefix("git+") {
        if rest.is_empty() {
            return Err(bad());
        }
        rest.to_string()
    } else {
        return Err(bad());
    };

    let rev = decl.rev.as_deref();
    let dir = cache_path(&opts.cache_dir, &url, rev.unwrap_or("HEAD"));
    clone(&url, rev, &dir)?;
    let commit = git::git(&dir, &["rev-parse", "HEAD"])?;
    Ok((dir, Some(commit)))
}

/// `<cache>/git/<host>/<owner>/<repo>/<rev>`. The three name segments are read
/// off the end of the URL, so a `file://` source keys on its own path.
fn cache_path(cache_dir: &Path, url: &str, rev: &str) -> PathBuf {
    let rest = match url.split_once("://") {
        Some((_, r)) => r.to_string(),
        // scp-like: git@host:owner/repo.git
        None => url
            .rsplit_once('@')
            .map_or(url, |(_, r)| r)
            .replace(':', "/"),
    };
    let mut segs: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    let repo = segs.pop().unwrap_or("repo").trim_end_matches(".git");
    let owner = segs.pop().unwrap_or("_");
    let host = segs.first().copied().unwrap_or("_");
    cache_dir
        .join("git")
        .join(host)
        .join(owner)
        .join(repo)
        .join(rev.replace('/', "-"))
}

/// A cache directory that already exists is a pinned checkout and is reused
/// without touching the network.
fn clone(url: &str, rev: Option<&str>, dir: &Path) -> Result<(), SkillError> {
    if dir.exists() {
        return Ok(());
    }
    let parent = dir.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let target = dir.to_string_lossy().to_string();

    if let Some(rev) = rev {
        if git::git_ok(
            parent,
            &["clone", "--depth", "1", "--branch", rev, url, &target],
        ) {
            return Ok(());
        }
        // --branch takes a branch or a tag, never a commit sha.
        let _ = fs::remove_dir_all(dir);
        git::git(parent, &["clone", url, &target])?;
        git::git(dir, &["checkout", "-q", rev])?;
        return Ok(());
    }
    git::git(parent, &["clone", "--depth", "1", url, &target])?;
    Ok(())
}

/// Copies `src` over `dst`, replacing whatever was there, and returns the
/// `SKILL.md` body.
fn vendor(src: &Path, dst: &Path) -> Result<String, SkillError> {
    if !src.join("SKILL.md").is_file() {
        return Err(SkillError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} has no SKILL.md", src.display()),
        )));
    }
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    copy_dir(src, dst)?;
    Ok(fs::read_to_string(dst.join("SKILL.md"))?)
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Replaces every `{{skill:<id>}}` token of a role prompt. A tool preset gets
/// the invocation it understands; a preset with no skills mechanism gets the
/// body appended as a section and a pointer to it. Tokens whose id was not
/// resolved are left alone.
pub fn render(
    role_text: &str,
    resolved: &[ResolvedSkill],
    preset: &Preset,
    cfg: &Config,
) -> String {
    let inline = preset.skills_dir.is_none();
    let invocation = preset
        .invocation
        .as_deref()
        .unwrap_or(&cfg.layout.skill_invocation);

    let mut out = role_text.to_string();
    let mut sections = String::new();
    for skill in resolved {
        let id = &skill.id;
        let replacement = if inline {
            format!("the `{id}` skill (see the section `## Skill: {id}` below)")
        } else {
            format!("`{id}` ({})", invocation.replace("<id>", id))
        };
        out = out.replace(&format!("{{{{skill:{id}}}}}"), &replacement);
        if inline {
            sections.push_str(&format!("\n\n## Skill: {id}\n\n{}", skill.body));
        }
    }
    out.push_str(&sections);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::presets;
    use crate::events::Log;
    use crate::fixture::Repo;

    fn writer(root: &Path) -> Writer {
        Writer::new(Log::open(&root.join(".harness")))
    }

    fn config(source: &str, path: &str, rev: Option<&str>) -> Config {
        let mut cfg = Config::default();
        cfg.layout.harness_dir = ".harness".into();
        cfg.layout.skill_invocation = "invoke it via the Skill tool".into();
        cfg.skill = vec![SkillDecl {
            id: "tdd".into(),
            source: source.into(),
            path: path.into(),
            rev: rev.map(String::from),
            gate: "none".into(),
            why: "because".into(),
        }];
        cfg
    }

    fn preset(name: &str) -> Preset {
        presets().get(name).expect("preset").clone()
    }

    const BODY: &str = "---\nname: tdd\n---\n\nWrite the failing test first.\n";

    /// A `path:` source directory holding one SKILL.md, outside the repo.
    fn source_dir(repo: &Repo, rel: &str) -> String {
        repo.write(&format!("{rel}/SKILL.md"), BODY);
        repo.write(&format!("{rel}/references/more.md"), "more\n");
        rel.to_string()
    }

    fn opts(repo: &Repo, frozen: bool) -> ResolveOpts {
        ResolveOpts {
            frozen,
            cache_dir: repo.root.join("cache"),
        }
    }

    #[test]
    fn required_ids_reads_tokens_in_order() {
        assert_eq!(
            required_ids("use {{skill:tdd}} then {{skill:ponytail}} and {{skill:tdd}}"),
            ["tdd", "ponytail"]
        );
        assert!(required_ids("{{skill:}} {{skill:unclosed").is_empty());
    }

    #[test]
    fn a_path_source_is_vendored_and_locked() {
        let repo = Repo::new();
        let rel = source_dir(&repo, "vendor/tdd");
        let cfg = config(&format!("path:{rel}"), "", None);
        let claude = preset("claude");
        let mut w = writer(&repo.root);

        let ids = vec!["tdd".to_string()];
        let got =
            resolve(&repo.root, &cfg, &claude, &ids, &opts(&repo, false), &mut w).expect("resolve");
        assert_eq!(got[0].result, "fetched");
        let vendored = repo.root.join(".claude/skills/tdd/SKILL.md");
        assert_eq!(fs::read_to_string(&vendored).expect("vendored"), BODY);
        assert!(repo
            .root
            .join(".claude/skills/tdd/references/more.md")
            .is_file());

        let lock = read_lock(&repo.root).expect("lock");
        assert_eq!(lock.version, 1);
        assert_eq!(lock.skill[0].id, "tdd");
        assert_eq!(lock.skill[0].sha256, sha256(BODY.as_bytes()));
        assert_eq!(lock.skill[0].commit, None);
        let before = fs::read_to_string(lock_path(&repo.root)).expect("lock text");

        let again = resolve(&repo.root, &cfg, &claude, &ids, &opts(&repo, false), &mut w)
            .expect("resolve again");
        assert_eq!(again[0].result, "cached");
        assert_eq!(
            fs::read_to_string(lock_path(&repo.root)).expect("lock text"),
            before
        );
    }

    #[test]
    fn a_git_source_is_cloned_at_rev_into_the_cache() {
        // A bare repo in a tempdir: no network, but a real git source.
        let upstream = Repo::new();
        upstream.write("skills/tdd/SKILL.md", BODY);
        upstream.commit_all("skill");
        crate::git::git(&upstream.root, &["tag", "v1"]).expect("tag");
        let bare_home = tempfile::TempDir::new().expect("tempdir");
        let bare = bare_home.path().join("upstream.git");
        let from = upstream.root.to_string_lossy().to_string();
        let to = bare.to_string_lossy().to_string();
        crate::git::git(bare_home.path(), &["clone", "--bare", "-q", &from, &to])
            .expect("bare clone");

        let repo = Repo::new();
        let url = format!("file://{}", bare.display());
        let cfg = config(&format!("git+{url}"), "skills/tdd", Some("v1"));
        let claude = preset("claude");
        let mut w = writer(&repo.root);
        let got = resolve(
            &repo.root,
            &cfg,
            &claude,
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect("resolve");

        assert_eq!(got[0].result, "fetched");
        assert_eq!(
            fs::read_to_string(repo.root.join(".claude/skills/tdd/SKILL.md")).expect("vendored"),
            BODY
        );
        let tag_sha = crate::git::git(&upstream.root, &["rev-parse", "v1^{commit}"]).expect("sha");
        let lock = read_lock(&repo.root).expect("lock");
        assert_eq!(lock.skill[0].commit.as_deref(), Some(tag_sha.as_str()));
        assert_eq!(lock.skill[0].rev.as_deref(), Some("v1"));
        let cached_clone = cache_path(&repo.root.join("cache"), &url, "v1");
        assert!(cached_clone.ends_with("upstream/v1"), "{cached_clone:?}");
        assert!(
            cached_clone.join("skills/tdd/SKILL.md").is_file(),
            "expected a clone under {}",
            cached_clone.display()
        );
    }

    #[test]
    fn a_hash_mismatch_refuses_under_frozen_and_refetches_otherwise() {
        let repo = Repo::new();
        let rel = source_dir(&repo, "vendor/tdd");
        let cfg = config(&format!("path:{rel}"), "", None);
        let claude = preset("claude");
        let mut w = writer(&repo.root);
        let ids = vec!["tdd".to_string()];
        resolve(&repo.root, &cfg, &claude, &ids, &opts(&repo, false), &mut w)
            .expect("first resolve");

        let vendored = repo.root.join(".claude/skills/tdd/SKILL.md");
        fs::write(&vendored, "tampered\n").expect("tamper");

        let err = resolve(&repo.root, &cfg, &claude, &ids, &opts(&repo, true), &mut w)
            .expect_err("frozen refuses");
        assert!(matches!(&err, SkillError::Unresolved { id, .. } if id == "tdd"));
        let events = Log::open(&repo.root.join(".harness"))
            .read()
            .expect("events");
        assert!(events.iter().any(|e| matches!(
            &e.kind,
            Kind::SkillResolved { id, result, .. } if id == "tdd" && result == "refused"
        )));

        let got = resolve(&repo.root, &cfg, &claude, &ids, &opts(&repo, false), &mut w)
            .expect("unfrozen refetches");
        assert_eq!(got[0].result, "fetched");
        assert_eq!(fs::read_to_string(&vendored).expect("restored"), BODY);
    }

    #[test]
    fn an_inline_preset_appends_the_body() {
        let repo = Repo::new();
        let rel = source_dir(&repo, "vendor/tdd");
        let cfg = config(&format!("path:{rel}"), "", None);
        let aider = preset("aider");
        let mut w = writer(&repo.root);
        let got = resolve(
            &repo.root,
            &cfg,
            &aider,
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect("resolve");
        assert!(repo.root.join(".harness/skills/tdd/SKILL.md").is_file());

        let out = render("first {{skill:tdd}} then work.", &got, &aider, &cfg);
        assert!(out.starts_with(
            "first the `tdd` skill (see the section `## Skill: tdd` below) then work."
        ));
        assert!(out.ends_with(&format!("\n\n## Skill: tdd\n\n{BODY}")));
    }

    #[test]
    fn a_tool_preset_renders_the_invocation() {
        let repo = Repo::new();
        let rel = source_dir(&repo, "vendor/tdd");
        let cfg = config(&format!("path:{rel}"), "", None);
        let mut w = writer(&repo.root);
        for (name, expected) in [
            ("claude", "`tdd` (invoke it via the Skill tool)"),
            ("pi", "`tdd` (invoke it as /skill:tdd)"),
        ] {
            let p = preset(name);
            let got = resolve(
                &repo.root,
                &cfg,
                &p,
                &["tdd".to_string()],
                &opts(&repo, false),
                &mut w,
            )
            .expect("resolve");
            let out = render("use {{skill:tdd}}.", &got, &p, &cfg);
            assert_eq!(out, format!("use {expected}."));
        }
    }

    #[test]
    fn a_source_that_is_not_a_known_scheme_is_refused() {
        let repo = Repo::new();
        let cfg = config("https://example.com/x.git", "", None);
        let mut w = writer(&repo.root);
        let err = resolve(
            &repo.root,
            &cfg,
            &preset("claude"),
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect_err("bad source");
        assert!(matches!(err, SkillError::BadSource { .. }));
    }
}
