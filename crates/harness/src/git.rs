use std::path::Path;
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git {args}: {stderr}")]
    Failed { args: String, stderr: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn cmd(root: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.current_dir(root).args(args);
    c
}

pub fn git(root: &Path, args: &[&str]) -> Result<String, GitError> {
    let out = cmd(root, args).output()?;
    if !out.status.success() {
        return Err(GitError::Failed {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

pub fn git_ok(root: &Path, args: &[&str]) -> bool {
    git(root, args).is_ok()
}

pub fn head(root: &Path) -> Option<String> {
    git(root, &["rev-parse", "--short", "HEAD"]).ok()
}

pub fn porcelain(root: &Path) -> Vec<String> {
    git(root, &["status", "--porcelain"])
        .map(|s| s.lines().map(String::from).collect())
        .unwrap_or_default()
}

pub fn diff_names(root: &Path, base: &str) -> Vec<String> {
    git(root, &["diff", "--name-only", base, "HEAD"])
        .map(|s| {
            s.lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn commit_paths(root: &Path, paths: &[&str], msg: &str) -> Result<bool, GitError> {
    // `git add -- a b` fails as a whole when any path is missing, so filter first or a missing path silently no-ops the rest
    for path in paths.iter().filter(|p| root.join(p).exists()) {
        let _ = git(root, &["add", "--", path]);
    }
    if git_ok(root, &["diff", "--cached", "--quiet"]) {
        return Ok(false);
    }
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", msg],
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;
    #[test]
    fn head_and_porcelain_read_a_fixture() {
        let r = Repo::new();
        assert!(head(&r.root).is_some());
        assert!(porcelain(&r.root).is_empty());
        r.write("src/a.ts", "x");
        assert_eq!(porcelain(&r.root), vec!["?? src/a.ts".to_string()]);
        assert!(commit_paths(&r.root, &["src/a.ts"], "add").unwrap());
        assert!(!commit_paths(&r.root, &["src/a.ts"], "again").unwrap());
        assert_eq!(diff_names(&r.root, "HEAD~1"), vec!["src/a.ts".to_string()]);
    }

    #[test]
    fn a_missing_path_does_not_stop_the_others_from_being_committed() {
        let r = Repo::new();
        r.write("TASKS.md", "queue\n");
        assert!(commit_paths(&r.root, &["TASKS.md", "DECISIONS.md"], "queue").unwrap());
        assert_eq!(diff_names(&r.root, "HEAD~1"), vec!["TASKS.md".to_string()]);
    }
}
