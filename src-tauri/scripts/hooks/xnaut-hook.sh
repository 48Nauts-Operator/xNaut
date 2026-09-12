#!/bin/sh
# xnaut-hook.sh — report an agent status to the xNAUT hook listener.
# usage: xnaut-hook.sh <state>
#   state: working|blocked|waiting|done|idle|permission|interrupted (default: done)
#
# Contract (see agent_hooks.rs): POST to $XNAUT_HOOK_URL with header
# X-Xnaut-Session: $XNAUT_HOOK_TOKEN and body {"state":"<state>","cwd":"<pwd>"}.
# Both env vars are injected into the agent process at launch, so hooks
# spawned by the agent inherit them.
#
# cwd is how a boundary reaches the right project's decision log. The listener
# derives the project from it; without it a state change is a status dot and
# nothing more. Sent from the hook rather than remembered at launch because the
# agent may well have moved into a worktree since.
#
# Must be silent + non-blocking: short timeout, always exit 0, so a slow or
# unreachable listener can never wedge the agent it is attached to.
[ -n "$XNAUT_HOOK_URL" ] || exit 0
# The listener is always loopback: a sandbox reaches it through a reverse
# tunnel (agent_profiles::start_beacon), so 127.0.0.1 is true on both sides.
# Anything else is not our listener, and the session token does not travel to
# a host we did not start. Still exit 0: declining to send is not a reason
# to wedge the agent (XNAUT-350).
case "$XNAUT_HOOK_URL" in
  http://127.0.0.1:*|http://localhost:*|'http://[::1]:'*) ;;
  *) exit 0 ;;
esac
# Provider hook input may contain the actual model; otherwise the harness
# supplies the launched model. JSON encoding also handles unusual cwd strings.
command -v python3 >/dev/null 2>&1 || exit 0
payload=$(python3 -c '
import json, os, sys
try:
    supplied = json.load(sys.stdin) if not sys.stdin.isatty() else {}
except (ValueError, OSError):
    supplied = {}
if not isinstance(supplied, dict): supplied = {}
print(json.dumps({"state":sys.argv[1], "cwd":os.getcwd(),
    "run_id":os.environ.get("XNAUT_RUN_ID"),
    "model":supplied.get("model") or os.environ.get("XNAUT_MODEL")}))
' "${1:-done}") || exit 0
curl -s -m 2 "${XNAUT_HOOK_URL%/v1/hook}/v1/hook" \
  -H "X-Xnaut-Session: $XNAUT_HOOK_TOKEN" \
  -H "Content-Type: application/json" \
  -d "$payload" >/dev/null 2>&1 || true
exit 0
