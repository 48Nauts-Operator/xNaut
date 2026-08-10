#!/usr/bin/env bash
# One turn of the develop -> test -> fix loop, driven from the dev machine.
#
#   scripts/tron-cycle.sh              build this worktree on tron, click it, bring the result home
#   scripts/tron-cycle.sh --installed  skip the build, click whatever is installed on tron
#   TASK="what changed" scripts/tron-cycle.sh
#
# The order this enforces is the whole point: build the code, click the thing you
# built, and only then consider a tag. v1.13.7 through v1.13.10 shipped in the
# other order -- tag, release, then test -- so the test could only ever confirm a
# regression users already had. Worse, tron had never built the app at all: every
# run before this script pointed at /Applications/xNAUT.app, so "tested before
# release" was testing the PREVIOUS release.
#
# What this does NOT do is file the issue. tron has no route to Forgejo (no
# tailscale, curl to cosmos:3000 times out), so the run comes home and the issue
# is filed from here, where the token lives. Putting a token on the test machine
# to save one hop is not a trade worth making.
#
# ponytail: git bundle over ssh rather than a shared remote. tron's only remote is
# the GitHub mirror, which dev branches must never touch.
set -uo pipefail

TRON="${TRON:-tron}"
# Resolve remotely once, so every later command can quote the path without
# worrying whether ~ or $HOME survives this shell, ssh's shell, and the quoting
# in between. It does not, reliably, and a path that silently fails to expand
# produces "no such file" about a file that is right there.
TRON_DIR="$(ssh "$TRON" "cd ${TRON_DIR:-\$HOME/xnaut} && pwd" 2>/dev/null)"
[ -n "$TRON_DIR" ] || { echo "cannot resolve checkout on $TRON"; exit 1; }
MODE=build
[ "${1:-}" = "--installed" ] && MODE=installed

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE" || exit 1
VERSION="$(grep -m1 '^version' src-tauri/Cargo.toml | cut -d'"' -f2)"
SHA="$(git rev-parse --short HEAD)"

say() { printf '\n== %s\n' "$*"; }

if [ "$MODE" = build ]; then
  say "Ship $SHA (v$VERSION) to $TRON"

  # Bundle only what tron is missing. The receiving side must fetch the ref name
  # the bundle actually carries: `git bundle create f A..HEAD` writes it as HEAD,
  # and asking for the branch name instead fails with "couldn't find remote ref".
  # That failure was invisible once already because the command was piped to
  # tail without pipefail on the far side, so the && chain sailed past it.
  TRON_HEAD="$(ssh "$TRON" "cd $TRON_DIR && git rev-parse HEAD" 2>/dev/null)"
  BUNDLE="$(mktemp -t xnaut-cycle).bundle"
  if [ -n "$TRON_HEAD" ] && git cat-file -e "$TRON_HEAD^{commit}" 2>/dev/null; then
    git bundle create "$BUNDLE" "$TRON_HEAD..HEAD" 2>/dev/null
  else
    echo "  tron's HEAD is unknown here — sending full history"
    git bundle create "$BUNDLE" HEAD 2>/dev/null
  fi
  [ -s "$BUNDLE" ] || { echo "  nothing to send (tron already has $SHA)"; }

  if [ -s "$BUNDLE" ]; then
    scp -q "$BUNDLE" "$TRON:/tmp/xnaut-cycle.bundle" || exit 1
    ssh "$TRON" "set -e; cd $TRON_DIR
      git fetch -q /tmp/xnaut-cycle.bundle HEAD
      git reset -q --hard FETCH_HEAD" || exit 1
  fi
  rm -f "$BUNDLE"
  echo "  tron now at $(ssh "$TRON" "cd $TRON_DIR && git rev-parse --short HEAD")"

  say "Build on $TRON"
  APP="$TRON_DIR/src-tauri/target/release/bundle/macos/xNAUT.app"

  # `--bundles app` because a test build needs the .app and nothing else. The
  # full build also produces a DMG and an updater tarball, and the updater step
  # then fails for want of TAURI_SIGNING_PRIVATE_KEY -- a key that lives in CI
  # secrets and has no business on a test machine. That failure comes AFTER the
  # .app is written, so the exit code says "failed" about an app that built fine.
  #
  # Which is why the success test below is the artifact, not the exit code:
  # delete the bundle first, then require it to exist afterwards. Trusting an
  # exit code that can be nonzero on success (or, piped, zero on failure) is the
  # exact bug class this whole loop exists to kill.
  #
  # Log to a file rather than piping to tail: a pipe hides the real status behind
  # tail's, and ${PIPESTATUS[0]} is no guard here -- tron's login shell is zsh,
  # where the array is $pipestatus and PIPESTATUS expands to nothing.
  # Delete the binary too, not just the bundle. The frontend is EMBEDDED in the
  # executable, and cargo has no idea src/js/app.js changed -- no Rust file moved,
  # so it skips the link step and re-bundles the previous build's assets. The .app
  # is then freshly stamped with the right version and the wrong frontend. Cheap
  # insurance: a rebuild costs minutes, a stale frontend costs a whole cycle.
  #
  # What this is NOT is the cause of the blank Settings panes in cycles 1-3. I
  # wrote that here and it was wrong twice over. The evidence cited --
  # `strings <binary> | grep -c 'settings pane'` -> 0 -- proves nothing, because
  # the embedded frontend is COMPRESSED: the control string `loadSettingsSection`
  # scores 0 in the same binary. Live AX probes on tron then found
  # `AXGroup  AI settings pane` in the tree and all seven markers passing, so the
  # binary tron built had been correct the whole time.
  #
  # The real cause was in the harness: the surfaces loop left the "More actions"
  # menu open, and the Settings walk pressed that same button again, toggling the
  # menu shut before asking it for "xNAUT settings". Fixed in gui-smoke.sh, which
  # now also photographs the modal -- the missing picture is why seven shots of
  # an Observatory tab read as seven blank panes for three cycles running.
  ssh "$TRON" "rm -rf '$APP' '$TRON_DIR/src-tauri/target/release/xnaut'"
  ssh "$TRON" "cd $TRON_DIR/src-tauri && PATH=\$HOME/.cargo/bin:\$PATH cargo tauri build --bundles app > /tmp/xnaut-build.log 2>&1"
  RC=$?
  if ! ssh "$TRON" "test -x '$APP/Contents/MacOS/xnaut'"; then
    echo "  BUILD FAILED (rc=$RC) — no app to click. Last 25 lines:"
    ssh "$TRON" 'tail -25 /tmp/xnaut-build.log'
    echo "  Full log: $TRON:/tmp/xnaut-build.log"
    exit 1
  fi
  [ $RC -ne 0 ] && echo "  note: tauri exited $RC after bundling the app (see log); the app is present, continuing"
  ssh "$TRON" "/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' '$APP/Contents/Info.plist' 2>/dev/null | sed 's/^/  built version: /'"

  # The click tests are a baseline, not the test suite. Twenty modules carry
  # #[cfg(test)] blocks -- including designer_local.rs and nautloom.rs, the two
  # nobody could reach through the GUI at all -- and until now not one of them ran
  # anywhere in this loop. A walk that presses every button in an app whose logic
  # is broken still comes back green, because pressing a button is not the claim.
  #
  # Before the walk, not after: these are seconds against a GUI walk's minutes, and
  # there is no point photographing an app whose unit tests already say it is wrong.
  say "Run the unit tests on $TRON"
  if ssh "$TRON" "cd $TRON_DIR/src-tauri && PATH=\$HOME/.cargo/bin:\$PATH cargo test --bin xnaut > /tmp/xnaut-test.log 2>&1"; then
    ssh "$TRON" "grep -E '^test result:' /tmp/xnaut-test.log | sed 's/^/  /'"
  else
    echo "  TESTS FAILED — not clicking a build that fails its own suite."
    ssh "$TRON" "grep -E '^(failures:|test result:|---- )' /tmp/xnaut-test.log | head -30 | sed 's/^/  /'"
    echo "  Full log: $TRON:/tmp/xnaut-test.log"
    exit 1
  fi
else
  say "Click the installed app on $TRON (no build)"
  APP="/Applications/xNAUT.app"
fi

say "Walk the GUI"
# gui-smoke.sh self-elevates for TCC (Accessibility is granted to a bundle, and
# an ssh session is not one). Never wrap it in tcc-run: the two nest onto the
# same $D/cmd.sh and deadlock.
ssh "$TRON" "cd $TRON_DIR && APP='$APP' TASK='${TASK:-}' ./scripts/gui-smoke.sh" 2>&1 | tail -40

say "Bring the run home"
RUN="$(ssh "$TRON" 'ls -1dt $HOME/xnaut-testing/runs/tron/*/ 2>/dev/null | head -1')"
if [ -z "$RUN" ]; then echo "  no run directory on tron — the walk never started"; exit 1; fi
mkdir -p "$HOME/xnaut-testing/runs/tron"
rsync -a "$TRON:$RUN" "$HOME/xnaut-testing/runs/tron/$(basename "$RUN")/" || exit 1
LOCAL="$HOME/xnaut-testing/runs/tron/$(basename "$RUN")"
echo "  $LOCAL"

say "Verdict"
# The same gate the tag has to pass, run now so a red cycle is obvious here
# rather than at release time.
"$HERE/scripts/release-gate.sh" "$VERSION" HEAD
GATE=$?
if [ $GATE -ne 0 ]; then
  echo
  echo "  Not releasable. File the incident from here (tron has no Forgejo route):"
  echo "    scripts/report-issue.mjs $LOCAL"
fi
exit $GATE
