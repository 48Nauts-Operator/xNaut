# XNAUT-296 test bundle

Author: @codex
Date: 2026-09-07
Approved plan: `PLAN.md`, Mesh review `in-4bcf21d1-97f4-4cd2-9a86-bb0ff6670a61`.

## What changed

- Tickets persist `model_requirement`, available through the existing create/update APIs and agent tools. Empty disables swapping. Requirements match explicit model identities, trimmed and case-insensitive; no quality ranking is inferred.
- Authenticated hooks report the actual model when supplied, falling back to the launched model and carrying the run ID. Capture detection recognizes runtime notice lines, including real ANSI login/trust excerpts. Quoted task prose is rejected. Degradation survives progress hooks and reconciliation; declared waits prevent swaps.
- Retirement is durable and nonblocking. `Retiring` keeps its lease while SIGTERM and `zellij delete-session --force` are requested. Sweep polls every five seconds during retirement. Transfer requires confirmed PID absence, a successful session query showing the session fully absent (including exited sessions), and unchanged capture bytes across an observed 60-second window. Unknown evidence cannot authorize transfer. A delayed first observation of final buffered output gets its own quiet window.
- A writer that will not stop reaches `Undead`, keeps its lease and ticket, and generates an owner inbox notification. Hooks and same-handle/dead-supervisor lease reclamation cannot undo that refusal. Adoption refreshes the registry's lease-holder PID under the admission lock.
- After stop proof, the ledger records proof before lease release and ticket return. A successor is reserved as `Requested`, linked in both directions, on the same worktree and branch. Triage filters installed runtimes by configured model and observed health. Admission enforces the requirement, and dispatch sends CONTINUE even when only uncommitted work exists. Admission refusals retain the continuation location and receive a distinct retry ID.
- The pre-existing routing test now owns an ephemeral TCP listener instead of assuming an owner's gateway at localhost:8090. Production gateway routing is unchanged.

A continuation whose launch fails after admission is conservatively refused further dispatch until that failed launch is explicitly recovered. It cannot silently fall back to a fresh worktree. Admission refusals before a process exists are automatically retryable and are tested.

NautGate detection is Phase 3 and is outside this ticket. No per-project default or automatic quality-tier selection was added.

## Suite results

State was isolated by sourcing `.xnaut/bundles/XNAUT-300-env.sh` from this worktree root.

| Command | Result |
| --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml` | **941 passed, 0 failed, 38 ignored** (15.95s test execution) |
| `XNAUT_TEST_PORT=4291 npx playwright test` | **119 passed** (2.6m) |
| `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut registry_live_model_swap_and_undead -- --ignored --nocapture` | **1 passed, 0 failed** (182.81s), both live legs |
| `python3 .xnaut/bundles/XNAUT-296-hook-proof.py` | **2 HTTP requests passed**; observed-model override, launch-model fallback, and run-ID propagation |
| `git diff --check` | Clean |

The ordinary Rust suite retains 38 explicitly ignored tests, including the opt-in live proof above. Existing compiler warnings remain; there were no compile errors. Raw suite logs are in `.xnaut/test-state/final-rust.log`, `final-ui.log`, and `live-swap-final.log`.

## Mutation proof

Temporarily replaced the production `let proven = ...` transfer guard in `retirement_step_in` with `true`, then ran:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
```

The suite exited 101. Both safety tests failed:

```text
swap_guard_requires_every_proof_and_retains_the_lease_on_refusal:
  missing pid must refuse the swap
  left: Retired
  right: Undead
swap_waits_for_grace_reserves_same_worktree_and_replays_once:
  mtime or an immediate dead pid cannot skip the observed grace
```

The original guard was restored before further edits. The full restored suite is green as recorded above. Mutation output: `.xnaut/test-state/mutation-rust.log`.

## Live registry and ledger proof

Portable copies of the final registry snapshots and ledger rows are in `XNAUT-296-live.json` beside this bundle. The test uses real local processes through the production launch wrapper and the production registry/board reconciliation boundary. It advances the nonblocking retirement through repeated observations; it does not fake the clock or stop proofs.

| Leg | Run | Result | Lease | Successor |
| --- | --- | --- | --- | --- |
| Real wrapper inside isolated zellij | `00000000Z890QAMN89QXFKSG0B` | `Retired` after 61.646s | Released after stop proof | `01M1X83JRC9ESR9X3AESPY3GSB`, `Requested` |
| Standalone wrapper trapping TERM/HUP | `00000000Z88E0RFQ9X4Q6ER2ZX` | `Undead` after 120.697s | Retained | None |

The successful successor's `previous_run_id` names the retired run; the retired run's `next_run_id` names the successor. Both retain branch `agent/test` and exactly the same worktree path. Its ledger order is `registry_retiring`, `registry_stop_requested`, `registry_stop_proven`, `registry_lease_released`, `registry_swap_returned`.

The resistant process was alive at the refusal verdict, its ticket remained `in_progress` with the original owner, and no lease-release or successor event was written. Fixture cleanup killed that process **after** evidence was captured; the isolated registry and retained lease remain available for review.

Original evidence directories:

- `/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-296/.xnaut/test-state/live-swap-b9f06598-cc19-41ec-a230-1e273b988e97`
- `/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-296/.xnaut/test-state/live-undead-9d80e551-ae61-42b8-a279-e618391a0ea9`

## How to verify

From this worktree root:

```sh
source .xnaut/bundles/XNAUT-300-env.sh
cargo test --manifest-path src-tauri/Cargo.toml
XNAUT_TEST_PORT=4291 npx playwright test
python3 .xnaut/bundles/XNAUT-296-hook-proof.py
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut registry_live_model_swap_and_undead -- --ignored --nocapture
```

The live test creates its own control repository, capture, registry, leases, and a uniquely named zellij session. It prints each evidence directory. Inspect its `registry/*.run.json`, `registry/*.events.jsonl`, `ledger`, and ticket JSON to compare the transfer order and predecessor/successor paths. Do not use an owner's active ticket or process as a fixture.

For a manual application check on a disposable run, set the ticket's `model_requirement` through the existing update-ticket API to an installed profile's explicit model ID. Report a different `model` to `/v1/hook` using that run's authenticated session and `run_id`. Observe `Degraded`, then `Retiring` with its lease held, then `Retired` plus an unowned ready ticket and a linked `Requested` continuation. Triage must select an eligible profile; dispatch must continue the existing branch and worktree. Repeat with an empty requirement and with a declared `waiting_on`: neither should swap. A writer that survives termination must yield `Undead` and an owner inbox notification.

## Not finished

Nothing within the approved Phase 2 scope. The running owner application was not restarted or replaced; this handback supplies the committed implementation and verification for NautBot's review.
