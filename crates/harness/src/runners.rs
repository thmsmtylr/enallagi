//! Test-runner detection: one preset per runner, matched against the files the repository already carries.

use std::path::Path;

const PRESET_FILES: &[(&str, &str)] = &[
    ("bun", include_str!("../runners/bun.toml")),
    ("cargo", include_str!("../runners/cargo.toml")),
    ("go", include_str!("../runners/go.toml")),
    ("jest", include_str!("../runners/jest.toml")),
    ("node", include_str!("../runners/node.toml")),
    ("pytest", include_str!("../runners/pytest.toml")),
    ("vitest", include_str!("../runners/vitest.toml")),
];

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runner {
    pub name: String,
    /// Read in order; the first one that exists and matches `detect_re` names the runner.
    pub detect_files: Vec<String>,
    /// Empty means the file need only exist.
    pub detect_re: String,
    pub test_command: String,
    /// What a CI `run:` step looks like when it runs the tests.
    pub run_re: String,
    pub fail_name: String,
    pub test_file_suffix_re: String,
    pub test_decl_patterns: Vec<String>,
    pub source_ext: Vec<String>,
    pub source_roots: Vec<String>,
}

pub fn presets() -> Vec<Runner> {
    PRESET_FILES
        .iter()
        .map(|(name, text)| toml::from_str(text).expect(name))
        .collect()
}

/// One key init writes, with the `file:line` that decided it.
#[derive(Debug, Clone)]
pub struct Detected {
    pub key: String,
    pub value: toml::Value,
    pub origin: String,
}

/// A runner whose detection matched, and the `file:line` that matched it.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub name: String,
    pub origin: String,
}

#[derive(Debug, Default)]
pub struct Detection {
    pub candidates: Vec<Candidate>,
    /// Empty unless exactly one runner matched: two candidates are a report, never a guess.
    pub keys: Vec<Detected>,
}

/// The runners the tree matches, and — when exactly one does — the keys init writes for it.
pub fn detect(root: &Path) -> Detection {
    let mut found = Detection::default();
    let mut matched: Vec<(Runner, String)> = Vec::new();
    for runner in presets() {
        if let Some(origin) = detect_at(root, &runner) {
            found.candidates.push(Candidate {
                name: runner.name.clone(),
                origin: origin.clone(),
            });
            matched.push((runner, origin));
        }
    }
    if let [(runner, origin)] = matched.as_slice() {
        found.keys = keys(root, runner, origin);
    } else if matched.is_empty() {
        found.keys = layout_from_tree(root);
    }
    found
}

// no runner matched, so the tree alone answers the two layout keys a lane cannot run without: the
// conventional source directory, and the test files a git pathspec over the conventional test
// directory reaches; the check itself is the operator's to name
fn layout_from_tree(root: &Path) -> Vec<Detected> {
    let paths = tracked(root);
    let first_under = |dir: &str| {
        paths
            .iter()
            .find(|rel| rel.starts_with(&format!("{dir}/")))
            .cloned()
    };
    let mut out = Vec::new();
    if let Some((dir, file)) = ["src", "lib", "app"]
        .iter()
        .find_map(|dir| first_under(dir).map(|file| (dir.to_string(), file)))
    {
        out.push(at("layout.source_root", dir, &format!("{file}:1")));
    }
    let mut globs: Vec<String> = Vec::new();
    let mut origin: Option<String> = None;
    for dir in ["tests", "test", "spec"] {
        if let Some(file) = first_under(dir) {
            globs.push(format!("{dir}/*"));
            origin.get_or_insert(file);
        }
    }
    if let Some(origin) = origin {
        out.push(at("layout.test_glob", globs, &format!("{origin}:1")));
    }
    out
}

fn detect_at(root: &Path, runner: &Runner) -> Option<String> {
    let re = match runner.detect_re.is_empty() {
        true => None,
        false => Some(regex::Regex::new(&runner.detect_re).ok()?),
    };
    for name in &runner.detect_files {
        if !root.join(name).is_file() {
            continue;
        }
        let Some(re) = &re else {
            return Some(format!("{name}:1"));
        };
        let text = std::fs::read_to_string(root.join(name)).unwrap_or_default();
        if let Some(n) = text.lines().position(|line| re.is_match(line)) {
            return Some(format!("{name}:{}", n + 1));
        }
    }
    None
}

fn at(key: &str, value: impl Into<toml::Value>, origin: &str) -> Detected {
    Detected {
        key: key.to_string(),
        value: value.into(),
        origin: origin.to_string(),
    }
}

fn keys(root: &Path, runner: &Runner, origin: &str) -> Vec<Detected> {
    let (command, from) = check_command(root, runner)
        .unwrap_or_else(|| (runner.test_command.clone(), origin.to_string()));
    let mut out = vec![
        at("check.command", command, &from),
        at("check.fail_name", runner.fail_name.clone(), origin),
        at(
            "layout.test_file_suffix_re",
            runner.test_file_suffix_re.clone(),
            origin,
        ),
        at(
            "layout.test_decl_patterns",
            runner.test_decl_patterns.clone(),
            origin,
        ),
    ];
    let dirs = source_dirs(root, runner);
    let paths = tracked(root);
    if let Some((dir, file)) = source_root(runner, &dirs) {
        out.push(at("layout.source_root", dir, &format!("{file}:1")));
    }
    let from = dirs
        .first()
        .map(|(_, file)| file.clone())
        .or_else(|| paths.first().cloned());
    if let Some(file) = from {
        let prefixes: Vec<String> = allowed_prefixes(&dirs, &paths);
        out.push(at(
            "layout.allowed_prefixes",
            prefixes,
            &format!("{file}:1"),
        ));
    }
    if let Some((globs, file)) = test_globs(runner, &paths) {
        out.push(at("layout.test_glob", globs, &format!("{file}:1")));
    }
    out
}

/// Every path git tracks, sorted; empty where the tree is no repository or holds no commit yet.
fn tracked(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = crate::git::git(root, &["ls-files"])
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect();
    out.sort();
    out
}

/// One pathspec per top-level directory holding a tracked test file, with the first such file.
fn test_globs(runner: &Runner, paths: &[String]) -> Option<(Vec<String>, String)> {
    let is_test = regex::Regex::new(&runner.test_file_suffix_re).ok()?;
    let mut out: Vec<String> = Vec::new();
    let mut origin: Option<String> = None;
    for rel in paths.iter().filter(|rel| is_test.is_match(rel)) {
        let Some((_, ext)) = rel.rsplit_once('.') else {
            continue;
        };
        // a git pathspec `*` crosses `/`, so one glob per top-level directory reads the whole tree under it
        let spec = match rel.split_once('/') {
            Some((dir, _)) => format!("{dir}/*.{ext}"),
            None => format!("*.{ext}"),
        };
        if !out.contains(&spec) {
            out.push(spec);
        }
        origin.get_or_insert_with(|| rel.clone());
    }
    out.sort();
    Some((out, origin?))
}

// every dot-directory the defaults allow stays allowed: the harness's own files live under one
fn allowed_prefixes(dirs: &[(String, String)], paths: &[String]) -> Vec<String> {
    let mut out: Vec<String> = dirs.iter().map(|(dir, _)| format!("{dir}/")).collect();
    // a tracked directory holding no source file, and a tracked root file, are product too
    for rel in paths {
        let entry = match rel.split_once('/') {
            Some((dir, _)) => format!("{dir}/"),
            None => rel.clone(),
        };
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
    let defaults: Vec<String> = toml::from_str::<toml::Value>(crate::config::DEFAULT_TOML)
        .ok()
        .and_then(|v| {
            v.get("layout")?
                .get("allowed_prefixes")?
                .as_array()
                .cloned()
        })
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .filter(|p| p.starts_with('.'))
        .collect();
    for prefix in defaults {
        if !out.contains(&prefix) {
            out.push(prefix);
        }
    }
    out.sort();
    out
}

fn source_root(runner: &Runner, dirs: &[(String, String)]) -> Option<(String, String)> {
    runner
        .source_roots
        .iter()
        .find_map(|want| dirs.iter().find(|(dir, _)| dir == want))
        .cloned()
}

/// Every top-level directory holding a source or test file, each with the first such file under it.
fn source_dirs(root: &Path, runner: &Runner) -> Vec<(String, String)> {
    let is_test = regex::Regex::new(&runner.test_file_suffix_re).ok();
    let mut out: Vec<(String, String)> = Vec::new();
    for rel in tree(root) {
        let counts = runner.source_ext.iter().any(|ext| rel.ends_with(ext))
            || is_test.as_ref().is_some_and(|re| re.is_match(&rel));
        let Some((dir, _)) = rel.split_once('/') else {
            continue;
        };
        if !counts || out.iter().any(|(seen, _)| seen == dir) {
            continue;
        }
        out.push((dir.to_string(), rel.clone()));
    }
    out.sort();
    out
}

const SKIP: &[&str] = &[
    ".git",
    ".venv",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "out",
    "target",
    "vendor",
];

fn tree(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if SKIP.contains(&name.as_str()) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn check_command(root: &Path, runner: &Runner) -> Option<(String, String)> {
    let runs = regex::Regex::new(&runner.run_re).ok()?;
    let installs =
        regex::Regex::new(r"^(npm|pnpm|yarn|bun) (ci|install|add|i)\b|^pip3? install|^python3? -m pip install|^go mod (download|tidy)|^cargo fetch|^uv (sync|pip install)").ok()?;
    let mut workflows: Vec<String> = tree(root)
        .into_iter()
        .filter(|p| {
            p.starts_with(".github/workflows/") && (p.ends_with(".yml") || p.ends_with(".yaml"))
        })
        .collect();
    workflows.sort();
    for workflow in workflows {
        let text = std::fs::read_to_string(root.join(&workflow)).unwrap_or_default();
        // one job's steps are the check; a second job in the same file builds or deploys
        for (offset, job) in jobs(&text) {
            let steps = run_steps(&job);
            if !steps.iter().any(|(_, cmd)| runs.is_match(cmd)) {
                continue;
            }
            let kept: Vec<&(usize, String)> = steps
                .iter()
                .filter(|(_, cmd)| !installs.is_match(cmd))
                .collect();
            let Some((line, _)) = kept.first() else {
                continue;
            };
            let command = kept
                .iter()
                .map(|(_, cmd)| cmd.as_str())
                .collect::<Vec<&str>>()
                .join(" && ");
            return Some((command, format!("{workflow}:{}", line + offset)));
        }
    }
    None
}

/// Each job's own lines, with the count of lines before it. A file with no `jobs:` is one job.
fn jobs(text: &str) -> Vec<(usize, String)> {
    let head = regex::Regex::new(r"^  [A-Za-z_][A-Za-z0-9_-]*:\s*$").expect("literal pattern");
    let lines: Vec<&str> = text.lines().collect();
    let Some(at) = lines.iter().position(|l| l.trim_end() == "jobs:") else {
        return vec![(0, text.to_string())];
    };
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, line) in lines.iter().enumerate().skip(at + 1) {
        // a line at column 0 ends the jobs: mapping
        if !line.trim().is_empty() && !line.starts_with(' ') {
            break;
        }
        if head.is_match(line) {
            if let Some(from) = start {
                out.push((from, lines[from..i].join("\n")));
            }
            start = Some(i);
        }
    }
    if let Some(from) = start {
        out.push((from, lines[from..].join("\n")));
    }
    out
}

/// Every `run:` step in a workflow, with the 1-based line it starts on; a block scalar is one step per line.
fn run_steps(text: &str) -> Vec<(usize, String)> {
    let open = regex::Regex::new(r"^(\s*)(?:-\s+)?run:\s*(.*)$").expect("literal pattern");
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(caps) = open.captures(lines[i]) else {
            i += 1;
            continue;
        };
        let indent = caps[1].len();
        let rest = caps[2].trim();
        i += 1;
        if rest != "|" && rest != ">" && rest != "|-" && rest != ">-" {
            if !rest.is_empty() {
                out.push((i, rest.to_string()));
            }
            continue;
        }
        while i < lines.len() {
            let line = lines[i];
            let width = line.len() - line.trim_start().len();
            if !line.trim().is_empty() && width <= indent {
                break;
            }
            if !line.trim().is_empty() {
                out.push((i + 1, line.trim().to_string()));
            }
            i += 1;
        }
    }
    out
}

/// Writes each detected key into `text`; a text that does not parse is returned for init to report.
pub fn apply(text: &str, keys: &[Detected]) -> String {
    let Ok(mut doc) = toml::from_str::<toml::Value>(text) else {
        return text.to_string();
    };
    for key in keys {
        let (Some((table, leaf)), Some(root)) = (key.key.split_once('.'), doc.as_table_mut())
        else {
            continue;
        };
        let slot = root
            .entry(table.to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(table) = slot.as_table_mut() {
            table.insert(leaf.to_string(), key.value.clone());
        }
    }
    toml::to_string(&doc).unwrap_or_else(|_| text.to_string())
}

/// One line per detected value, or per candidate when the tree does not name exactly one runner.
pub fn notes(found: &Detection) -> Vec<String> {
    let detected = found
        .keys
        .iter()
        .map(|k| format!("detected: {} = {} ({})", k.key, k.value, k.origin));
    let refusal = match found.candidates.len() {
        1 => None,
        0 => Some("no test runner detected; check.command keeps its default and the layout keys come from the tree"),
        _ => Some("more than one test runner matches; check.command and the layout keys keep their defaults"),
    };
    refusal
        .map(str::to_string)
        .into_iter()
        .chain(
            found
                .candidates
                .iter()
                .filter(|_| found.candidates.len() != 1)
                .map(|c| format!("candidate: {} ({})", c.name, c.origin)),
        )
        .chain(detected)
        .collect()
}
