# XNAUT-317: Parallel integration builds

Author: @codex. Verified 2026-09-11 on the existing `agent/codex/xnaut-317` worktree, rebased onto `origin/dev` at `a25182b07c61179381d8e01b3f228146bb22bc3a`.

## What changed

Production changes are confined to `src-tauri/src/jury_signoff.rs`:

- Reserve sign-off admission in the existing job store, including first-run store creation. Repeated sweeps reuse a review before it is attached to its ticket. Release `SIGNOFF_LOCK` before reviewers and integration commands.
- Serialize Git merge preparation/publication, rollback, revocation decisions and promotion with `INTEGRATION_LOCK`. Release it before running commands in each job's existing private clone. Keep compare-and-swap publication, journal states, compensation commits and author requeue behavior.
- Refuse promotion of a green historical snapshot when the integration tip has changed. A late green cannot overwrite a newer merge or compensation.
- Set the integration clone's Git discovery ceiling to TMPDIR's parent, so tests probing TMPDIR do not discover the enclosing clone.
- Six new tests exercise two concurrent green builds against one bare remote, independent red compensation, revocation while the second build runs, revocation before publication, admission deduplication/first-run initialization, and isolated Git discovery. The new fixtures do not mutate process-global environment.

`PLAN.md` contains the plan approved through `in-b0ce7e5c-0784-4c20-82dc-436bd32fe589`. The owner approved the ticket's parallel-clone alternative and its concurrency/independent-compensation acceptance interpretation. There are two overlapping builds, each with its own record. No batching is claimed.

Design document: `work:xnaut/Development/features/2026-09-10_XNAUT-317.md`. The original Markdown link was absent. The replacement is linked on the ticket.

## Rebase resolution

The owner authorized this continuation after the attempted merge conflicted. Original implementation `596772a` is retained as rebased commit `e6f91be`. The only production conflict was in `start`: retain XNAUT-319 swarm refusal first, then XNAUT-317 admission deduplication. `git range-diff 2ed03c3..596772a origin/dev..e6f91be` confirms only that contextual adjustment. The existing real-start swarm refusal test passes. No merge revert had to be restored.

## Own full-suite results

```sh
source .xnaut/bundles/XNAUT-303-env.sh
export GIT_CEILING_DIRECTORIES="$PWD/.xnaut/test-state"
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
cargo test --manifest-path src-tauri/Cargo.toml
XNAUT_TEST_PORT=4291 npx playwright test
```

- Binary Rust suite: **1092 passed, 0 failed, 45 ignored**, 41.81s, exit 0.
- Full Cargo invocation: **1092 passed, 0 failed, 45 ignored**, 63.93s, exit 0.
- Full UI suite: **150 passed**, 2.7m, exit 0.
- `git diff --check`: exit 0.

XNAUT_TEST_TOTALS={"rust":[{"passed":1092,"failed":0,"ignored":45}],"ui":[150]}

The totals line counts one full Rust suite and one full UI suite; the second Cargo command is a required repeat, not additional tests. Existing compiler warnings remain. Logs: `.xnaut-317-rebase-rust-final.log`, `.xnaut-317-rebase-rust-all-final.log`, `.xnaut-317-rebase-ui.log`.

The rebased initial binary suite also passed: 1092 passed, 0 failed, 45 ignored, 59.15s. Port 4291 was confirmed free before Playwright started its server from this worktree. No UI source or tests changed.

## Mutation evidence

Repeated on the rebased tree on 2026-09-11. Temporarily inserted `let _serialized_build = SIGNOFF_LOCK.lock().unwrap();` at the beginning of `verify_integration`, restoring serialized build execution. Ran:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut jury_signoff::tests::two_green_integrations_build_concurrently_in_private_clones -- --exact --nocapture
```

Observed exit 101 and an assertion failure, not a compiler error:

```text
both integration commands must start before either is released
FAILED
failures:
    jury_signoff::tests::two_green_integrations_build_concurrently_in_private_clones
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1136 filtered out; finished in 6.62s
```

The serialization guard was removed, and `git diff --exit-code -- src-tauri/src/jury_signoff.rs` confirmed exact restoration before both final Rust commands. Both full Rust commands and the UI suite passed on the final source. Full mutation log: `.xnaut-317-rebase-mutation.log`.

## Reproduce the behavior

After sourcing the environment above, run:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut jury_signoff::tests:: -- --nocapture
```

In each `xnaut-jury-parallel-*` fixture under `.xnaut/test-state/tmp`, inspect the two `registry/jury/integration-<job-id>.started` files. They contain different clone paths and both are written before either `.release` file is created. Each job names a different integration verify run. The test asserts this overlap, remote `dev` and `uat` contents, preservation of the independent change after rollback, and idempotent compensation. Red and revoked jobs return to `in_progress` with revocation/revert evidence. Review `*-integration-proof.json` and the per-command logs beside the clones.

## Limits and deliberately excluded work

This is the approved parallel alternative; batching and batch bisection are not implemented. Existing startup reconciliation remains the recovery mechanism. Installed-app deployment and machine-wide throughput benchmarking are separate from this change. XNAUT-315's structural instrumentation is present; the live doctor used for the initial observation had no throughput field, and local dev showed zero machine merges in a 24-hour window. No measured production speedup is claimed. Nothing remains unfinished within the approved scope.
