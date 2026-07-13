#!/bin/sh
# xnaut-hook.sh — report an agent status to the xNAUT hook listener.
# usage: xnaut-hook.sh <state>
#   state: working|blocked|waiting|done|idle|permission|interrupted (default: done)
#
# Contract (see agent_hooks.rs): POST to $XNAUT_HOOK_URL with header
# X-Xnaut-Session: $XNAUT_HOOK_TOKEN and body {"state":"<state>"}.
# Both env vars are injected into the agent process at launch, so hooks
# spawned by the agent inherit them.
#
# Must be silent + non-blocking: short timeout, always exit 0, so a slow or
# unreachable listener can never wedge the agent it is attached to.
[ -n "$XNAUT_HOOK_URL" ] || exit 0
curl -s -m 2 "$XNAUT_HOOK_URL" \
  -H "X-Xnaut-Session: $XNAUT_HOOK_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"state\":\"${1:-done}\"}" >/dev/null 2>&1 || true
exit 0
