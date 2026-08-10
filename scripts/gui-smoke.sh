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
#   CROP=0 ./scripts/gui-smoke.sh     record the whole desktop instead of the app
#   TASK="$(cat task.txt)" ./scripts/gui-smoke.sh    record the instruction too
#
# The run directory is the contract the dashboard reads (scripts/testing-report.mjs):
# run.json written here, report.md appended by whoever drove the run, the shots and
# the video. Pass TASK when the run was delegated, so the instruction sits next to
# the result it produced; without it a report is unauditable.
#
# Video and screenshots are cropped to the app window, measured once after launch.
# A window moved mid-run would drift out of frame; nothing here moves it, and the
# alternative (re-reading the frame per shot) buys nothing for a test that does
# not drag windows.
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
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AXUI="${AXUI:-$HERE/.axui}"
STEP=0
FAILED=0
APP_PID=""
APP_VER=""
STARTED=""
WIN_R=""   # "x,y,w,h" in points once the window is known; empty means full screen
# Newline-delimited, not arrays: bash 3.2 under `set -u` errors on "${arr[@]}"
# when the array is empty, and macOS ships bash 3.2. Same reason as the $VF below.
SURF_OK=""
SURF_BAD=""

say()  { printf '  %s\n' "$*"; }
head_() { printf '\n== %s\n' "$*"; }

shot() {
  STEP=$((STEP + 1))
  local name; name=$(printf '%02d-%s' "$STEP" "$1")
  # -R takes POINTS, which is exactly the unit axui reports, so unlike the ffmpeg
  # crop below this needs no scale arithmetic. Unquoted on purpose: the expansion
  # must split into two words, and a rect never contains a space.
  screencapture -x ${WIN_R:+-R "$WIN_R"} "$OUT/$name.png" 2>/dev/null
  if [ -s "$OUT/$name.png" ]; then say "shot $name.png"; else say "SHOT FAILED $name"; FAILED=1; fi
}

# Press an element by its accessibility label rather than a pixel guess, so the
# test survives layout changes. Never presses blind: axui refuses an ambiguous
# label instead of picking one, because a wrong press in a real app can do real
# damage.
#
# This does NOT use osascript. The UI is a WKWebView and System Events cannot see
# into it: `entire contents of front window` returns zero named elements while the
# tree is in fact fully populated. That silent emptiness is what made every
# surface report NOT FOUND on 2026-08-10. axui is a real AX client and sees it.
click_named() {
  local want="$1" out
  if ! out=$("$AXUI" "$APP_PID" press "$want" 2>&1); then
    say "NOT PRESSED: $want -- ${out#axui: }"
    FAILED=1
    return 1
  fi
  say "${out}"
  sleep 2
}

# Nothing here sends keystrokes. AXPress does not move focus, so a synthetic
# Escape goes to whatever already had it (the terminal textarea) and never
# reaches the overlay we just opened: both the help overlay and the worktree
# modal bind Escape and neither closed. Overlays are dismissed by pressing their
# own close control by identity instead.
#
# The sleep is load-bearing, not politeness. Closing a tab rebuilds the whole tab
# bar, so firing these back to back presses elements that are already detached:
# every call returned success and not one tab actually closed. Two seconds apart,
# all four close.
close_named() { "$AXUI" "$APP_PID" press "$1" >/dev/null 2>&1 && say "closed '$1'"; sleep 2; }

# Do not trust the press. A run reported all four tabs closed with three of them
# still on screen: AXPress returns success against an element the tab bar has
# already replaced. close-tab exits non-zero once no such tab exists, so that is
# the confirmation -- press, then look again, and say so loudly if it survives.
close_tab() {
  local t="$1" i
  for i in 1 2 3; do
    "$AXUI" "$APP_PID" close-tab "$t" >/dev/null 2>&1 || { say "tab '$t' closed"; return 0; }
    sleep 2
  done
  say "TAB STILL OPEN: $t"
  FAILED=1
}

preflight() {
  head_ "Preflight"
  local ok=0
  if [ -d "$APP" ]; then
    APP_VER=$(defaults read "$APP/Contents/Info.plist" CFBundleShortVersionString 2>/dev/null)
    say "app: $APP_VER"
  else
    say "MISSING: $APP"; ok=1
  fi
  command -v cliclick >/dev/null || { say "MISSING: cliclick (brew install cliclick)"; ok=1; }
  command -v ffmpeg  >/dev/null || { say "MISSING: ffmpeg"; ok=1; }

  # axui is ours and tiny, so build it rather than make the operator install it.
  if [ ! -x "$AXUI" ] || [ "$HERE/axui.c" -nt "$AXUI" ]; then
    if cc -O2 -o "$AXUI" "$HERE/axui.c" -framework ApplicationServices 2>/dev/null; then
      say "axui: built"
    else
      say "MISSING: axui, and $HERE/axui.c failed to compile"; ok=1
    fi
  else
    say "axui: present"
  fi

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
# Seconds precision on purpose: BSD date has no %N, and `date +%6N` prints the
# literal ".6N" while exiting 0, so an || fallback would never fire.
STARTED=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# Launch BEFORE recording, because the window has no frame until the app exists
# and the frame is what we frame the video on. Cost: the launch animation is no
# longer in the video. Shot 01 still captures the launched state, and a recording
# of the whole desktop was worth less than one of the app.
head_ "Launch"
open -a "$APP"; sleep 6
# The Mach-O is lowercase `xnaut`, so pgrep -x xNAUT finds nothing.
APP_PID=$(pgrep -x xnaut | head -1)
if [ -z "$APP_PID" ]; then say "APP DID NOT START"; FAILED=1; else say "pid $APP_PID"; fi

head_ "Recording to $OUT"
# The screen's device index is not fixed: a Mac with a camera puts the screen at
# 1, a headless mini puts it at 0. Hardcoding 1 gave "Invalid device index" and a
# zero-byte video while every other step reported success.
SCREEN=$(ffmpeg -f avfoundation -list_devices true -i "" 2>&1 |
         awk -F'[][]' '/Capture screen/{print $4; exit}')

# Frame the app, not the desktop. Two unit systems meet here and mixing them
# crops a video of empty wallpaper: AX and screencapture -R speak POINTS, while
# ffmpeg's crop filter speaks PIXELS. The ratio is not a constant to assume (1 on
# a plain display, 2 on Retina, something else again on a scaled mode), so
# measure it: capture a known 100pt square and see how many pixels come back.
CROPF=""
if [ -n "$APP_PID" ] && [ "${CROP:-1}" != "0" ]; then
  RECT=$("$AXUI" "$APP_PID" window 2>/dev/null)
  PROBE="${TMPDIR:-/tmp}/xnaut-scale.png"
  screencapture -x "$PROBE" 2>/dev/null
  PXW=$(sips -g pixelWidth  "$PROBE" 2>/dev/null | awk '/pixelWidth/{print $2}')
  PXH=$(sips -g pixelHeight "$PROBE" 2>/dev/null | awk '/pixelHeight/{print $2}')
  screencapture -x -R 0,0,100,100 "$PROBE" 2>/dev/null
  SCALE=$(sips -g pixelWidth "$PROBE" 2>/dev/null | awk '/pixelWidth/{print $2/100}')
  rm -f "$PROBE"

  if [ -n "$RECT" ] && [ -n "${PXW:-}" ] && [ -n "${SCALE:-}" ]; then
    WIN_R=$(echo "$RECT" | tr ' ' ',')
    # Pad, convert to pixels, clamp to the screen, and force every number even
    # because yuv420p cannot encode an odd width or height. Emit nothing if the
    # result is degenerate: a full-screen video beats a 4-pixel one.
    CROPF=$(echo "$RECT" | awk -v s="$SCALE" -v pw="$PXW" -v ph="$PXH" '{
      p = 8; x = ($1 - p) * s; y = ($2 - p) * s; w = ($3 + 2*p) * s; h = ($4 + 2*p) * s;
      if (x < 0) { w += x; x = 0 }
      if (y < 0) { h += y; y = 0 }
      if (x + w > pw) w = pw - x;
      if (y + h > ph) h = ph - y;
      x = int(x) - int(x) % 2; y = int(y) - int(y) % 2;
      w = int(w) - int(w) % 2; h = int(h) - int(h) % 2;
      if (w >= 160 && h >= 160) printf "crop=%d:%d:%d:%d", w, h, x, y;
    }')
  fi
fi
if [ -n "$CROPF" ]; then say "window ${WIN_R} pt, scale ${SCALE}x -> ${CROPF}"
else say "no window rect: recording the full screen"; WIN_R=""; fi

# Unquoted on purpose, same reason as in shot(): the filter must arrive as two
# words and contains no spaces. An empty array would be a cleaner idiom and
# breaks under bash 3.2 + set -u, which is what macOS ships.
VF=""; [ -n "$CROPF" ] && VF="-vf $CROPF"
ffmpeg -nostdin -loglevel error -f avfoundation -framerate 15 \
       -i "${SCREEN:-0}:none" $VF -pix_fmt yuv420p "$OUT/run.mp4" & REC=$!
trap 'kill $REC 2>/dev/null; wait $REC 2>/dev/null' EXIT
sleep 2

shot launch

# Read-only surfaces only. Deliberately excluded: New terminal (spawns a PTY),
# Start work log (mutates state), Open project repository in browser (leaves the
# app), and every tab/pane close button. A smoke test proves the UI responds; it
# must not change anything the operator then has to undo.
head_ "Walk the surfaces"
if [ -n "$APP_PID" ]; then
  for target in \
    "Toggle projects sidebar" \
    "Toggle project pane" \
    "Command snippets" \
    "Open new browser tab" \
    "Open new markdown tab" \
    "Open new diff tab" \
    "Open Projects (tasks & plan)" \
    "Open worktree manager" \
    "More actions" \
    "Help and keyboard shortcuts" \
    "Refresh usage"
  do
    if click_named "$target"; then
      shot "$(echo "$target" | tr '[:upper:] ' '[:lower:]-' | tr -cd 'a-z0-9-')"
      SURF_OK="$SURF_OK$target
"
    else
      SURF_BAD="$SURF_BAD$target
"
    fi
  done
fi

# A smoke test that leaves four tabs behind is a smoke test you can only run once
# before the evidence is buried under its own residue. Put the app back.
head_ "Clean up"
if [ -n "$APP_PID" ]; then
  # Help is a toggle, so the way to close it is to press the control that opened
  # it. The worktree modal is not: it has its own close button.
  close_named "Close worktree manager"
  close_named "Help and keyboard shortcuts"
  for t in Browser Markdown Diff Projects; do close_tab "$t"; done
  shot cleaned
fi

# The record the dashboard reads. Only two cases, and that is deliberate: this
# script knows whether a press landed, and nothing more. Whether the terminal
# actually ran a command, whether a surface that opened showed real content,
# whether a scenario was even reachable -- those need judgement, and only the
# agent driving the run has it. It appends report.md and may add cases and bugs.
# Guessing here would manufacture passes for things nobody looked at, which is
# exactly what the untested state exists to prevent.
emit_run_json() {
  # Quoted delimiter: an unquoted heredoc processes backslashes and would corrupt
  # any TASK containing them. json.dumps does the escaping.
  OUT="$OUT" STARTED="$STARTED" FINISHED="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  APP_VER="$APP_VER" APP_PID="$APP_PID" TASK="${TASK:-}" \
  SURF_OK="$SURF_OK" SURF_BAD="$SURF_BAD" \
  python3 - <<'PY'
import json, os, pathlib

out  = pathlib.Path(os.environ["OUT"])
ok   = [s for s in os.environ["SURF_OK"].splitlines()  if s]
bad  = [s for s in os.environ["SURF_BAD"].splitlines() if s]
launched = bool(os.environ["APP_PID"])

if bad:       surf = ("failed", f"{len(bad)} of {len(ok) + len(bad)} did not open: " + ", ".join(bad))
elif ok:      surf = ("passed", f"{len(ok)} surfaces opened")
else:         surf = ("untested", "app never started, nothing was pressed")

run = {
    "id": out.name,
    "host": os.uname().nodename.split(".")[0],
    "suite": "gui-smoke",
    "app_version": os.environ["APP_VER"],
    "started": os.environ["STARTED"],
    "finished": os.environ["FINISHED"],
    "task": os.environ["TASK"],
    "cases": [
        {"id": "launch", "title": "App launches and reports its version",
         "status": "passed" if launched else "failed",
         "shots": ["01-launch.png"]},
        # ponytail: no per-surface shot list; the dashboard renders every shot in
        # the run directory anyway, so mapping them back would be bookkeeping.
        {"id": "surfaces", "title": "Every top-level surface opens",
         "status": surf[0], "note": surf[1], "shots": []},
    ],
    "bugs": [],
}
(out / "run.json").write_text(json.dumps(run, indent=2) + "\n")
print(f"  run.json: {run['cases'][0]['status']} launch, {surf[0]} surfaces")
PY
}

head_ "Done"
sleep 2; kill $REC 2>/dev/null; wait $REC 2>/dev/null; trap - EXIT
emit_run_json
say "video:       $OUT/run.mp4 ($(du -h "$OUT/run.mp4" 2>/dev/null | cut -f1))"
say "screenshots: $(ls "$OUT"/*.png 2>/dev/null | wc -l | tr -d ' ')"
[ "$FAILED" -eq 0 ] && say "RESULT: every step succeeded" || say "RESULT: FAILURES above, see the shots"
exit $FAILED
