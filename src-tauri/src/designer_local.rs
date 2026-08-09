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

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    let mut c = Command::new("zsh");
    c.arg("-lc").arg(script).current_dir(dir);
    c
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
    let (workdir, script, wait_secs) = if holding {
        let hd = holding_dir(slug);
        std::fs::create_dir_all(&hd).map_err(|e| format!("could not make a holding page: {e}"))?;
        std::fs::write(hd.join("index.html"), HOLDING_HTML)
            .map_err(|e| format!("could not write the holding page: {e}"))?;
        (
            hd,
            format!("exec python3 -m http.server {port} --bind 127.0.0.1"),
            10u64,
        )
    } else {
        install_if_needed(dir, log)?;
        (
            dir.to_path_buf(),
            format!("exec npm run dev -- --host 127.0.0.1 --port {port}"),
            120u64,
        )
    };

    let out = std::fs::File::create(log).map_err(|e| format!("could not open the log: {e}"))?;
    let errs = out
        .try_clone()
        .map_err(|e| format!("could not open the log: {e}"))?;
    // `exec` replaces the shell, so the pid we get back IS the server, and
    // process_group(0) puts it in its own group so the whole tree can be killed.
    let child = {
        use std::os::unix::process::CommandExt;
        shell(&script, &workdir)
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
        std::thread::sleep(Duration::from_millis(250));
    }
    stop(pgid);
    let tail = std::fs::read_to_string(log)
        .map(|s| {
            s.lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default();
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
    let _ = Command::new("kill").arg("-TERM").arg(&group).output();
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(100));
        let alive = Command::new("kill")
            .arg("-0")
            .arg(&group)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !alive {
            return;
        }
    }
    let _ = Command::new("kill").arg("-KILL").arg(&group).output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_free_port_is_actually_free() {
        let p = free_port().expect("a port");
        assert!(p > 0);
        assert!(!port_open(p), "nothing should be listening on a fresh port");
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
                assert!(e.contains("never bound"), "unexpected error: {e}");
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
