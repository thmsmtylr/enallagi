//! Linux: bwrap with a private network namespace whose loopback ports are bridged out over Unix
//! sockets to the launcher, and a seccomp filter that keeps the lane off every other socket.

use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::Policy;
use crate::proxy::pump;

/// Host side: one Unix socket per port under the run's dir, each relayed to `127.0.0.1:<port>`.
pub(super) struct Relays {
    socks: Vec<PathBuf>,
    stop: Arc<AtomicBool>,
}

impl Relays {
    pub(super) fn start(dir: &Path, ports: &[u16]) -> Relays {
        let stop = Arc::new(AtomicBool::new(false));
        let mut socks = Vec::new();
        for &port in ports {
            let sock = dir.join(port.to_string());
            let Ok(listener) = UnixListener::bind(&sock) else {
                continue;
            };
            socks.push(sock);
            let halt = Arc::clone(&stop);
            std::thread::spawn(move || {
                for conn in listener.incoming() {
                    if halt.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(conn) = conn else { continue };
                    std::thread::spawn(move || {
                        if let Ok(host) = TcpStream::connect(("127.0.0.1", port)) {
                            pump(conn, host);
                        }
                    });
                }
            });
        }
        Relays { socks, stop }
    }
}

impl Drop for Relays {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // wakes each blocked accept so its thread sees the flag
        for sock in &self.socks {
            let _ = UnixStream::connect(sock);
        }
    }
}

/// A `bwrap` on PATH that is not inside the product, where a lane could have planted one.
pub(super) fn find(root: &Path) -> Option<PathBuf> {
    let root = super::canon(root);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join("bwrap"))
        .find(|p| p.is_file() && !super::canon(p).starts_with(&root))
}

/// The binary that runs `sandbox bridge`: this one when it is enallagi, else `ENALLAGI_BIN`.
pub(super) fn exe() -> PathBuf {
    let own = std::env::current_exe().ok();
    match own.filter(|p| p.file_name().is_some_and(|n| n == "enallagi")) {
        Some(own) => own,
        None => std::env::var_os("ENALLAGI_BIN").map_or_else(|| "enallagi".into(), PathBuf::from),
    }
}

pub(super) fn refusal(root: &Path, off: &str) -> Option<String> {
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
        return Some(format!(
            "no seccomp filter for {}; {off}",
            std::env::consts::ARCH
        ));
    }
    let Some(bwrap) = find(root) else {
        return Some(format!(
            "bwrap is not on PATH outside the product, so no lane can be confined; install \
             bubblewrap (`apt install bubblewrap`), {off}"
        ));
    };
    let probe = std::process::Command::new(&bwrap)
        .args([
            "--ro-bind",
            "/",
            "/",
            "--unshare-user",
            "--unshare-net",
            "true",
        ])
        .output();
    match probe {
        Ok(out) if out.status.success() => None,
        Ok(out) => Some(format!(
            "{} cannot create a user namespace ({}); on Ubuntu 24.04 run `sudo sysctl -w \
             kernel.apparmor_restrict_unprivileged_userns=0`, {off}",
            bwrap.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(err) => Some(format!("{} did not run ({err}); {off}", bwrap.display())),
    }
}

pub(super) fn argv(
    bwrap: &Path,
    policy: &Policy,
    socks: &Path,
    ports: &[u16],
    cwd: &Path,
    argv: Vec<String>,
) -> Vec<String> {
    let mut out: Vec<String> = [
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--unshare-user",
        "--unshare-net",
        "--unshare-pid",
        "--unshare-ipc",
        "--die-with-parent",
        "--new-session",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    out.insert(0, bwrap.display().to_string());
    // parents before children, so a pinned ancestor stays a mountpoint a rename cannot move
    let mut rw: Vec<&PathBuf> = policy.write.iter().chain(&policy.pinned).collect();
    rw.sort();
    rw.dedup();
    let binds = rw
        .into_iter()
        .map(|p| ("--bind", p))
        .chain(policy.deny.iter().map(|p| ("--ro-bind", p)));
    for (flag, path) in binds.filter(|(_, p)| p.exists()) {
        let path = path.display().to_string();
        out.extend([flag.to_string(), path.clone(), path]);
    }
    out.extend([
        "--chdir".to_string(),
        super::canon(cwd).display().to_string(),
    ]);
    out.extend([
        exe().display().to_string(),
        "sandbox".to_string(),
        "bridge".to_string(),
        "--dir".to_string(),
        socks.display().to_string(),
    ]);
    for port in ports {
        out.extend(["--port".to_string(), port.to_string()]);
    }
    out.push("--".to_string());
    out.extend(argv);
    out
}

/// Inside the namespace: listen on each loopback port, forward to its socket, run argv filtered.
pub fn bridge(dir: &Path, ports: &[u16], argv: &[String]) -> i32 {
    for &port in ports {
        let listener = match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => l,
            Err(err) => {
                eprintln!("enallagi sandbox bridge: 127.0.0.1:{port}: {err}");
                return 125;
            }
        };
        let sock = dir.join(port.to_string());
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let sock = sock.clone();
                std::thread::spawn(move || {
                    if let Ok(out) = UnixStream::connect(&sock) {
                        pump(conn, out);
                    }
                });
            }
        });
    }
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    if let Err(err) = filter(&mut cmd) {
        eprintln!("enallagi sandbox bridge: {err}");
        return 125;
    }
    match cmd.status() {
        Ok(status) => {
            use std::os::unix::process::ExitStatusExt;
            status
                .code()
                .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
        }
        Err(err) => {
            eprintln!("enallagi sandbox bridge: {}: {err}", argv[0]);
            127
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn filter(_: &mut std::process::Command) -> Result<(), String> {
    Err("the seccomp filter is Linux only".into())
}

// fails socket(AF_UNIX, ..) and io_uring_setup (whose ops can open sockets) with EPERM; a foreign
// arch or an x32 syscall number is refused outright
#[cfg(target_os = "linux")]
fn filter(cmd: &mut std::process::Command) -> Result<(), String> {
    use libc::{
        sock_filter, BPF_ABS, BPF_JEQ, BPF_JGE, BPF_JMP, BPF_K, BPF_LD, BPF_RET, BPF_W, EPERM,
        SECCOMP_RET_ALLOW, SECCOMP_RET_ERRNO,
    };
    use std::os::unix::process::CommandExt;
    #[cfg(target_arch = "x86_64")]
    const ARCH: u32 = 0xC000_003E;
    #[cfg(target_arch = "aarch64")]
    const ARCH: u32 = 0xC000_00B7;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    return Err(format!("no seccomp filter for {}", std::env::consts::ARCH));
    let deny = SECCOMP_RET_ERRNO | EPERM as u32;
    let (ld, jeq, jge, ret) = (
        (BPF_LD | BPF_W | BPF_ABS) as u16,
        (BPF_JMP | BPF_JEQ | BPF_K) as u16,
        (BPF_JMP | BPF_JGE | BPF_K) as u16,
        (BPF_RET | BPF_K) as u16,
    );
    // offsets into seccomp_data: nr 0, arch 4, args[0] 16; a jump counts from the next instruction
    let prog: Vec<sock_filter> = unsafe {
        vec![
            libc::BPF_STMT(ld, 4),
            libc::BPF_JUMP(jeq, ARCH, 1, 0),
            libc::BPF_STMT(ret, deny),
            libc::BPF_STMT(ld, 0),
            libc::BPF_JUMP(jge, 0x4000_0000, 5, 0),
            libc::BPF_JUMP(jeq, libc::SYS_io_uring_setup as u32, 4, 0),
            libc::BPF_JUMP(jeq, libc::SYS_socket as u32, 0, 2),
            libc::BPF_STMT(ld, 16),
            libc::BPF_JUMP(jeq, libc::AF_UNIX as u32, 1, 0),
            libc::BPF_STMT(ret, SECCOMP_RET_ALLOW),
            libc::BPF_STMT(ret, deny),
        ]
    };
    // only prctl runs between fork and exec, which is async-signal-safe
    unsafe {
        cmd.pre_exec(move || {
            let fprog = libc::sock_fprog {
                len: prog.len() as u16,
                filter: prog.as_ptr() as *mut sock_filter,
            };
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::prctl(
                    libc::PR_SET_SECCOMP,
                    libc::SECCOMP_MODE_FILTER,
                    &fprog as *const libc::sock_fprog,
                ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}
