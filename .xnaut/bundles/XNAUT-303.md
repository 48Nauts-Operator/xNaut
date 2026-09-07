# XNAUT-303 handback: shared plan and sign-off jury

@codex on tron.candoo, 2026-09-07. Worktree branch `agent/codex/xnaut-303`. Owner approved plan `in-8b318530-c9d0-4ea4-acc8-b3ba47f1d700`; resumed dispatch session `8f8fde0f-6676-4a72-bbd9-78808a6eb2ad`. This replaces the earlier baseline-only report.

## Result

Plan review and NautBot sign-off use one jury service: policy classification, two blind parallel headless reviewers on different configured runtimes, registry accounting, evidence-shaped rubrics, recorded decisions, owner escalation and revocable inbox notifications. Missing, late, uncertain, conflicting or malformed votes never approve. Both change requests return merged reasons for at most two revision rounds. Additions to a plan receive a new review of the changed input.

Policy loads from the owner-controlled project record's `approval.toml`, then the global config, then shipped defaults in `.xnaut/approval.example.toml`. Worktree files cannot override their own review policy. Defaults are Codex + Claude (XNAUT-302 repaired Claude on tron), threshold 0.75, deadline 600 seconds, spend estimate ceiling 5, integration `feat/xnaut-264-orphan-reap`. Gemini remains configurable. Malformed/unreadable overrides fail closed. Owner flags, outside-worktree paths, scope/spend uncertainty, protected release/permission/secret paths and outward/irreversible actions escalate independently of reviewer scores. Main is never an integration target.

Review processes have separate private authentication/config directories, disabled tools, a native macOS file sandbox denying peer/control/worktree data, and identical input hashes with prior votes removed. Codex uses its isolated auth store; Claude copies the effective Keychain credential first and refuses one expiring before the deadline rather than refreshing a shared login. File-backed stdin avoids a blocked prompt pipe defeating the deadline. Process groups are killed on timeout; authentication copies are removed afterward. Unsupported OS isolation escalates to owner.

Sign-off requires NautBot completion, passed verification/evidence, a typed handback, accepted unfinished work, exact bundle totals and the verified clean source SHA. Both approvals permit a clean local integration merge with compare-and-swap publication, followed by a registered `kind=verify` full build/suites. Conflict, changed scope/policy/source, or a checked-out integration branch escalates. Red verification revokes sign-off, actually reverts the merge and returns the ticket to its author with failure evidence. Each verification retains its own logs. A dirty verification clone's diff is preserved before cleaning that disposable clone for compensation.

Revocation blocks the ticket first, then uses registry retiring/undead states and stop/prove/release sequencing. A still-live writer retains its lease. Recovery covers missing reviewers, interrupted decisions, interrupted merges/verifications and pending compensation. Owner approval preserves the actual reviewer votes as separate authority. Foundation text explains reviewer approval and revocation; Mesh shows an actionable revoke button.

## Full verification

From this worktree:

```sh
source .xnaut/bundles/XNAUT-303-env.sh
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
XNAUT_TEST_PORT=4291 npx playwright test
```

- Full Rust: **945 passed, 0 failed, 40 ignored**, 16.82 seconds, exit 0.
- Full explicit binary suite: **945 passed, 0 failed, 40 ignored**, 17.78 seconds, exit 0.
- Full Playwright: **120 passed**, 2.6 minutes, exit 0. Includes the rendered Mesh revoke interaction.
- `git diff --check`: clean.

XNAUT_TEST_TOTALS={"rust":[{"passed":945,"failed":0,"ignored":40}],"ui":[120]}

The totals line describes one full verification of each suite; the second Rust invocation is a repeat check, not additional tests. Environment isolation follows XNAUT-300: worktree-local state and TMPDIR, Git discovery ceiling, and serial legacy tests that mutate process-global environment. The new environment file also redirects inbox and verify stores. The prior fixed-port NautGate unit test now supplies its own ephemeral listener. Existing compiler warnings remain unrelated to this ticket.

Logs: `.xnaut/test-state/jury-final-rust.log`, `jury-final-bin.log`, `jury-final-ui.log`.

## Mutations and recovery proof

1. Changed the absence decision arm to return approved, then ran `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut jury::tests::absent_reviewer_never_approves -- --exact --nocapture`. Exit 101, **0 passed, 1 failed**, actual assertion `Approved` versus `Owner`. Restored the arm.
2. Removed the real rollback call after red integration and substituted state `integrated`, then ran `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut jury_signoff::tests::red_integration_reverts_git_and_returns_ticket_to_author -- --exact --nocapture`. Exit 101, **0 passed, 1 failed**, actual assertion `integrated` versus `reverted`. This fixture leaves a tracked file dirty and exits 17. Restored compensation and ran both full Rust suites above.

Neither failure was a compiler error. Logs: `.xnaut/test-state/jury-mutation-absence.log`, `jury-mutation-revert.log`. No mutation remains. Other tests exercise the native sandbox, live TERM-resistant writer/lease retention, owner decision staleness, merge conflict, idempotent compensation and interrupted supervisor recovery.

## Live tron proof

```sh
source .xnaut/bundles/XNAUT-303-env.sh
XNAUT_JURY_PROOF_REVIEWERS=codex,claude cargo test --manifest-path src-tauri/Cargo.toml jury_proof:: -- --ignored --nocapture
```

**2 passed, 0 failed**, 69.55 seconds, exit 0. Full log: `.xnaut/test-state/jury-live-final.log`. Portable receipt: `.xnaut/bundles/XNAUT-303-live-proof.json` (actual reviewer votes, IDs, hashes, paths, merge/revert receipts).

As in XNAUT-300, the proof uses an isolated real Git PM ticket repository (`XNAUT-930`) and local production functions, with a registered `/bin/sleep` dispatch process. Unlike the earlier registry-only proof, the reviewers are real authenticated Codex and Claude processes. It runs actual Rust and browser fixture suites, files the typed handback, settles through NautBot's production verification boundary, and invokes both jury gates. It does not replace the installed app, run its GUI timer, mutate the owner's integration checkout, or claim a push was delivered to a physical phone. The production owner-escalation path calls the existing push service; the isolated proof checks persisted inbox escalation.

Lifecycle evidence root: `/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-303/.xnaut/test-state/tmp/xnaut-jury-lifecycle-7031033c-4f0f-41db-85d9-55558df9cd67`.

| Gate | Codex run / confidence | Claude run / confidence | Receipt |
| --- | --- | --- | --- |
| Plan | `01M1XFEF5Q9D494MKW05SRYTDZ` / 0.94 | `01M1XFEF6P95R8YWQV413FMPBV` / 0.82 | `in-63351c13-6343-41a1-9044-c9b0d701d22f` |
| Sign-off | `01M1XFF52J9Y7V73AS3AQRVRQK` / 0.96 | `01M1XFF54095GTCTXP8VF8Z1JC` / 0.86 | `in-ca3fc8e0-a187-43cb-bbec-3f5af3ce8ba2` |

Both pairs' actual registry manifests were read back as `kind=review`, state `done`. The fixture's Rust and UI assertions rejected baseline (exits 101 and 1) and passed after implementation. The handback names every file from Git's actual diff, including Cargo.lock.

- Green integration run: `01M1XFG1BS9X4AVGZQT086F2VM`; `cargo build`, `cargo test` (1 unit test and 0 doc tests), full fixture Playwright (1 test), all exit 0.
- Merge: `3d612bacbf8abab32569e8cfe225d8c80d136399` into the configured integration branch in the isolated repository.
- Deliberately appended a failing Rust test after sign-off; verification `01M1XFG40Z8ART2JC6GKXQTTKV` failed.
- Compensation commit: `ece3007f3208fb17b0339b65dde9132a1fa46222`. Ticket sign-off read back revoked, ticket returned to `in_progress` owned by codex, integration tree exactly matched baseline.
- Killed-reviewer job `fe1dddad-4888-476f-bc68-bbacf5b8ca28`: SIGKILLed Codex `01M1XFE1CY9HRSMNQBK43B7GVW` while Claude `01M1XFE1F7973SHYA4P4GCDZP4` independently approved at 0.79. Result owner escalation `in-f8e8b27c-63ef-43cf-9b34-22c94493d221`, never approval.

Earlier live attempts are preserved locally: Gemini missing login, stale file-based Claude auth, a valid 0.72-confidence escalation, a spend-cap refusal, and a sign-off disagreement where Codex caught the omitted Cargo.lock. These prompted concrete fixture/auth corrections, not reduced thresholds. A successful merge plus dirty-clone revert failure exposed the compensation fix; the final command above passes the corrected path.

## Configuration and manual review

1. Inspect `.xnaut/approval.example.toml`. Owner overrides belong in the project record or global config. Author plus two parallel reviewers requires at least three concurrent slots under the existing spend ceiling; the live fixture explicitly sets 3 in isolated state. Production owner ceilings remain authoritative and were not changed. The integration branch must not be checked out in another live worktree.
2. Run the live command above, then inspect each printed root's `registry/<run_id>.run.json`, `registry/jury/<job-id>.json`, and `control/projects/XNAUT/tickets/XNAUT-930.json`. The shared isolated inbox is `.xnaut/test-state/inbox`; match the receipt IDs above. Read each integration proof JSON and its separate command logs, plus the saved pre-revert diff.
3. Run the full suites. In Mesh, inspect a jury approval's two reviews and click **Revoke jury approval**; the ticket should block before any lease release. The UI suite exercises the action; the Rust suite exercises real process-stop/lease/revert behavior.
4. To review failure policy, run the missing/uncertain-vote tests and the red-integration test. A missing reviewer must never approve, and a red integration must change Git, not only ticket status.

## Not finished

Nothing remains in the approved implementation and isolated tron proof. Installed-app deployment, physical push delivery and NautBot's independent acceptance are separate from this handback. This session's resumed token authorizes Mesh but its agent metadata is absent from `/v1/tickets/mine`; attribution is retained here, in commit authorship, and in @codex ticket progress rather than fabricated session metadata.
