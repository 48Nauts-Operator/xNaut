#!/bin/sh
# xnaut-brief.sh — hand the agent its project brief when a session starts.
#
# Claude's SessionStart hook reads stdout and prepends it to the session's
# context, so anything printed here is what the agent wakes up knowing. That is
# the whole mechanism: instead of an agent that must know to ask for project
# state, one that already has it.
#
# Contract (see agent_hooks.rs): GET $XNAUT_BRIEF_URL with header
# X-Xnaut-Session: $XNAUT_HOOK_TOKEN and a cwd query, answering the rendered
# agent-mode brief as plain text. cwd is how the listener picks the project —
# same reasoning as xnaut-hook.sh, and read at call time because the agent may
# have moved into a worktree since launch.
#
# Silence is the correct failure. A brief that cannot be fetched must leave the
# session exactly as it would have been without this hook, never a curl error
# pasted into the agent's context as though it were project state. So: short
# timeout, -f so an HTTP error yields no body, and exit 0 always.
[ -n "$XNAUT_BRIEF_URL" ] || exit 0
esc_cwd=$(printf '%s' "$PWD" | sed 's/\\/\\\\/g; s/"/\\"/g')
brief=$(curl -sf -m 3 -G "$XNAUT_BRIEF_URL" \
  -H "X-Xnaut-Session: $XNAUT_HOOK_TOKEN" \
  --data-urlencode "cwd=$esc_cwd" 2>/dev/null) || exit 0
[ -n "$brief" ] || exit 0
printf '%s\n' "$brief"
exit 0
