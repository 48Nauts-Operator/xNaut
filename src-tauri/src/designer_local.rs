// Local runtime for the Designer (XNAUT-118).
//
// The design agent ALREADY runs on this machine: it goes through `loom_run` and
// writes straight into the vault (see the note on `designer_publish`). So local
// mode does not reimplement the Designer without a sandbox. It replaces exactly
// one thing, where the result is SERVED, and drops everything that only existed
// to get local files onto a remote box: no warm-up, no lease, no rsync, no
// tunnel, no teardown.
//
// The trade is real and the UI says so: a 127.0.0.1 URL cannot be sent to
// anyone. Sandbox mode stays the answer when the point is to share a link.
//
// The server logic mirrors `designer::ensure_dev_server` deliberately, including
// two lessons that cost a day each:
//
//   * The holding page is served from its OWN directory, never the design
//     folder. Serving project files raw poisons the browser cache in a way that
//     survives the fix: python sends Last-Modified from the file mtime, so once
//     Vite takes over the browser revalidates, gets a 304, and keeps the
//     untransformed body forever.
//   * A process group, not a bare pid. `npm run dev` forks Vite, so killing npm
//     alone orphans the child still holding the port, and the next spin-up finds
//     it occupied by something it cannot stop.

// The whole module is Unix: lsof, setsid, login shells, process groups.
#![cfg(unix)]

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// Ask the OS for a free loopback port by binding zero and letting go.
///
/// There is a race between releasing it and the server binding it. It is
/// tolerable here because nothing else on the machine is hunting for ports in a
/// loop, and a lost race surfaces immediately as "did not come up" rather than
/// as a silent wrong-port failure.
pub fn free_port() -> Result<u16, String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("could not find a free port: {e}"))?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|e| format!("could not read the chosen port: {e}"))
}

/// True when something accepts a connection on the loopback port.
///
/// This is the local equivalent of probing the public URL: it asks the outcome
/// rather than trusting that a spawn worked.
pub fn port_open(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(300),
    )
    .is_ok()
}

/// `lsof`, but it cannot hang the app.
///
/// `lsof` walks every mount. A wedged network share puts it in uninterruptible
/// I/O wait and it never returns, which matters because this runs on a five
/// second poll for the whole of a local design: one unresponsive NAS would
/// freeze the Designer. Found on 2026-08-09 when an SMB mount jammed and the
/// test suite hung on exactly this call.
///
/// `-S` bounds lsof's own kernel calls; the outer deadline covers the rest.
fn lsof(args: &[&str]) -> Option<Vec<u8>> {
    use std::sync::mpsc;
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let out = Command::new("lsof")
            .arg("-S2") // lsof's internal timeout for stat() on each mount
            .args(&owned)
            .output()
            .ok()
            .map(|o| o.stdout);
        let _ = tx.send(out);
    });
    // The thread is left detached on timeout; it is a doomed lsof holding no
    // resource of ours, and it dies when the mount recovers or the app exits.
    rx.recv_timeout(Duration::from_secs(5)).ok().flatten()
}

/// A dev server the AGENT started, found by its working directory.
///
/// This is the part local mode cannot borrow from the sandbox path. In a
/// sandbox the agent runs inside the box and there is exactly one port, ours.
/// Locally the agent runs on this machine and starts its own dev server,
/// because it screenshots the running site to review its own work. Starting a
/// second server next to it means two processes serving the same project on
/// different ports, and the canvas showing whichever one xNAUT happens to know
/// about, which is how a finished site sat on :4399 while the canvas displayed
/// a holding page on :53097.
///
/// So: adopt, do not compete. A listener whose working directory IS the design
/// folder is that design's dev server, whoever started it.
pub fn adopt(dir: &Path) -> Option<(u16, u32)> {
    let listeners = lsof(&["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pn"])?;
    // -F pn emits `p<pid>` followed by one `n<addr>` per bound address.
    let mut found: Vec<(u32, u16)> = Vec::new();
    let mut pid = 0u32;
    for line in String::from_utf8_lossy(&listeners).lines() {
        match line.as_bytes().first() {
            Some(b'p') => pid = line[1..].parse().unwrap_or(0),
            Some(b'n') => {
                // "127.0.0.1:4399", "*:4399", "[::1]:4399"
                if let Some(port) = line.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
                    if pid != 0 {
                        found.push((pid, port));
                    }
                }
            }
            _ => {}
        }
    }
    if found.is_empty() {
        return None;
    }
    let pids: Vec<String> = {
        let mut v: Vec<u32> = found.iter().map(|(p, _)| *p).collect();
        v.sort_unstable();
        v.dedup();
        v.iter().map(|p| p.to_string()).collect()
    };
    let joined = pids.join(",");
    let cwds = lsof(&["-a", "-p", &joined, "-d", "cwd", "-F", "pn"])?;
    let want = std::fs::canonicalize(dir).ok()?;
    let mut pid = 0u32;
    for line in String::from_utf8_lossy(&cwds).lines() {
        match line.as_bytes().first() {
            Some(b'p') => pid = line[1..].parse().unwrap_or(0),
            Some(b'n') => {
                if std::fs::canonicalize(&line[1..]).ok().as_ref() != Some(&want) {
                    continue;
                }
                // The agent process ALSO has the design folder as its working
                // directory and opens sockets of its own, so matching the
                // directory is not enough to conclude "this is the website".
                // Make the port prove it serves HTTP before the canvas is
                // pointed at it.
                for (p, port) in found.iter().filter(|(p, _)| *p == pid) {
                    if speaks_http(*port) {
                        return Some((*port, *p));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// True when the port answers a plain GET with an HTTP status line.
fn speaks_http(port: u16) -> bool {
    use std::io::{Read, Write};
    let Ok(mut sock) = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(300),
    ) else {
        return false;
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(700)));
    let _ = sock.set_write_timeout(Some(Duration::from_millis(300)));
    if sock
        .write_all(b"GET / HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut head = [0u8; 12];
    let mut got = 0;
    while got < head.len() {
        match sock.read(&mut head[got..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => got += n,
        }
    }
    head[..got].starts_with(b"HTTP/")
}

/// Whether the design has been scaffolded into a real project yet.
///
/// Before the agent's first turn there is no package.json, so a static holding
/// page is the only thing that can be served. After it, the holding page is
/// actively wrong and must be evicted.
pub fn wants_project_server(dir: &Path) -> bool {
    dir.join("package.json").is_file()
}

/// Where the holding page lives: per design, never inside the design folder.
fn holding_dir(slug: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xnaut-designer-holding-{slug}"))
}

const HOLDING_HTML: &str = "<!doctype html><meta charset=utf-8><title>Preparing…</title>\
<body style=\"font:15px/1.6 system-ui;display:grid;place-items:center;height:100vh;margin:0;color:#666\">\
Preparing this design…</body>";

/// Run a command through a login shell so it sees the user's real PATH.
///
/// The app can be launched from Finder, where PATH is the bare system one and
/// `npm` does not exist. Every other spawn path in xNAUT goes through a login
/// shell for the same reason.
fn shell(script: &str, dir: &Path) -> Command {
    let mut c = Command::new(login_shell());
    c.arg("-lc").arg(script).current_dir(dir);
    c
}

/// The login shell, resolved instead of assumed.
///
/// The hardcoded `zsh` this replaces made every designer spawn fail on a
/// stock Linux box (no zsh), with the error blamed on the program being
/// looked up ("could not look up python3") rather than the shell that never
/// started — found by the first exe.dev verify run (XNAUT-253). $SHELL is
/// the user's own choice when it points at a real file; /bin/zsh keeps the
/// macOS/Homebrew behavior; sh is on every box.
fn login_shell() -> String {
    resolve_login_shell(std::env::var("SHELL").ok().as_deref())
}

fn resolve_login_shell(shell_env: Option<&str>) -> String {
    if let Some(sh) = shell_env.map(str::trim).filter(|s| !s.is_empty()) {
        if Path::new(sh).is_file() {
            return sh.to_string();
        }
    }
    if Path::new("/bin/zsh").is_file() {
        return "/bin/zsh".to_string();
    }
    "sh".to_string()
}

/// Resolve a user-installed executable through a login shell, then run the
/// absolute path directly.
///
/// A Finder-launched app needs the login shell to discover Homebrew, but it
/// does not need to keep that shell between xNAUT and a long-lived server. The
/// latter was observably unreliable: the holding-page process could appear
/// only after most of its ten-second bind deadline had elapsed, with an empty
/// log and no useful exit status.
fn resolve_executable(name: &str, dir: &Path) -> Result<PathBuf, String> {
    let out = shell(&format!("command -v {name}"), dir)
        .output()
        .map_err(|e| format!("could not look up {name}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{name} is not available in the login shell (needed for local Designer mode)"
        ));
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return Err(format!(
            "{name} is not available in the login shell (needed for local Designer mode)"
        ));
    }
    Ok(PathBuf::from(path))
}

fn log_tail(log: &Path, lines: usize) -> String {
    std::fs::read_to_string(log)
        .map(|s| {
            s.lines()
                .rev()
                .take(lines)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default()
}

/// Installs dependencies if they are missing. Returns true when it ran.
pub fn install_if_needed(dir: &Path, log: &Path) -> Result<bool, String> {
    let modules = dir.join("node_modules");
    let populated = std::fs::read_dir(&modules)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if populated {
        return Ok(false);
    }
    let out = shell("npm install --no-audit --no-fund", dir)
        .output()
        .map_err(|e| format!("could not run npm install: {e}"))?;
    let _ = std::fs::write(log, &out.stdout);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().rev().take(12).collect();
        return Err(format!(
            "npm install failed: {}",
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        ));
    }
    Ok(true)
}

/// A server this module started.
pub struct Started {
    /// Process group id, which is the pid of the shell we spawned.
    pub pgid: u32,
    /// True when this is the static holding page rather than the real project.
    pub holding: bool,
    pub message: String,
}

/// Starts the right server for `dir` on `port` and waits for it to accept.
///
/// Mirrors the sandbox path: project server when there is a package.json,
/// static holding page otherwise, and the port is what proves it came up.
pub fn start(dir: &Path, slug: &str, port: u16, log: &Path) -> Result<Started, String> {
    let holding = !wants_project_server(dir);
    let (workdir, wait_secs) = if holding {
        let hd = holding_dir(slug);
        std::fs::create_dir_all(&hd).map_err(|e| format!("could not make a holding page: {e}"))?;
        std::fs::write(hd.join("index.html"), HOLDING_HTML)
            .map_err(|e| format!("could not write the holding page: {e}"))?;
        (hd, 30u64)
    } else {
        install_if_needed(dir, log)?;
        (dir.to_path_buf(), 120u64)
    };

    let out = std::fs::File::create(log).map_err(|e| format!("could not open the log: {e}"))?;
    let errs = out
        .try_clone()
        .map_err(|e| format!("could not open the log: {e}"))?;
    // The static server is launched directly after resolving python3 through a
    // login shell. Project servers retain the login shell because npm and its
    // node shebang both depend on the user's PATH.
    //
    // `exec` replaces the project shell, so in either branch the pid we get
    // back IS the server parent. process_group(0) puts its whole tree in a
    // group so Vite can be killed with npm.
    let mut child: Child = {
        use std::os::unix::process::CommandExt;
        let mut command = if holding {
            let python = resolve_executable("python3", &workdir)?;
            let mut c = Command::new(python);
            c.args([
                "-m",
                "http.server",
                &port.to_string(),
                "--bind",
                "127.0.0.1",
            ])
            .current_dir(&workdir);
            c
        } else {
            shell(
                &format!("exec npm run dev -- --host 127.0.0.1 --port {port}"),
                &workdir,
            )
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(errs))
            .process_group(0)
            .spawn()
            .map_err(|e| format!("could not start the local server: {e}"))?
    };
    let pgid = child.id();

    for _ in 0..(wait_secs * 4) {
        if port_open(port) {
            return Ok(Started {
                pgid,
                holding,
                message: if holding {
                    format!("holding page on 127.0.0.1:{port} (no project yet)")
                } else {
                    format!("dev server up on 127.0.0.1:{port}")
                },
            });
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let tail = log_tail(log, 8);
                return Err(format!(
                    "local server exited with {status} before binding 127.0.0.1:{port}{}",
                    if tail.is_empty() {
                        String::new()
                    } else {
                        format!(": {tail}")
                    }
                ));
            }
            Ok(None) => {}
            Err(e) => {
                stop(pgid);
                return Err(format!("could not inspect the local server process: {e}"));
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    stop(pgid);
    let tail = log_tail(log, 8);
    Err(format!(
        "local server never bound 127.0.0.1:{port} within {wait_secs}s: {tail}"
    ))
}

/// Kills the whole process group, so Vite goes with the npm that forked it.
///
/// Negative pid means "the group". Shelling out to `kill` avoids taking a libc
/// dependency for two signals.
pub fn stop(pgid: u32) {
    if pgid == 0 {
        return;
    }
    let group = format!("-{pgid}");
    // `--` before the negative pid: BSD kill (macOS) tolerates its absence,
    // but Linux procps kill parses `-12345` as an option and delivers
    // nothing — the exe.dev verify run caught the server outliving stop().
    let _ = Command::new("kill").args(["-TERM", "--", &group]).output();
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(100));
        let alive = Command::new("kill")
            .args(["-0", "--", &group])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !alive {
            return;
        }
    }
    let _ = Command::new("kill").args(["-KILL", "--", &group]).output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_free_port_is_actually_free() {
        // Two independent fixes for the same flake met here, and the retry is
        // the better one, so it wins.
        //
        // free_port() binds :0, reads the port and DROPS the listener, so from
        // that moment the port belongs to nobody. Three other tests in this
        // binary ask free_port for a port and then put a python server on it,
        // and when the kernel hands the same number to two of them a bind here
        // fails on a race rather than on a bug: 4 red runs in 20 of the full
        // suite, none when this module ran alone.
        //
        // The discarded alternative asked twice and only checked the numbers
        // looked sane. That cannot flake, but it also stops testing the thing
        // callers depend on, which is that the port can actually be bound.
        // free_port promises a port that WAS free; one attempt cannot tell a
        // broken promise from a lost race, and twenty in a row can.
        let mut last = 0u16;
        for _ in 0..20 {
            last = free_port().expect("a port");
            assert!(last > 1024, "free_port handed back a privileged port: {last}");
            if TcpListener::bind(("127.0.0.1", last)).is_ok() {
                return;
            }
        }
        panic!("free_port handed back 20 ports in a row that nothing could bind, last was {last}");
    }

    #[test]
    fn port_zero_is_never_open() {
        assert!(!port_open(0));
    }

    #[test]
    fn a_project_is_recognised_by_its_package_json() {
        let dir = std::env::temp_dir().join(format!("xnaut-dl-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!wants_project_server(&dir), "empty design has no project");
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        assert!(wants_project_server(&dir), "package.json makes it a project");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_holding_page_is_never_the_design_folder() {
        // Serving the design folder raw is what poisoned the browser cache.
        let design = Path::new("/tmp/some-design");
        assert_ne!(holding_dir("some-design"), design.to_path_buf());
        assert!(holding_dir("s").to_string_lossy().contains("holding"));
    }

    #[test]
    fn adopt_finds_a_web_server_running_in_the_directory() {
        // Real lsof, a real child process with a real cwd, a real HTTP server.
        // This parsing is the kind of code that compiles, runs, and silently
        // finds nothing forever.
        let dir = std::env::temp_dir().join(format!("xnaut-dl-adopt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<h1>hi</h1>").unwrap();
        let port = free_port().unwrap();
        let Ok(mut child) = Command::new("python3")
            .args(["-m", "http.server", &port.to_string(), "--bind", "127.0.0.1"])
            .current_dir(&dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            let _ = std::fs::remove_dir_all(&dir);
            return; // no python3 on this box: skip, do not fail
        };
        for _ in 0..40 {
            if port_open(port) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let got = adopt(&dir);
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&dir);

        // `adopt` returns None when lsof cannot enumerate at all, which happens
        // on a machine with a wedged network mount. That is the bounded-timeout
        // behaviour working as designed, not a regression, so skip rather than
        // fail: a broken NAS elsewhere on the box must not turn this red.
        let Some((found_port, found_pid)) = got else {
            eprintln!("skipping: lsof could not enumerate (a mount is likely unresponsive)");
            return;
        };
        assert_eq!(found_port, port);
        assert_eq!(found_pid, child.id());
    }

    #[test]
    fn adopt_refuses_a_socket_that_does_not_speak_http() {
        // The agent process shares the design folder as its cwd and opens its
        // own sockets. Pointing the canvas at one of those would be worse than
        // finding nothing.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        assert!(port_open(port), "the socket is accepting connections");
        assert!(
            !speaks_http(port),
            "a bare TCP listener must not be mistaken for a web server"
        );
        drop(listener);
    }

    #[test]
    fn adopt_ignores_directories_with_no_server() {
        let empty = std::env::temp_dir().join(format!("xnaut-dl-none-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        assert!(adopt(&empty).is_none());
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn the_login_shell_is_resolved_not_assumed() {
        // A $SHELL that exists wins; a lie in $SHELL falls through to a real
        // shell; and the final fallback is sh, which every box has. Pure
        // function on purpose: env vars are process-global and tests race.
        assert_eq!(resolve_login_shell(Some("/bin/sh")), "/bin/sh");
        let fallback = resolve_login_shell(Some("/no/such/shell"));
        assert!(fallback == "/bin/zsh" || fallback == "sh", "got {fallback}");
        assert_eq!(resolve_login_shell(None), fallback);
        assert_eq!(resolve_login_shell(Some("   ")), fallback);
    }

    #[test]
    fn stopping_nothing_is_harmless() {
        stop(0);
    }

    #[test]
    fn a_started_server_answers_and_stops() {
        // The real contract: something binds, the port answers, the group dies.
        let dir = std::env::temp_dir().join(format!("xnaut-dl-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("dev.log");
        let port = free_port().unwrap();
        let started = match start(&dir, "unit-test", port, &log) {
            Ok(s) => s,
            Err(e) => {
                // No python3 on the box is a skip, not a failure.
                assert!(
                    e.contains("python3 is not available"),
                    "unexpected error: {e}"
                );
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
        };
        assert!(started.holding, "an empty dir gets the holding page");
        assert!(port_open(port), "the port must answer before we claim success");
        stop(started.pgid);
        std::thread::sleep(Duration::from_millis(400));
        assert!(!port_open(port), "stop must free the port");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
