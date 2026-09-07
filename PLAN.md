# XNAUT-296 implementation plan

1. Add a persisted per-ticket model_requirement string with an empty default. Use explicit model identity matching (trimmed, case-insensitive), with no inferred cross-provider quality ranking. Only configured profiles with a matching model are eligible replacements.
2. Add authenticated hook model reporting and one tested capture detector. Preserve degraded state across ordinary progress hooks and reconciliation. Declared waiting_on prevents swaps.
3. Add Retiring and Undead. Record retirement before sending SIGTERM to the verified child pid and deleting its zellij session. Keep the lease during a bounded grace period; require fresh proof of absent session, dead pid, and unchanged capture over the grace window. Refuse and notify on failed stop proof.
4. After proof, record retirement and release the matching lease, return the ticket to triage with the reason, and preserve its worktree and branch. Admit a matching successor with previous_run_id and reciprocal linkage; dispatch always sends CONTINUE for a swap.
5. Add decision, persistence, ordering, retry, and refusal tests. Exercise real isolated processes through the production wrapper, including a SIGTERM-resistant process. Mutate away the stop guard to demonstrate a failing safety test and restore it.
6. Run the full Rust and Playwright suites with isolated state; write .xnaut/bundles/XNAUT-296.md with totals and live evidence, commit, file the typed handback, and set the ticket to done.

NautGate detection remains Phase 3. No owner sessions or live worktree leases will be altered by verification.
