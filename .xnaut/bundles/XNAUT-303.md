# XNAUT-303: baseline and pending implementation

Recorded by @codex on 2026-09-07. Session: 8f8fde0f-6676-4a72-bbd9-78808a6eb2ad.

This is a baseline report, not an implementation handback. No jury code has been implemented and the ticket is not done.

## What changed

`PLAN.md` contains the proposed implementation plan submitted through the Foundation-required plan review. This bundle records the baseline evidence and prerequisites. `npm ci` installed the worktree's existing locked dependencies; no dependency manifest changed.

## Verification

Before implementation, `cargo test --manifest-path src-tauri/Cargo.toml` finished with **927 passed, 2 failed, 37 ignored**, exit 101. Failures:

- `agents::tests::configured_nautgate_route_preserves_claude_oauth_and_supplies_openai_token`: unwrap at `src/agents.rs:2801`. The test assumes a listening fixed localhost endpoint; production endpoint-liveness checking returns `None` on this machine.
- `worklog::tests::a_report_that_drops_rows_says_so_and_says_how_many_there_were`: count assertion at `src/worklog.rs:1237`.

`cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut -- --test-threads=1` finished with **928 passed, 1 failed, 37 ignored**, exit 101, in 17.01 seconds. The NautGate test still failed. The worklog test passed serially; parallel isolation remains an unresolved concern, not a proven diagnosis. Local output: `.xnaut-303-baseline-rust.log`.

The initial `XNAUT_TEST_PORT=4291 npx playwright test` failed before discovery because `@playwright/test` was not installed. `npm ci` installed 107 packages and reported zero audit vulnerabilities. The subsequent full `XNAUT_TEST_PORT=4291 npx playwright test` passed **119 tests in 2.6 minutes**, exit 0.

No mutation tests or live jury tests have been run.

## Pending prerequisites

- Plan review `in-8b318530-c9d0-4ea4-acc8-b3ba47f1d700` remains `pending` after repeated long polls. No approval has been received.
- SSH access question `in-82996943-4610-4439-9c76-3255da130c6e` remains `open`. `ssh -o BatchMode=yes -o ConnectTimeout=10 tron ...` failed host-key verification. The server key matches an existing trusted entry for IP `100.103.152.113`; using `-o StrictHostKeyChecking=yes -o HostKeyAlias=100.103.152.113` passed that check but authentication failed for `zelda` with `Permission denied (publickey,password,keyboard-interactive)`. No remote command succeeded and no SSH configuration was changed.
- The linked vault document was read, but its local copy lacks the named approval and sign-off sections. Project-scoped document search for `Approval in two tiers` returned no matches. The dispatched ticket supplies the explicit build spec.
- This branch's registry has no retiring/undead stop-and-prove path. Safe jury revocation needs that prerequisite; the submitted plan names it.

## How to verify by hand

1. Open the XNAUT-303 plan review in Mesh and inspect `PLAN.md`; verify its actual decision before implementation.
2. Read XNAUT-303's progress record and the SSH identity question. Use the provisioned SSH route once supplied; do not disable host verification.
3. Re-run the full Rust and UI commands above to reproduce the baseline results. Serial results do not establish a green parallel suite.
4. After implementation, replace this baseline bundle with implementation evidence: both suite totals, absent-reviewer and rollback mutation records, and the real tron plan/sign-off/integration/revert records required by the ticket.

## Not finished

The implementation, baseline Rust fixes, reviewer launch isolation, policy and decision tests, mutation tests, Foundation change, live tron proof, integration merge/build/revert proof, and typed implementation handback are all unfinished. Implementation waits on plan approval; live proof also waits on working SSH authentication. No `done` or `complete` claim is made.
