#!/usr/bin/env bash
# What rig-launchd.sh would install, checked without launchd, without an app
# and without the rig.
#
# The plist is the whole of the launch contract, and every field in it is there
# because its absence cost a cycle: a bare PATH that could not spawn zellij, a
# stdout pipe that panicked the app when ssh went away, a KeepAlive that would
# restart the app the instant a quit test succeeded. None of those failed
# loudly. They failed as "the rig came up and did nothing useful", which is the
# same shape as a hundred other problems and takes an hour to tell apart.
#
# So they are assertions, and they run anywhere:
#
#   ./tests/rig-launchd.test.sh

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
RIG="$REPO/scripts/rig-launchd.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
FAILED=0

ok()   { printf '  ok   %s\n' "$*"; }
fail() { printf '  FAIL %s\n' "$*"; FAILED=1; }

[ -x "$RIG" ] || { echo "scripts/rig-launchd.sh is missing or not executable"; exit 1; }

PLIST="$TMP/rig.plist"
"$RIG" plist --app "/Applications/xNAUT TEST.app" --label com.xnaut.rigtest > "$PLIST"
[ -s "$PLIST" ] || { echo "the plist verb wrote nothing"; exit 1; }

read_key() { # read_key <key>  -> the value, via the same parser launchd uses
  plutil -extract "$1" raw -o - "$PLIST" 2>/dev/null
}

echo "== the plist is a plist"
if plutil -lint "$PLIST" >/dev/null 2>&1; then
  ok "plutil accepts it"
else
  fail "plutil rejects what install would write"
fi

echo "== it runs the executable inside the bundle we named"
prog="$(read_key Program)"
case "$prog" in
  "/Applications/xNAUT TEST.app/Contents/MacOS/xnaut") ok "Program = $prog" ;;
  *) fail "Program is $prog, not the Mach-O in the named bundle" ;;
esac
# The default bundle carries a space. A plist built by string-mashing survives
# that; one built by a shell that forgot to quote does not, and the failure is
# a job that silently never starts.
if [ "$(read_key Label)" = "com.xnaut.rigtest" ]; then
  ok "--label is honoured"
else
  fail "Label is $(read_key Label), not the one passed"
fi

echo "== zellij is on the job's PATH"
# The landmine from 1.14.0: launchd hands a job PATH=/usr/bin:/bin:/usr/sbin:/sbin
# and nothing in Homebrew can be spawned. The app listed no zellij sessions
# while five were live, and nothing errored. See zellij.rs::zellij_bin.
path="$(read_key EnvironmentVariables.PATH)"
case ":$path:" in
  *:/opt/homebrew/bin:*) ok "PATH carries /opt/homebrew/bin" ;;
  *) fail "PATH is '$path' — a launchd job with that cannot spawn zellij" ;;
esac

echo "== stdout and stderr are files, not pipes"
# The broken-pipe abort (tron rust-panics.log, 2026-08-31 12:07 UTC): the app
# inherited an ssh pipe, ssh went away, and the next println! killed it. A file
# has no reader to lose.
for key in StandardOutPath StandardErrorPath; do
  value="$(read_key "$key")"
  case "$value" in
    /*) ok "$key = $value" ;;
    *)  fail "$key is '$value'; it must be an absolute file path" ;;
  esac
done

echo "== the rig is driven, not supervised"
# KeepAlive true would restart the app the moment a quit test managed to stop
# it, so the test could never observe the state it exists to check.
if [ "$(read_key KeepAlive)" = "false" ]; then
  ok "KeepAlive is false"
else
  fail "KeepAlive is $(read_key KeepAlive); a quit verb cannot win against a restarter"
fi
if [ "$(read_key RunAtLoad)" = "false" ]; then
  ok "RunAtLoad is false"
else
  fail "RunAtLoad is $(read_key RunAtLoad); installing the job would start the app as a side effect"
fi
if [ "$(read_key LimitLoadToSessionType)" = "Aqua" ]; then
  ok "the job is limited to the Aqua session, so the window is really on screen"
else
  fail "LimitLoadToSessionType is $(read_key LimitLoadToSessionType), not Aqua"
fi

echo "== install refuses a bundle that is not there"
# Bootstrapping a job whose Program does not exist succeeds, and then every
# launch fails with a code nobody reads. Refuse at the point where the message
# can still say what is wrong.
out="$("$RIG" install --app "$TMP/nonexistent.app" --label com.xnaut.rigtest 2>&1)"
rc=$?
if [ $rc -ne 0 ] && printf '%s' "$out" | grep -q "ship the bundle first"; then
  ok "install exits $rc and names the missing executable"
else
  fail "install accepted a bundle with no executable (rc=$rc): $out"
fi

echo "== an unknown verb is an error, not a no-op"
"$RIG" definitely-not-a-verb >/dev/null 2>&1
[ $? -ne 0 ] && ok "unknown verbs exit non-zero" || fail "an unknown verb exited 0"

echo "== status answers as JSON even with nothing installed"
# The lever's callers branch on this. Prose here means every caller grows its
# own parser, which is how control-xnaut ended up with three of them.
status="$("$RIG" status --label com.xnaut.rigtest-unused)"
if printf '%s' "$status" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["installed"] is False and d["running"] is False' 2>/dev/null; then
  ok "status is parseable JSON reporting installed:false running:false"
else
  fail "status is not the JSON its callers expect: $status"
fi

echo "== the lever refuses to stop the app on this machine by accident"
# CLAUDE.md §8. `quit` and `cycle` exist to restart a RIG; --host is what says
# which one, and its default is 127.0.0.1. Forgetting the flag has to be an
# error message, not a stopped app on André's desk.
CONTROL="$REPO/scripts/control-xnaut.mjs"
for verb in quit cycle; do
  out="$(node "$CONTROL" "$verb" 2>&1)"
  rc=$?
  if [ $rc -ne 0 ] && printf '%s' "$out" | grep -q "THIS machine"; then
    ok "$verb refuses a local target and says how to aim it"
  else
    fail "$verb ran against 127.0.0.1 without --force (rc=$rc): $out"
  fi
done

echo
[ $FAILED -eq 0 ] && echo "rig-launchd: all checks passed" || echo "rig-launchd: FAILURES above"
exit $FAILED
