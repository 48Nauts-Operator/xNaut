#!/usr/bin/env bash
# Start, stop and inspect the rig's xNAUT under launchd (XNAUT-255).
#
# The rig used to be launched over an ssh tty, because the obvious way did not
# work: an `open -a` instance does not serve the mobile bridge to the network,
# so a run driven from the dev Mac could see the app on screen and reach none of
# it. The ssh-tty workaround got the bridge, and bought two problems with it.
#
#   1. The app is a CHILD OF THE SSH SESSION. Closing the connection takes it
#      with it, so "launch the rig and come back in ten minutes" needs a nohup
#      and a held terminal, and a dropped Tailscale link ends the run.
#   2. Its stdout is that connection's pipe. When the pipe closes, the next
#      `println!` panics — that is the 'failed printing to stdout: Broken pipe'
#      abort in rust-panics.log on tron at 12:07 UTC on 2026-08-31, and the same
#      signature on the dev Mac since 08-18. `main.rs` now points a non-tty
#      stdout at /dev/null in release builds, so this is fixed at the source;
#      launchd's file redirect means it cannot come back by another route, and
#      the log is somewhere to read afterwards.
#
# launchd solves both by owning the process instead of borrowing a terminal:
# the app runs in the console user's `gui/<uid>` domain with a real Aqua session
# (so the window is on screen and Accessibility works), its parent is launchd
# (so ssh can go away), and stdout is a file. And `kickstart`/`kill`/`print` are
# a lifecycle control surface that answers, which `open -a` never was.
#
#   scripts/rig-launchd.sh install [--app "/Applications/xNAUT TEST.app"]
#   scripts/rig-launchd.sh launch            # (re)start it, wait for the pid
#   scripts/rig-launchd.sh quit              # TERM, wait for it to be gone
#   scripts/rig-launchd.sh status            # JSON: installed, running, pid
#   scripts/rig-launchd.sh uninstall
#   scripts/rig-launchd.sh plist             # what install would write; writes nothing
#
#   --host tron        do any of the above on the rig instead of here
#
# ponytail: this drives the raw Mach-O rather than the bundle, which is what
# LaunchServices would not let us do. The cost is that the app is not registered
# as a foreground application in the usual way; the benefit is that it behaves
# exactly like the ssh-tty launch that was already proven to serve the bridge.

set -uo pipefail

LABEL="${LABEL:-com.xnaut.rig}"
APP="${APP:-/Applications/xNAUT TEST.app}"
HOST=""
VERB="${1:-}"
shift 2>/dev/null || true

while [ $# -gt 0 ]; do
  case "$1" in
    --app)   APP="${2:-}"; shift 2 ;;
    --label) LABEL="${2:-}"; shift 2 ;;
    --host)  HOST="${2:-}"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

# Anything remote is this same script, run there. Re-implementing each verb
# behind an `ssh ... "$(quoting)"` is how the launch path grew three subtly
# different copies in the first place; there is exactly one implementation and
# it always runs on the machine it is talking about.
if [ -n "$HOST" ]; then
  scp -q "${BASH_SOURCE[0]}" "$HOST:/tmp/rig-launchd.sh" || {
    echo "cannot copy this script to $HOST" >&2; exit 1; }
  exec ssh -o BatchMode=yes "$HOST" \
    "chmod +x /tmp/rig-launchd.sh && /tmp/rig-launchd.sh $(printf '%q ' "$VERB" --app "$APP" --label "$LABEL")"
fi

UID_="$(id -u)"
DOMAIN="gui/$UID_"
SERVICE="$DOMAIN/$LABEL"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
LOG_DIR="$HOME/Library/Logs"
BIN_NAME="xnaut"

die() { echo "$*" >&2; exit 1; }

# The plist, on stdout. A separate verb rather than a heredoc buried in
# `install`, so the thing that actually gets written can be linted and asserted
# on by a test with no launchd, no app and no rig — the same reason
# gui-smoke.sh's emit block is extractable.
emit_plist() {
  local bin="$APP/Contents/MacOS/$BIN_NAME"
  cat <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>Program</key><string>$bin</string>

  <!-- Explicit, and not negotiable: a launchd job inherits
       PATH=/usr/bin:/bin:/usr/sbin:/sbin, where a bare \`zellij\` cannot spawn
       at all. 1.14.0 shipped that way and the Observatory listed nothing while
       five sessions were live (see zellij.rs::zellij_bin). Homebrew first
       because that is where zellij, claude and node live on the rig. -->
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
  </dict>

  <!-- Files, never a pipe. This is the second half of the broken-pipe fix:
       even a build without main.rs's /dev/null guard cannot abort on EPIPE
       here, because there is no reader to go away. -->
  <key>StandardOutPath</key><string>$LOG_DIR/$LABEL.out.log</string>
  <key>StandardErrorPath</key><string>$LOG_DIR/$LABEL.err.log</string>

  <!-- The rig is driven, not supervised. KeepAlive would restart the app the
       instant \`quit\` succeeded, which turns every quit test into a race the
       test loses. RunAtLoad likewise: bootstrapping the job is not the same
       request as starting the app, and conflating them means \`install\` has a
       side effect nobody asked for. -->
  <key>RunAtLoad</key><false/>
  <key>KeepAlive</key><false/>

  <!-- A GUI app being watched by a human-equivalent: not throttled, not
       treated as batch work. -->
  <key>ProcessType</key><string>Interactive</string>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
</dict>
</plist>
PLIST
}

# Is the job running, and as what pid? `launchctl print` is the only answer
# that distinguishes "loaded but stopped" from "not loaded", which `pgrep`
# cannot: pgrep would also happily report the owner's own xNAUT.
job_pid() {
  launchctl print "$SERVICE" 2>/dev/null \
    | awk -F'= *' '/^[[:space:]]*pid = /{print $2; exit}'
}
job_loaded() { launchctl print "$SERVICE" >/dev/null 2>&1; }

case "$VERB" in
  plist)
    emit_plist
    ;;

  install)
    [ -x "$APP/Contents/MacOS/$BIN_NAME" ] \
      || die "no executable at $APP/Contents/MacOS/$BIN_NAME — ship the bundle first"
    mkdir -p "$(dirname "$PLIST")" "$LOG_DIR"
    emit_plist > "$PLIST" || die "could not write $PLIST"
    plutil -lint "$PLIST" >/dev/null || die "the plist this wrote is malformed: $PLIST"
    # bootout first so a re-install replaces the definition rather than being
    # ignored: bootstrap on an already-loaded label fails with EEXIST and
    # leaves the OLD program path in force, which is the failure mode where you
    # ship a new bundle and keep testing the previous one.
    launchctl bootout "$SERVICE" >/dev/null 2>&1
    launchctl bootstrap "$DOMAIN" "$PLIST" \
      || die "launchctl bootstrap failed; is there a console session for uid $UID_?"
    echo "{\"ok\":true,\"installed\":\"$PLIST\",\"service\":\"$SERVICE\",\"app\":\"$APP\"}"
    ;;

  launch)
    job_loaded || die "$LABEL is not installed; run: rig-launchd.sh install"
    # -k so this is (re)start rather than start: a launch verb that silently
    # does nothing because the app is already up is how a stale build gets
    # tested and reported as the new one.
    launchctl kickstart -k "$SERVICE" || die "launchctl kickstart failed for $SERVICE"
    for _ in $(seq 1 30); do
      pid="$(job_pid)"
      [ -n "$pid" ] && { echo "{\"ok\":true,\"pid\":$pid,\"service\":\"$SERVICE\"}"; exit 0; }
      sleep 1
    done
    die "$LABEL was kickstarted but never reported a pid; see $LOG_DIR/$LABEL.err.log"
    ;;

  quit)
    job_loaded || { echo "{\"ok\":true,\"note\":\"not installed; nothing to quit\"}"; exit 0; }
    launchctl kill TERM "$SERVICE" >/dev/null 2>&1
    for _ in $(seq 1 30); do
      [ -z "$(job_pid)" ] && { echo "{\"ok\":true,\"stopped\":true}"; exit 0; }
      sleep 1
    done
    # Report the refusal rather than escalating to KILL. A quit test whose
    # subject had to be SIGKILLed did not pass, and hiding that is the whole
    # class of bug this rig exists to catch.
    die "$LABEL did not exit within 30s of SIGTERM (pid $(job_pid))"
    ;;

  status)
    pid="$(job_pid)"
    printf '{"ok":true,"service":"%s","installed":%s,"running":%s,"pid":%s,"app":"%s","log":"%s"}\n' \
      "$SERVICE" \
      "$([ -f "$PLIST" ] && echo true || echo false)" \
      "$([ -n "$pid" ] && echo true || echo false)" \
      "${pid:-null}" "$APP" "$LOG_DIR/$LABEL.err.log"
    ;;

  uninstall)
    launchctl bootout "$SERVICE" >/dev/null 2>&1
    rm -f "$PLIST"
    echo "{\"ok\":true,\"uninstalled\":\"$SERVICE\"}"
    ;;

  ""|help|-h|--help)
    sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    [ -z "$VERB" ] && exit 2 || exit 0
    ;;

  *)
    die "unknown verb: $VERB (try: install launch quit status uninstall plist)"
    ;;
esac
