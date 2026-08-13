#!/usr/bin/env bash
# gui-smoke.sh must refuse to drive an xnaut it did not start.
#
# On 2026-08-13 it drove André's production app while it was being used. The
# command named a bundle explicitly -- APP=<worktree>/…/xNAUT.app -- and that
# reads as "drive this bundle". It is not what happens: `open -a` on an app that
# is already running activates the running instance rather than starting a
# second one, and `pgrep -x xnaut | head -1` then hands back whichever came
# first. The walk closed four of his tabs and killed the app at cleanup.
#
# ATTACH had carried a refusal for two releases. APP= had none, and APP= is the
# flag you reach for when you specifically do not want the installed app.
#
# Everything here runs against shims. `pgrep` is shimmed to a pid that does not
# exist, so even a total failure of the guard cannot reach a real process: this
# test is safe to run on a machine with xNAUT open, which is the only kind of
# machine where the bug happens.
#
#   ./tests/gui-smoke-refuses.test.sh

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
FAILED=0

ok()   { printf '  ok   %s\n' "$*"; }
fail() { printf '  FAIL %s\n' "$*"; FAILED=1; }

# A pid nothing owns. The guard is what is under test; this is what makes the
# test safe while it is under test.
GHOST=4294967

# Preflight checks the bundle exists before anything else, so the decoy has to
# be a real directory or the run never reaches the guard under test.
mkdir -p "$TMP/bin" "$TMP/out" "$TMP/nothing.app/Contents/MacOS"
cat > "$TMP/bin/pgrep" <<EOF
#!/bin/sh
echo $GHOST
EOF
cat > "$TMP/bin/lsof" <<'EOF'
#!/bin/sh
echo "xnaut 1 u txt REG 1,2 3 4 /Applications/xNAUT.app/Contents/MacOS/xnaut"
EOF
# The two things that must never happen leave a file behind if they do.
cat > "$TMP/bin/open" <<EOF
#!/bin/sh
echo launched >> "$TMP/open.called"
EOF
cat > "$TMP/bin/osascript" <<EOF
#!/bin/sh
echo "\$*" >> "$TMP/osascript.called"
EOF
for stub in cliclick screencapture ffmpeg axui defaults; do
  printf '#!/bin/sh\nexit 0\n' > "$TMP/bin/$stub"
done
chmod +x "$TMP/bin"/*

echo "== APP= refuses when an xnaut is already running"
out="$(PATH="$TMP/bin:$PATH" APP="$TMP/nothing.app" OUT="$TMP/out" \
       bash "$REPO/scripts/gui-smoke.sh" 2>&1)"

case "$out" in
  *REFUSING*) ok "said REFUSING" ;;
  *) fail "did not refuse; output was:"; printf '%s\n' "$out" | sed 's/^/       /' ;;
esac

case "$out" in
  *"ATTACH=$GHOST"*) ok "named the pid and the escape hatch" ;;
  *) fail "did not tell the caller how to proceed deliberately" ;;
esac

[ -f "$TMP/open.called" ] \
  && fail "launched the app anyway" \
  || ok "never called open"

# The one that actually cost something: quitting the app it refused to drive.
if [ -f "$TMP/osascript.called" ] && grep -q 'quit app' "$TMP/osascript.called"; then
  fail "tried to quit the running app"
else
  ok "never tried to quit anything"
fi

echo
[ "$FAILED" = 0 ] && echo "all good" || echo "FAILURES"
exit "$FAILED"
