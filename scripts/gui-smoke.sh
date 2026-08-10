#!/usr/bin/env bash
# End-to-end GUI smoke test for the shipped xNAUT app, with video.
#
# Runs ON the machine under test (the Mac mini), driving the real app the way a
# user does: synthetic clicks, screenshots at every step, one screen recording
# of the whole run. Everything else we have proves the code compiles; this is
# the only thing that proves the buttons work.
#
# WHY THIS EXISTS: on 2026-08-09 the Designer was reported fixed three times and
# was broken all three, because every check was a unit test or a curl. Nobody
# had clicked it.
#
#   ./scripts/gui-smoke.sh            full run, video + screenshots
#   ./scripts/gui-smoke.sh --check    just verify the machine can do this at all
#
# REQUIRES, and none of these can be granted from a script (Apple's design):
#   * a logged-in GUI session (auto-login, a display or a dummy HDMI plug, or
#     one screen-sharing login)
#   * Accessibility        -> System Settings > Privacy & Security
#   * Screen Recording     -> same place
# Run with --check first; it names whichever of these is missing.

set -uo pipefail

OUT="${OUT:-$HOME/xnaut-gui-smoke/$(date +%Y%m%d-%H%M%S)}"
APP="/Applications/xNAUT.app"
STEP=0
FAILED=0

say()  { printf '  %s\n' "$*"; }
head_() { printf '\n== %s\n' "$*"; }

shot() {
  STEP=$((STEP + 1))
  local name; name=$(printf '%02d-%s' "$STEP" "$1")
  screencapture -x "$OUT/$name.png" 2>/dev/null
  if [ -s "$OUT/$name.png" ]; then say "shot $name.png"; else say "SHOT FAILED $name"; FAILED=1; fi
}

# Click an element by its accessibility label rather than a pixel guess, so the
# test survives layout changes. Falls back to nothing and reports, never clicks
# blind: a wrong click in a real app can do real damage.
click_named() {
  local want="$1" pos
  pos=$(osascript <<OSA 2>/dev/null
tell application "System Events" to tell process "xNAUT"
  set hits to (every UI element of front window whose name contains "$want")
  if (count of hits) = 0 then return ""
  set p to position of item 1 of hits
  set s to size of item 1 of hits
  return ((item 1 of p) + (item 1 of s) / 2 as integer) & "," & ((item 2 of p) + (item 2 of s) / 2 as integer)
end tell
OSA
)
  pos=$(printf '%s' "$pos" | tr -d ' ')
  if [ -z "$pos" ]; then say "NOT FOUND: $want"; FAILED=1; return 1; fi
  cliclick "c:${pos%,*},${pos#*,}" >/dev/null 2>&1
  say "clicked '$want' at $pos"
  sleep 1
}

preflight() {
  head_ "Preflight"
  local ok=0
  [ -d "$APP" ] && say "app: $(defaults read "$APP/Contents/Info.plist" CFBundleShortVersionString)" \
                || { say "MISSING: $APP"; ok=1; }
  command -v cliclick >/dev/null || { say "MISSING: cliclick (brew install cliclick)"; ok=1; }
  command -v ffmpeg  >/dev/null || { say "MISSING: ffmpeg"; ok=1; }

  local console; console=$(stat -f%Su /dev/console)
  if [ "$console" = "root" ]; then
    say "NO GUI SESSION: console user is root. Log in (auto-login or screen share) first."
    ok=1
  else
    say "GUI session: $console"
  fi

  # Filename must not start with a dot: screencapture refuses to write a hidden
  # file and fails with "cannot write file to intended destination", which reads
  # exactly like a missing Screen Recording grant. Cost us an hour.
  local probe="${TMPDIR:-/tmp}/xnaut-probe.png"
  screencapture -x "$probe" 2>/dev/null
  if [ -s "$probe" ]; then say "screen recording: granted"; else
    say "NO SCREEN RECORDING: grant it in Privacy & Security"; ok=1; fi
  rm -f "$probe"

  if cliclick p 2>&1 | grep -qi 'accessibility'; then
    say "NO ACCESSIBILITY: grant it to this terminal in Privacy & Security"; ok=1
  else
    say "accessibility: granted (cursor $(cliclick p 2>/dev/null))"
  fi
  return $ok
}

[ "${1:-}" = "--check" ] && { preflight; exit $?; }

preflight || { echo; echo "  Preflight failed. Fix the items above; nothing was run."; exit 1; }

mkdir -p "$OUT"
head_ "Recording to $OUT"
ffmpeg -nostdin -loglevel error -f avfoundation -capturecursor 1 -framerate 15 \
       -i "1:none" -pix_fmt yuv420p "$OUT/run.mp4" & REC=$!
trap 'kill $REC 2>/dev/null; wait $REC 2>/dev/null' EXIT
sleep 2

head_ "Launch"
open -a "$APP"; sleep 6
shot launch

head_ "Walk the surfaces"
for target in Observatory Projects Designer NAUT-Flow Docs Artifacts Work Delivery Settings; do
  click_named "$target" && shot "$(echo "$target" | tr '[:upper:] ' '[:lower:]-')"
done

head_ "Done"
sleep 2; kill $REC 2>/dev/null; wait $REC 2>/dev/null; trap - EXIT
say "video:       $OUT/run.mp4 ($(du -h "$OUT/run.mp4" 2>/dev/null | cut -f1))"
say "screenshots: $(ls "$OUT"/*.png 2>/dev/null | wc -l | tr -d ' ')"
[ "$FAILED" -eq 0 ] && say "RESULT: every step succeeded" || say "RESULT: FAILURES above, see the shots"
exit $FAILED
