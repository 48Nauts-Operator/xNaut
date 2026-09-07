# XNAUT-305: completed runs survive process cleanup

Author: @codex. Session: 7f1ca108-f094-4cbf-9aff-559689e1b642.
Branch: `agent/codex/xnaut-305`. Plan: `PLAN.md`, approved by both independent reviewers under `in-efaf9a1e-5394-4e57-ad16-990e1989c698`.

## Change and scope

Accepted typed handbacks now carry an optional `run_id`, inferred from the authenticated HTTP/MCP session when omitted. Explicit IDs must match the session, ticket, and agent handle. Legacy handbacks without a registry entry remain compatible. The app never uses its own environment as the filing agent's identity.

The shared PM filing path stores the accepted handback and records `Done` while holding the registry lock, so reconciliation cannot interleave between those operations. A failed PM write does not mark completion. The production sweep first recovers any interrupted registry write from the exact run's accepted handback, then runs its existing reconciliation. An unrelated run's report cannot suppress a dead-process failure. Existing terminal and retirement states remain protected.

A ticket transition from `in_progress` to `done` or `review` also marks its newest matching run `Done` when its actual process is alive. The ticket mutation releases the PM lock before taking the registry lock, preventing lock inversion with handback filing. The previous owner is captured before the normal reassignment to NautBot. Current board status alone is never used to infer an old run's completion.

No UI or model-switch behavior changed. The existing handback literals in jury_proof.rs, jury_signoff.rs and sandbox_verify.rs only gained `run_id: None` for compatibility. No installed app was replaced, no branch was pushed, and no historical uncorrelated run records were rewritten.

## Full verification

From this worktree root:

```sh
source .xnaut/bundles/XNAUT-300-env.sh
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
XNAUT_TEST_PORT=4291 npx playwright test
```

| Command | Result |
| --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 966 passed, 0 failed, 42 ignored; 21.31s |
| `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut` after restoring the mutation | 966 passed, 0 failed, 42 ignored; 20.64s |
| `XNAUT_TEST_PORT=4291 npx playwright test` | 133 passed; 2.6m |
| `git diff --check` | passed |

The normal full Rust/UI pair has these machine-readable totals. The repeat binary suite and explicit live proof are reported separately above and below, not double-counted:

XNAUT_TEST_TOTALS={"rust":[{"passed":966,"failed":0,"ignored":42}],"ui":[133]}

The ignored Rust count includes this ticket's live test, explicitly run below. Existing unrelated ignored tests remain ignored. Registry, PM fixtures, leases, ledger, temporary directories and other supported state paths are redirected beneath `.xnaut/test-state` by the XNAUT-300 environment script. No fixture changes process-global environment variables. `npm ci` installed 107 packages and reported zero vulnerabilities.

Local logs: `.xnaut/bundles/XNAUT-305-rust.log`, `XNAUT-305-rust-bin.log`, `XNAUT-305-playwright.log`, `XNAUT-305-live.log`, and `XNAUT-305-mutation.log` in the same directory. Relevant output is preserved in this committed bundle.

The first targeted run exposed two incorrect test assumptions about the admission state (`Starting`, not `Running`/`Requested`). Those fixture assertions were corrected before both full green runs.

## Regression coverage

- Pure identity and dead-process verdict tests distinguish completed runs from disappeared runs, including exit 137 and protected terminal/retirement states.
- Session binding rejects wrong sessions, missing authentication for an explicit ID, and ticket/handle mismatches. Failed PM storage leaves the run unchanged, and later Working hooks cannot resurrect a completed run.
- Completion requires the qualifying ticket transition, liveness, and the newest matching owner; dead processes, other status transitions and unrelated owners do not qualify.
- The actual PM update path is exercised for both `done` and `review`, with a live PID/birth stamp and without a live process.
- The shared accepted-handback filing path is checked immediately for durable `Done`, before reconciliation can mask a missing signal. Rejected handbacks leave state unchanged.
- Production sweep recovery is exercised after simulating an interrupted registry write. A report belonging to another run leaves the dead run `Failed`.

## Live proof on this machine

```sh
source .xnaut/bundles/XNAUT-300-env.sh
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut registry_live_handback_then_kill_stays_done -- --ignored --nocapture
```

Result: **1 passed, 0 failed**, 0.20s. The test launches a real `/bin/sleep 60` through production `launch_argv_in`, with `XNAUT_RUN_ID`. It proves the actual PID and birth stamp, files an accepted typed handback through the shared production PM filing function into an isolated Git repository, reads back `Done`, sends SIGKILL to that child, waits for exit 137, reconciles once through the production sweep boundary, and reads durable `Done` again. The ticket is not handed back for reassignment. This uses the compiled production functions, not a replacement implementation, and does not restart the owner's GUI.

```json
{
  "evidence_dir": "/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/.xnaut/test-state/live-handback-65d10808-097f-4a8e-9305-72f7b30fea25",
  "exit_code": 137,
  "handback_run_id": "01M1Y719E59WJSKJZHD2EHT8ZD",
  "pid": 43313,
  "pid_absent": true,
  "process_birth": "Mon Sep  7 17:15:52 2026",
  "run_id": "01M1Y719E59WJSKJZHD2EHT8ZD",
  "state_after_reconcile": "done",
  "state_before_kill": "done",
  "ticket_returned": false
}
```

## Mutation proof

The filing-time call `mark_completed(&mut run, "accepted typed handback")` in `record_handback_in` was disabled while retaining the PM write and recovery path. Ran:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut accepted_handback_marks_done_before_reconcile_and_recovers_interruption -- --nocapture
```

Captured exit **101** and the required failure:

```text
assertion `left == right` failed
  left: Starting
 right: Done
FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1007 filtered out
```

The completion call was restored in a `finally` block. The full binary Rust suite then passed again (966 passed, 0 failed, 42 ignored). The mutation proves recovery cannot hide a missing filing-time transition from this test.

## Manual verification

1. Source the environment script and run the explicit live proof command above.
2. Copy `evidence_dir` and `run_id` from its `LIVE_HANDBACK` output. Inspect `registry/<run_id>.run.json` and the corresponding `.events.jsonl`: the stored state is `done` and the signal is `accepted typed handback`.
3. Inspect `control/projects/XNAUT/tickets/XNAUT-900.json`: its typed handback names that same run ID. The ticket remains `in_progress` in this isolated proof, demonstrating that the handback alone supplies the completion evidence.
4. Inspect `registry/<run_id>.exit`: it contains `137`. The output proves `pid_absent: true` and `state_after_reconcile: done` after that exit.
5. For the contrasting path, run `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut another_runs_handback_does_not_hide_a_dead_process`. It asserts durable `Failed` when the only handback belongs to another run.

Outstanding ticket work: nothing.
