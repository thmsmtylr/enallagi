//! Fetches declared skills from git or a path, vendors and pins them, and renders them into role prompts.

use crate::agent::Preset;
use crate::config::Config;
use crate::events::{Kind, Writer};
use crate::git;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("skill {id}: source {spec} is not github:<owner>/<repo>, git+<url> or path:<dir>")]
    BadSource { id: String, spec: String },
    #[error("skill id `{id}` must match ^[a-z0-9-]+$")]
    BadId { id: String },
    #[error("skill {id}: path `{path}` must be relative and free of `..`")]
    BadPath { id: String, path: String },
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
    // skipped when empty so a lock written before roles existed round-trips unchanged
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role: Vec<LockEntry>,
}

impl Default for Lock {
    fn default() -> Self {
        Lock {
            version: 1,
            skill: Vec::new(),
            role: Vec::new(),
        }
    }
}

pub fn lock_path(root: &Path, harness_dir: &str) -> PathBuf {
    crate::config::instance_path(root, harness_dir, "harness.lock")
}

pub fn parse_lock(text: &str) -> Result<Lock, SkillError> {
    toml::from_str(text).map_err(|e| SkillError::Lock(e.to_string()))
}

// a missing lock reads as empty, but an unparseable one is an error -- silently re-fetching over a corrupt pin defeats the pin
pub fn read_lock(root: &Path, harness_dir: &str) -> Result<Lock, SkillError> {
    let text = match fs::read_to_string(lock_path(root, harness_dir)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Lock::default()),
        Err(e) => return Err(e.into()),
    };
    parse_lock(&text)
}

pub fn write_lock(root: &Path, harness_dir: &str, lock: &Lock) -> Result<(), SkillError> {
    let mut lock = lock.clone();
    lock.skill.sort_by(|a, b| a.id.cmp(&b.id));
    lock.role.sort_by(|a, b| a.id.cmp(&b.id));
    let text = toml::to_string(&lock).map_err(|e| SkillError::Lock(e.to_string()))?;
    fs::write(lock_path(root, harness_dir), text)?;
    Ok(())
}

// the id becomes a filesystem path and a lock key, so a `..`, separator or shell metacharacter is refused before it can be either
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

// joined onto a fetched directory, so it must stay inside it; empty means the source root itself
pub fn valid_path(path: &str) -> bool {
    let p = Path::new(path);
    p.is_relative() && !p.components().any(|c| c == Component::ParentDir)
}

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

// configured dir, else the preset's own, else <harness_dir>/skills for a preset with no skills mechanism
// None is `custom`, or any preset this build does not know: it has no directory of its own
pub fn skills_dir(cfg: &Config, preset: Option<&Preset>) -> PathBuf {
    if let Some(dir) = cfg.layout.skills_dir.as_deref() {
        return PathBuf::from(dir);
    }
    if let Some(dir) = preset.and_then(|p| p.skills_dir.as_deref()) {
        return PathBuf::from(dir.replace("{harness_dir}", &cfg.layout.harness_dir));
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
            frozen: frozen_from_env(std::env::var_os("CI").as_deref()),
            cache_dir: default_cache_dir(),
        }
    }
}

// CI freezes skills like --frozen: an unattended run must fail on a stale lock, not fetch new instructions
pub(crate) fn frozen_from_env(ci: Option<&OsStr>) -> bool {
    ci.is_some()
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

pub fn resolve(
    root: &Path,
    cfg: &Config,
    preset: Option<&Preset>,
    ids: &[String],
    opts: &ResolveOpts,
    events: &mut Writer,
) -> Result<Vec<ResolvedSkill>, SkillError> {
    let base = root.join(skills_dir(cfg, preset));
    let mut lock = read_lock(root, &cfg.layout.harness_dir)?;
    let mut out = Vec::new();
    let mut dirty = false;

    for id in ids {
        // validate() refuses these at load; this is the second lock on the door since the id becomes a directory name
        if !valid_id(id) {
            return Err(SkillError::BadId { id: id.clone() });
        }
        let decl = cfg
            .skill
            .iter()
            .find(|s| &s.id == id)
            .ok_or_else(|| SkillError::Undeclared { id: id.clone() })?;
        // same second lock on the door as valid_id: the path becomes a filesystem join below
        if !valid_path(&decl.path) {
            return Err(SkillError::BadPath {
                id: id.clone(),
                path: decl.path.clone(),
            });
        }
        let dir = base.join(id);
        let p = Pin {
            id,
            source: &decl.source,
            rev: decl.rev.as_deref(),
            file: dir.join("SKILL.md"),
        };
        let (result, body) = pin(root, &p, &mut lock.skill, opts, events, |src| {
            vendor(&src.join(&decl.path), &dir)
        })?;
        dirty |= result == "fetched";
        out.push(ResolvedSkill {
            id: id.clone(),
            dir,
            result,
            body,
        });
    }

    if dirty {
        write_lock(root, &cfg.layout.harness_dir, &lock)?;
    }
    Ok(out)
}

/// One thing to pin: a skill or a role. `file` is the vendored file the lock's sha256 covers.
pub(crate) struct Pin<'a> {
    pub id: &'a str,
    pub source: &'a str,
    pub rev: Option<&'a str>,
    pub file: PathBuf,
}

/// The cache, fetch, vendor and lock cycle shared by skills and roles. `vendor` is given the
/// fetched source root and returns the pinned body. Returns (`cached` | `fetched`, body).
pub(crate) fn pin(
    root: &Path,
    p: &Pin,
    entries: &mut Vec<LockEntry>,
    opts: &ResolveOpts,
    events: &mut Writer,
    vendor: impl FnOnce(&Path) -> Result<String, SkillError>,
) -> Result<(String, String), SkillError> {
    let at = entries.iter().position(|e| e.id == p.id);
    let entry = at.map(|i| &entries[i]);
    let commit = entry.and_then(|e| e.commit.clone());
    match cached(entry, p) {
        Ok(body) => {
            events.emit(Kind::SkillResolved {
                id: p.id.into(),
                commit,
                result: "cached".into(),
            });
            Ok(("cached".into(), body))
        }
        Err(why) => {
            if opts.frozen {
                events.emit(Kind::SkillResolved {
                    id: p.id.into(),
                    commit,
                    result: "refused".into(),
                });
                return Err(SkillError::Unresolved {
                    id: p.id.into(),
                    why,
                });
            }
            let (source_root, commit) = fetch(root, p, opts)?;
            let body = vendor(&source_root)?;
            let new = LockEntry {
                id: p.id.into(),
                source: p.source.into(),
                rev: p.rev.map(String::from),
                commit: commit.clone(),
                sha256: sha256(body.as_bytes()),
            };
            match at {
                Some(i) => entries[i] = new,
                None => entries.push(new),
            }
            events.emit(Kind::SkillResolved {
                id: p.id.into(),
                commit,
                result: "fetched".into(),
            });
            Ok(("fetched".into(), body))
        }
    }
}

fn cached(entry: Option<&LockEntry>, p: &Pin) -> Result<String, String> {
    let Some(e) = entry else {
        return Err("no lock entry".into());
    };
    if e.source != p.source || e.rev.as_deref() != p.rev {
        return Err("the declaration moved away from the lock".into());
    }
    let body = fs::read_to_string(&p.file).map_err(|_| "not vendored".to_string())?;
    if sha256(body.as_bytes()) != e.sha256 {
        return Err(format!(
            "the vendored {} does not match its locked sha256",
            p.file.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    Ok(body)
}

fn fetch(
    root: &Path,
    p: &Pin,
    opts: &ResolveOpts,
) -> Result<(PathBuf, Option<String>), SkillError> {
    let bad = || SkillError::BadSource {
        id: p.id.into(),
        spec: p.source.into(),
    };
    let source = p.source.trim();

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

    let dir = cache_path(&opts.cache_dir, &url, p.rev.unwrap_or("HEAD"));
    clone(&url, p.rev, &dir)?;
    let commit = git::git(&dir, &["rev-parse", "HEAD"])?;
    Ok((dir, Some(commit)))
}

// every URL segment is kept, so two repos sharing a trailing <owner>/<repo> under different prefixes don't collide
fn cache_path(cache_dir: &Path, url: &str, rev: &str) -> PathBuf {
    let rest = match url.split_once("://") {
        Some((_, r)) => r.to_string(),
        // scp-like: git@host:owner/repo.git
        None => url
            .rsplit_once('@')
            .map_or(url, |(_, r)| r)
            .replace(':', "/"),
    };
    let segs: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    let last = segs.len().saturating_sub(1);
    let mut dir = cache_dir.join("git");
    for (i, seg) in segs.iter().enumerate() {
        let seg = if i == last {
            seg.trim_end_matches(".git")
        } else {
            seg
        };
        dir.push(segment(seg));
    }
    dir.push(segment(&rev.replace('/', "-")));
    dir
}

// a URL is attacker-supplied as far as this is concerned, and it's becoming a filesystem path
fn segment(s: &str) -> String {
    if s.is_empty() || s.chars().all(|c| c == '.') {
        return "_".to_string();
    }
    s.to_string()
}

// cloned beside the target and renamed into place, so a directory under that name is always a complete checkout, never a half clone
fn clone(url: &str, rev: Option<&str>, dir: &Path) -> Result<(), SkillError> {
    if dir.exists() {
        return Ok(());
    }
    let parent = dir.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    // not with_extension: a rev like `v1.2` would lose its `.2`
    let tmp = PathBuf::from(format!("{}.tmp", dir.display()));
    let _ = fs::remove_dir_all(&tmp);

    if let Err(e) = clone_into(url, rev, parent, &tmp) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if let Err(e) = fs::rename(&tmp, dir) {
        let _ = fs::remove_dir_all(&tmp);
        // losing the race to another process is not a failure -- it made the same checkout
        if !dir.exists() {
            return Err(e.into());
        }
    }
    Ok(())
}

fn clone_into(url: &str, rev: Option<&str>, parent: &Path, tmp: &Path) -> Result<(), SkillError> {
    let target = tmp.to_string_lossy().to_string();
    let Some(rev) = rev else {
        // `--` stops a url or target that starts with `-` from being read as a flag
        git::git(parent, &["clone", "--depth", "1", "--", url, &target])?;
        return Ok(());
    };
    if git::git_ok(
        parent,
        &["clone", "--depth", "1", "--branch", rev, "--", url, &target],
    ) {
        return Ok(());
    }
    // --branch takes a branch or a tag, never a commit sha
    let _ = fs::remove_dir_all(tmp);
    git::git(parent, &["clone", "--", url, &target])?;
    git::git(tmp, &["checkout", "-q", rev])?;
    Ok(())
}

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

// unresolved {{skill:<id>}} tokens are left alone; inline presets get the body appended, others get the invocation
pub fn render(
    role_text: &str,
    resolved: &[ResolvedSkill],
    preset: &Preset,
    cfg: &Config,
) -> String {
    let inline = preset.skills_dir.is_none();
    let mut invocation = preset
        .invocation
        .as_deref()
        .unwrap_or(&cfg.layout.skill_invocation)
        .to_string();
    // a configured skills directory is outside the preset's plugin, so its skills carry no plugin prefix
    if cfg.layout.skills_dir.is_some() {
        if let Ok(prefix) = regex::Regex::new(r"[\w-]+:<id>") {
            invocation = prefix.replace_all(&invocation, "<id>").into_owned();
        }
    }

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
    use crate::config::SkillDecl;
    use crate::events::Log;
    use crate::fixture::Repo;

    fn writer(root: &Path) -> Writer {
        Writer::new(Log::open(&root.join(".enallagi")))
    }

    fn config(source: &str, path: &str, rev: Option<&str>) -> Config {
        let mut cfg = Config::default();
        cfg.layout.harness_dir = ".enallagi".into();
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
        let got = resolve(
            &repo.root,
            &cfg,
            Some(&claude),
            &ids,
            &opts(&repo, false),
            &mut w,
        )
        .expect("resolve");
        assert_eq!(got[0].result, "fetched");
        let vendored = repo
            .root
            .join(".enallagi/adapters/claude/skills/tdd/SKILL.md");
        assert_eq!(fs::read_to_string(&vendored).expect("vendored"), BODY);
        assert!(repo
            .root
            .join(".enallagi/adapters/claude/skills/tdd/references/more.md")
            .is_file());

        let lock = read_lock(&repo.root, &cfg.layout.harness_dir).expect("lock");
        assert_eq!(lock.version, 1);
        assert_eq!(lock.skill[0].id, "tdd");
        assert_eq!(lock.skill[0].sha256, sha256(BODY.as_bytes()));
        assert_eq!(lock.skill[0].commit, None);
        let before =
            fs::read_to_string(lock_path(&repo.root, &cfg.layout.harness_dir)).expect("lock text");

        let again = resolve(
            &repo.root,
            &cfg,
            Some(&claude),
            &ids,
            &opts(&repo, false),
            &mut w,
        )
        .expect("resolve again");
        assert_eq!(again[0].result, "cached");
        assert_eq!(
            fs::read_to_string(lock_path(&repo.root, &cfg.layout.harness_dir)).expect("lock text"),
            before
        );
    }

    #[test]
    fn a_git_source_is_cloned_at_rev_into_the_cache() {
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
            Some(&claude),
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect("resolve");

        assert_eq!(got[0].result, "fetched");
        assert_eq!(
            fs::read_to_string(
                repo.root
                    .join(".enallagi/adapters/claude/skills/tdd/SKILL.md")
            )
            .expect("vendored"),
            BODY
        );
        let tag_sha = crate::git::git(&upstream.root, &["rev-parse", "v1^{commit}"]).expect("sha");
        let lock = read_lock(&repo.root, &cfg.layout.harness_dir).expect("lock");
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
    fn a_hash_mismatch_refuses_under_frozen() {
        let repo = Repo::new();
        let rel = source_dir(&repo, "vendor/tdd");
        let cfg = config(&format!("path:{rel}"), "", None);
        let claude = preset("claude");
        let mut w = writer(&repo.root);
        let ids = vec!["tdd".to_string()];
        resolve(
            &repo.root,
            &cfg,
            Some(&claude),
            &ids,
            &opts(&repo, false),
            &mut w,
        )
        .expect("first resolve");

        let vendored = repo
            .root
            .join(".enallagi/adapters/claude/skills/tdd/SKILL.md");
        fs::write(&vendored, "tampered\n").expect("tamper");

        let err = resolve(
            &repo.root,
            &cfg,
            Some(&claude),
            &ids,
            &opts(&repo, true),
            &mut w,
        )
        .expect_err("frozen refuses");
        assert!(matches!(&err, SkillError::Unresolved { id, .. } if id == "tdd"));
        let events = Log::open(&repo.root.join(".enallagi"))
            .read()
            .expect("events");
        assert!(events.iter().any(|e| matches!(
            &e.kind,
            Kind::SkillResolved { id, result, .. } if id == "tdd" && result == "refused"
        )));

        let got = resolve(
            &repo.root,
            &cfg,
            Some(&claude),
            &ids,
            &opts(&repo, false),
            &mut w,
        )
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
            Some(&aider),
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect("resolve");
        assert!(repo.root.join(".enallagi/skills/tdd/SKILL.md").is_file());

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
            (
                "claude",
                "`tdd` (invoke it via the Skill tool as `harness:tdd`)",
            ),
            ("pi", "`tdd` (invoke it as /skill:tdd)"),
        ] {
            let p = preset(name);
            let got = resolve(
                &repo.root,
                &cfg,
                Some(&p),
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
    fn a_configured_dir_drops_the_plugin_name() {
        let repo = Repo::new();
        let rel = source_dir(&repo, "vendor/tdd");
        let mut cfg = config(&format!("path:{rel}"), "", None);
        cfg.layout.skills_dir = Some(".claude/skills".into());
        let p = preset("claude");
        let got = resolve(
            &repo.root,
            &cfg,
            Some(&p),
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut writer(&repo.root),
        )
        .expect("resolve");
        assert!(repo.root.join(".claude/skills/tdd/SKILL.md").is_file());
        assert_eq!(
            render("use {{skill:tdd}}.", &got, &p, &cfg),
            "use `tdd` (invoke it via the Skill tool as `tdd`)."
        );
    }

    #[test]
    fn a_source_that_is_not_a_known_scheme_is_refused() {
        let repo = Repo::new();
        let cfg = config("https://example.com/x.git", "", None);
        let mut w = writer(&repo.root);
        let err = resolve(
            &repo.root,
            &cfg,
            Some(&preset("claude")),
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect_err("bad source");
        assert!(matches!(err, SkillError::BadSource { .. }));
    }

    fn bare_source(home: &Path, rel: &str, body: &str) -> String {
        let upstream = Repo::new();
        upstream.write("skills/tdd/SKILL.md", body);
        upstream.commit_all("skill");
        let bare = home.join(rel);
        if let Some(parent) = bare.parent() {
            fs::create_dir_all(parent).expect("parents");
        }
        let from = upstream.root.to_string_lossy().to_string();
        let to = bare.to_string_lossy().to_string();
        crate::git::git(home, &["clone", "--bare", "-q", &from, &to]).expect("bare clone");
        format!("git+file://{}", bare.display())
    }

    #[test]
    fn an_id_that_is_not_a_plain_name_is_refused() {
        assert!(valid_id("tdd-2") && !valid_id("Tdd") && !valid_id("") && !valid_id(".."));
        assert!(valid_path("skills/x") && valid_path("") && !valid_path("../x"));
        assert!(!valid_path("a/../../b") && !valid_path("/abs"));

        let repo = Repo::new();
        let mut cfg = config("path:vendor/tdd", "", None);
        cfg.skill[0].id = "../escape".into();
        let mut w = writer(&repo.root);
        let err = resolve(
            &repo.root,
            &cfg,
            Some(&preset("claude")),
            &["../escape".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect_err("bad id");
        assert!(matches!(err, SkillError::BadId { .. }));
    }

    #[test]
    fn an_escaping_path_is_refused() {
        let repo = Repo::new();
        let cfg = config("path:vendor/tdd", "../escape", None);
        let mut w = writer(&repo.root);
        let err = resolve(
            &repo.root,
            &cfg,
            Some(&preset("claude")),
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect_err("bad path");
        assert!(matches!(err, SkillError::BadPath { .. }));
    }

    #[test]
    fn two_similar_urls_do_not_share_a_cache() {
        let home = tempfile::TempDir::new().expect("tempdir");
        let a = bare_source(home.path(), "a/sub/repo.git", "A body\n");
        let b = bare_source(home.path(), "b/sub/repo.git", "B body\n");

        let repo = Repo::new();
        let mut cfg = config(&a, "skills/tdd", None);
        cfg.skill[0].id = "one".into();
        cfg.skill.push(SkillDecl {
            id: "two".into(),
            source: b,
            path: "skills/tdd".into(),
            rev: None,
            gate: "none".into(),
            why: "because".into(),
        });
        let mut w = writer(&repo.root);
        resolve(
            &repo.root,
            &cfg,
            Some(&preset("claude")),
            &["one".to_string(), "two".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect("resolve");

        let body = |id: &str| {
            fs::read_to_string(
                repo.root
                    .join(format!(".enallagi/adapters/claude/skills/{id}/SKILL.md")),
            )
            .expect("vendored")
        };
        assert_eq!(body("one"), "A body\n");
        assert_eq!(body("two"), "B body\n");
    }

    #[test]
    fn an_abandoned_partial_clone_is_thrown_away() {
        let home = tempfile::TempDir::new().expect("tempdir");
        let source = bare_source(home.path(), "up.git", BODY);
        let repo = Repo::new();
        let cfg = config(&source, "skills/tdd", None);

        let url = source.trim_start_matches("git+");
        let dir = cache_path(&repo.root.join("cache"), url, "HEAD");
        let tmp = PathBuf::from(format!("{}.tmp", dir.display()));
        fs::create_dir_all(&tmp).expect("mkdir");
        fs::write(tmp.join("junk.txt"), "half a clone").expect("junk");

        let mut w = writer(&repo.root);
        resolve(
            &repo.root,
            &cfg,
            Some(&preset("claude")),
            &["tdd".to_string()],
            &opts(&repo, false),
            &mut w,
        )
        .expect("resolve");

        assert_eq!(
            fs::read_to_string(
                repo.root
                    .join(".enallagi/adapters/claude/skills/tdd/SKILL.md")
            )
            .expect("vendored"),
            BODY
        );
        assert!(!tmp.exists(), "the .tmp clone must not survive");
        assert!(!dir.join("junk.txt").exists(), "junk must not be adopted");
        assert!(dir.join("skills/tdd/SKILL.md").is_file());
    }

    #[test]
    fn ci_freezes_the_default_options() {
        assert!(frozen_from_env(Some(OsStr::new("true"))));
        assert!(!frozen_from_env(None));
    }
}
