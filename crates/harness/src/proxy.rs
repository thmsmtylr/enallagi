//! The lanes' one egress: an HTTP CONNECT proxy on loopback holding a domain allowlist (ADR 0004).

use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Denial {
    pub host: String,
    pub port: u16,
    pub reason: String,
}

// resolves and vets; a test swaps it to reach a loopback echo server
type Resolve = fn(&str, u16) -> Result<Vec<SocketAddr>, String>;

pub struct Proxy {
    pub port: u16,
    denials: Arc<Mutex<Vec<Denial>>>,
    stop: Arc<AtomicBool>,
}

impl Proxy {
    pub fn start(allow: Vec<String>) -> io::Result<Proxy> {
        Proxy::with(allow, resolve)
    }

    fn with(allow: Vec<String>, resolve: Resolve) -> io::Result<Proxy> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let denials = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (allow, kept, halt) = (Arc::new(allow), Arc::clone(&denials), Arc::clone(&stop));
        std::thread::spawn(move || {
            for client in listener.incoming() {
                if halt.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(client) = client else { continue };
                let (allow, kept) = (Arc::clone(&allow), Arc::clone(&kept));
                std::thread::spawn(move || serve(client, &allow, resolve, &kept));
            }
        });
        Ok(Proxy {
            port,
            denials,
            stop,
        })
    }

    pub fn denials(&self) -> Vec<Denial> {
        match self.denials.lock() {
            Ok(d) => d.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // wakes the blocked accept so the listener thread sees the flag
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn serve(mut client: TcpStream, allow: &[String], resolve: Resolve, kept: &Mutex<Vec<Denial>>) {
    let _ = client.set_read_timeout(Some(Duration::from_secs(30)));
    let Some(head) = read_head(&mut client) else {
        return;
    };
    let mut words = head.split_whitespace();
    let (method, target) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    let (host, port) = match method {
        "CONNECT" => split_target(target),
        _ => (target.to_string(), 0),
    };
    let verdict = if method != "CONNECT" {
        Err(format!("method {method}"))
    } else {
        judge(allow, &host, port).and_then(|()| resolve(&host, port))
    };
    let upstream = match verdict {
        Ok(addrs) => addrs,
        Err(reason) => {
            if let Ok(mut d) = kept.lock() {
                d.push(Denial { host, port, reason });
            }
            let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    let Ok(server) = TcpStream::connect(&upstream[..]) else {
        let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
        return;
    };
    if client
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .is_err()
    {
        return;
    }
    let _ = client.set_read_timeout(None);
    pump(client, server);
}

// one byte at a time so nothing past the blank line is swallowed before the tunnel opens
fn read_head(client: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 8192 || client.read(&mut byte).ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    Some(String::from_utf8_lossy(&head).into_owned())
}

/// A stream whose two halves can be pumped from two threads.
pub trait Duplex: Read + Write + Send + Sized + 'static {
    fn dup(&self) -> io::Result<Self>;
    fn close_write(&self);
}

impl Duplex for TcpStream {
    fn dup(&self) -> io::Result<Self> {
        self.try_clone()
    }
    fn close_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }
}

impl Duplex for std::os::unix::net::UnixStream {
    fn dup(&self) -> io::Result<Self> {
        self.try_clone()
    }
    fn close_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }
}

/// Copies both ways until each side closes.
pub fn pump<A: Duplex, B: Duplex>(client: A, server: B) {
    let (Ok(mut c2), Ok(mut s2)) = (client.dup(), server.dup()) else {
        return;
    };
    let (mut client, mut server) = (client, server);
    let up = std::thread::spawn(move || {
        let _ = io::copy(&mut c2, &mut s2);
        s2.close_write();
    });
    let _ = io::copy(&mut server, &mut client);
    client.close_write();
    let _ = up.join();
}

fn split_target(target: &str) -> (String, u16) {
    match target.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().unwrap_or(0)),
        None => (target.to_string(), 0),
    }
}

fn norm(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

fn judge(allow: &[String], host: &str, port: u16) -> Result<(), String> {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.parse::<IpAddr>().is_ok() {
        return Err("ip literal".into());
    }
    if !allow.iter().any(|entry| listed(entry, host, port)) {
        return Err("not on the allowlist".into());
    }
    Ok(())
}

/// An entry is `name` or `*.suffix`, with an optional `:port` defaulting to 443; `*.suffix` matches
/// subdomains, never the apex.
pub fn listed(entry: &str, host: &str, port: u16) -> bool {
    let (name, want) = match entry.rsplit_once(':') {
        Some((name, p)) => match p.parse::<u16>() {
            Ok(p) => (name, p),
            Err(_) => return false,
        },
        None => (entry, 443),
    };
    let (name, host) = (norm(name), norm(host));
    if want != port {
        return false;
    }
    match name.strip_prefix("*.") {
        Some(suffix) => host
            .strip_suffix(suffix)
            .is_some_and(|sub| sub.ends_with('.') && sub.len() > 1),
        None => host == name,
    }
}

// a listed name may still resolve to the host's own services or a cloud metadata endpoint
fn resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("unresolved: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err("unresolved".into());
    }
    match addrs.iter().find(|a| !public(a.ip())) {
        Some(bad) => Err(format!("resolves to {}", bad.ip())),
        None => Ok(addrs),
    }
}

fn public(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    };
    match ip {
        IpAddr::V4(v4) => !(v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()),
        IpAddr::V6(v6) => {
            !(v6.is_loopback() || v6.is_unspecified() || v6.segments()[0] & 0xffc0 == 0xfe80)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a lane's Seatbelt profile reaches no loopback port but its own proxy's
    fn nested() -> bool {
        let set = std::env::var_os("ENALLAGI_SANDBOX").is_some();
        if set {
            println!("nested: skipped");
        }
        set
    }

    fn ask(proxy: &Proxy, request: &str) -> (String, TcpStream) {
        let mut s = TcpStream::connect(("127.0.0.1", proxy.port)).expect("connect");
        s.write_all(request.as_bytes()).expect("send");
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") && s.read(&mut byte).unwrap_or(0) == 1 {
            head.push(byte[0]);
        }
        (String::from_utf8_lossy(&head).into_owned(), s)
    }

    #[test]
    fn suffix_entry_matches() {
        assert!(listed("*.example.com", "a.example.com", 443));
        assert!(listed("*.example.com", "a.b.Example.COM.", 443));
        assert!(!listed("*.example.com", "example.com", 443));
        assert!(!listed("*.example.com", "badexample.com", 443));
        assert!(listed("Example.com.", "example.com", 443));
        assert!(!listed("example.com", "a.example.com", 443));
        assert!(!listed("example.com", "example.com", 80));
        assert!(listed("example.com:8443", "example.com", 8443));
        assert!(!listed("example.com:8443", "example.com", 443));
    }

    #[test]
    fn refuses_loopback_target() {
        for ip in [
            "127.0.0.1",
            "169.254.169.254",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public(ip.parse().unwrap()), "{ip}");
        }
        assert!(public("140.82.112.3".parse().unwrap()));
        if nested() {
            return;
        }
        let proxy = Proxy::start(vec!["localhost".into()]).expect("proxy");
        let (head, _) = ask(
            &proxy,
            "CONNECT localhost:443 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(head.starts_with("HTTP/1.1 403"), "{head}");
        let d = proxy.denials();
        assert_eq!((d[0].host.as_str(), d[0].port), ("localhost", 443), "{d:?}");
        assert!(d[0].reason.starts_with("resolves to"), "{d:?}");
    }

    #[test]
    fn refuses_unlisted_host() {
        if nested() {
            return;
        }
        let proxy = Proxy::with(vec!["github.com".into()], |_, _| {
            panic!("an unlisted or literal target is never resolved")
        })
        .expect("proxy");
        for (request, host, port) in [
            (
                "CONNECT example.com:443 HTTP/1.1\r\n\r\n",
                "example.com",
                443,
            ),
            (
                "CONNECT 140.82.112.3:443 HTTP/1.1\r\n\r\n",
                "140.82.112.3",
                443,
            ),
            ("CONNECT [::1]:443 HTTP/1.1\r\n\r\n", "[::1]", 443),
            (
                "GET http://github.com/ HTTP/1.1\r\n\r\n",
                "http://github.com/",
                0,
            ),
        ] {
            let (head, _) = ask(&proxy, request);
            assert!(head.starts_with("HTTP/1.1 403"), "{request}: {head}");
            let last = proxy.denials().pop().expect("a denial");
            assert_eq!((last.host.as_str(), last.port), (host, port), "{last:?}");
        }
        let reasons: Vec<String> = proxy.denials().into_iter().map(|d| d.reason).collect();
        assert_eq!(
            reasons,
            [
                "not on the allowlist",
                "ip literal",
                "ip literal",
                "method GET"
            ]
        );
    }

    #[test]
    fn tunnels_listed_host() {
        if nested() {
            return;
        }
        let echo = TcpListener::bind("127.0.0.1:0").expect("echo");
        let at = echo.local_addr().expect("addr");
        std::thread::spawn(move || {
            let (mut s, _) = echo.accept().expect("accept");
            let mut buf = [0u8; 5];
            s.read_exact(&mut buf).expect("read");
            s.write_all(&buf).expect("echo back");
        });
        static AT: Mutex<Option<SocketAddr>> = Mutex::new(None);
        *AT.lock().unwrap() = Some(at);
        // the echo server is on loopback, which the real resolver refuses
        let proxy = Proxy::with(vec!["echo.test".into()], |_, _| {
            Ok(vec![AT.lock().unwrap().expect("echo address")])
        })
        .expect("proxy");
        let (head, mut s) = ask(&proxy, "CONNECT echo.test:443 HTTP/1.1\r\n\r\n");
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        s.write_all(b"hello").expect("send");
        let mut back = [0u8; 5];
        s.read_exact(&mut back).expect("tunnelled");
        assert_eq!(&back, b"hello");
        assert!(proxy.denials().is_empty());
    }
}
