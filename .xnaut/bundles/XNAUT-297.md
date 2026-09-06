# XNAUT-297 test bundle

Author: @codex
Session: 738b0f95-243e-4024-b3f9-68882ec792c2
Branch: agent/codex/xnaut-297

## Change

`src-tauri/src/project_management.rs`: `ticket_update_in` now fetches origin and rebases the current branch before reading the ticket and checking `expected_revision`, while holding the existing PM mutation lock. A conflicting rebase is aborted and retried once with a fresh fetch. The second failure reaches the caller with local commits preserved. Existing rebases and dirty working trees are refused. Repositories without origin keep their local-only behavior; an empty remote can still receive its first sync.

No sweep, registry, single-writer routing, merge driver, or push behavior changed. This is pre-write reconciliation, not a distributed transaction: another machine can still publish after the fetch, and a subsequent normal push may reject divergence. No reset or conflict-side preference is introduced.

The existing handback announcement source check now inspects the function boundary instead of assuming the call fits within its first 6,000 characters.

## Verification

Run from the worktree root unless noted:

- `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut project_management::tests::fleet_`: 3 passed, 0 failed.
- Mutation check: temporarily replaced only `match run_git(repo, &["rebase", &remote_ref])` with an immediate successful result. Re-ran the command above: 0 passed, 3 failed, exit 101. Different-ticket publication was rejected as non-fast-forward; the same-ticket conflict and stale revision were incorrectly accepted. Restored the implementation before running full suites.
- `XNAUT_AGENTS_PATH="$PWD/.xnaut/test-state/agents.toml" cargo test --manifest-path src-tauri/Cargo.toml`: 905 passed, 0 failed, 36 ignored, 0 filtered out.
- From `src-tauri`: `XNAUT_AGENTS_PATH="$PWD/../.xnaut/test-state/agents.toml" cargo test --bin xnaut`: 905 passed, 0 failed, 36 ignored, 0 filtered out.
- `XNAUT_TEST_PORT=4291 npx playwright test`: 119 passed, 0 failed (2.6 minutes).
- `git diff --check`: no errors.

Initial setup failures were resolved: `npm ci` installed missing frontend dependencies (107 packages, no audit vulnerabilities). An unredirected Rust run had 902 passed, 3 failed: two registry-dependent tests could not parse the owner's newer `gemini` runtime enum, and the announcement source check hit its fixed character cutoff. The documented registry redirect isolated subsequent runs; no registry code or owner configuration was edited.

## What the regression tests prove

The fixtures use two independent clones and a local bare Git remote. They deliberately create overlapping unpushed histories and publish one writer before the other reconciles; this controls the collision window without timing-dependent threads.

1. Different tickets: a local committed agent handback and a remote Studio edit both survive rebase, follow-up mutation, and a normal push. Both ticket values are read back from the bare remote; all four commits remain represented in history.
2. Same ticket, divergent commits: the losing caller receives the second rebase failure. Reflog records two aborted attempts. Its original HEAD and typed handback survive unchanged, no new body is written, the checkout is clean, and the remote still contains the winning edit.
3. Same ticket, stale checkout: fetch/rebase brings in the winning edit; the old expected revision is rejected and the winning body remains unchanged.

## Manual verification

Use disposable control repositories, never the live fleet control repo.

1. Seed tickets A and B in a control repo, publish it to a disposable bare remote, and clone it for a second app instance. Both clones must track origin on the same branch.
2. Leave an agent handback commit for A unpushed on the first instance. Edit B on the second instance and publish its commit. Update A using its current revision on the first instance, then perform a normal push. Inspect the remote: both the typed handback on A and the edit on B must exist.
3. Repeat from fresh clones, this time editing A on both sides before publishing the second instance. Attempt a further A update on the first instance. It must report failure after two rebase attempts. Compare its HEAD to the pre-attempt SHA and inspect the handback JSON; neither may be lost. The remote winning edit must also remain intact.
4. From fresh clones, load A on the first instance, publish an A edit from the second, and submit the stale first-instance edit. Expect an expected/current revision error.

Reproduce the automated Git surface checks with the targeted command above. Logs are beside this bundle: `XNAUT-297-targeted.log`, `XNAUT-297-mutation.log`, `XNAUT-297-rust.log`, `XNAUT-297-rust-bin.log`, and `XNAUT-297-ui.log`.

## Handback

Implementation and required checks finished. No live two-machine app exercise or sandbox verify record was produced; the real Git tests exercise the shared Rust mutation function directly. The 36 existing ignored Rust tests were not enabled. Branch is committed for NautBot review, not merged or pushed.
