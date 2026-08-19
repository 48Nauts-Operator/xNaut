#!/bin/sh
# One window per headless agent, in a session of its own.
#
#   scripts/agent-windows.sh 192 193 199 200 201 202
#
# The point is the session boundary. zellij refuses to nest, so running this
# from inside a session used to leave two bad options: a new TAB, which buries
# whatever you were doing, or nothing at all. `attach --create-background`
# builds the session detached, so your current one is untouched and the agents
# are one `zellij attach` away. Learned 2026-08-19 by burying a live
# conversation under six agent panes.
#
# Each pane follows <worktree>/agent.log through watch-agent.cjs. Re-running is
# safe: an existing session is left alone.

set -e
SESSION=${XNAUT_AGENT_SESSION:-xnaut-tickets}
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
WATCH="$(cd "$(dirname "$0")" && pwd)/watch-agent.cjs"
LAYOUT="${TMPDIR:-/tmp}/$SESSION.kdl"

[ $# -gt 0 ] || { echo "usage: $0 <ticket>..." >&2; exit 1; }

{
  printf 'layout {\n  tab name="agents" {\n    pane split_direction="vertical" {\n'
  for t in "$@"; do
    w="$ROOT/.worktrees/xnaut-$t"
    [ -d "$w" ] || { echo "no worktree for XNAUT-$t at $w" >&2; exit 1; }
    printf '      pane command="sh" name="XNAUT-%s" {\n' "$t"
    printf '        args "-c" "node %s %s/agent.log"\n' "$WATCH" "$w"
    printf '        cwd "%s"\n      }\n' "$w"
  done
  printf '    }\n  }\n}\n'
} > "$LAYOUT"

# Unset ZELLIJ* so this works from inside a session too; -b keeps it detached.
env -u ZELLIJ -u ZELLIJ_SESSION_NAME -u ZELLIJ_PANE_ID \
  zellij attach --create-background "$SESSION" >/dev/null 2>&1 || true
zellij --session "$SESSION" action new-tab --layout "$LAYOUT" >/dev/null

echo "$SESSION ready. In a terminal outside zellij:  zellij attach $SESSION"
