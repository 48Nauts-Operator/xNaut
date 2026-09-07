# XNAUT-300 Phase 1 handback

@codex, session fb8a0161-780f-4ddf-b818-d02ad9414cbf. Branch `agent/codex/xnaut-300`, based on `da179a0`. Owner approved PLAN.md; design read from `/Users/zelda/.xnaut-vault/work/xnaut/Development/features/2026-09-06_Run-Registry.md`.

## What changed

Local agent launches now request a ULID and persist a requested/admitted record before spawning. `run_control.rs` adapts XNAUT-277 commit 18b7632's manifest and append-only journal, with atomic snapshots, journal recovery, explicit directories, serialized admission and lifecycle updates. The header credits xNAUT (MIT), Tuxedo's Bulletin Board, Erlang/OTP and Condor.

The CLI wrapper records its actual PID and process birth stamp before exec and its exit status afterward. The record binds the agent, runtime/model, worktree/branch, ticket, PTY, zellij session and capture path. Authenticated hooks and Mesh ask/approve answers update progress and explicit waits. Repeated working hooks count as progress; an unanswered inbox request survives polling hooks.

A pure verdict distinguishes liveness from progress. Dead proofs name what failed; alive runs with nonempty `waiting_on` are exempt from progress timeouts. Startup grace is 60 seconds; the progress window is 15 minutes. The production sweep reconciles at startup and each 180-second tick. Failed runs return only their still-current ticket assignment. PID/session/capture evidence must prove the writer stopped before its ticket and matching lease are released. A live stalled writer retains both. Newer local runs and dispatch sessions prevent stale returns; journal/PM acknowledgement recovery prevents duplicate effects. Final capture bytes are sampled on failed-run retries so they cannot block return forever.

No swapping, capture-pattern detection, new NautGate routes, cross-machine publishing, merge or installed-app replacement. Pre-existing sessions are not silently adopted into new records. The approved launch wiring covers local agent launch paths; other process kinds remain schema values for later integration.

## Required verification

Run from the worktree root:

```sh
source .xnaut/bundles/XNAUT-300-env.sh
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
XNAUT_TEST_PORT=4291 npx playwright test
```

- Full Rust command: **927 passed, 0 failed, 37 ignored**, 14.20 seconds. Earlier full `--bin xnaut` run also passed; the final all-target command runs this same binary suite. The explicitly invoked live test below accounts for one ignored test.
- Playwright: **119 passed**, 2.7 minutes. Dependencies installed with `npm ci --cache .xnaut/test-state/npm-cache`.
- `git diff --check`: clean.

All registry, lease, ledger and isolated control-repository fixtures are beneath this worktree. The environment file also redirects existing suites' supported state paths and TMPDIR. It sets a Git discovery ceiling so non-repository fixtures do not discover this checkout, uses a relative zellij socket path to stay below macOS's socket length limit, and serializes legacy tests that mutate process-global environment. The zellij integration test now starts its PTY in the same cwd as its probes so that relative socket path identifies the same session.

Initial attempts exposed missing fixture fields (fixed), the Git/socket path issues above (corrected), a legacy parallel environment race (serialized), and absent workspace Node dependencies (installed). They were followed by the full passing runs, not treated as successes.

## 1. Pure proof and production integration

`run_control::tests::pure_proofs_distinguish_dead_waiting_and_stalled` checks named dead proofs, explicit waits, blank waits past the progress window, and real progress. Other tests cover terminal states, wrong worktrees, PID reuse, rejected admission, journal repair, waiting updates and newer worktree holders.

`sweep::registry_tests::registry_tick_reclaims_dead_run_and_waits_for_a_live_writer` invokes the production registry/board boundary with an explicit isolated Git control repository and ledger. It checks failed state, ready/unowned ticket, lease release, deduplication, recovery after the PM write, retention of a live writer, and eventual return after its capture settles. The waiting/reassignment integration test protects approval waits and newer dispatches.

## 2. Live kill proof on tron

One command, after sourcing the environment above:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut registry_live_kill_reclaims_ticket -- --ignored --nocapture
```

**1 passed, 0 failed**, 0.21 seconds. This launches `/bin/sleep 600` through the production CLI wrapper, proves its PID/birth, executes `/bin/kill -9`, then invokes the production sweep registry boundary once. It uses an isolated `XNAUT-900` ticket repository and ledger. It does not start a provider session, the full GUI timer, or the installed supervisor.

```json
{
  "evidence_dir": "/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-300/.xnaut/test-state/live-registry-4e5a58ef-2c26-4da6-9531-8a501cfed02a",
  "killed_at_ms": 1788750575827,
  "machine": "tron.candoo",
  "pid": 28432,
  "reason": "process exited with status 137",
  "reconciled_at_ms": 1788750575892,
  "run_id": "01M1WXFE208CCAJ9BWXDEJVJ6K",
  "started_at_ms": 1788750575680,
  "ticket_revision": 2,
  "ticket_status": "ready"
}
```

Kill to reconciliation: **65 ms**, within one 180-second sweep interval. Ticket revision 1 (`in_progress`, owner `codex`) became revision 2 (`ready`, no owner). Registry state was read back as `failed`, reason `process exited with status 137`.

Ledger rows:

```jsonl
{"run_id":"01M1WXFE208CCAJ9BWXDEJVJ6K","at":"2026-09-07T03:09:35.847165+00:00","kind":"registry_failed","agent":"codex","ticket":"XNAUT-900","detail":"process exited with status 137","elapsed_secs":null,"session":""}
{"run_id":"01M1WXFE208CCAJ9BWXDEJVJ6K","at":"2026-09-07T03:09:35.879357+00:00","kind":"registry_ticket_returned","agent":"codex","ticket":"XNAUT-900","detail":"process exited with status 137","elapsed_secs":null,"session":""}
```

## 3. Mutation

Removed the actual `run_control::reconcile_in(registry, at, observe)?` call from `registry_tick_in`, replacing its result with an empty vector. Ran:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut sweep::registry_tests::registry_tick_reclaims_dead_run_and_waits_for_a_live_writer -- --exact --nocapture
```

Exit 101; **0 passed, 1 failed**. This was a behavioral assertion failure, not a compile error:

```text
thread 'sweep::registry_tests::registry_tick_reclaims_dead_run_and_waits_for_a_live_writer' (30738826) panicked at src/sweep.rs:2364:9:
assertion `left == right` failed: sweep must reconcile the dead run
  left: Starting
 right: Failed
```

Restored the production call and ran the full Rust suite: **927 passed, 0 failed, 37 ignored**. The mutation output is also in the implementation commit message.

## Manual review

1. Run the isolated live command above; use the printed `evidence_dir` and run ID.
2. Read `registry/<run_id>.run.json` and its journal: failed state, exit reason, actual PID/birth, and `ticket_returned: true`.
3. Read `control/projects/XNAUT/tickets/XNAUT-900.json`: ready status, cleared owner and exact run receipt. Read `ledger.jsonl`: one failure row and one return row sharing the run ID.
4. Re-run the integration test to inspect waiting and still-live writer retention. Do not point these tests at the owner's control or registry directories.

No work remains in the approved Phase 1 implementation. NautBot's independent review/acceptance remains separate. Local detailed logs are `.xnaut/test-state/rust-final.log`, `ui-suite.log`, `mutation.log` and `live.log`.
