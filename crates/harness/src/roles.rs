//! roles declared in `[[role]]`, pinned like skills and vendored to `<harness_dir>/roles/<name>.md`

use crate::config::Config;
use crate::events::Writer;
use crate::sandbox::RolePolicy;
use crate::skills::{
    pin, read_lock, valid_id, valid_path, write_lock, Pin, ResolveOpts, SkillError,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ResolvedRole {
    pub name: String,
    pub path: PathBuf,
    /// `cached` when the vendored file already matched the lock, `fetched` when it was copied in
    pub result: String,
    pub body: String,
}

pub fn resolve(
    root: &Path,
    cfg: &Config,
    names: &[String],
    opts: &ResolveOpts,
    events: &mut Writer,
) -> Result<Vec<ResolvedRole>, SkillError> {
    let base = root.join(&cfg.layout.harness_dir).join("roles");
    let mut lock = read_lock(root, &cfg.layout.harness_dir)?;
    let mut out = Vec::new();
    let mut dirty = false;

    for name in names {
        // validate() refuses these at load; the second lock on the door, since both become paths
        if !valid_id(name) {
            return Err(SkillError::BadId { id: name.clone() });
        }
        let decl = cfg
            .role
            .iter()
            .find(|r| &r.name == name)
            .ok_or_else(|| SkillError::Undeclared { id: name.clone() })?;
        if !valid_path(&decl.path) {
            return Err(SkillError::BadPath {
                id: name.clone(),
                path: decl.path.clone(),
            });
        }
        let path = base.join(format!("{name}.md"));
        let p = Pin {
            id: name,
            source: &decl.source,
            rev: decl.rev.as_deref(),
            file: path.clone(),
        };
        let (result, body) = pin(root, &p, &mut lock.role, opts, events, |src| {
            vendor(&src.join(&decl.path).join(format!("{name}.md")), &path)
        })?;
        dirty |= result == "fetched";
        out.push(ResolvedRole {
            name: name.clone(),
            path,
            result,
            body,
        });
    }

    if dirty {
        write_lock(root, &cfg.layout.harness_dir, &lock)?;
    }
    Ok(out)
}

fn vendor(src: &Path, dst: &Path) -> Result<String, SkillError> {
    if !src.is_file() {
        return Err(SkillError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} is not a file", src.display()),
        )));
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst)?;
    Ok(fs::read_to_string(dst)?)
}

// the lines between a leading `---` and the next; empty when there is none
pub(crate) fn frontmatter(text: &str) -> &str {
    text.strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---"))
        .map_or("", |(front, _)| front)
}

/// the `writes:` and `hosts:` a role's frontmatter declares, comma-separated
pub fn policy(text: &str) -> RolePolicy {
    let list = |key: &str| {
        frontmatter(text).lines().find_map(|l| {
            let value = l.strip_prefix(key)?;
            Some(
                value
                    .split(',')
                    .map(|v| v.trim().trim_matches(['"', '\'']))
                    .filter(|v| !v.is_empty())
                    .map(String::from)
                    .collect::<Vec<_>>(),
            )
        })
    };
    RolePolicy {
        writes: list("writes:"),
        hosts: list("hosts:").unwrap_or_default(),
        ..RolePolicy::default()
    }
}
