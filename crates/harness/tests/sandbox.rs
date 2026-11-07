//! Real commands under the host's real sandbox, through `Sandbox::wrap` (ADR 0004).

use enallagi::config::{Config, SandboxMode};
use enallagi::fixture::Repo;
use enallagi::sandbox::Sandbox;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Output;

// Seatbelt cannot nest, so inside a lane these prove nothing; CI runs them unsandboxed
fn nested() -> bool {
    let set = std::env::var_os("ENALLAGI_SANDBOX").is_some();
    if set {
        println!("nested: skipped");
    }
    set
}

// operator credentials in the launcher's env, which no lane may inherit
const TOKENS: [&str; 4] = ["GH_TOKEN", "GITHUB_TOKEN", "ENALLAGI_GH_TOKEN", "NPM_TOKEN"];

// the operator's secrets, planted in a home of our own: no test reads the real ones
const SECRETS: [&str; 5] = [
    ".ssh/id",
    ".aws/credentials",
    ".gnupg/key",
    ".netrc",
    ".config/gh/hosts.yml",
];

fn native(ports: &[u16]) -> Config {
    static HOME: std::sync::Once = std::sync::Once::new();
    HOME.call_once(|| {
        let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("home");
        for rel in SECRETS {
            let path = home.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, "secret\n").expect("write");
        }
        std::env::set_var("HOME", home);
        for var in TOKENS {
            std::env::set_var(var, "planted-token");
        }
    });
    let mut cfg = Config::default();
    cfg.layout.harness_dir = ".enallagi".into();
    cfg.sandbox.mode = SandboxMode::Native;
    cfg.sandbox.ports = ports.to_vec();
    cfg
}

fn sh(sb: &Sandbox, cwd: &Path, script: &str) -> Output {
    sb.wrap(&["sh", "-c", script], cwd, &[])
        .command()
        .output()
        .expect("spawn")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn git(dir: &Path, args: &[&str]) -> String {
    enallagi::git::git(dir, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

// answers one fixed body to every connection, so a reachable port prints `hi`
fn serve() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nhi");
        }
    });
    port
}

// not under /tmp, which bwrap masks: the path is visible to the lane, so only the rule stops it
fn outside() -> tempfile::TempDir {
    tempfile::TempDir::new_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir")
}

fn fetch(port: u16) -> String {
    format!("curl -s --max-time 5 --noproxy '*' http://127.0.0.1:{port}/")
}

// a product repo with its own state repo under .enallagi, as init leaves it
fn nested_repo() -> Repo {
    let r = Repo::new();
    let state = r.root.join(".enallagi");
    std::fs::create_dir_all(&state).expect("mkdir");
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@t"],
        &["config", "user.name", "t"],
        &["commit", "-q", "--allow-empty", "-m", "init"],
    ] {
        git(&state, args);
    }
    r
}

#[test]
fn direct_egress_refused() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let sb = Sandbox::start(&r.root, &native(&[]));
    let out = sh(
        &sb,
        &r.root,
        "curl -s --max-time 5 --noproxy '*' http://1.1.1.1/",
    );
    assert!(!out.status.success(), "{}", text(&out));
}

#[test]
fn credentials_unset() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let sb = Sandbox::start(&r.root, &native(&[]));
    let out = text(&sh(&sb, &r.root, "env"));
    assert!(!out.contains("planted-token"), "{out}");
    for var in ["HOME=", "PATH="] {
        assert!(out.lines().any(|l| l.starts_with(var)), "{out}");
    }
}

#[test]
fn unlisted_host_refused() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let sb = Sandbox::start(&r.root, &native(&[]));
    let out = sh(
        &sb,
        &r.root,
        "curl -s --max-time 5 https://unlisted.example/",
    );
    assert!(!out.status.success(), "{}", text(&out));
    let denied = sb.denials_since(0);
    assert!(
        denied.iter().any(|d| d.host == "unlisted.example"),
        "{denied:?}"
    );
}

#[test]
fn declared_port_reachable() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let port = serve();
    let sb = Sandbox::start(&r.root, &native(&[port]));
    let out = sh(&sb, &r.root, &fetch(port));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi", "{}", text(&out));
}

#[test]
fn undeclared_port_refused() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let (declared, other) = (serve(), serve());
    let sb = Sandbox::start(&r.root, &native(&[declared]));
    let out = sh(&sb, &r.root, &fetch(other));
    assert!(!out.status.success(), "{}", text(&out));
    assert_ne!(String::from_utf8_lossy(&out.stdout), "hi");
}

#[test]
fn host_unix_socket_refused() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let outside = outside();
    let path = outside.path().join("d.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let _ = s.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nhi");
        }
    });
    let sb = Sandbox::start(&r.root, &native(&[]));
    let script = format!(
        "curl -s --max-time 5 --noproxy '*' --unix-socket '{}' http://x/",
        path.display()
    );
    let out = sh(&sb, &r.root, &script);
    assert_ne!(String::from_utf8_lossy(&out.stdout), "hi", "{}", text(&out));
}

#[test]
fn writes_outside_lane_refused() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let outside = outside();
    let sb = Sandbox::start(&r.root, &native(&[]));
    let script = format!(
        "echo x > '{}/stray'; echo x > in-lane && echo x > \"$TMPDIR/scratch\"",
        outside.path().display()
    );
    let out = sh(&sb, &r.root, &script);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!outside.path().join("stray").exists());
    assert!(r.root.join("in-lane").exists());
}

#[test]
fn lane_commits_both_repos() {
    if nested() {
        return;
    }
    let r = nested_repo();
    let lane = r.root.join(".enallagi/worktrees/lane-a");
    let lane_s = lane.display().to_string();
    git(&r.root, &["worktree", "add", "-q", "-b", "lane/a", &lane_s]);
    let state_s = lane.join(".enallagi").display().to_string();
    git(
        &r.root.join(".enallagi"),
        &["worktree", "add", "-q", "-b", "lane/a", &state_s],
    );
    let sb = Sandbox::start(&r.root, &native(&[]));
    let commit = "git -c commit.gpgsign=false commit -q --allow-empty -m lane";
    let out = sh(
        &sb,
        &lane,
        &format!(
            "{commit} && git -C .enallagi -c commit.gpgsign=false commit -q --allow-empty -m lane"
        ),
    );
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        git(&r.root, &["log", "-1", "--format=%s", "lane/a"]),
        "lane"
    );
    assert_eq!(
        git(
            &r.root.join(".enallagi"),
            &["log", "-1", "--format=%s", "lane/a"]
        ),
        "lane"
    );
}

fn as_role(sb: &Sandbox, cwd: &Path, role: &str, script: &str) -> Output {
    let policy = enallagi::roles::policy(&format!("---\nname: r\n{role}---\nbody\n"));
    sb.wrap_as(&["sh", "-c", script], cwd, &[], &policy)
        .command()
        .output()
        .expect("spawn")
}

#[test]
fn role_writes_none() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let sb = Sandbox::start(&r.root, &native(&[]));
    for script in ["echo x > f", "sed -i.bak s/x/y/ src/schema.ts"] {
        let out = as_role(&sb, &r.root, "writes:\n", script);
        assert!(!out.status.success(), "{script}: {}", text(&out));
    }
    assert_eq!(git(&r.root, &["status", "--porcelain"]), "");
}

#[test]
fn role_writes_named() {
    if nested() {
        return;
    }
    let r = Repo::new();
    r.write("REFERENCES.md", "# refs\n");
    r.commit_all("refs");
    let sb = Sandbox::start(&r.root, &native(&[]));
    let role = "writes: REFERENCES.md\n";
    let out = as_role(&sb, &r.root, role, "echo x >> REFERENCES.md");
    assert!(out.status.success(), "{}", text(&out));
    let out = as_role(&sb, &r.root, role, "echo x >> src/schema.ts");
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(git(&r.root, &["status", "--porcelain"]), " M REFERENCES.md");
}

#[test]
fn role_writes_contained() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let out_dir = outside();
    let abs = std::fs::canonicalize(out_dir.path()).expect("canon");
    let up = "../".repeat(r.root.components().count());
    let rel = format!("{up}{}", abs.strip_prefix("/").expect("absolute").display());
    let sb = Sandbox::start(&r.root, &native(&[]));
    for entry in [abs.display().to_string(), rel] {
        let role = format!("writes: {entry}\n");
        let script = format!("echo x > '{}/f'", abs.display());
        let out = as_role(&sb, &r.root, &role, &script);
        assert!(!out.status.success(), "{entry}: {}", text(&out));
    }
    assert!(!abs.join("f").exists());
}

#[test]
fn role_writes_absent() {
    if nested() {
        return;
    }
    let r = nested_repo();
    let sb = Sandbox::start(&r.root, &native(&[]));
    let commit = "git -c commit.gpgsign=false commit -q --allow-empty -m lane";
    let script = format!(
        "{commit} && git -C .enallagi -c commit.gpgsign=false commit -q --allow-empty -m lane"
    );
    let out = as_role(&sb, &r.root, "hosts: example.com\n", &script);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(git(&r.root, &["log", "-1", "--format=%s"]), "lane");
    assert_eq!(
        git(&r.root.join(".enallagi"), &["log", "-1", "--format=%s"]),
        "lane"
    );
}

// offline: a listed name that does not resolve is refused as unresolved, not as unlisted
#[test]
fn role_hosts_listed() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let sb = Sandbox::start(&r.root, &native(&[]));
    let curl = "curl -s -o /dev/null -x \"$HTTPS_PROXY\" https://example.invalid/";
    as_role(&sb, &r.root, "hosts: example.invalid\n", curl);
    sh(&sb, &r.root, curl);
    let reasons: Vec<String> = sb.denials_since(0).into_iter().map(|d| d.reason).collect();
    assert_eq!(reasons.len(), 2, "{reasons:?}");
    assert!(reasons[0].starts_with("unresolved"), "{reasons:?}");
    assert_eq!(reasons[1], "not on the allowlist");
}

#[test]
fn git_config_unwritable() {
    if nested() {
        return;
    }
    let r = nested_repo();
    let lane = r.root.join(".enallagi/worktrees/lane-a");
    git(
        &r.root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "lane/a",
            &lane.display().to_string(),
        ],
    );
    let sb = Sandbox::start(&r.root, &native(&[]));
    let attempts = [
        "git config core.hooksPath /tmp",
        "git -C .enallagi config core.hooksPath /tmp",
        "echo x > .git/hooks/pre-commit",
        "echo x > .enallagi/.git/hooks/pre-commit",
        "mkdir -p .git/modules/x && echo x > .git/modules/x/config",
        "mv .git .g",
        "mv .git/hooks .git/h",
        "mv .git/worktrees .git/w",
        "echo x > .git/worktrees/lane-a/gitdir",
    ];
    for attempt in attempts {
        let out = sh(&sb, &r.root, attempt);
        assert!(!out.status.success(), "{attempt}: {}", text(&out));
    }
    let out = sh(&sb, &lane, "echo 'gitdir: /tmp' > .git");
    assert!(!out.status.success(), "{}", text(&out));
    assert!(!r.root.join(".git/hooks/pre-commit").exists());
    assert!(r.root.join(".git/config").exists());
    assert!(!git(&r.root, &["config", "--list"]).contains("hookspath"));
}

// a product with its parent instance's holdouts, and a worktree lane with its own nested instance
fn holdouts_repo() -> (Repo, std::path::PathBuf) {
    let r = nested_repo();
    r.write(".enallagi/holdouts/s/t", "scenario\n");
    let lane = r.root.join(".enallagi/worktrees/lane-a");
    git(
        &r.root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "lane/a",
            &lane.display().to_string(),
        ],
    );
    let state = lane.join(".enallagi");
    git(
        &r.root.join(".enallagi"),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "lane/a",
            &state.display().to_string(),
        ],
    );
    std::fs::create_dir_all(state.join("holdouts/s")).expect("mkdir");
    std::fs::write(state.join("holdouts/s/t"), "scenario\n").expect("write");
    (r, lane)
}

#[test]
fn holdouts_unreadable() {
    if nested() {
        return;
    }
    let (r, lane) = holdouts_repo();
    let sb = Sandbox::start(&r.root, &native(&[]));
    for (cwd, path) in [
        (&r.root, ".enallagi/holdouts/s/t"),
        (&lane, ".enallagi/holdouts/s/t"),
        (&lane, "../../holdouts/s/t"),
    ] {
        let out = sh(
            &sb,
            cwd,
            &format!(
                "cat {path}; ls {}",
                Path::new(path).parent().unwrap().display()
            ),
        );
        assert!(!text(&out).contains("scenario"), "{path}: {}", text(&out));
        assert!(
            !String::from_utf8_lossy(&out.stdout).contains('t'),
            "{path}: {}",
            text(&out)
        );
    }
}

#[test]
fn holdouts_unwritable() {
    if nested() {
        return;
    }
    let (r, _) = holdouts_repo();
    let sb = Sandbox::start(&r.root, &native(&[]));
    for attempt in [
        "echo x > .enallagi/holdouts/s/t",
        "echo x > .enallagi/holdouts/s/new",
        "rm .enallagi/holdouts/s/t",
        "mv .enallagi/holdouts .enallagi/h",
    ] {
        let out = sh(&sb, &r.root, attempt);
        assert!(!out.status.success(), "{attempt}: {}", text(&out));
    }
    let held = std::fs::read_to_string(r.root.join(".enallagi/holdouts/s/t")).expect("read");
    assert_eq!(held, "scenario\n");
    assert!(!r.root.join(".enallagi/holdouts/s/new").exists());
}

// no state repo here, so only the holdouts rule pins `.enallagi` against a rename
#[test]
fn holdouts_survive_parent_rename() {
    if nested() {
        return;
    }
    let r = Repo::new();
    r.write(".enallagi/holdouts/s/t", "scenario\n");
    let sb = Sandbox::start(&r.root, &native(&[]));
    let out = sh(
        &sb,
        &r.root,
        "mv .enallagi .x; cat .x/holdouts/s/t .enallagi/holdouts/s/t",
    );
    assert!(!text(&out).contains("scenario"), "{}", text(&out));
    // bwrap's mask follows a renamed parent, so the proof is that nothing moved
    assert!(
        r.root.join(".enallagi/holdouts/s/t").exists(),
        "{}",
        text(&out)
    );
}

#[test]
fn secrets_unreadable() {
    if nested() {
        return;
    }
    let r = Repo::new();
    let sb = Sandbox::start(&r.root, &native(&[]));
    for rel in SECRETS {
        let out = sh(&sb, &r.root, &format!("cat \"$HOME/{rel}\""));
        assert!(!out.status.success(), "{rel}: {}", text(&out));
        assert!(!text(&out).contains("secret"), "{rel}: {}", text(&out));
    }
}

// the unfiltered bridge is the lane's parent at its uid: readable, it could be driven to a host socket
#[cfg(target_os = "linux")]
#[test]
fn bridge_not_dumpable() {
    // root reads a root-owned /proc file anyway, so only a user run proves the rule
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .expect("id");
    if nested() || String::from_utf8_lossy(&uid.stdout).trim() == "0" {
        return;
    }
    let r = Repo::new();
    let sb = Sandbox::start(&r.root, &native(&[]));
    let own = sh(&sb, &r.root, "cat /proc/self/environ > /dev/null");
    assert!(own.status.success(), "{}", text(&own));
    let out = sh(&sb, &r.root, "cat /proc/$PPID/environ > /dev/null");
    assert!(!out.status.success(), "{}", text(&out));
}

#[cfg(target_os = "linux")]
#[test]
fn native_without_bwrap_refuses() {
    if nested() {
        return;
    }
    let r = Repo::new();
    r.write(".enallagi/enallagi.toml", "[sandbox]\nmode = \"native\"\n");
    // PATH holds git and a bwrap planted inside the product, which must not count
    let tools = r.root.join("bin");
    std::fs::create_dir_all(&tools).expect("mkdir");
    let git_bin = std::env::split_paths(&std::env::var_os("PATH").expect("PATH"))
        .map(|d| d.join("git"))
        .find(|p| p.is_file())
        .expect("git on PATH");
    std::os::unix::fs::symlink(git_bin, tools.join("git")).expect("symlink");
    std::fs::write(tools.join("bwrap"), "#!/bin/sh\nexec \"$@\"\n").expect("write");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(tools.join("bwrap"), std::fs::Permissions::from_mode(0o755))
        .expect("chmod");
    let out = enallagi::fixture::command(env!("CARGO_BIN_EXE_enallagi"))
        .args(["run", "--iterations", "1", "--no-tui"])
        .env("PATH", &tools)
        .current_dir(&r.root)
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("apt install bubblewrap"), "{err}");
    assert!(err.contains("mode = \"off\""), "{err}");
}
