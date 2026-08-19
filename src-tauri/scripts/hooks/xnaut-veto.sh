#!/bin/sh
# xnaut-veto.sh - ask xNAUT whether a tool call may proceed (XNAUT-132).
#
# Ported from jcode (github.com/1jehuang/jcode, MIT), docs/HOOKS.md: exit 0
# permits, exit 2 blocks, and stderr is handed back to the model as the tool
# error so it can adapt rather than guess. We depart in one way: the decision
# is made by the app rather than by this script, because a useful rule needs to
# know which agent is calling and what it owns, and this file is readable by
# the agent it governs.
#
# Wired as a PreToolUse hook. The harness passes the tool call as JSON on
# stdin; we forward it and read one word back.
#
# FAILS OPEN, DELIBERATELY AND AT EVERY STEP. No URL, no curl, no answer, a
# slow app, a malformed reply: exit 0. An agent wedged by its own safety rail
# happens on every call; a call that should have been stopped is rare and
# recoverable. The only path to exit 2 is an explicit deny.

payload=$(cat 2>/dev/null || true)

[ -n "$XNAUT_VETO_URL" ] || exit 0
command -v curl >/dev/null 2>&1 || exit 0
[ -n "$payload" ] || exit 0

# The harness names the caller nowhere in its envelope, so the identity comes
# from the environment xNAUT launched this agent with. Without it a rule cannot
# be scoped to one agent and two agents editing one file cannot be told apart.
if [ -n "$XNAUT_AGENT_HANDLE" ]; then
  payload=$(printf '%s' "$payload" | sed "s/^{/{\"agent\":\"$XNAUT_AGENT_HANDLE\",/")
fi

answer=$(printf '%s' "$payload" | curl -s -m 3 "$XNAUT_VETO_URL" \
  -H "X-Xnaut-Session: $XNAUT_HOOK_TOKEN" \
  -H "Content-Type: application/json" \
  --data-binary @- 2>/dev/null) || exit 0

# Anything that is not a deny is an allow, including an empty body.
#
# Whitespace is stripped before matching. The first version compared against
# the exact bytes serde happens to emit, so a pretty-printed or space-separated
# reply stopped blocking and said nothing about it: a control that fails open
# on a formatting change is worse than no control, because it still looks
# installed.
compact=$(printf '%s' "$answer" | tr -d ' \t\n\r')

# "ask" is the middle tier (XNAUT-189): the app has put the question to the
# owner and handed back the inbox id to wait on. The first call stays short on
# purpose — a hung app must not cost every tool call a long timeout — so the
# waiting happens here, and only here.
case "$compact" in
  *'"decision":"ask"'*)
    id=$(printf '%s' "$answer" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    [ -n "$id" ] || exit 0
    [ -n "$XNAUT_HOOK_URL" ] || exit 0
    # Every failure below is an allow: no answer, a closed app, a timeout. The
    # owner not being at the desk must not wedge the agent.
    verdict=$(curl -s -m 900 "${XNAUT_HOOK_URL%/}/v1/inbox/wait/$id?timeout_ms=880000" \
      -H "X-Xnaut-Session: $XNAUT_HOOK_TOKEN" 2>/dev/null) || exit 0
    verdict=$(printf '%s' "$verdict" | tr -d ' \t\n\r')
    case "$verdict" in
      *'"status":"denied"'*) ;;
      *) exit 0 ;;
    esac
    reason=$(printf '%s' "$answer" | sed -n 's/.*"reason"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    [ -n "$reason" ] || reason="The owner declined this call."
    printf '%s\n' "$reason" >&2
    exit 2
    ;;
esac

case "$compact" in
  *'"decision":"deny"'*) ;;
  *) exit 0 ;;
esac

# The reason is what the model reads. Pull it out without needing jq, which is
# not on every machine; fall back to a plain sentence if the shape surprises us.
# Tolerant of whitespace around the colon, for the same reason the match above
# is: the reply's exact formatting is not ours to depend on. Extracted from the
# ORIGINAL answer rather than the compacted one, or the spaces inside the
# sentence would be stripped along with the ones around it.
reason=$(printf '%s' "$answer" | sed -n 's/.*"reason"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
[ -n "$reason" ] || reason="This call is not allowed by the current policy."

printf '%s\n' "$reason" >&2
exit 2
