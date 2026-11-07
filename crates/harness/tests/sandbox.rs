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

fn native(ports: &[u16]) -> Config {
    let mut cfg = Config::default();
    cfg.layout.harness_dir = ".enallagi".into();
    cfg.sandbox.mode = SandboxMode::Native;
    cfg.sandbox.ports = ports.to_vec();
    cfg
}

fn sh(sb: &Sandbox, cwd: &Path, script: &str) -> Output {
    let mut c = sb.wrap(&["sh", "-c", script], cwd, &[]).command();
    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_enallagi") {
        c.env("ENALLAGI_BIN", bin);
    }
    c.output().expect("spawn")
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
    let outside = tempfile::TempDir::new().expect("tempdir");
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
    let outside = tempfile::TempDir::new().expect("tempdir");
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
        "mv .git .g",
        "mv .git/hooks .git/h",
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
