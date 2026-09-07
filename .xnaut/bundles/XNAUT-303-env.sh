# Source from the assigned worktree root; legacy fixtures mutate global env.
source .xnaut/bundles/XNAUT-300-env.sh
export XNAUT_INBOX_DIR="$PWD/.xnaut/test-state/inbox"
export XNAUT_VERIFY_DIR="$PWD/.xnaut/test-state/verify"
mkdir -p "$XNAUT_INBOX_DIR" "$XNAUT_VERIFY_DIR"
