#!/bin/bash
# Executed before EVERY repository task, including a fresh worker. Package
# managers arbitrate concurrent installs; already prepared workers do no work.
set -eu
# Serialize cold installs on shared workers. flock is part of the supported
# Linux base images; package-manager locks remain the fallback elsewhere.
if command -v flock >/dev/null; then
  mkdir -p "$HOME/.local/state/xnaut"
  # Provider login shells may wrap `exec` in a function. A redirection on that
  # function closes when it returns. `command exec` bypasses the function AND
  # preserves exec's persistent-redirection semantics (`builtin exec` does not).
  command exec 9>"$HOME/.local/state/xnaut/tool-install.lock"
  flock -w 540 9 || { echo XNAUT_BOOTSTRAP_INSTALL_FAILED; exit 21; }
fi
ready() {
  command -v python3 >/dev/null && python3 -c 'import sys; sys.exit(sys.version_info < (3, 9))' && command -v git >/dev/null &&
  git lfs version >/dev/null 2>&1 && command -v ssh-keygen >/dev/null &&
  command -v ssh >/dev/null && command -v tmux >/dev/null && command -v curl >/dev/null
}
if ! ready; then
  if [ "$(id -u)" = 0 ]; then elevate=();
  elif sudo -n true 2>/dev/null; then elevate=(sudo -n);
  else echo XNAUT_BOOTSTRAP_INSTALL_PERMISSION; exit 20; fi
  if command -v apt-get >/dev/null; then
    "${elevate[@]}" env DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=180 update -qq >/dev/null 2>&1 &&
    "${elevate[@]}" env DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=180 install -y -qq python3 git git-lfs openssh-client tmux curl ca-certificates >/dev/null 2>&1 || { echo XNAUT_BOOTSTRAP_INSTALL_FAILED; exit 21; }
  elif command -v dnf >/dev/null; then
    "${elevate[@]}" dnf install -y python3 git git-lfs openssh-clients tmux curl ca-certificates >/dev/null 2>&1 || { echo XNAUT_BOOTSTRAP_INSTALL_FAILED; exit 21; }
  elif command -v apk >/dev/null; then
    "${elevate[@]}" apk add python3 git git-lfs openssh-client tmux curl ca-certificates >/dev/null 2>&1 || { echo XNAUT_BOOTSTRAP_INSTALL_FAILED; exit 21; }
  else echo XNAUT_BOOTSTRAP_UNSUPPORTED_OS; exit 22; fi
fi
ready || { echo XNAUT_BOOTSTRAP_TOOLS_MISSING; exit 23; }
echo XNAUT_BOOTSTRAP_TOOLS_READY
