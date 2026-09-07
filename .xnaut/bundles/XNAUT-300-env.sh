# Source from the worktree root before the Rust suites.
export TMPDIR="$PWD/.xnaut/test-state/tmp"
export XNAUT_REGISTRY_DIR="$PWD/.xnaut/test-state/registry"
export XNAUT_LEDGER_PATH="$PWD/.xnaut/test-state/ledger.jsonl"
export XNAUT_SWITCHES_DIR="$PWD/.xnaut/test-state/switches"
export XNAUT_AGENTS_PATH="$PWD/.xnaut/test-state/agents.json"
export XNAUT_TEST_VAULT="$PWD/.xnaut/test-state/vault"
export XNAUT_LEASE_DIR="$PWD/.xnaut/test-state/leases"
export XNAUT_SPEND_DIR="$PWD/.xnaut/test-state/spend"
export XNAUT_WORKLOG_DIR="$PWD/.xnaut/test-state/worklog"
export XNAUT_WORKLOG_ROOT="$PWD/.xnaut/test-state/worklog"
export XNAUT_VAULT_ROOT="$PWD/.xnaut/test-state/vault"
export XNAUT_EVIDENCE_DIR="$PWD/.xnaut/test-state/evidence"
export XNAUT_PLUGINS_PATH="$PWD/.xnaut/test-state/plugins.json"
export XNAUT_AUTOMATIONS_PATH="$PWD/.xnaut/test-state/automations.json"
export GIT_CEILING_DIRECTORIES="$PWD/.xnaut/test-state/tmp"
export ZELLIJ_SOCKET_DIR=../.xnaut/test-state/sockets
export RUST_TEST_THREADS=1
mkdir -p "$TMPDIR" "$XNAUT_REGISTRY_DIR" "$XNAUT_SWITCHES_DIR" "$XNAUT_LEASE_DIR" "$XNAUT_SPEND_DIR" "$XNAUT_TEST_VAULT" "$XNAUT_WORKLOG_DIR" "$XNAUT_EVIDENCE_DIR"
