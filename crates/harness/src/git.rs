use std::path::{Path, PathBuf};
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
        git(root, &["add", "--", path])?;
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

// the harness directory's own repository once it has one; until then instance files share the product's
pub fn state_root(root: &Path, harness_dir: &str) -> PathBuf {
    let dir = root.join(harness_dir);
    if !harness_dir.is_empty() && dir.join(".git").exists() {
        dir
    } else {
        root.to_path_buf()
    }
}

// a path relative to the product root, as it is named inside the repository that holds it
pub fn locate(root: &Path, harness_dir: &str, path: &str) -> (PathBuf, String, bool) {
    let state = state_root(root, harness_dir);
    match path.strip_prefix(&format!("{harness_dir}/")) {
        Some(inner) if state != root => (state, inner.to_string(), true),
        _ => (root.to_path_buf(), path.to_string(), false),
    }
}

// the subject names the product HEAD the state was committed against, which is what `harness base` reads back
pub fn commit_instance(
    root: &Path,
    harness_dir: &str,
    names: &[&str],
    msg: &str,
) -> Result<bool, GitError> {
    let state = state_root(root, harness_dir);
    if state == root && names.is_empty() {
        return Ok(false);
    }
    let sha = git(root, &["rev-parse", "HEAD"])?;
    let msg = format!("{msg} at {sha}");
    if state == root {
        let paths: Vec<String> = names
            .iter()
            .map(|name| crate::config::instance_rel(root, harness_dir, name))
            .collect();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        return commit_paths(root, &paths, &msg);
    }
    git(&state, &["add", "-A"])?;
    if git_ok(&state, &["diff", "--cached", "--quiet"]) {
        return Ok(false);
    }
    let mut args: Vec<String> = Vec::new();
    for key in ["user.name", "user.email"] {
        if let Ok(value) = git(root, &["config", key]) {
            args.extend(["-c".to_string(), format!("{key}={value}")]);
        }
    }
    args.extend(["-c", "commit.gpgsign=false", "commit", "-q", "-m", &msg].map(String::from));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    git(&state, &args)?;
    Ok(true)
}

// a product revision as the state repository's own: the first state commit recorded at it, or an Err, never a revision that repository lacks
pub fn state_rev(root: &Path, harness_dir: &str, rev: &str) -> Result<String, String> {
    let state = state_root(root, harness_dir);
    if state == root {
        return Ok(rev.to_string());
    }
    let sha = git(
        root,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )
    .map_err(|e| e.to_string())?;
    let found = git(
        &state,
        &[
            "log",
            "--reverse",
            "--format=%H",
            &format!("--grep= at {sha}$"),
        ],
    )
    .map_err(|e| e.to_string())?;
    found
        .lines()
        .next()
        .map(String::from)
        .ok_or_else(|| format!("no commit in {harness_dir} records product revision {rev} ({sha})"))
}

// the product sha to diff a task against; the root layout keeps the pickaxe the verifier ran before
pub fn task_base(root: &Path, harness_dir: &str, task: &str) -> Result<String, String> {
    let tasks = crate::config::instance_rel(root, harness_dir, "TASKS.md");
    let (repo, inner, nested) = locate(root, harness_dir, &tasks);
    let block = crate::queue::Block {
        id: task.to_string(),
        title: String::new(),
        line: 0,
        body: Vec::new(),
    };
    let heading = crate::queue::block_text(&block).trim_end().to_string();
    if !nested {
        return git(
            &repo,
            &["log", "-1", "--format=%H", "-S", &heading, "--", &inner],
        )
        .map_err(|e| e.to_string());
    }
    let heading = format!("^{}", regex::escape(&heading));
    let subjects = git(
        &repo,
        &[
            "log",
            "--reverse",
            "--format=%s",
            "--pickaxe-regex",
            "-S",
            &heading,
            "--",
            &inner,
        ],
    )
    .map_err(|e| e.to_string())?;
    let Some(first) = subjects.lines().next() else {
        return Ok(String::new());
    };
    first
        .rsplit_once(" at ")
        .map(|(_, sha)| sha.to_string())
        .filter(|sha| sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| {
            format!("the state commit that added {task}, \"{first}\", records no product sha")
        })
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
    fn a_failed_add_for_an_existing_path_is_an_err() {
        let r = Repo::new();
        r.write(".gitignore", "src/a.ts\n");
        assert!(commit_paths(&r.root, &[".gitignore"], "ignore").unwrap());
        r.write("src/a.ts", "x");
        assert!(commit_paths(&r.root, &["src/a.ts"], "add ignored").is_err());
    }

    #[test]
    fn a_missing_path_still_commits_the_others() {
        let r = Repo::new();
        r.write("TASKS.md", "queue\n");
        assert!(commit_paths(&r.root, &["TASKS.md", "DECISIONS.md"], "queue").unwrap());
        assert_eq!(diff_names(&r.root, "HEAD~1"), vec!["TASKS.md".to_string()]);
    }
}
