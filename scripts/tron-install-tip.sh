#!/usr/bin/env bash
# Put the current dev build in front of the fleet on tron.
#
# The binary is whatever the last build on tron produced. This script only stops the supervisor, swaps the binary into the
# installed bundle, re-signs it ad-hoc and starts it again. Stopping a running
# app and replacing a signed binary are both refused to an agent, which is why
# this is a script for you rather than something already done.
#
#   scripts/tron-install-tip.sh            swap in what is already built
#   BUILD=1 scripts/tron-install-tip.sh    rebuild on tron first
#
# Safe to run when tron is idle. Check first:
#   ssh tron 'zellij list-sessions'      -> no active sessions
# A restart during a review or an integration build is survivable as of
# 0697cb9, which is part of what this deploys, but the running build does not
# have those protections yet. So: idle, then swap.
set -euo pipefail

TRON="${TRON:-tron}"
APP='$HOME/Applications/xnaut-tip/xNAUT.app'
SRC='$HOME/DevHub_Studio/factory/02-Development/xnaut'

if [ "${BUILD:-0}" = "1" ]; then
  echo "== building on $TRON"
  ssh "$TRON" "cd $SRC/src-tauri && export PATH=\$HOME/.cargo/bin:/opt/homebrew/bin:\$PATH && cargo build --release" 2>&1 | tail -3
fi

echo "== what is running now"
ssh "$TRON" 'pgrep -x xnaut | sed "s/^/  pid /" || echo "  (not running)"'
# "No active zellij sessions found." is a message, not a session; a guard
# that counts lines must drop it or it refuses to swap on an idle machine.
ssh "$TRON" "zsh -lc 'zellij list-sessions 2>&1 | grep -v EXITED | grep -v \"No active\"' | sed 's/^/  /'"

read -r -p "stop it and swap the binary? [y/N] " ok
[ "$ok" = "y" ] || { echo "nothing done"; exit 0; }

ssh "$TRON" "
  set -e
  APP=$APP
  NEW=$SRC/src-tauri/target/release/xnaut
  [ -x \"\$NEW\" ] || { echo 'no release binary; run with BUILD=1'; exit 1; }
  echo \"  old \$(stat -f '%z bytes  %Sm' \"\$APP/Contents/MacOS/xnaut\")\"
  echo \"  new \$(stat -f '%z bytes  %Sm' \"\$NEW\")\"
  osascript -e 'tell application \"xNAUT\" to quit' 2>/dev/null || true
  for i in \$(seq 1 15); do pgrep -x xnaut >/dev/null || break; sleep 1; done
  pgrep -x xnaut >/dev/null && kill \$(pgrep -x xnaut) && sleep 3 || true
  pgrep -x xnaut >/dev/null && { echo '  still running, stopping here'; exit 1; } || echo '  stopped'
  cp \"\$NEW\" \"\$APP/Contents/MacOS/xnaut\"
  codesign --force --deep --sign - \"\$APP\" >/dev/null 2>&1 || true
  # tron's supervisor is a launchd job. On 2026-09-11 an 'open' here started
  # a SECOND copy beside it: two sweeps, two orphan reapers, one registry, and
  # every verify re-queued every two minutes by the process that could not see
  # it. Restart the job; never open the bundle by hand on a machine that has one.
  if launchctl print gui/\$(id -u)/com.48nauts.xnaut-supervisor >/dev/null 2>&1; then
    launchctl kickstart -k gui/\$(id -u)/com.48nauts.xnaut-supervisor
  else
    open \"\$APP\"
  fi
  sleep 15
  n=\$(pgrep -x xnaut | wc -l | tr -d ' ')
  pgrep -x xnaut | sed 's/^/  pid /'
  [ \"\$n\" = 1 ] || echo \"  WARNING: \$n xnaut processes; expected exactly one\"
"

echo "== what it now has"
ssh "$TRON" "strings '$APP/Contents/MacOS/xnaut' 2>/dev/null | grep -c 'something outside this merge' | sed 's/^/  verifier bound present: /'"
