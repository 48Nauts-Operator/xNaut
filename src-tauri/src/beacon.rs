//! `xnaut-beacon` — the thing in the sandbox that says "I am still here".
//!
//! XNAUT-307. XNAUT-266 shipped a reaper that destroyed a GitVM sandbox 45
//! minutes after it was launched, and destroying a sandbox destroys
//! `/workspace` with it (XNAUT-40). Age was the trigger, so an agent still
//! typing at minute 46 lost its uncommitted work to a timer. André's answer,
//! 2026-09-08: "ideally we have a beacon on every sandbox... a script that
//! polls every 60 seconds, checks if there is an agent running, checks some
//! data, sends a pong to xNaut and sleeps."
//!
//! The load-bearing property is that the beacon pongs UNCONDITIONALLY. It
//! pongs while the agent works; it pongs with `agent_pid: null` after the
//! agent has exited; it pongs with the same byte count and the same head when
//! nothing at all has moved. A pong is never a claim that work is happening —
//! `run_control::apply_pong` decides that from the numbers. Because the pong
//! is unconditional, SILENCE carries exactly one meaning, and it is the one
//! the reaper is allowed to act on: the VM is gone.
//!
//! That inversion is the whole ticket. The old reaper asked "how old is this
//! box" and destroyed on the answer. This one asks "did the box answer" and
//! destroys only on silence, or on a stall the run itself has not declared as
//! a wait.
//!
//! ## Why the script and not an agent
//!
//! It is POSIX-ish bash with `curl`, staged as a file and started in its own
//! tmux session. Three consequences worth stating:
//!
//! - **Its own session, not the agent's.** If it shared the agent's session it
//!   would die with the agent, and the one report we most need — "the agent
//!   has exited but the box is still up and still costing" — would arrive as
//!   silence, which means something else entirely.
//! - **It never exits on failure.** A failed `curl` is a network blip, not a
//!   reason to stop reporting; the loop swallows it and pongs again in 60 s.
//!   `set -e` is deliberately absent for the same reason.
//! - **It establishes its own capture.** The agent runs under tmux with no
//!   redirection, so there is no file to measure. The beacon turns on
//!   `tmux pipe-pane` once, which gives a monotonically growing log — and a
//!   monotonic file is what makes "bytes grew" mean progress rather than
//!   scrollback churn.

/// What the launcher must hand the beacon for it to be able to report.
///
/// Every field is passed IN rather than discovered on the far side: the VM has
/// no idea which run it is, what the hook server's address is, or what the
/// agent's binary is called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeaconConfig {
    /// `$XNAUT_HOOK_URL` — a base, no trailing route. Reachable from inside
    /// the sandbox only because the launcher opened a reverse tunnel for the
    /// hook port first.
    pub hook_url: String,
    /// The session token, sent as `X-Xnaut-Session`. Never logged.
    pub token: String,
    /// The registry run this beacon reports for.
    pub run_id: String,
    /// The agent binary to `pgrep` for: `claude`, `codex`, `pi`.
    pub agent_binary: String,
    /// Where the pane log is written and measured.
    pub capture_path: String,
    /// The agent's tmux session, whose pane is piped into `capture_path`.
    pub agent_session: String,
    /// The git working tree to read HEAD and the dirty count from.
    pub workspace: String,
    /// Seconds between pongs. 60 by the ticket.
    pub interval_secs: u32,
}

/// The tmux session the beacon itself runs in.
///
/// Derived from the run id and never remembered, for the same reason
/// `launch_env::session_name` is: after an app restart the only way to find
/// this again is to be able to compute it.
pub fn session_name(run_id: &str) -> String {
    let short: String = run_id.chars().take(8).collect();
    format!("xnaut-beacon-{}", short.to_ascii_lowercase())
}

/// The default place a sandboxed run's pane log is written.
pub fn capture_path(session: &str) -> String {
    format!("/workspace/.xnaut/{session}.log")
}

/// The beacon script body.
///
/// Pure and returned as a string so it can be executed by a test on this
/// machine rather than only string-matched. A script that parses and reports
/// the wrong numbers would satisfy any `contains` assertion and still get a
/// working agent's sandbox destroyed.
pub fn script(cfg: &BeaconConfig) -> String {
    use crate::sandbox::exe::shell_single_quote as q;
    let interval = cfg.interval_secs.max(1);
    format!(
        r#"#!/bin/bash
# xnaut-beacon (XNAUT-307): liveness for a sandboxed run.
# Pongs every {interval}s for as long as this VM is alive, whatever the agent
# is doing. Silence means the VM is gone -- nothing else.
# No `set -e`: a failed curl is a blip, not a reason to stop reporting.
set -u

HOOK={hook}
TOKEN={token}
RUN={run}
BIN={bin}
CAP={cap}
AGENT_SESSION={agent_session}
WS={ws}

while :; do
  # Capture, established once. `cat >>` creates the file the moment the pipe
  # is on, so this condition clears itself and pipe-pane is never toggled off
  # by a second call.
  if [ ! -f "$CAP" ] && tmux has-session -t "$AGENT_SESSION" 2>/dev/null; then
    mkdir -p "$(dirname "$CAP")" 2>/dev/null || true
    tmux pipe-pane -o -t "$AGENT_SESSION" "cat >> $CAP" 2>/dev/null || true
  fi

  # The agent process, or null once it has exited. Reported either way.
  pid=$(pgrep -x "$BIN" 2>/dev/null | head -n1)
  case "$pid" in ''|*[!0-9]*) pid=null ;; esac

  bytes=$(wc -c < "$CAP" 2>/dev/null | tr -d ' ')
  case "$bytes" in ''|*[!0-9]*) bytes=0 ;; esac

  # Short, per the ticket. The registry knows the full sha and compares by
  # prefix, so the two forms cannot fake movement between them.
  head=$(git -C "$WS" rev-parse --short HEAD 2>/dev/null || true)
  if [ -z "$head" ]; then
    # The GitVM case, and it is the normal one rather than the exception:
    # `gitvm` rsyncs with `--exclude '.git/objects'`, so /workspace has a .git
    # that git itself refuses ("not a git repository"). Measured on
    # sb-8cac65d0, 2026-09-08. HEAD and the ref it points at are plain text and
    # ARE synced, so the sha is still readable without a single object.
    ref=$(cat "$WS/.git/HEAD" 2>/dev/null || true)
    case "$ref" in
      'ref: '*) head=$(cut -c1-7 < "$WS/.git/${{ref#ref: }}" 2>/dev/null || true) ;;
      *) head=$(printf '%s' "$ref" | cut -c1-7) ;;
    esac
  fi
  case "$head" in *[!0-9a-fA-F]*) head='' ;; esac

  dirty=$(git -C "$WS" status --porcelain 2>/dev/null | wc -l | tr -d ' ')
  case "$dirty" in ''|*[!0-9]*) dirty=0 ;; esac

  body=$(printf '{{"run_id":"%s","agent_pid":%s,"capture_bytes":%s,"head":"%s","dirty":%s}}' \
    "$RUN" "$pid" "$bytes" "$head" "$dirty")

  # The token travels in a header from a variable, never on the command line:
  # argv is world-readable in the VM's process table.
  curl -sS -m 15 -X POST "${{HOOK%/}}/v1/beacon" \
    -H "X-Xnaut-Session: $TOKEN" \
    -H 'content-type: application/json' \
    -d "$body" >/dev/null 2>&1 || true

  sleep {interval}
done
"#,
        hook = q(&cfg.hook_url),
        token = q(&cfg.token),
        run = q(&cfg.run_id),
        bin = q(&cfg.agent_binary),
        cap = q(&cfg.capture_path),
        agent_session = q(&cfg.agent_session),
        ws = q(&cfg.workspace),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> BeaconConfig {
        BeaconConfig {
            hook_url: "http://127.0.0.1:8971".into(),
            token: "tok-1".into(),
            run_id: "run-1".into(),
            agent_binary: "claude".into(),
            capture_path: "/workspace/.xnaut/s.log".into(),
            agent_session: "xnaut-claude-abcd1234".into(),
            workspace: "/workspace".into(),
            interval_secs: 60,
        }
    }

    /// Run one iteration of the beacon's body against a real shell and a real
    /// git repository, and return the JSON it would have POSTed.
    ///
    /// The script is EXECUTED rather than string-matched. The failure being
    /// prevented is a beacon that reports the wrong numbers and gets a working
    /// agent's `/workspace` destroyed; a `contains("capture_bytes")` assertion
    /// would pass for a script that always reported zero.
    fn one_pong(cfg: &BeaconConfig) -> serde_json::Value {
        // Take the body of the loop and stop after the first pass: `curl` is
        // replaced by `printf`, and `sleep` ends it.
        let body = script(cfg)
            .replace("while :; do", "run_once() {")
            .replace(
                r#"  curl -sS -m 15 -X POST "${HOOK%/}/v1/beacon" \
    -H "X-Xnaut-Session: $TOKEN" \
    -H 'content-type: application/json' \
    -d "$body" >/dev/null 2>&1 || true"#,
                r#"  printf '%s' "$body""#,
            )
            .replace(&format!("  sleep {}\ndone", cfg.interval_secs), "}\nrun_once");
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(&body)
            .output()
            .expect("the beacon body ran");
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        serde_json::from_str(&text).unwrap_or_else(|e| {
            panic!(
                "the beacon must emit valid JSON, got {text:?} ({e}); stderr: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }

    /// The report the reaper acts on, measured rather than asserted: a real
    /// repository with a real commit and a real untracked file.
    #[test]
    fn a_pong_carries_the_head_and_the_dirty_count_it_measured() {
        let ws = std::env::temp_dir().join(format!("xnaut-beacon-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&ws).unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&ws)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        std::fs::write(ws.join("a.txt"), "one").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "first"]);
        let head = String::from_utf8_lossy(&git(&["rev-parse", "--short", "HEAD"]).stdout)
            .trim()
            .to_string();
        // Uncommitted work: the thing a teardown would destroy.
        std::fs::write(ws.join("b.txt"), "two").unwrap();

        // Outside the work tree on purpose, so the dirty count below measures
        // the agent's uncommitted work and not the beacon's own log.
        let capture = std::env::temp_dir().join(format!("xnaut-cap-{}", uuid::Uuid::new_v4()));
        std::fs::write(&capture, "0123456789").unwrap();

        let mut cfg = cfg();
        cfg.workspace = ws.to_string_lossy().into_owned();
        cfg.capture_path = capture.to_string_lossy().into_owned();
        // A binary that is certainly not running, so the agent reads as gone.
        cfg.agent_binary = "xnaut-no-such-binary".into();

        let pong = one_pong(&cfg);
        assert_eq!(pong["run_id"], "run-1");
        assert_eq!(pong["head"], head, "the beacon must report the real HEAD");
        assert_eq!(pong["capture_bytes"], 10);
        assert_eq!(pong["dirty"], 1, "the untracked file must be counted");

        let _ = std::fs::remove_dir_all(&ws);
        let _ = std::fs::remove_file(&capture);
    }

    /// The report that must NOT be silence. An agent that has exited leaves a
    /// VM that is still up and still billing; reporting `null` is how the
    /// reaper learns it may take the box, and it is a different fact from the
    /// box having vanished.
    #[test]
    fn the_agent_being_gone_is_reported_as_null_and_not_as_silence() {
        let mut cfg = cfg();
        cfg.agent_binary = "xnaut-no-such-binary".into();
        cfg.workspace = "/nonexistent-workspace".into();
        cfg.capture_path = "/nonexistent-workspace/none.log".into();

        let pong = one_pong(&cfg);
        assert!(pong["agent_pid"].is_null(), "{pong}");
        // Everything else still answers, with zeroes rather than absences: a
        // missing field would deserialise as a default anyway, but a missing
        // POST would read as a dead VM.
        assert_eq!(pong["capture_bytes"], 0);
        assert_eq!(pong["dirty"], 0);
        assert_eq!(pong["head"], "");
    }

    /// The GitVM case, reproduced exactly: a `.git` with no `objects`, which
    /// is what every sandbox has because the CLI rsyncs with
    /// `--exclude '.git/objects'`. `git rev-parse` fails outright there, so
    /// without the file fallback the beacon loses half its progress evidence
    /// in the one environment this ticket is about.
    #[test]
    fn the_head_is_still_read_when_git_itself_cannot_open_the_repository() {
        let ws = std::env::temp_dir().join(format!("xnaut-noobj-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(ws.join(".git/refs/heads")).unwrap();
        std::fs::write(ws.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let sha = "275e93bf2817cebb9f8335078b8845ef45a3eb2d";
        std::fs::write(ws.join(".git/refs/heads/main"), format!("{sha}\n")).unwrap();
        // Deliberately no .git/objects and no .git/config: git must refuse.
        let refused = std::process::Command::new("git")
            .args(["-C", &ws.to_string_lossy(), "rev-parse", "--short", "HEAD"])
            .output()
            .unwrap();
        assert!(
            !refused.status.success(),
            "the fixture must be one git cannot open, or this proves nothing"
        );

        let mut cfg = cfg();
        cfg.workspace = ws.to_string_lossy().into_owned();
        cfg.capture_path = ws.join("none.log").to_string_lossy().into_owned();
        cfg.agent_binary = "xnaut-no-such-binary".into();

        let pong = one_pong(&cfg);
        assert_eq!(pong["head"], &sha[..7], "{pong}");

        // A detached HEAD is the bare sha in the same file.
        std::fs::write(ws.join(".git/HEAD"), format!("{sha}\n")).unwrap();
        assert_eq!(one_pong(&cfg)["head"], &sha[..7]);

        let _ = std::fs::remove_dir_all(&ws);
    }

    /// A running process is found by name, which is the whole of "is there an
    /// agent in here".
    #[test]
    fn a_running_agent_is_reported_by_pid() {
        // `sleep` is on every image this could run on, and pgrep -x matches it
        // by exact name.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawned");
        let mut cfg = cfg();
        cfg.agent_binary = "sleep".into();
        cfg.workspace = "/nonexistent-workspace".into();
        cfg.capture_path = "/nonexistent-workspace/none.log".into();

        let pong = one_pong(&cfg);
        let _ = child.kill();
        let _ = child.wait();

        assert!(
            pong["agent_pid"].as_u64().is_some_and(|p| p > 0),
            "a running agent must be reported as a pid, got {pong}"
        );
    }

    /// The token is a secret in a multi-tenant VM. `curl -d @-` style argv is
    /// world-readable in `/proc`, so it travels in a header read from a shell
    /// variable, and the variable is assigned once at the top.
    #[test]
    fn the_token_never_reaches_a_command_line() {
        let body = script(&cfg());
        assert!(
            body.contains(r#"-H "X-Xnaut-Session: $TOKEN""#),
            "the token must be interpolated by the shell, not baked into argv"
        );
        assert_eq!(
            body.matches("'tok-1'").count(),
            1,
            "the literal token belongs in exactly one assignment: {body}"
        );
    }

    /// A workspace path containing a quote is DATA. Every value the launcher
    /// passes in is single-quoted, so a directory named `it's` cannot end the
    /// assignment and turn the rest of the line into syntax.
    #[test]
    fn a_path_containing_quotes_is_data_and_not_syntax() {
        let mut c = cfg();
        c.workspace = "/tmp/it's here".into();
        let body = script(&c);
        let out = std::process::Command::new("bash")
            .args(["-n", "-c", &body])
            .output()
            .expect("bash parsed it");
        assert!(
            out.status.success(),
            "the script must parse: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Not a test: the way a live run gets the REAL script rather than a
    /// hand-copy of it. `#[ignore]`d so it never runs in the suite.
    ///
    ///   XNAUT_BEACON_HOOK=http://127.0.0.1:51899 XNAUT_BEACON_TOKEN=… \
    ///   XNAUT_BEACON_RUN=… XNAUT_BEACON_BIN=sleep XNAUT_BEACON_SESSION=… \
    ///   cargo test --bin xnaut dump_the_beacon -- --ignored --nocapture
    ///
    /// A live check that pasted a re-typed script would prove the paste ran,
    /// which is not the claim anyone needs.
    #[test]
    #[ignore]
    fn dump_the_beacon_script_for_a_live_run() {
        let var = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.into());
        let session = var("XNAUT_BEACON_SESSION", "xnaut-live-agent");
        print!(
            "{}",
            script(&BeaconConfig {
                hook_url: var("XNAUT_BEACON_HOOK", "http://127.0.0.1:51899"),
                token: var("XNAUT_BEACON_TOKEN", "live-token"),
                run_id: var("XNAUT_BEACON_RUN", "live-run"),
                agent_binary: var("XNAUT_BEACON_BIN", "sleep"),
                capture_path: capture_path(&session),
                agent_session: session,
                workspace: "/workspace".into(),
                interval_secs: var("XNAUT_BEACON_INTERVAL", "60").parse().unwrap_or(60),
            })
        );
    }

    /// The second half of the live check: take the pongs a REAL sandbox
    /// actually sent and drive the real registry and the real reaper with
    /// them, rather than with a fixture written on this side.
    ///
    ///   XNAUT_BEACON_REPLAY=/tmp/pongs.jsonl \
    ///   cargo test --bin xnaut replay_live_pongs -- --ignored --nocapture
    ///
    /// What it can catch that nothing else can: the shell builds its JSON with
    /// `printf`, and `Pong` deserialises it. If those two ever disagree about
    /// a field name or a type, every pong 400s, the run goes silent, and the
    /// reaper destroys a working sandbox — the original bug, arrived at from a
    /// different direction. Nothing in the offline suite compares the two.
    #[test]
    #[ignore]
    fn replay_live_pongs_through_the_registry_and_the_reaper() {
        use crate::run_control as rc;
        use crate::sandbox::launch_env::live;

        let path = std::env::var("XNAUT_BEACON_REPLAY").expect("XNAUT_BEACON_REPLAY");
        let recorded = std::fs::read_to_string(&path).expect("the pong log");
        let pongs: Vec<rc::Pong> = recorded
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter_map(|r| r.get("body")?.as_str().map(str::to_string))
            // Deserialised with the PRODUCTION type. A field the shell spells
            // differently fails here rather than in six months.
            .filter_map(|b| serde_json::from_str::<rc::Pong>(&b).ok())
            .collect();
        assert!(pongs.len() >= 3, "want at least three real pongs, got {}", pongs.len());

        let dir = rc::tests::directory("live-replay");
        let mut run = rc::tests::run();
        run.remote_env = Some("gitvm".into());
        run.capture_bytes = 0;
        run.last_commit = String::new();
        let run = rc::request_in(&dir, run, || Ok(())).unwrap();

        let mut at = 1_000_000i64;
        let mut progressed_count = 0;
        for pong in &pongs {
            let pong = rc::Pong { run_id: run.run_id.clone(), ..pong.clone() };
            let (stored, progressed) = rc::beacon_in(&dir, &pong, at).unwrap();
            progressed_count += progressed as u32;
            println!(
                "  pong at {at}: agent={:?} bytes={} head={:?} -> seen={} progress={} ({})",
                pong.agent_pid, pong.capture_bytes, pong.head,
                stored.last_seen_at, stored.last_progress_at,
                if progressed { "moved" } else { "no movement" },
            );
            at += 20_000;
        }
        assert!(progressed_count > 0, "the real pongs showed no movement at all");

        let entry = live::Environment {
            env: "gitvm".into(),
            handle: "claude".into(),
            project: "/live".into(),
            dir: "/tmp/xnaut-307-live".into(),
            created_ms: 0,
            last_used_ms: 0,
            run_id: Some(run.run_id.clone()),
        };
        let ledger = live::Ledger { environments: vec![entry.clone()] };
        let latest = rc::load_manifest_in(&dir, &run.run_id).unwrap();
        let look = |id: &str| (id == latest.run_id).then(|| latest.clone());

        // While the beacon was reporting: untouchable, and the entry is as old
        // as the run (created_ms 0, last_used_ms 0) on purpose — age must not
        // be able to reach it.
        let while_alive = live::liveness(&entry, Some(&latest), at);
        println!("  liveness while reporting: {while_alive:?}");
        assert_eq!(while_alive, live::Liveness::Alive);
        assert!(ledger.reapable_now("gitvm", None, at, look).is_empty());

        // After the beacon was killed: silence past the lapse window.
        let after = at + rc::BEACON_LAPSE_MS + 60_000;
        let look = |id: &str| (id == latest.run_id).then(|| latest.clone());
        let gone = live::liveness(&entry, Some(&latest), after);
        println!("  liveness after silence:    {gone:?}");
        assert_eq!(gone, live::Liveness::Gone);
        assert_eq!(ledger.reapable_now("gitvm", None, after, look).len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Derivable, never remembered — the same rule the agent's own session
    /// name follows, and for the same reason: an app restart must be able to
    /// find the beacon again without having stored anything.
    #[test]
    fn the_beacon_session_is_derived_from_the_run_id() {
        assert_eq!(session_name("ABCDEF0123456789"), "xnaut-beacon-abcdef01");
        // Its own session, never the agent's: the beacon has to outlive the
        // process it reports on, or "the agent exited but the box is still
        // up" would arrive as silence.
        assert_ne!(
            session_name("ABCDEF0123456789"),
            crate::sandbox::launch_env::session_name("claude", "ABCDEF0123456789")
        );
    }
}
