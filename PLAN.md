# XNAUT-296 implementation plan

1. Add a persisted per-ticket model_requirement string with an empty default. Use explicit model identity matching (trimmed, case-insensitive), with no inferred cross-provider quality ranking. Only configured profiles with a matching model are eligible replacements.
2. Add authenticated hook model reporting and one tested capture detector. Preserve degraded state across ordinary progress hooks and reconciliation. Declared waiting_on prevents swaps.
3. Add Retiring and Undead. Record retirement before sending SIGTERM to the verified child pid and deleting its zellij session. Keep the lease during a bounded grace period; require fresh proof of absent session, dead pid, and unchanged capture over the grace window. Refuse and notify on failed stop proof.
4. After proof, record retirement and release the matching lease, return the ticket to triage with the reason, and preserve its worktree and branch. Admit a matching successor with previous_run_id and reciprocal linkage; dispatch always sends CONTINUE for a swap.
5. Add decision, persistence, ordering, retry, and refusal tests. Exercise real isolated processes through the production wrapper, including a SIGTERM-resistant process. Mutate away the stop guard to demonstrate a failing safety test and restore it.
6. Run the full Rust and Playwright suites with isolated state; write .xnaut/bundles/XNAUT-296.md with totals and live evidence, commit, file the typed handback, and set the ticket to done.

NautGate detection remains Phase 3. No owner sessions or live worktree leases will be altered by verification.

---

# XNAUT-303 implementation plan

Implement the supplied build spec in this assigned worktree and branch. The named vault document was read; its local copy lacks the two approval sections, so the dispatched ticket is the design authority.

1. Add one durable jury module shared by plan and sign-off gates. Load approval.toml with configurable reviewer runtimes (codex + gemini on tron), threshold 0.75, review deadline, spend ceiling, and integration branch feat/xnaut-264-orphan-reap. Invalid policy escalates. Owner-only flags, irreversible/outward actions, protected paths, work outside the assigned tree, and scope expansion override reviewer scores.
2. Register two isolated headless kind=review runs before launching in parallel. Give each identical ticket/evidence/rubric inputs without the other review. Persist identity, confidence, reasons, deadline, round, and reviewed content/commit identity. Both confident approvals approve; two change requests return merged notes for at most two rounds; disagreement, low confidence, missing/failed reviewer or expired deadline escalates with evidence and push notification.
3. Connect the plan endpoint to this jury, preserving the existing owner canvas for escalations. Record decisions on the review and ticket and announce every approval with revocation support. Reject stale decisions when the reviewed inputs changed.
4. Connect the same jury after NautBot completion. Require green verification and evidence rule, complete handback, scope/path checks, accepted outstanding work, and bundle totals matching verification. Merge only the reviewed commit into the configured integration branch. Conflicts escalate. Persist sign-off and merge SHA.
5. Run integration build/full verification as kind=verify. On red, revoke and revert the recorded merge, then return ticket to its owner with the failure. Revocation blocks the ticket and uses proven process-stop-before-lease-release semantics; if the prerequisite registry stop path is absent, implement the necessary bounded stop/proof seam without model switching.
6. Update Foundation plan/review guidance to describe reviewer decisions and revocation. Add policy/decision, absent-reviewer, stale-review, revoke and integration rollback tests, with mutation evidence for absence and failed-build revert. Run cargo test --manifest-path src-tauri/Cargo.toml, cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut, and XNAUT_TEST_PORT=4291 npx playwright test.
7. Produce the required real tron proof: two review registry records, plan approval and notify, killed-reviewer escalation; a real ticket through sign-off, integration merge and green build, then deliberate failing-test rollback. Any live shared-surface operations requiring owner permission are presented concretely before execution. Record unavailable prerequisites as unfinished rather than simulated proof.
8. Write .xnaut/bundles/XNAUT-303.md with commands, totals and manual checks, commit completed changes, file typed handback and set done only when the required verification and live proof pass.

Spend estimate: local implementation/testing plus two headless reviewers per gate; four reviewer runs for a successful end-to-end ticket, plus failure-path proof runs. No paid external purchases or release/main promotion. Reviewer launches must obey the existing admission/spend gate.
