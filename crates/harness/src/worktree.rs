//! One lane runs in its own git worktree. Its product work is fast-forwarded back onto the parent branch, or, under `[pr] per_task`, left on the branch `enallagi pr` pushes. A state parent that moved under the lane is merged.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::git;

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("worktree: the parent checkout is detached; check out a branch first")]
    Detached,
    #[error("worktree: could not create {0}")]
    Create(PathBuf),
    #[error(
        "worktree: {repo} has uncommitted work; commit it before a lane branches from it: {files}"
    )]
    Dirty { repo: PathBuf, files: String },
    #[error("worktree: {repo} fast-forwarded to {branch}, a later merge was refused, and `git reset --keep {pre}` failed: {stderr}. Both lane worktrees are left")]
    Rollback {
        repo: PathBuf,
        branch: String,
        pre: String,
        stderr: String,
    },
    #[error(transparent)]
    Git(#[from] git::GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct LaneReport {
    pub merged: bool,
    pub left: Option<PathBuf>,
    // the lane's worktree of the harness directory's own repository, inside `left` at the harness directory's path
    pub state: Option<PathBuf>,
    pub reason: String,
    pub branch: String,
    // recorded but never used to decide whether the lane merges: only the worktree's committed state does that
    pub run_error: Option<String>,
    // the checkout did not follow its upstream: the lane's own work may still have merged
    pub follow_error: Option<String>,
    // the ticket the lane's claim held, under `writers > 1`
    pub task: Option<String>,
}

// (parent repository, lane worktree) pairs, the state repository first: it sits inside the product worktree
type Pairs = Vec<(PathBuf, PathBuf)>;

fn create_worktree(
    root: &Path,
    cfg: &Config,
    base: &str,
) -> Result<(String, PathBuf, Pairs), WorktreeError> {
    let harness_dir = &cfg.layout.harness_dir;
    let state_root = git::state_root(root, harness_dir);
    let start = by_branch(root, cfg)
        .then(|| resumable_base(root, cfg))
        .flatten()
        .unwrap_or_else(|| "HEAD".to_string());
    let mut last_dir = PathBuf::new();
    for n in 0..=10u32 {
        let name = if n == 0 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        let branch = format!("lane/{name}");
        let dir = root
            .join(harness_dir)
            .join("worktrees")
            .join(format!("lane-{name}"));
        let mut pairs = vec![(root.to_path_buf(), dir.clone())];
        if state_root != root {
            pairs.insert(0, (state_root.clone(), dir.join(harness_dir)));
        }
        let mut made: Vec<&(PathBuf, PathBuf)> = Vec::new();
        for pair in pairs.iter().rev() {
            let (repo, wt) = pair;
            let from = if repo.as_path() == root {
                &start
            } else {
                "HEAD"
            };
            if !git::git_ok(
                repo,
                &[
                    "worktree",
                    "add",
                    "-b",
                    &branch,
                    &wt.to_string_lossy(),
                    from,
                ],
            ) {
                break;
            }
            made.push(pair);
        }
        if made.len() == pairs.len() {
            if start != "HEAD" {
                catch_up(root, &dir, &start);
            }
            return Ok((branch, dir, pairs));
        }
        for (repo, wt) in made.into_iter().rev() {
            let _ = git::git(
                repo,
                &["worktree", "remove", "--force", &wt.to_string_lossy()],
            );
            let _ = git::git(repo, &["branch", "-D", &branch]);
        }
        last_dir = dir;
    }
    Err(WorktreeError::Create(last_dir))
}

// a resumed lane takes in what HEAD gained since it branched, or it runs on an old tree; a merge
// that conflicts is abandoned and the lane keeps its own work, as before
fn catch_up(root: &Path, dir: &Path, start: &str) {
    let Ok(head) = git::git(root, &["rev-parse", "HEAD"]) else {
        return;
    };
    let merge = [
        "-c",
        "commit.gpgsign=false",
        "merge",
        "-q",
        "--no-edit",
        head.as_str(),
    ];
    if let Err(err) = git::git(dir, &merge) {
        let _ = git::git(dir, &["merge", "--abort"]);
        eprintln!(
            "worktree: resumed {start} without HEAD's newer commits, the merge failed: {err}"
        );
    }
}

// under `[pr] per_task` the operator reviews the branch `enallagi pr` pushed, so the lane's product
// commits never reach the checkout's own branch. With no separate state repository the merge is also
// the only way a verdict lands, so there it stays.
pub fn by_branch(root: &Path, cfg: &Config) -> bool {
    cfg.pr.per_task && git::state_root(root, &cfg.layout.harness_dir) != root
}

// the checkout's branch carries what its own tracking ref carries and nothing else, so a branch
// named for neither its remote nor its upstream still follows the ref `enallagi pr` builds onto
fn follow_upstream(root: &Path, branch: &str) -> Result<String, String> {
    // the caller decides what a failure costs, so each step names itself rather than returning prose
    let step = |what: &str, r: Result<String, git::GitError>| {
        r.map_err(|e| format!("{branch} did not follow its upstream, {what} failed: {e}"))
    };
    let upstream = step(
        "git rev-parse --abbrev-ref @{upstream}",
        git::git(root, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
    )?;
    step("git fetch", git::git(root, &["fetch", "-q"]))?;
    let out = step(
        "git merge --ff-only @{upstream}",
        git::git(root, &["merge", "--ff-only", "@{upstream}"]),
    )?;
    Ok(format!("{branch} follows {upstream}: {out}"))
}

// under `[pr] per_task` the checkout's branch never carries the lane's product commits, so a task
// left unfinished lives on the last lane branch, and the next lane starts there rather than at HEAD
fn resumable_base(root: &Path, cfg: &Config) -> Option<String> {
    let blocks = crate::tracker::Tracker::open(root, &cfg.layout.harness_dir)
        .blocks()
        .unwrap_or_default();
    let open: Vec<&str> = blocks
        .iter()
        .filter(|b| crate::tracker::state(b).as_deref() != Some("done"))
        .map(|b| b.id.as_str())
        .collect();
    let listed = git::git(
        root,
        &[
            "branch",
            "--list",
            "lane/*",
            "--format=%(refname:short)",
            "--sort=-refname",
        ],
    )
    .ok()?;
    // a lane branch is resumed while a commit HEAD lacks names a task not yet done; a pull request
    // lands a task as a rebuilt commit, so a finished lane still carries commits HEAD does not
    listed
        .lines()
        .map(str::trim)
        .find(|b| {
            !b.is_empty()
                && git::git(root, &["log", "--format=%s", &format!("HEAD..{b}")]).is_ok_and(
                    |subjects| {
                        subjects
                            .lines()
                            .any(|s| open.iter().any(|id| crate::gates::names_task(s, id)))
                    },
                )
        })
        .map(str::to_string)
}

// the commits the parent gained while the lane ran, which a merge takes in and a fast-forward never meets
fn absorbed_by(repo: &Path, branch: &str) -> String {
    git::git(repo, &["log", "--format=%h %s", &format!("{branch}..HEAD")])
        .unwrap_or_default()
        .lines()
        .collect::<Vec<_>>()
        .join("; ")
}

// a merge git refused before it began has nothing to abort, and its stderr names what stood in the way
struct Unmerged {
    err: git::GitError,
    aborted: bool,
}

// a merge commit needs an identity the state repository rarely configures, so it borrows the product's
fn merge_into(repo: &Path, root: &Path, branch: &str, ff: bool) -> Result<String, Unmerged> {
    if ff {
        return git::git(repo, &["merge", "--ff-only", branch]).map_err(|err| Unmerged {
            err,
            aborted: false,
        });
    }
    let mut args = git::identity_args(root);
    args.extend(
        [
            "-c",
            "commit.gpgsign=false",
            "merge",
            "--no-ff",
            "--no-edit",
            branch,
        ]
        .map(String::from),
    );
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    git::git(repo, &args).map_err(|err| {
        // git leaves the conflict in the index and the tree, and a parent left mid-merge is worse than
        // an unmerged lane; `--abort` itself says whether a merge had started
        let aborted = git::git(repo, &["merge", "--abort"]).is_ok();
        Unmerged { err, aborted }
    })
}

// an untracked file is the operator's own and a lane never sees it, so it is named and not refused
fn untracked_notice(paths: &[String]) -> String {
    format!(
        "worktree: {} untracked path(s) stay in the parent: {}",
        paths.len(),
        paths.join(" ")
    )
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hold {
    Writer,
    Reader,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Claim {
    pub id: String,
    pub scope: Vec<String>,
    pub branch: String,
    pub worktree: PathBuf,
    pub base: String,
    pub hold: Hold,
}

// every lane reaches the parent's `run/` through the main worktree, as `events::Log::open` does
fn run_file(root: &Path, harness_dir: &str, name: &str) -> PathBuf {
    let dir = root.join(harness_dir);
    git::main_worktree_path(&dir)
        .unwrap_or(dir)
        .join("run")
        .join(name)
}

// advisory, and released with the process, so a crashed launcher leaves no stale lock
fn lock_file(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    f.lock()?;
    Ok(f)
}

// a claim is live while its worktree exists, so a dead one is dropped on every read
fn with_claims<T>(
    root: &Path,
    harness_dir: &str,
    f: impl FnOnce(&mut Vec<Claim>) -> T,
) -> std::io::Result<T> {
    use std::io::{Read, Seek, Write};
    let mut file = lock_file(&run_file(root, harness_dir, "claims.json"))?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let mut claims: Vec<Claim> = serde_json::from_str(&text).unwrap_or_default();
    claims.retain(|c| c.worktree.exists());
    let out = f(&mut claims);
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(serde_json::to_string_pretty(&claims)?.as_bytes())?;
    Ok(out)
}

pub fn claims(root: &Path, harness_dir: &str) -> Vec<Claim> {
    with_claims(root, harness_dir, |c| c.clone()).unwrap_or_default()
}

/// Claims the lowest-id takeable ticket in `root`'s queue that no live claim holds and whose scope overlaps no live writer's, for the lane at `wt`.
pub fn claim(root: &Path, cfg: &Config, wt: &Path) -> std::io::Result<Option<String>> {
    let dir = &cfg.layout.harness_dir;
    with_claims(root, dir, |claims| {
        let blocks = crate::tracker::Tracker::open(root, dir)
            .blocks()
            .map_err(std::io::Error::other)?;
        let scope = |b: &crate::block::Block| {
            crate::block::field(b, "scope")
                .map(|l| crate::gates::scope_globs(&l))
                .unwrap_or_default()
        };
        let id = crate::tracker::takeable_by(&blocks, |b| {
            claims.iter().all(|c| {
                c.id != b.id
                    && (c.hold == Hold::Reader || !crate::gates::overlaps(&c.scope, &scope(b)))
            })
        });
        if let Some(id) = &id {
            let b = blocks
                .iter()
                .find(|b| &b.id == id)
                .expect("taken from blocks");
            claims.push(Claim {
                id: id.clone(),
                scope: scope(b),
                branch: git::git(wt, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default(),
                worktree: wt.to_path_buf(),
                base: git::head(wt).unwrap_or_default(),
                hold: Hold::Writer,
            });
        }
        Ok(id)
    })?
}

pub fn release(root: &Path, harness_dir: &str, wt: &Path) -> std::io::Result<()> {
    with_claims(root, harness_dir, |claims| {
        claims.retain(|c| c.worktree != wt)
    })
}

// the lane's commits replayed onto what landed first, then judged again on the tree they now land on
fn rebase_checked(
    root: &Path,
    cfg: &Config,
    dir: &Path,
    branch: &str,
    parent: &str,
) -> Result<(), String> {
    let landed = absorbed_by(root, branch);
    let mut args = git::identity_args(root);
    args.extend(["-c", "commit.gpgsign=false", "rebase", "-q", parent].map(String::from));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if git::git(dir, &args).is_err() {
        let paths = git::git(dir, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
        let _ = git::git(dir, &["rebase", "--abort"]);
        let tip = git::git(dir, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
        return Err(format!(
            "integrate: {branch} conflicts with {parent} over {} after {landed}; see `git show {tip}`",
            paths.lines().collect::<Vec<_>>().join(" ")
        ));
    }
    let report =
        crate::gates::check_delta(&crate::sandbox::Sandbox::start(dir, cfg), dir, cfg, false);
    if report.accepts() {
        return Ok(());
    }
    let tip = git::git(dir, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let named = if report.unforgiven.is_empty() {
        "no failure could be named".to_string()
    } else {
        report.unforgiven.join(", ")
    };
    Err(format!(
        "integrate: the check was red (exit {}) on {branch} rebased onto {parent} after {landed}: {named}; see `git show {tip}`",
        report.exit
    ))
}

// the reason is committed in the parent and the branch kept, so the reason's `git show` resolves
fn send_back(
    root: &Path,
    cfg: &Config,
    id: &str,
    pairs: &Pairs,
    reason: &str,
) -> Result<(), String> {
    let dir = &cfg.layout.harness_dir;
    crate::tracker::Tracker::open(root, dir)
        .set_status(id, "ready-for-agent", reason)
        .map_err(|e| e.to_string())?;
    git::commit_instance(
        root,
        dir,
        &[crate::tracker::SPECS],
        &format!("chore({id}): integrate sent it back"),
    )
    .map_err(|e| e.to_string())?;
    for (repo, wt) in pairs {
        let _ = git::git(
            repo,
            &["worktree", "remove", "--force", &wt.to_string_lossy()],
        );
    }
    Ok(())
}

// what happens next is decided by the worktrees' own committed state, never run's exit status
pub fn lane(
    root: &Path,
    cfg: &Config,
    run: &mut dyn FnMut(&Path) -> anyhow::Result<()>,
) -> Result<LaneReport, WorktreeError> {
    let report = landed(root, cfg, run)?;
    if cfg.parallel.writers.get() > 1 {
        let mut w = crate::events::Writer::new(crate::events::Log::open(
            &root.join(&cfg.layout.harness_dir),
        ));
        w.emit(crate::events::Kind::Gate {
            gate: "integrate".to_string(),
            task: report.task.clone().unwrap_or_default(),
            pass: report.merged,
            reason: report.reason.clone(),
            tally: None,
        });
    }
    Ok(report)
}

fn landed(
    root: &Path,
    cfg: &Config,
    run: &mut dyn FnMut(&Path) -> anyhow::Result<()>,
) -> Result<LaneReport, WorktreeError> {
    let parent_branch = git::git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if parent_branch == "HEAD" {
        return Err(WorktreeError::Detached);
    }

    // a lane branches from the parent's HEAD, so tracked work uncommitted there is work it never sees
    let state_root = git::state_root(root, &cfg.layout.harness_dir);
    let mut parents = vec![root.to_path_buf()];
    if state_root != root {
        parents.insert(0, state_root);
    }
    let mut untracked: Vec<String> = Vec::new();
    for repo in parents {
        let (tracked, loose): (Vec<String>, Vec<String>) = git::porcelain(&repo)
            .into_iter()
            .partition(|l| !l.starts_with("??"));
        // the operator's own queue edits are what the lane should run on, so they are committed, not refused
        if !tracked.is_empty() && repo != root {
            git::commit_instance(root, &cfg.layout.harness_dir, &[], "queue: operator edits")?;
            eprintln!(
                "worktree: committed the operator's edits in {} before branching: {}",
                repo.display(),
                tracked.join(", ")
            );
            continue;
        }
        if !tracked.is_empty() {
            return Err(WorktreeError::Dirty {
                repo,
                files: tracked.join(", "),
            });
        }
        untracked.extend(loose.into_iter().map(|l| {
            repo.join(l.trim_start_matches("??").trim_start())
                .display()
                .to_string()
        }));
    }
    if !untracked.is_empty() {
        eprintln!("{}", untracked_notice(&untracked));
    }

    // before the worktree, never after: a lane branched from a checkout behind its upstream builds
    // a pull request `enallagi pr` then refuses, and no later fast-forward reaches that lane
    let by_branch = by_branch(root, cfg);
    let (followed, follow_error) = match by_branch.then(|| follow_upstream(root, &parent_branch)) {
        None => (None, None),
        Some(Ok(said)) => (Some(said), None),
        Some(Err(err)) => (Some(err.clone()), Some(err)),
    };

    let ts = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    let base = format!("{ts}-{}", std::process::id());
    let (branch, dir, pairs) = create_worktree(root, cfg, &base)?;
    let state = (pairs.len() > 1).then(|| pairs[0].1.clone());

    let run_error = run(&dir).err().map(|e| e.to_string());
    let parallel = cfg.parallel.writers.get() > 1;
    let held = if parallel {
        claims(root, &cfg.layout.harness_dir)
    } else {
        Vec::new()
    }
    .into_iter()
    .find(|c| c.worktree == dir)
    .map(|c| c.id)
    .filter(|id| !id.is_empty());
    let left = |reason: String| LaneReport {
        merged: false,
        left: Some(dir.clone()),
        state: state.clone(),
        reason,
        branch: branch.clone(),
        run_error: run_error.clone(),
        follow_error: follow_error.clone(),
        task: held.clone(),
    };
    // a writer lane's failed landing goes back to the queue; an unpinned one is left as under one writer
    let back = |reason: String| match held
        .as_deref()
        .map(|id| send_back(root, cfg, id, &pairs, &reason))
    {
        Some(Ok(())) => LaneReport {
            left: None,
            state: None,
            ..left(reason)
        },
        Some(Err(e)) => left(format!("{reason}; it was not sent back: {e}")),
        None => left(reason),
    };

    // Uncommitted work in the lane is the lane's to finish, not ours to throw away.
    if let Some((_, wt)) = pairs.iter().find(|(_, wt)| !git::porcelain(wt).is_empty()) {
        return Ok(left(format!(
            "{branch} has uncommitted work in {}",
            wt.display()
        )));
    }

    // ponytail: one landing at a time, each paying one check, and held before the ancestry check; a merge queue pays off only once the check is slow next to a lane
    let _landing = match parallel {
        true => Some(lock_file(&run_file(
            root,
            &cfg.layout.harness_dir,
            "land.lock",
        ))?),
        false => None,
    };

    let merging: Pairs = pairs
        .iter()
        .filter(|(repo, _)| !(by_branch && repo.as_path() == root))
        .cloned()
        .collect();

    // checked before any merge, so a product branch that cannot fast-forward merges neither. The
    // state repository's lane commits name product shas, so the product side never merges a commit
    let stuck: Vec<String> = merging
        .iter()
        .filter(|(repo, _)| repo.as_path() == root)
        .filter(|(repo, _)| !git::git_ok(repo, &["merge-base", "--is-ancestor", "HEAD", &branch]))
        .map(|(repo, _)| format!("{branch} cannot fast-forward into {}", repo.display()))
        .collect();
    if !stuck.is_empty() && parallel {
        if let Err(reason) = rebase_checked(root, cfg, &dir, &branch, &parent_branch) {
            return Ok(back(reason));
        }
    } else if !stuck.is_empty() {
        return Ok(left(stuck.join("\n")));
    }

    let mut said: Vec<String> = followed.into_iter().collect();
    let mut merged: Vec<(&PathBuf, String)> = Vec::new();
    for (repo, _) in &merging {
        let pre = git::git(repo, &["rev-parse", "HEAD"])?;
        // only the state repository reaches this not-an-ancestor: the product was screened above
        let ff = git::git_ok(repo, &["merge-base", "--is-ancestor", "HEAD", &branch]);
        let absorbed = absorbed_by(repo, &branch);
        match merge_into(repo, root, &branch, ff) {
            Ok(out) => {
                said.push(if ff {
                    out
                } else {
                    format!("{} merged {branch} over {absorbed}: {out}", repo.display())
                });
                merged.push((repo, pre));
            }
            Err(Unmerged { err, aborted }) => {
                let reason = match &err {
                    git::GitError::Failed { .. } if aborted => Some(format!(
                        "{branch} conflicts with {} over {absorbed}, and the merge was aborted",
                        repo.display()
                    )),
                    git::GitError::Failed { stderr, .. } => Some(stderr.clone()),
                    _ => None,
                };
                // a merge git refuses after the ancestry check (a dirty parent tree) undoes the ones before it
                for (done, pre) in merged {
                    if let Err(e) = git::git(done, &["reset", "--keep", &pre]) {
                        return Err(WorktreeError::Rollback {
                            repo: done.clone(),
                            branch,
                            pre,
                            stderr: e.to_string(),
                        });
                    }
                }
                match reason {
                    Some(reason) if aborted => return Ok(back(reason)),
                    Some(reason) => return Ok(left(reason)),
                    None => return Err(err.into()),
                }
            }
        }
    }
    // `git branch -d` refuses an unmerged branch, so a lane delivered by branch keeps its own until
    // `enallagi pr` has replayed it
    for (repo, wt) in &pairs {
        let _ = git::git(repo, &["worktree", "remove", &wt.to_string_lossy()]);
        let _ = git::git(repo, &["branch", "-d", &branch]);
    }
    Ok(LaneReport {
        merged: true,
        left: None,
        state,
        reason: said.join("\n"),
        branch,
        run_error,
        follow_error,
        task: held,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Repo;
    use crate::tracker::Tracker;

    fn repo_with_harness_dir() -> Repo {
        let r = Repo::new();
        r.write(".gitignore", ".enallagi/worktrees/\n");
        r.write(".enallagi/enallagi.toml", "");
        r.commit_all("gitignore worktrees");
        r
    }

    fn cfg(r: &Repo) -> Config {
        crate::config::load(&r.root).expect("load config")
    }

    fn commit_all_in(dir: &Path, msg: &str) {
        git::git(dir, &["add", "-A"]).expect("add");
        git::git(
            dir,
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", msg],
        )
        .expect("commit");
    }

    fn commit_in(dir: &Path, msg: &str) {
        git::git(dir, &["add", "-A"]).expect("add");
        git::git(
            dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                msg,
            ],
        )
        .expect("commit");
    }

    fn nested_repo() -> Repo {
        let r = Repo::new();
        crate::init::install(&r.root, &crate::init::InitOpts::default()).expect("install");
        r.queue(
            ".enallagi",
            "## [T-001] one\nscope: one.txt\nstatus: ready-for-agent\n\n## [T-002] two\nscope: two.txt\nstatus: ready-for-agent\n",
        );
        commit_in(&r.root, "install");
        commit_in(&r.root.join(".enallagi"), "queue");
        r
    }

    fn land_at(wt: &Path, id: &str, file: &str, status: &str) {
        std::fs::write(wt.join(file), id).expect("write");
        commit_in(wt, &format!("{id} work"));
        let state = wt.join(".enallagi");
        Tracker::open(wt, ".enallagi")
            .set_status(id, status, "")
            .expect("the lane's verdict");
        commit_in(&state, &format!("verify {id}"));
        std::fs::write(state.join("events.jsonl"), "{}\n").expect("an ignored run log");
    }

    fn land(wt: &Path, id: &str, file: &str) {
        land_at(wt, id, file, "done");
    }

    // the parent decides the same block the lane decides, so the state merge conflicts rather than applying
    fn decide_in_the_parent(state: &Path, status: &str) {
        let root = state.parent().expect("the product root");
        Tracker::open(root, ".enallagi")
            .set_status("T-001", status, "")
            .expect("the parent's decision");
        commit_in(state, "a second writer decided the same block");
    }

    fn state_of(root: &Path, id: &str) -> Option<String> {
        let b = Tracker::open(root, ".enallagi")
            .get(id)
            .expect("the queue")?;
        crate::tracker::state(&b)
    }

    fn per_task_repo() -> Repo {
        let r = nested_repo();
        let toml = crate::config::config_path(&r.root);
        let text = std::fs::read_to_string(&toml).expect("enallagi.toml");
        std::fs::write(&toml, format!("{text}\n[pr]\nper_task = true\n")).expect("write");
        commit_in(&r.root.join(".enallagi"), "per_task");
        r
    }

    fn origin_of(r: &Repo) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let bare = dir.path().join("origin.git");
        git::git(
            dir.path(),
            &[
                "clone",
                "-q",
                "--bare",
                &r.root.to_string_lossy(),
                "origin.git",
            ],
        )
        .expect("clone");
        git::git(
            &r.root,
            &["remote", "add", "origin", &bare.to_string_lossy()],
        )
        .expect("remote");
        git::git(&r.root, &["fetch", "-q", "origin"]).expect("fetch");
        let branch = git::git(&r.root, &["rev-parse", "--abbrev-ref", "HEAD"]).expect("branch");
        let tracking = format!("--set-upstream-to=origin/{branch}");
        git::git(&r.root, &["branch", &tracking, &branch]).expect("upstream");
        (dir, bare)
    }

    // a commit the checkout has never seen, on the branch a clone of `origin` checks out
    fn commit_on_origin(bare: &Path, file: &str) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().expect("tempdir");
        git::git(
            dir.path(),
            &["clone", "-q", &bare.to_string_lossy(), "clone"],
        )
        .expect("clone");
        let wt = dir.path().join("clone");
        std::fs::write(wt.join(file), "x").expect("write");
        commit_in(&wt, "upstream work");
        git::git(&wt, &["push", "-q", "origin", "HEAD"]).expect("push");
        dir
    }

    fn remove_left(r: &Repo, report: &LaneReport) {
        let state = r.root.join(".enallagi");
        if let Some(left) = &report.left {
            let _ = git::git(
                &state,
                &[
                    "worktree",
                    "remove",
                    "--force",
                    &left.join(".enallagi").to_string_lossy(),
                ],
            );
            let _ = git::git(
                &r.root,
                &["worktree", "remove", "--force", &left.to_string_lossy()],
            );
        }
        let _ = git::git(&state, &["branch", "-D", &report.branch]);
        let _ = git::git(&r.root, &["branch", "-D", &report.branch]);
    }

    fn scoped_repo() -> Repo {
        let r = repo_with_harness_dir();
        r.queue(
            ".enallagi",
            "## [T-001] a\nscope: src/a/**\nstatus: ready-for-agent\n\n## [T-002] b\nscope: src/a/x.rs\nstatus: ready-for-agent\n\n## [T-003] c\nscope: src/b/**\nstatus: ready-for-agent\n",
        );
        r
    }

    #[test]
    fn claim_skips_overlapping_scope() {
        let r = scoped_repo();
        let cfg = cfg(&r);
        let (one, two) = (
            tempfile::TempDir::new().unwrap(),
            tempfile::TempDir::new().unwrap(),
        );

        let first = claim(&r.root, &cfg, one.path()).expect("claim");
        let second = claim(&r.root, &cfg, two.path()).expect("claim");

        assert_eq!(first.as_deref(), Some("T-001"));
        assert_eq!(second.as_deref(), Some("T-003"));
        let held: Vec<String> = claims(&r.root, ".enallagi")
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(held, ["T-001", "T-003"]);
    }

    #[test]
    fn claim_drops_dead_worktree() {
        let r = scoped_repo();
        let cfg = cfg(&r);
        let one = tempfile::TempDir::new().unwrap();
        claim(&r.root, &cfg, one.path()).expect("claim");
        drop(one);

        let two = tempfile::TempDir::new().unwrap();
        assert_eq!(
            claim(&r.root, &cfg, two.path()).expect("claim").as_deref(),
            Some("T-001")
        );
        let held: Vec<PathBuf> = claims(&r.root, ".enallagi")
            .into_iter()
            .map(|c| c.worktree)
            .collect();
        assert_eq!(held, [two.path().to_path_buf()]);
    }

    #[test]
    fn claim_holds_file_lock() {
        let r = scoped_repo();
        let path = run_file(&r.root, ".enallagi", "claims.json");
        let blocked = with_claims(&r.root, ".enallagi", |_| {
            let f = std::fs::File::open(&path).expect("open");
            matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock))
        })
        .expect("claims");
        assert!(blocked);
    }

    #[test]
    fn release_frees_scope() {
        let r = scoped_repo();
        let cfg = cfg(&r);
        let (one, two) = (
            tempfile::TempDir::new().unwrap(),
            tempfile::TempDir::new().unwrap(),
        );
        claim(&r.root, &cfg, one.path()).expect("claim");
        release(&r.root, ".enallagi", one.path()).expect("release");
        assert_eq!(
            claim(&r.root, &cfg, two.path()).expect("claim").as_deref(),
            Some("T-001")
        );
    }

    // two writers and a check that is red only once one.txt and two.txt sit in one tree
    fn parallel_repo(check: &str) -> Repo {
        let r = nested_repo();
        let cmd = r.stub_check(check);
        commit_in(&r.root, "check");
        let toml = crate::config::config_path(&r.root);
        let text = std::fs::read_to_string(&toml).expect("enallagi.toml");
        std::fs::write(
            &toml,
            format!("{text}\n[parallel]\nwriters = 2\n\n[check]\ncommand = \"{cmd}\"\nfail_name = '\\(fail\\) (.+)$'\n"),
        )
        .expect("write");
        commit_in(&r.root.join(".enallagi"), "parallel");
        r
    }

    fn claimed(r: &Repo, cfg: &Config, wt: &Path, id: &str) {
        assert_eq!(claim(&r.root, cfg, wt).expect("claim").as_deref(), Some(id));
    }

    // lane B's run lands lane A first, so B's parent moved under it
    fn after_first(r: &Repo, files: (&str, &str)) -> LaneReport {
        let cfg = cfg(r);
        lane(&r.root, &cfg, &mut |wt| {
            claimed(r, &cfg, wt, "T-001");
            let first = lane(&r.root, &cfg, &mut |wt| {
                claimed(r, &cfg, wt, "T-002");
                land(wt, "T-002", files.1);
                Ok(())
            })
            .expect("first lane");
            assert!(first.merged, "{}", first.reason);
            land(wt, "T-001", files.0);
            Ok(())
        })
        .expect("lane")
    }

    fn integrations(r: &Repo) -> Vec<(String, bool, String)> {
        crate::events::Log::open(&r.root.join(".enallagi"))
            .read()
            .expect("events")
            .into_iter()
            .filter_map(|e| match e.kind {
                crate::events::Kind::Gate {
                    gate,
                    task,
                    pass,
                    reason,
                    ..
                } if gate == "integrate" => Some((task, pass, reason)),
                _ => None,
            })
            .collect()
    }

    fn gate_line(r: &Repo, id: &str) -> String {
        let b = Tracker::open(&r.root, ".enallagi")
            .get(id)
            .expect("queue")
            .expect("ticket");
        crate::block::field(&b, "gate").unwrap_or_default()
    }

    fn kept(r: &Repo, report: &LaneReport) {
        assert!(report.left.is_none(), "{}", report.reason);
        let listed = git::git(&r.root, &["branch", "--list", &report.branch]).expect("branches");
        assert!(!listed.trim().is_empty(), "{} was deleted", report.branch);
        let worktrees = git::git(&r.root, &["worktree", "list"]).expect("worktrees");
        assert!(!worktrees.contains("lane-"), "{worktrees}");
    }

    #[test]
    fn moved_parent_lands() {
        let r = parallel_repo("exit 0\n");
        let report = after_first(&r, ("one.txt", "two.txt"));

        assert!(report.merged, "{}", report.reason);
        assert_eq!(report.task.as_deref(), Some("T-001"));
        assert!(r.root.join("one.txt").exists() && r.root.join("two.txt").exists());
        for id in ["T-001", "T-002"] {
            assert_eq!(state_of(&r.root, id).as_deref(), Some("done"), "{id}");
        }
        let passes: Vec<(String, bool)> = integrations(&r)
            .into_iter()
            .map(|(t, p, _)| (t, p))
            .collect();
        assert_eq!(passes, [("T-002".into(), true), ("T-001".into(), true)]);
    }

    #[test]
    fn rebase_conflict_sends_back() {
        let r = parallel_repo("exit 0\n");
        let report = after_first(&r, ("shared.txt", "shared.txt"));

        assert!(!report.merged, "{}", report.reason);
        assert_eq!(
            state_of(&r.root, "T-001").as_deref(),
            Some("ready-for-agent")
        );
        let gate = gate_line(&r, "T-001");
        for want in ["T-002", "shared.txt", "git show"] {
            assert!(gate.contains(want), "{want}: {gate}");
        }
        kept(&r, &report);
        assert!(git::porcelain(&r.root.join(".enallagi")).is_empty());
        let last = integrations(&r).pop().expect("an integrate event");
        assert_eq!((last.0.as_str(), last.1), ("T-001", false), "{}", last.2);
    }

    #[test]
    fn rebase_red_returns() {
        let r = parallel_repo(
            "if [ -f one.txt ] && [ -f two.txt ]; then echo '(fail) combo'; exit 1; fi\nexit 0\n",
        );
        let report = after_first(&r, ("one.txt", "two.txt"));

        assert!(!report.merged, "{}", report.reason);
        assert_eq!(
            state_of(&r.root, "T-001").as_deref(),
            Some("ready-for-agent")
        );
        assert!(!r.root.join("one.txt").exists());
        let gate = gate_line(&r, "T-001");
        for want in ["combo", "T-002", "git show"] {
            assert!(gate.contains(want), "{want}: {gate}");
        }
        kept(&r, &report);
        let last = integrations(&r).pop().expect("an integrate event");
        assert_eq!((last.0.as_str(), last.1), ("T-001", false), "{}", last.2);
    }

    #[test]
    fn state_conflict_sends_back() {
        let r = parallel_repo("exit 0\n");
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let report = lane(&r.root, &cfg, &mut |wt| {
            claimed(&r, &cfg, wt, "T-001");
            land(wt, "T-001", "one.txt");
            decide_in_the_parent(&state, "needs-info");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "{}", report.reason);
        assert_eq!(
            state_of(&r.root, "T-001").as_deref(),
            Some("ready-for-agent")
        );
        assert!(gate_line(&r, "T-001").contains("conflicts with"));
        assert!(!r.root.join("one.txt").exists());
        kept(&r, &report);
    }

    #[test]
    fn two_lanes_merge_and_both_verdicts_land() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");

        for (id, file) in [("T-001", "one.txt"), ("T-002", "two.txt")] {
            let report = lane(&r.root, &cfg, &mut |wt| {
                land(wt, id, file);
                Ok(())
            })
            .expect("lane");
            assert!(report.merged, "{id}: {}", report.reason);
        }

        for id in ["T-001", "T-002"] {
            assert_eq!(state_of(&r.root, id).as_deref(), Some("done"), "{id}");
        }
        assert!(r.root.join("one.txt").exists() && r.root.join("two.txt").exists());
        for repo in [&r.root, &state] {
            let branches = git::git(repo, &["branch", "--list", "lane/*"]).expect("branch list");
            assert!(branches.trim().is_empty(), "{}: {branches}", repo.display());
        }
    }

    #[test]
    fn per_task_leaves_the_checkout_unmoved() {
        let r = per_task_repo();
        let cfg = cfg(&r);
        let pre_head = git::head(&r.root);

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            land(wt, "T-002", "two.txt");
            Ok(())
        })
        .expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert_eq!(git::head(&r.root), pre_head);
        assert!(!r.root.join("one.txt").exists() && !r.root.join("two.txt").exists());
        for id in ["T-001", "T-002"] {
            assert_eq!(state_of(&r.root, id).as_deref(), Some("done"), "{id}");
        }
    }

    // a reviewer caught this: a checkout that did not follow its upstream reported success
    #[test]
    fn a_follow_that_fails_is_carried_to_the_caller() {
        let r = per_task_repo();
        let cfg = cfg(&r);

        // no origin and no upstream, so the first step of the follow fails
        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            Ok(())
        })
        .expect("lane");

        let err = report.follow_error.expect("the follow failed and said so");
        assert!(err.contains("did not follow its upstream"), "{err}");
        assert!(err.contains("@{upstream}"), "{err}");
        assert!(
            report.reason.contains("did not follow its upstream"),
            "{}",
            report.reason
        );
    }

    #[test]
    fn per_task_follows_the_upstream_branch() {
        let r = per_task_repo();
        let cfg = cfg(&r);
        let (_origin, bare) = origin_of(&r);
        let branch = git::git(&r.root, &["rev-parse", "--abbrev-ref", "HEAD"]).expect("branch");

        // the lane pushes what `enallagi pr` pushes, and the operator merges it on the remote
        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            git::git(wt, &["push", "-q", "origin", "HEAD:refs/heads/task/T-001"])?;
            Ok(())
        })
        .expect("lane");
        assert!(report.merged, "reason: {}", report.reason);
        git::git(
            &bare,
            &[
                "update-ref",
                &format!("refs/heads/{branch}"),
                "refs/heads/task/T-001",
            ],
        )
        .expect("merge the pull request");

        let (mut parent_inside, mut lane_inside) = (false, false);
        lane(&r.root, &cfg, &mut |wt| {
            parent_inside = r.root.join("one.txt").exists();
            lane_inside = wt.join("one.txt").exists();
            Ok(())
        })
        .expect("the next lane");

        assert!(
            parent_inside,
            "the checkout followed after the lane, not before"
        );
        assert!(
            lane_inside,
            "the lane was created before the checkout followed"
        );
        let count =
            |range: String| git::git(&r.root, &["rev-list", "--count", &range]).expect("rev-list");
        assert_eq!(count(format!("{branch}..origin/{branch}")), "0");
        assert_eq!(count(format!("origin/{branch}..{branch}")), "0");
        assert!(r.root.join("one.txt").exists());
    }

    #[test]
    fn a_follow_reads_the_branchs_tracking_ref() {
        let r = per_task_repo();
        let cfg = cfg(&r);
        let (_origin, bare) = origin_of(&r);
        let default = git::git(&r.root, &["rev-parse", "--abbrev-ref", "HEAD"]).expect("branch");
        let _clone = commit_on_origin(&bare, "upstream.txt");
        // the checkout's branch is not named for its upstream, and `origin/work` does not exist
        git::git(&r.root, &["branch", "-m", &default, "work"]).expect("rename");
        let tracking = format!("--set-upstream-to=origin/{default}");
        git::git(&r.root, &["branch", &tracking, "work"]).expect("upstream");

        let report = lane(&r.root, &cfg, &mut |_wt| Ok(())).expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert!(
            r.root.join("upstream.txt").exists(),
            "reason: {}",
            report.reason
        );
    }

    #[test]
    fn an_unfinished_task_keeps_its_implementation() {
        let r = per_task_repo();
        let cfg = cfg(&r);

        let first = lane(&r.root, &cfg, &mut |wt| {
            land_at(wt, "T-001", "one.txt", "review");
            Ok(())
        })
        .expect("the first lane");
        assert!(first.merged, "reason: {}", first.reason);
        assert!(!r.root.join("one.txt").exists());

        let mut seen = false;
        let second = lane(&r.root, &cfg, &mut |wt| {
            seen = wt.join("one.txt").exists();
            Ok(())
        })
        .expect("the second lane");

        assert!(
            seen,
            "the second lane started without T-001's implementation: {}",
            second.reason
        );
    }

    // an upstream commit moves HEAD off the lane branch; the task at review is still on it
    #[test]
    fn an_unfinished_task_survives_upstream_work() {
        let r = per_task_repo();
        let cfg = cfg(&r);
        let (_origin, bare) = origin_of(&r);

        let first = lane(&r.root, &cfg, &mut |wt| {
            land_at(wt, "T-001", "one.txt", "review");
            Ok(())
        })
        .expect("the first lane");
        assert!(first.merged, "reason: {}", first.reason);
        let _clone = commit_on_origin(&bare, "unrelated.txt");

        let mut seen = false;
        let second = lane(&r.root, &cfg, &mut |wt| {
            seen = wt.join("one.txt").exists();
            Ok(())
        })
        .expect("the second lane");

        assert!(
            seen,
            "the second lane started without T-001's implementation: {}",
            second.reason
        );
    }

    #[test]
    fn a_resumed_lane_takes_in_upstream_work() {
        let r = per_task_repo();
        let cfg = cfg(&r);
        let (_origin, bare) = origin_of(&r);

        lane(&r.root, &cfg, &mut |wt| {
            land_at(wt, "T-001", "one.txt", "review");
            Ok(())
        })
        .expect("the first lane");
        let _clone = commit_on_origin(&bare, "unrelated.txt");

        let mut seen = (false, false);
        let second = lane(&r.root, &cfg, &mut |wt| {
            seen = (
                wt.join("one.txt").exists(),
                wt.join("unrelated.txt").exists(),
            );
            Ok(())
        })
        .expect("the second lane");
        assert_eq!(seen, (true, true), "{}", second.reason);
    }

    #[test]
    fn a_lane_whose_tasks_are_done_is_not_resumed() {
        let r = per_task_repo();
        let cfg = cfg(&r);

        lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            Ok(())
        })
        .expect("the first lane");

        let mut seen = true;
        let second = lane(&r.root, &cfg, &mut |wt| {
            seen = wt.join("one.txt").exists();
            Ok(())
        })
        .expect("the second lane");
        assert!(!seen, "a finished lane was resumed: {}", second.reason);
    }

    #[test]
    fn a_checkout_in_step_with_origin_is_unchanged() {
        let r = per_task_repo();
        let cfg = cfg(&r);
        let (_origin, _bare) = origin_of(&r);
        let pre_head = git::head(&r.root);

        let report = lane(&r.root, &cfg, &mut |_wt| Ok(())).expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert_eq!(git::head(&r.root), pre_head);
    }

    #[test]
    fn stuck_state_branch() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let pre_head = git::head(&r.root);

        let mut before_the_merge = None;
        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            decide_in_the_parent(&state, "needs-info");
            before_the_merge = git::head(&state);
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        assert!(report.reason.contains(&report.branch), "{}", report.reason);
        assert!(
            report.reason.contains(&state.display().to_string()),
            "{}",
            report.reason
        );
        assert_eq!(git::head(&state), before_the_merge);
        assert!(
            git::porcelain(&state).is_empty(),
            "the merge was not aborted"
        );
        assert_eq!(git::head(&r.root), pre_head);
        assert!(!r.root.join("one.txt").exists());
        let left = report.left.clone().expect("left in place");
        assert!(left.join("one.txt").exists());
        assert_eq!(state_of(&left, "T-001").as_deref(), Some("done"));
        remove_left(&r, &report);
    }

    #[test]
    fn a_dirty_state_parent_names_the_file() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let pre_head = git::head(&r.root);

        let ticket = Tracker::open(&r.root, ".enallagi")
            .path_of("T-001")
            .expect("T-001's file");
        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            std::fs::write(state.join("moved.txt"), "y").expect("write");
            commit_in(&state, "a second writer moved the state");
            // an operator's edit arriving mid-run: tracked, uncommitted, and what git refuses to overwrite
            std::fs::write(&ticket, "edited mid-run\n").expect("write");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        let name = ticket.file_name().unwrap().to_string_lossy();
        assert!(report.reason.contains(&*name), "{}", report.reason);
        assert!(
            !report.reason.contains("conflicts with"),
            "{}",
            report.reason
        );
        assert_eq!(git::head(&r.root), pre_head);
        remove_left(&r, &report);
    }

    fn append_progress(state: &Path, entry: &str) {
        let path = state.join("PROGRESS.md");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        std::fs::write(&path, format!("{text}\n{entry}\n")).expect("append");
        commit_in(state, entry);
    }

    #[test]
    fn progress_appends_union_merge() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");

        let report = lane(&r.root, &cfg, &mut |wt| {
            let second = lane(&r.root, &cfg, &mut |wt| {
                append_progress(&wt.join(".enallagi"), "entry two");
                Ok(())
            })
            .expect("second lane");
            assert!(second.merged, "{}", second.reason);
            land(wt, "T-001", "one.txt");
            append_progress(&wt.join(".enallagi"), "entry one");
            Ok(())
        })
        .expect("lane");

        assert!(report.merged, "{}", report.reason);
        let progress = std::fs::read_to_string(state.join("PROGRESS.md")).expect("progress");
        for entry in ["entry one", "entry two"] {
            assert!(progress.contains(entry), "{entry}:\n{progress}");
        }
    }

    #[test]
    fn a_moved_state_parent_merges_with_a_commit() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let pre_head = git::head(&r.root);

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            std::fs::write(state.join("moved.txt"), "y").expect("write");
            commit_in(&state, "a second writer moved the state");
            Ok(())
        })
        .expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert!(
            report.reason.contains("a second writer moved the state"),
            "{}",
            report.reason
        );
        assert_ne!(git::head(&r.root), pre_head);
        assert!(r.root.join("one.txt").exists());
        assert!(state.join("moved.txt").exists());
        assert_eq!(state_of(&r.root, "T-001").as_deref(), Some("done"));
        // a sha and two parents: the state absorbed the second writer rather than replaying over them
        let parents = git::git(&state, &["rev-list", "--parents", "-1", "HEAD"]).expect("rev-list");
        assert_eq!(parents.split_whitespace().count(), 3, "{parents}");
    }

    #[test]
    fn a_state_merge_rolls_back_with_the_product() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let pre_root = git::head(&r.root);

        let mut before_the_merge = None;
        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            std::fs::write(state.join("moved.txt"), "y").expect("write");
            commit_in(&state, "a second writer moved the state");
            before_the_merge = git::head(&state);
            // the parent's own untracked copy is what the product's fast-forward refuses
            r.write("one.txt", "an operator's own untracked edit");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        // the state merged and the product refused: the reason is git's, not the ancestry check's
        assert!(report.reason.contains("one.txt"), "{}", report.reason);
        assert_eq!(git::head(&r.root), pre_root);
        assert_eq!(git::head(&state), before_the_merge, "{}", report.reason);
        let parents = git::git(&state, &["rev-list", "--parents", "-1", "HEAD"]).expect("rev-list");
        assert_eq!(parents.split_whitespace().count(), 2, "{parents}");
        assert_ne!(state_of(&r.root, "T-001").as_deref(), Some("done"));
        remove_left(&r, &report);
    }

    // the run a lane could not merge is the one its log is most needed for, so the log is not in the
    // worktree the operator is told to remove
    #[test]
    fn stuck_lane_events() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");

        let report = lane(&r.root, &cfg, &mut |wt| {
            let mut w = crate::events::Writer::new(crate::events::Log::open(&wt.join(".enallagi")));
            w.emit(crate::events::Kind::Halt {
                halt: "x".into(),
                reason: "y".into(),
            });
            land(wt, "T-001", "one.txt");
            decide_in_the_parent(&state, "needs-info");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        let log = std::fs::read_to_string(state.join("events.jsonl")).expect("the parent's log");
        assert!(log.contains(r#""kind":"halt""#), "{log}");
        remove_left(&r, &report);
    }

    #[test]
    fn a_stuck_product_branch_merges_neither() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let pre_state = git::head(&state);

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            r.write("moved.txt", "y");
            commit_in(&r.root, "a second writer moved the parent");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        assert_eq!(git::head(&state), pre_state);
        assert_ne!(state_of(&r.root, "T-001").as_deref(), Some("done"));
        remove_left(&r, &report);
    }

    #[test]
    fn a_dirty_product_parent_refuses_the_lane() {
        let r = nested_repo();
        let cfg = cfg(&r);
        r.write("tracked.txt", "committed");
        commit_in(&r.root, "a tracked file to edit");
        r.write("tracked.txt", "an operator's uncommitted work");

        let mut ran = false;
        let err = lane(&r.root, &cfg, &mut |_wt| {
            ran = true;
            Ok(())
        })
        .err()
        .expect("refused");

        assert!(!ran, "the lane ran against a dirty parent");
        let msg = err.to_string();
        assert!(msg.contains("tracked.txt"), "{msg}");
        assert!(msg.contains(&r.root.display().to_string()), "{msg}");
        let listed = git::git(&r.root, &["worktree", "list"]).expect("worktree list");
        assert!(!listed.contains("/worktrees/lane-"), "{listed}");
    }

    #[test]
    fn an_untracked_parent_file_does_not_block_a_lane() {
        let r = nested_repo();
        let cfg = cfg(&r);
        r.write("scratch.txt", "an operator's scratch file");

        let mut ran = false;
        let report = lane(&r.root, &cfg, &mut |_wt| {
            ran = true;
            Ok(())
        })
        .expect("the lane ran");

        assert!(ran, "the lane was refused over an untracked file");
        assert!(report.merged, "reason: {}", report.reason);
        assert_eq!(
            std::fs::read_to_string(r.root.join("scratch.txt")).expect("scratch.txt"),
            "an operator's scratch file"
        );
    }

    #[test]
    fn the_untracked_notice_names_every_path_once() {
        let notice = untracked_notice(&["a/one.txt".to_string(), "b/two.txt".to_string()]);
        assert!(!notice.trim_end().contains('\n'), "{notice}");
        assert!(
            notice.contains("a/one.txt") && notice.contains("b/two.txt"),
            "{notice}"
        );
    }

    #[test]
    fn dirty_state_parent_commits_first() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        std::fs::write(state.join("PROGRESS.md"), "an uncommitted queue edit\n").expect("write");

        let mut seen = String::new();
        lane(&r.root, &cfg, &mut |wt| {
            seen = std::fs::read_to_string(wt.join(".enallagi/PROGRESS.md")).expect("PROGRESS.md");
            Ok(())
        })
        .expect("lane");

        assert!(seen.contains("an uncommitted queue edit"), "{seen}");
        assert!(git::porcelain(&state).is_empty());
    }

    #[test]
    fn a_locked_parent_index_leaves_both_worktrees() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let lock = r.root.join(".git/index.lock");
        let (pre_root, pre_state) = (git::head(&r.root), git::head(&state));

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            // another process holding the parent index is what the merge hits
            std::fs::write(&lock, "").expect("write the lock");
            Ok(())
        })
        .expect("lane");
        std::fs::remove_file(&lock).expect("release the lock");

        assert!(!report.merged, "reason: {}", report.reason);
        // both repositories have an index.lock, so the parent product path is what pins the case
        let named = r
            .root
            .canonicalize()
            .expect("canonicalize")
            .join(".git/index.lock");
        assert!(
            report.reason.contains(&named.display().to_string()),
            "{}",
            report.reason
        );
        assert_eq!(git::head(&r.root), pre_root);
        assert_eq!(git::head(&state), pre_state, "reason: {}", report.reason);
        let left = report.left.clone().expect("left in place");
        assert!(left.exists(), "{}", left.display());
        let left_state = report.state.clone().expect("the state worktree");
        assert!(left_state.exists(), "{}", left_state.display());
        remove_left(&r, &report);
    }

    #[test]
    fn a_refused_product_merge_rolls_back() {
        let r = nested_repo();
        let cfg = cfg(&r);
        let state = r.root.join(".enallagi");
        let (pre_root, pre_state) = (git::head(&r.root), git::head(&state));

        let report = lane(&r.root, &cfg, &mut |wt| {
            land(wt, "T-001", "one.txt");
            r.write("one.txt", "an operator's own untracked edit");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        assert_eq!(git::head(&r.root), pre_root);
        assert_eq!(git::head(&state), pre_state, "reason: {}", report.reason);
        assert_ne!(state_of(&r.root, "T-001").as_deref(), Some("done"));
        remove_left(&r, &report);
    }

    #[test]
    fn a_lane_leaves_the_parent_untouched() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);
        let pre_head = git::head(&r.root);

        let mut observed_head_inside: Option<String> = None;
        let mut observed_porcelain_inside: Vec<String> = Vec::new();
        let report = lane(&r.root, &cfg, &mut |wt| {
            observed_head_inside = git::head(&r.root);
            observed_porcelain_inside = git::porcelain(&r.root);
            let _ = wt;
            Ok(())
        })
        .expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert_eq!(observed_head_inside, pre_head);
        assert!(observed_porcelain_inside.is_empty());
    }

    #[test]
    fn a_lanes_worktree_lands_under_the_harness_directory() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);

        let mut seen_dir: Option<PathBuf> = None;
        let mut seen_in_worktree_list = false;
        lane(&r.root, &cfg, &mut |wt| {
            seen_dir = Some(wt.to_path_buf());
            let listed = git::git(&r.root, &["worktree", "list"]).unwrap_or_default();
            seen_in_worktree_list = listed.contains(&wt.to_string_lossy().into_owned());
            Ok(())
        })
        .expect("lane");

        let dir = seen_dir.expect("closure ran");
        assert!(dir.starts_with(r.root.join(&cfg.layout.harness_dir).join("worktrees")));
        assert!(
            seen_in_worktree_list,
            "worktree not listed while the lane ran"
        );
    }

    #[test]
    fn the_parent_branch_fast_forwarded_to_the_lane() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);
        let pre_head = git::head(&r.root);

        let report = lane(&r.root, &cfg, &mut |wt| {
            std::fs::write(wt.join("lane-work.txt"), "x").expect("write");
            commit_all_in(wt, "feat: lane work");
            Ok(())
        })
        .expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert_ne!(git::head(&r.root), pre_head);
        assert!(r.root.join("lane-work.txt").exists());
    }

    #[test]
    fn the_worktree_is_removed_once_it_has_merged() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);

        let report = lane(&r.root, &cfg, &mut |_wt| Ok(())).expect("lane");

        assert!(report.merged);
        assert!(report.left.is_none());
        let listed = git::git(&r.root, &["worktree", "list"]).expect("worktree list");
        assert!(!listed.contains("lane/"));
        let branches = git::git(&r.root, &["branch", "--list", "lane/*"]).expect("branch list");
        assert!(branches.trim().is_empty());
    }

    #[test]
    fn a_stuck_lane_is_left_with_its_work() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);
        let pre_head = git::head(&r.root);

        let report = lane(&r.root, &cfg, &mut |wt| {
            std::fs::write(wt.join("lane-work.txt"), "x").expect("write");
            commit_all_in(wt, "feat: lane work");
            // The parent moves under the lane: a second writer, so the merge can't fast-forward.
            r.write("moved.txt", "y");
            r.commit_all("a second writer moved the parent");
            Ok(())
        })
        .expect("lane");

        assert!(!report.merged, "reason: {}", report.reason);
        let left = report.left.clone().expect("left in place");
        assert!(left.exists());
        assert!(left.join("lane-work.txt").exists());
        assert_ne!(git::head(&r.root), pre_head); // the parent's own commit landed, not the lane's
        assert!(!report.reason.is_empty());

        // cleanup so the fixture's tempdir can be dropped without a lingering worktree lock
        let _ = git::git(
            &r.root,
            &["worktree", "remove", "--force", &left.to_string_lossy()],
        );
        let _ = git::git(&r.root, &["branch", "-D", &report.branch]);
    }

    #[test]
    fn a_run_that_errors_still_merges() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);

        let report = lane(&r.root, &cfg, &mut |_wt| {
            anyhow::bail!("the lane's run failed")
        })
        .expect("lane");

        assert!(report.merged, "reason: {}", report.reason);
        assert_eq!(report.run_error.as_deref(), Some("the lane's run failed"));
    }

    #[test]
    fn two_lanes_in_the_same_second_do_not_collide() {
        let r = repo_with_harness_dir();
        let cfg = cfg(&r);

        let first = lane(&r.root, &cfg, &mut |wt| {
            std::fs::write(wt.join("first.txt"), "x").expect("write");
            commit_all_in(wt, "feat: first lane");
            Ok(())
        })
        .expect("first lane");
        let second = lane(&r.root, &cfg, &mut |wt| {
            std::fs::write(wt.join("second.txt"), "x").expect("write");
            commit_all_in(wt, "feat: second lane");
            Ok(())
        })
        .expect("second lane");

        assert!(first.merged, "reason: {}", first.reason);
        assert!(second.merged, "reason: {}", second.reason);
        assert!(r.root.join("first.txt").exists());
        assert!(r.root.join("second.txt").exists());
    }
}
