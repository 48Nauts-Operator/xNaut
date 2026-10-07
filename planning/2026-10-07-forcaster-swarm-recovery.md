# Forcaster swarm regression: repair and release coverage

Investigation and source changes by Codex, 7 October 2026. This report supersedes the first diagnosis, which treated the new Fleet restriction as a requirement to preserve. That was an incomplete repair: making a regression more visible does not restore working dispatch.

## What changed

Commit `7d1bf19a4666ba5f2f80ffb8edd94f710ab20ac9`, authored 6 October at 17:30:36 +02:00 and committed at 17:33:11, replaced immediate swarm dispatch with persisted approval and sweep-driven queue refill (XNAUT-239).

Before this change, `swarm_plan::dispatch_plan` called `pm_ticket_dispatch` for each approved member. Afterwards it approved the group and called `refill`, which returned `Ok(())` immediately whenever the machine was not Fleet. This silently applied the unattended-dispatch restriction to an explicit approved action on the owner's Workstation.

The same commit added queued members but changed only the `remember(plan)` error handling in the tool adapter. It left the old response denominator (started + failed) and the instruction that plans were consumed. That response no longer matched the durable queue. Five queued tickets were presented as “0 of 0”. Nothing disappeared from storage.

The swarm tool also lacked an execution-destination parameter from its introduction in `c5ee3af` on 13 September. That is a separate older interface gap, not evidence that NautBot forgot a request. Commit `f967f43` on 6 October pinned plans to the profile's resolved environment. All five installed profiles were local, so the new plan accurately exposed local destinations instead of the requested exe.dev route.

The suspect October 6 commits use Git identity `Cand0rian` without an agent/session trailer. Searches of available Codex, Claude, project journal and test-run records found no independent attribution for `7d1bf19`; the one Codex match was this investigation. The Git account does not establish which human or agent authored the change.

## Observed installed state

- Installed application: 1.30.2, source release commit `8e9274d`; instance role Workstation.
- Three FORCASTER groups remained approved, each containing five queued members. Latest: `swarm-dfd6d80cfe09496f9a31a7d49c835c75`.
- Latest repository: `ssh://git@cosmos.tail138398.ts.net:2222/48Nauts/forcaster.git`.
- Recorded failed attempts were pre-execution admission refusals. No evidence of a started FORCASTER worker.
- Invalidating old approvals when the repository changed was correct and remains enforced.

## Repair

Worktree: `.worktrees/forcaster-swarm-recovery`, branch `fix/forcaster-swarm-recovery`. The earlier partial patch is commit `ef5f983`; the additional regression repair and release gate are working-tree changes.

1. Preserve an explicit destination through tool planning, persisted approval, configuration validation and native dispatch. Unknown or unconfigured explicit providers fail instead of falling back to local.
2. Separate unattended ticket admission from approved remote-group admission. Workstation may advance approved exe.dev/GitVM groups. Local swarm workers and unsolicited ticket dispatch remain restricted; Sandbox cannot dispatch. This does not change saved instance roles or broaden arbitrary automatic work.
3. Recheck role, kill switch and approved destination during confirmation, refill and immediately before scoped native dispatch. Scope/model/repository/runtime checks, capacity limits, findings review and run reservations remain in place.
4. Report total membership, queued tickets and per-member reasons. Repeated confirmation reloads the same durable group rather than inventing a consumed-plan failure.

The regression tests use the same confirmation store, policy, refill loop, real run registry and capacity arithmetic as production, with a fixture launch backend. They do not create cloud VMs or claim to verify SSH/repository/model availability.

## Why existing checks missed it

Correction after reading the Shared Agent Vault and this morning's release harness: the release already had substantial validation. The 7 October handover records 1,721 passing Rust tests, 469 passing frontend tests and real remote multi-agent acceptance on Tron, including overlap, restart, queue refill and repair. The native fixture in `~/xnaut-testing/fixes/20261007-full-tron-470/harness/prepare_fixture.py:18` explicitly configured `instance.role = fleet`. That successful acceptance did not cover the owner's Workstation role. The missing regression case was the role/destination combination, not an absence of end-to-end testing. The old model-facing response contract was also left unchanged when backend semantics changed.

The checked-in release workflow built candidate packages and performed platform smoke checks, but did not require this behavioral matrix before building. The generic CI workflow was triggered by main pushes/PRs; passing build or UI-launch checks did not prove this dispatch scenario. This is a finding about the configured coverage, not a claim to have retrieved every historical hosted CI log.

## Required automated release checks

`npm run test:release` runs 13 selected native suites covering dispatch, destination routing, instance roles, model tools, PM mutation recovery/write guards, project continuity, worker recovery, review/repair, run registry, settings and findings triage. Mandatory named tests must actually execute and pass. Empty, missing or ignored evidence is a failure.

New native scenarios include:

- Five remote tickets from Workstation, both exe.dev and GitVM, both owner-card and model confirmation: three start and two remain queued.
- Repeat confirmation and reload retain approval/run identities and launch no duplicates.
- Finishing a worker admits exactly one queued successor; all five eventually receive one run.
- Unapproved, stopped, read-only and Sandbox cases start nothing and retain stored work.
- Mixed/local/missing/unknown Workstation destinations are refused before approval; legacy local approvals cannot start local workers.
- Changed ticket instructions block the affected member without suppressing independent work.
- A findings refusal on the last member cannot partially approve or launch the other four.

Existing cases also check changed repository/runtime/provider pins, corrupt registries, recovery after interrupted launch, ownership changes and cross-process locks.

Nine browser suites (45 discovered cases) cover swarm cards/status persistence, individual ticket dispatch, approvals, startup console errors, settings lifecycle, sessions/Observatory and ticket links. New browser cases retain all five rows through confirmation/reload, distinguish queued from started, and show a refusal without claiming execution. These use the existing IPC fixture and are not live remote integration tests.

The gate also tests its own evidence parser: skipped cases, expected failures, retries, missing suites, runner errors and zero tests cannot authorize a release. The release browser config refuses to reuse another worktree's server.

`.github/workflows/behavior-tests.yml` runs the gate on macOS and Windows for branch pushes, pull requests and release calls. `release.yml` makes package builds depend on this job. Existing package-launch checks remain. Logs and a source-identified result are retained even on failure. Native-only diagnostic runs explicitly produce a partial receipt. The manual `scripts/release-gate.sh` also requires a passed native-and-browser receipt from the exact clean release commit before accepting GUI evidence; its isolated receipt tests cover missing, stale, dirty and partial behavior evidence.

## Validation and deployment

Validation completed within the available environment:

- Final focused native run: **43 passed, 0 failed**, covering swarm, dispatch, instance roles and the model-tool contract (`/tmp/xnaut-dispatch-final-tests.log`).
- Broader behavior run: **213 passed, 2 failed, 0 ignored**, out of 215 native tests. Both failures require real process observation, which this session blocks: `observe_reads_real_child_cpu_a_real_xnaut_tree_and_a_real_ticket` and `reused_pid_birth_is_not_proof_of_the_original_process`. They remain failures in the gate; neither was removed or marked passing. See `artifacts/release-behavior/native.stdout.log`.
- Regression proof: temporarily restoring the old non-Fleet early return makes the new Workstation remote test fail at **0 started versus 3 required**. Restoring the fix makes the final focused run pass. Evidence: `/tmp/xnaut-dispatch-regression-proof.json` and `.log`.
- Four evidence-parser tests and 12 manual release-gate tests pass, including missing/skipped/failed/retried cases and partial/different-source receipts.
- All 45 selected browser cases are discoverable. Execution is **unverified**: starting the local server is denied with `listen EPERM` (`/tmp/xnaut-swarm-browser-tests.log`).
- JavaScript syntax, ESLint on the changed application JS, shell syntax, YAML syntax and `git diff --check` pass. YAML parsing is not a hosted workflow execution.

No release, installation, role change, ticket reapproval or live remote launch has been performed.

The initial verification session was restricted to workspace/temporary writes and could not bind the browser test server. Full access has since been restored; Tron SSH succeeds. Full browser and signed-package/live-provider verification remains required. Do not treat native success as deployment evidence.

After a verified build is installed, request FORCASTER-2 through FORCASTER-6 with explicit exe.dev destination, review that destination on the card, and approve the new scope. The current local plan must not be silently repurposed. Preserve existing failure records and continuations. A real acceptance check must verify the remote run IDs, repository access and worker progress before claiming that execution works end to end.

## Agent-memory follow-up and access diagnosis

The Shared Agent Vault `Projects/xNaut/xNaut.md` records the completed 1.30.2 release at 2026-10-07T09:26:01Z. Its latest canonical source includes harness-only corrections through `a2d26a9` (PR159), beyond the immutable application tag `8e9274d`. A future release integration must retain those later harness corrections; do not publish this older-base branch as a replacement for canonical dev.

Tron is the established Mac mini build/test host: SSH alias `tron`, user `zelda`, tailnet address `100.104.49.67`. Its checkout is `/Users/zelda/DevHub_Studio/factory/02-Development/xnaut`; remote commands need `zsh -lc`. Reuse the SSH-native harness and isolated test app. Do not control the owner's Studio screen or replace the owner's running app.

The current task's own recorded permissions changed from `danger-full-access` at 2026-10-07T13:45:47.240Z to `workspace-write` with `network_access: false` at 2026-10-07T20:13:45.262Z (22:13 Zurich). Evidence: `~/.codex/sessions/2026/10/07/rollout-2026-10-07T15-43-51-01a1169b-2f28-7830-9710-1825b5a94dd8.jsonl`, lines 8 and 1224. This explains why earlier release access and current tool access differ. Direct SSH to the memory-confirmed IP returns `Operation not permitted` before authentication. No new Tron test was started. Access must be restored by the session controller; do not route around that restriction with another execution surface.

Full access was restored by the owner after this diagnosis. SSH to the established Tron host now succeeds. The new candidate will retain canonical harness corrections through `a2d26a9` and use isolated test state. The owner app is not the acceptance fixture.
