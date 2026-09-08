# XNAUT-306 verification bundle

Author: @codex
Date: 2026-09-08
Branch: agent/codex/xnaut-306
Session: 707707a2-7006-46d0-9737-37f34e4f34c5

## Change and approved scope

Only production file: src-tauri/src/sweep.rs. Fresh unowned high/critical in_progress tickets now enter the existing capped, oldest-first NautBot triage selector. Owned in_progress tickets remain untouched. The shared 14-day freshness cutoff, ready dispatch, registry, jury and housekeeper are unchanged.

Stale unowned ready/in_progress tickets in fleet projects are listed together in one inbox notify with IDs and titles, regardless of priority. No assignment or dispatch action is planned for them. Successful notifications are deduplicated by ticket across ticks in the running sweep; failed inbox writes retry. A ticket can be reported again after its stale condition clears. Deduplication is in memory, following Announced, and resets with the sweep process.

Owner explicitly approved revised plan in-f442b3bb-4eee-4736-9194-b2f294921e9b, input c5c170695255935a8c93b72b0db34f7a473930d879896198ced5999764db4e75. Approval was read back from /v1/plan/review. This settled the reviewers' question about the notify scope and 14-day cutoff; a six-week-old ticket is covered by the regression. PLAN.md carries the approved plan.

## Full verification

Run from this worktree root with `source .xnaut/bundles/XNAUT-300-env.sh` before each command. This redirects registry, ledger, switches, agents, vault, leases, spend, worklog, evidence and temp state under .xnaut/test-state and serializes Rust tests. No live app restart or live board fixture mutation was used.

| Command | Exit | Result | Local log |
| --- | --- | --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | 974 passed, 0 failed, 42 ignored; 21.96s | .xnaut/test-state/rust-final.log |
| `cd src-tauri && cargo test --bin xnaut` | 0 | 974 passed, 0 failed, 42 ignored; 20.65s | .xnaut/test-state/rust-bin-final.log |
| `XNAUT_TEST_PORT=4291 npx playwright test` | 0 | 133 passed; 2.6m | .xnaut/test-state/ui-final.log |
| `git diff --check` | 0 | No whitespace errors | Terminal check |

Canonical sign-off totals contain one full Rust run and one full UI run; the equivalent second Rust invocation is not double-counted.

XNAUT_TEST_TOTALS={"rust":[{"passed":974,"failed":0,"ignored":42}],"ui":[133]}

Unchanged baseline: Rust 969 passed, 0 failed, 42 ignored; Playwright 133 passed. The five added pure tests cover high/critical and absent/empty/whitespace owners; a six-week stale ticket's actual notification payload and absence of dispatch; owned tickets and fleet opt-out; both statuses at 14 days and one second beyond, unreadable timestamps and fresh low priority; notification deduplication, reorder, failed delivery retry and cleared conditions. Existing ready triage/dispatch tests remain green.

## Required mutation proof

Removed `| "in_progress"` from `unowned_triage_status`, then ran:

```sh
source .xnaut/bundles/XNAUT-300-env.sh
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut sweep::tests::fresh_unowned_in_progress_urgent_tickets_are_triaged -- --exact --nocapture
```

Exit 101. 0 passed, 1 failed. Behavioral assertion, not compilation failure:

```text
assertion `left == right` failed
  left: []
 right: [Triage { tickets: ["XNAUT-306: t"] }]
```

Restored the selector before both full final Rust runs. Local mutation log: .xnaut/test-state/mutation.log.

## Manual verification procedure

1. In an isolated test app/control repo with fleet enabled, create an unowned high-priority in_progress ticket updated today. Allow a sweep tick with NautBot available and triage cooldown clear. Confirm a sweep_triage wake; the selector test above proves the ticket is in the batch.
2. Create an unowned in_progress ticket updated six weeks ago. On the next tick, confirm one inbox notification lists its ID and title, with no owner/status mutation and no dispatch. Leave it unchanged for another tick and confirm no repeated notification in the same sweep lifetime.
3. Give a fresh and an old in_progress fixture an owner. Neither should enter this triage or stale notification path. Check a fresh ready urgent fixture still enters triage and an owned fresh ready fixture still dispatches as before.
4. Run the pure sweep tests with `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut sweep::tests` under the same isolated environment to reproduce the selector and notification assertions without a live app.

These are reviewer instructions. No native-app visual smoke test was performed; this ticket explicitly requests pure behavioral tests. No sandbox verify record was created. Nothing remains in the approved implementation and verification scope; independent acceptance belongs to NautBot.
