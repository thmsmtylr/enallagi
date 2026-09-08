//! Roles declared in `[[role]]`: fetched, cached and pinned through the skills machinery, vendored to `<harness_dir>/roles/<name>.md`.

use crate::config::Config;
use crate::events::Writer;
use crate::skills::{
    pin, read_lock, valid_id, valid_path, write_lock, Pin, ResolveOpts, SkillError,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ResolvedRole {
    pub name: String,
    pub path: PathBuf,
    /// `cached` when the vendored file already matched the lock, `fetched` when it was copied in.
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
    let mut lock = read_lock(root)?;
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
        write_lock(root, &lock)?;
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
