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

# Set by preflight when Screen Recording is not granted. The walk still runs: the
# verdict is the AX marker assertions, which need Accessibility only.
NO_CAPTURE=0

# Write where the dashboard reads. These were two different directories until
# 2026-08-10: runs landed in ~/xnaut-gui-smoke/<ts>/ and scripts/testing-report.mjs
# read ~/xnaut-testing/runs/<host>/<id>/, so eleven runs on tron were invisible
# and the dashboard sat three releases behind while looking healthy. The path is
# the contract; keep the two in step.
OUT="${OUT:-$HOME/xnaut-testing/runs/$(hostname -s)/$(date +%Y%m%d-%H%M%S)}"
# Overridable so a build can be tested before it is a release. The default is
# the installed app because that is the right target for a post-release check;
# it is the wrong target for a pre-release one, and being hardcoded meant only
# the post-release check was ever possible.
#   APP=target/release/bundle/macos/xNAUT.app scripts/gui-smoke.sh
APP="${APP:-/Applications/xNAUT.app}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AXUI="${AXUI:-$HERE/.axui}"

# TCC grants Accessibility and Screen Recording to a *bundle*, pinned by cdhash,
# and an ssh session is not one. So a walk started over ssh presses nothing: on
# 2026-08-10 all twelve controls on tron came back "not a trusted AX client"
# while the machine itself had been granted for weeks. The fix is a checkbox in
# System Settings that nobody can click over ssh.
#
# tcc-run re-runs us inside a granted stub bundle, whose children inherit the
# grants as their responsible process. Doing it here rather than in the caller
# means the skill, the cron and a human all get a working run from the same
# command; forgetting the wrapper was the whole failure.
if [ -z "${TCC_RUN_INSIDE:-}" ] && [ -x "$HOME/bin/tcc-run" ] \
   && ! "$AXUI" $$ window >/dev/null 2>&1; then
  echo "not AX-trusted from here; re-running under tcc-run" >&2
  exec "$HOME/bin/tcc-run" env TCC_RUN_INSIDE=1 \
    OUT="$OUT" APP="$APP" ATTACH="${ATTACH:-}" CROP="${CROP:-}" TASK="${TASK:-}" \
    bash "$HERE/$(basename "${BASH_SOURCE[0]}")" "$@"
fi
STEP=0
FAILED=0
APP_PID=""
APP_VER=""
STARTED=""
WIN_R=""   # "x,y,w,h" in points once the window is known; empty means full screen
# Newline-delimited, not arrays: bash 3.2 under `set -u` errors on "${arr[@]}"
# when the array is empty, and macOS ships bash 3.2. Same reason as the $VF below.
SURF_OK=""       # pressed (superset of verified + unverified)
SURF_BAD=""      # would not press at all
SURF_VERIF=""    # pressed AND the surface's marker appeared: the only real pass
SURF_UNVERIF=""  # pressed, but nothing in the AX tree distinguishes this surface
SURF_EMPTY=""    # pressed, and the surface stayed blank: the bug class we missed

say()  { printf '  %s\n' "$*"; }
head_() { printf '\n== %s\n' "$*"; }

shot() {
  STEP=$((STEP + 1))
  local name; name=$(printf '%02d-%s' "$STEP" "$1")
  # No grant, no image, and that is not a failure of the app under test.
  [ "$NO_CAPTURE" = 1 ] && return 0
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
# Presses by EXACT label. Substring is axui's default because an operator types
# enough of a label to be unambiguous; a script is never in that position, it
# knows every name in full. Matching loosely from here only invents collisions:
# "Settings" inside "Open Settings" cost the whole Settings walk in 1.13.8, and
# "AI settings" inside the AI pane's own "Save AI Settings" button cost the AI
# section in three consecutive runs after that. Both were unpressable for the
# same reason and neither name was ever actually ambiguous.
#
# Exact is also case-sensitive, so a needle that drifts from the real label now
# fails loudly as NOT PRESSED instead of quietly pressing a neighbour.
click_named() {
  local want="$1" out
  if ! out=$("$AXUI" "$APP_PID" press -x "$want" 2>&1); then
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

# Press a control, then prove the surface it opens actually rendered.
#
# click_named reports only that AXPress was accepted, which is a weaker claim
# than it reads as. A surface can take the press and render nothing, and that
# is precisely the failure this tier exists to catch: "a surface that opens but
# is empty when it should not be is also a failure" (tests/features/smoke.feature).
# Until now the script recorded a press as a pass, so the whole walk was a
# reachability check wearing a test's clothes.
#
# The marker is a label present in the AX tree only once the surface is up, and
# it is matched exactly. Substring matching manufactured five false failures on
# 2026-08-10: the marker for an open Browser tab is the tab title "Browser",
# which is also a substring of the button "Open new browser tab" that never
# leaves the screen, so no needle was both stable and honest.
#
# Markers are read off the AX tree in the two states, not out of
# tests/control-inventory.json -- that file is generated from the web DOM by
# tests/enumerate-controls.mjs and the two trees disagree on names (the DOM
# calls the tab "Browser x", AX calls it "Browser" next to a separate "x"
# button). Asserting DOM names against AX is what broke.
#
# An empty marker means the inventory found nothing that distinguishes the
# surface. That is a real gap and it is recorded as one: the seven Settings
# sections share a single nav rail and differ only inside a pane the AX tree
# does not expose, so pressing them proves the click landed and nothing more.
# Those record "unverified" and never "passed". Picking a marker they happen to
# share would manufacture the green, which is the failure mode this whole
# change exists to remove.
#
# Returns 0 verified, 1 not pressed, 2 pressed with nothing to assert on,
# 3 pressed but the surface stayed empty.
opened_named() {
  local target="$1" marker="${2:-}"
  click_named "$target" || return 1
  [ -z "$marker" ] && { say "  pressed; no marker exists to verify it"; return 2; }
  if "$AXUI" "$APP_PID" list -x "$marker" >/dev/null 2>&1; then
    say "  verified: '$marker' is on screen"
    return 0
  fi
  say "  OPENED BUT EMPTY: '$target' was pressed, '$marker' never appeared"
  FAILED=1
  return 3
}

# A toggle does not open a surface, it flips one, and the walk does not own the
# state it starts in. "Toggle project pane" passed one run and failed the next
# with no code change between them, purely because an earlier probe had left the
# pane open, so the press closed it. Asserting that the marker CHANGED is true
# whichever way it started, which is the only honest thing to assert about a
# control whose starting state is somebody else's.
#
# Same return codes as opened_named.
toggled_named() {
  local target="$1" marker="$2" before after
  "$AXUI" "$APP_PID" list -x "$marker" >/dev/null 2>&1 && before=1 || before=0
  click_named "$target" || return 1
  "$AXUI" "$APP_PID" list -x "$marker" >/dev/null 2>&1 && after=1 || after=0
  if [ "$before" != "$after" ]; then
    [ "$after" = 1 ] && say "  verified: '$marker' appeared" || say "  verified: '$marker' disappeared"
    return 0
  fi
  [ "$after" = 1 ] && say "  NOTHING TOGGLED: '$target' was pressed, '$marker' stayed on screen" \
                   || say "  NOTHING TOGGLED: '$target' was pressed, '$marker' never showed up"
  FAILED=1
  return 3
}

# Do not trust the press. A run reported all four tabs closed with three of them
# still on screen: AXPress returns success against an element the tab bar has
# already replaced. close-tab exits non-zero once no such tab exists, so that is
# the confirmation -- press, then look again, and say so loudly if it survives.
#
# The retry loop declared failure without looking once more, so a tab that closed
# on the third press was reported STILL OPEN -- run 20260810-185000 said exactly
# that about a Browser tab that was already gone. The final check is an exact
# label lookup rather than a fourth close-tab, so it observes instead of acting
# and cannot move the thing it is measuring.
close_tab() {
  local t="$1" i
  for i in 1 2 3; do
    "$AXUI" "$APP_PID" close-tab "$t" >/dev/null 2>&1 || { say "tab '$t' closed"; return 0; }
    sleep 2
  done
  "$AXUI" "$APP_PID" list -x "$t" >/dev/null 2>&1 || { say "tab '$t' closed"; return 0; }
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
  # Missing Screen Recording is a warning, not a stop. The verdict comes from the
  # AX marker assertions, which need Accessibility only; the images are there so a
  # human can see what the machine already decided. Granting this is a click in
  # System Settings that nobody can do over ssh, and blocking the whole walk on it
  # means a remote machine runs no tests at all rather than most of them. The run
  # record says `markers-only` so no reader assumes screenshots exist.
  local probe="${TMPDIR:-/tmp}/xnaut-probe.png"
  screencapture -x "$probe" 2>/dev/null
  if [ -s "$probe" ]; then say "screen recording: granted"; else
    say "no screen recording: markers only, no screenshots or video"
    NO_CAPTURE=1; fi
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
# ATTACH=1 tests whatever xnaut is already running instead of launching one.
#
# That is what makes a dev loop possible. Until now this script only ever drove
# /Applications/xNAUT.app, so the only thing it could test was a build that had
# already been tagged, built by CI, published and installed -- the fix was in
# production before anything checked it. The Settings surface failed on 1.13.8,
# failed identically on 1.13.9, and shipped both times, because the test ran
# downstream of the release and had no way to run anywhere else.
#
# With ATTACH=1 the same walk runs against `cargo tauri dev`: edit, rebuild,
# re-run the smoke, see the result before the tag exists.
#
#   cd src-tauri && cargo tauri dev &        # leave it running
#   ATTACH=1 scripts/gui-smoke.sh
#
# Attaching deliberately does not quit the app at the end (see Clean up): the
# dev process owns its lifetime, and killing it would end the loop after one
# iteration.
if [ "${ATTACH:-0}" != "0" ]; then
  # ATTACH=1 picks the running dev build; ATTACH=<pid> names one explicitly.
  #
  # It refuses to attach to /Applications/xNAUT.app, and that refusal is the
  # whole point rather than a nicety. This walk opens tabs, presses Settings and
  # closes things; run against the app André has on screen it would type into his
  # session. `pgrep | head -1` cheerfully returns exactly that app, because in
  # practice it is the one that is always running. A test harness that can
  # commandeer the user's live window is a worse bug than any it would find.
  APP_BIN=""
  if [ "$ATTACH" != "1" ]; then
    APP_PID="$ATTACH"
    kill -0 "$APP_PID" 2>/dev/null || { say "ATTACH=$ATTACH: no such process"; FAILED=1; APP_PID=""; }
    APP_BIN=$(lsof -p "$APP_PID" 2>/dev/null | awk '/ txt / && /xnaut/ {print $NF; exit}')
  else
    APP_PID=""
    for p in $(pgrep -x xnaut); do
      # The executable path distinguishes a dev build from the installed one.
      APP_BIN=$(lsof -p "$p" 2>/dev/null | awk '/ txt / && /xnaut/ {print $NF; exit}')
      case "$APP_BIN" in
        /Applications/*) say "skipping pid $p: that is the installed app, not a dev build" ;;
        # An unresolvable path is not permission to guess: the one process this
        # must never drive is the likeliest thing behind an lsof that came back
        # empty. Name it with ATTACH=<pid> if it really is the dev build.
        "")              say "skipping pid $p: cannot tell which build this is" ;;
        *) APP_PID="$p"; break ;;
      esac
    done
    [ -z "$APP_PID" ] && {
      say "ATTACH=1 found no dev build running -- start \`cargo tauri dev\`,"
      say "or name a pid explicitly with ATTACH=<pid> if you meant the installed app"
      FAILED=1
    }
  fi
  # Preflight read the version off the *installed* bundle, which under ATTACH is
  # not the build under test: the first attached run on tron reported 1.13.9
  # while the app on screen said 1.13.10. A report that names the wrong version
  # is worse than one that names none, because nobody doubts it.
  if [ -n "$APP_PID" ]; then
    say "attached to pid $APP_PID (not launched by this run)"
    APP_VER="dev"
    # Walk up from the binary looking for the Cargo.toml that built it. A fixed
    # ../../src-tauri/Cargo.toml guessed the layout wrong -- target/ lives INSIDE
    # src-tauri, so it resolved to src-tauri/src-tauri and run 20260810-185000
    # reported "dev" with no version at all. Walking cannot be wrong about a
    # layout it reads instead of assumes.
    local_toml="$(dirname "$APP_BIN")"
    for _ in 1 2 3 4 5; do
      [ -f "$local_toml/Cargo.toml" ] && break
      local_toml="$local_toml/.."
    done
    [ -f "$local_toml/Cargo.toml" ] \
      && APP_VER="$(awk -F'"' '/^version/{print $2; exit}' "$local_toml/Cargo.toml")-dev"
    say "version: $APP_VER"
  fi
else
  open -a "$APP"; sleep 6
  # The Mach-O is lowercase `xnaut`, so pgrep -x xNAUT finds nothing.
  APP_PID=$(pgrep -x xnaut | head -1)
  if [ -z "$APP_PID" ]; then say "APP DID NOT START"; FAILED=1; else say "pid $APP_PID"; fi
fi

head_ "Recording to $OUT"
# Without Screen Recording there is nothing to crop and nothing to record; the
# window rect is only ever used to frame an image.
[ "$NO_CAPTURE" = 1 ] && CROP=0
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
REC=""
if [ "$NO_CAPTURE" = 1 ]; then
  say "no video: screen recording not granted"
else
  ffmpeg -nostdin -loglevel error -f avfoundation -framerate 15 \
         -i "${SCREEN:-0}:none" $VF -pix_fmt yuv420p "$OUT/run.mp4" & REC=$!
  trap 'kill $REC 2>/dev/null; wait $REC 2>/dev/null' EXIT
  sleep 2
fi

shot launch

# Read-only surfaces only. Deliberately excluded: New terminal (spawns a PTY),
# Start work log (mutates state), Open project repository in browser (leaves the
# app), and every tab/pane close button. A smoke test proves the UI responds; it
# must not change anything the operator then has to undo.
head_ "Walk the surfaces"
if [ -n "$APP_PID" ]; then
  # "control|marker". The marker after the pipe is the exact AX label that must
  # appear on screen for the press to count as an opened surface; empty means
  # nothing distinguishes this one and the result is unverified, not passed.
  #
  # A marker prefixed "~" is a toggle: the walk does not own which way it starts,
  # so the assertion is that the marker changed, not that it appeared. Asserting
  # appearance made these two a coin flip -- "Add project" passed every run
  # whichever way the sidebar was sitting, and "Workspace" failed a run only
  # because an earlier probe had left the pane open.
  for pair in \
    "Toggle projects sidebar|~Add project" \
    "Toggle project pane|~Workspace" \
    "Command snippets|Command Snippets" \
    "Open new browser tab|Browser" \
    "Open new markdown tab|Markdown" \
    "Open new diff tab|Diff" \
    "Open Projects (tasks & plan)|Project filter" \
    "Open worktree manager|" \
    "More actions|Knowledge Graph" \
    "Help and keyboard shortcuts|Close help" \
    "Refresh usage|"
  do
    target="${pair%%|*}"; marker="${pair#*|}"
    case "$marker" in
      "~"*) toggled_named "$target" "${marker#\~}" ;;
      *)    opened_named "$target" "$marker" ;;
    esac; rc=$?
    [ $rc -ne 1 ] && shot "$(echo "$target" | tr '[:upper:] ' '[:lower:]-' | tr -cd 'a-z0-9-')"
    case $rc in
      0) SURF_OK="$SURF_OK$target
"; SURF_VERIF="$SURF_VERIF$target
" ;;
      2) SURF_OK="$SURF_OK$target
"; SURF_UNVERIF="$SURF_UNVERIF$target
" ;;
      3) SURF_EMPTY="$SURF_EMPTY$target
" ;;
      *) SURF_BAD="$SURF_BAD$target
" ;;
    esac
  done
fi

# The largest untested block in the app: 44 controls across five sections, and
# the place a user changes behaviour irreversibly. Switching sections only
# re-renders the right-hand pane, so the whole walk is read-only; nothing is
# saved unless Save is pressed, and Save is not pressed.
#
# These were unreachable until 2026-08-10, and not because the harness was weak:
# .settings-nav-item was a bare div, so it mapped to AXGroup and axui refuses to
# press one. That was an accessibility defect (no keyboard stop, no accessible
# name, invisible to a screen reader) and it was fixed in the product. The
# aria-labels are what make each section addressable: the visible "AI" is a
# substring of "Explain Screen", and axui refuses an ambiguous label.
head_ "Walk the Settings sections"
if [ -n "$APP_PID" ]; then
  if click_named "More actions" && opened_named "xNAUT settings" "Close settings"; then
    SURF_OK="${SURF_OK}xNAUT settings
"
    SURF_VERIF="${SURF_VERIF}xNAUT settings
"
    # No markers here, and that is the finding rather than an oversight. All
    # seven sections expose the same nav rail to the AX tree and differ only in
    # a right-hand pane it does not reach, so the enumerator sees nothing that
    # tells them apart. Pressing one proves the click landed; it cannot prove
    # the pane changed. They are recorded unverified until either the panes get
    # accessible names or the harness learns to read them.
    for sect in \
      "AI settings" \
      "Tasks Mode settings" \
      "Appearance settings" \
      "Keyboard Shortcuts settings" \
      "Mobile settings" \
      "Nautify settings" \
      "Triggers settings"
    do
      if click_named "$sect"; then
        shot "settings-$(echo "$sect" | tr '[:upper:] ' '[:lower:]-' | tr -cd 'a-z0-9-')"
        SURF_OK="$SURF_OK$sect
"
        SURF_UNVERIF="$SURF_UNVERIF$sect
"
      else
        SURF_BAD="$SURF_BAD$sect
"
      fi
    done
    close_named "Close settings"
  else
    # The panel never opened, so the seven sections were not tested rather than
    # failed. Only the thing that was actually pressed gets recorded.
    SURF_BAD="${SURF_BAD}xNAUT settings
"
  fi
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
  SURF_VERIF="$SURF_VERIF" SURF_UNVERIF="$SURF_UNVERIF" SURF_EMPTY="$SURF_EMPTY" \
  NO_CAPTURE="$NO_CAPTURE" \
  python3 - <<'PY'
import json, os, pathlib, re

lines = lambda k: [s for s in os.environ.get(k, "").splitlines() if s]

out      = pathlib.Path(os.environ["OUT"])
ok       = lines("SURF_OK")
bad      = lines("SURF_BAD")
verified = lines("SURF_VERIF")
unverif  = lines("SURF_UNVERIF")
empty    = lines("SURF_EMPTY")
launched = bool(os.environ["APP_PID"])

# "Pressed" is not "opened". Only the verified ones are a pass; the unverified
# ones are counted separately and named, because a number that folds them in is
# the number that let a broken Settings walk read as green for two releases.
total = len(verified) + len(unverif) + len(bad) + len(empty)
if bad or empty:
    parts = []
    if bad:   parts.append(f"{len(bad)} did not open: " + ", ".join(bad))
    if empty: parts.append(f"{len(empty)} opened blank: " + ", ".join(empty))
    surf = ("failed", f"of {total} surfaces, " + "; ".join(parts))
elif verified or unverif:
    surf = ("passed" if not unverif else "partial",
            f"{len(verified)} of {total} verified"
            + (f"; {len(unverif)} pressed but unverifiable: " + ", ".join(unverif) if unverif else ""))
else:
    surf = ("untested", "app never started, nothing was pressed")

# A failure that repeats is a different, worse fact than a failure that appears.
# The Settings surface failed identically on 1.13.8 and 1.13.9 and shipped both
# times: each run stated it calmly, in isolation, and nothing compared them. So
# compare them here. The previous run of this suite on this host is one
# directory over, and the whole check is a set equality.
def previous_failures():
    sibs = sorted(p for p in out.parent.iterdir() if p.is_dir() and p.name != out.name)
    for p in reversed(sibs):
        try:
            prev = json.loads((p / "run.json").read_text())
        except (OSError, ValueError):
            continue
        if prev.get("suite") != "gui-smoke":
            continue
        for c in prev.get("cases", []):
            if c["id"] == "surfaces" and c["status"] == "failed":
                return p.name, prev.get("app_version", "?"), c.get("note", "")
        return None          # the previous run passed; nothing is repeating
    return None

repeat = None
if bad or empty:
    prev = previous_failures()
    now_bad = set(bad) | set(empty)
    if prev:
        # Compare the named surfaces, not the note text: the counts move as the
        # app grows, the names are what actually recurred. Split on ":" as well
        # as "," and ";" -- the note reads "1 did not open: Settings", so the
        # first name is glued to the prefix and a comma-only split never sees it.
        prev_bad = {n.strip() for n in re.split(r"[,;:]", prev[2]) if n.strip() in now_bad}
        if prev_bad and prev_bad == now_bad:
            repeat = (f"UNCHANGED since {prev[0]} (v{prev[1]}): the same surfaces failed "
                      f"in the previous run and the release shipped anyway -- "
                      + ", ".join(sorted(now_bad)))
            surf = ("failed", surf[1] + " | " + repeat)

run = {
    "id": out.name,
    "host": os.uname().nodename.split(".")[0],
    "suite": "gui-smoke",
    "app_version": os.environ["APP_VER"],
    "started": os.environ["STARTED"],
    "finished": os.environ["FINISHED"],
    "task": os.environ["TASK"],
    # A reader who sees no images should learn why here rather than assume the
    # run crashed before it took any.
    "evidence": "markers-only" if os.environ.get("NO_CAPTURE") == "1" else "screenshots+video",
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
sleep 2
[ -n "$REC" ] && { kill $REC 2>/dev/null; wait $REC 2>/dev/null; trap - EXIT; }
emit_run_json
if [ "$NO_CAPTURE" = 1 ]; then
  say "evidence:    markers only (no Screen Recording grant on this machine)"
else
  say "video:       $OUT/run.mp4 ($(du -h "$OUT/run.mp4" 2>/dev/null | cut -f1))"
  say "screenshots: $(ls "$OUT"/*.png 2>/dev/null | wc -l | tr -d ' ')"
fi
[ "$FAILED" -eq 0 ] && say "RESULT: every step succeeded" || say "RESULT: FAILURES above, see the shots"
exit $FAILED
