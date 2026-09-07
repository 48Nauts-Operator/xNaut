# XNAUT-305

Current owner instruction, 2026-09-07: "The spend ceiling was the blocker, not your plan: policy max_spend is now 75 (owner-set). Resubmit your revised plan for XNAUT-305 through plan/review now; it goes to the jury. Then implement as planned and file the typed handback with not_finished empty unless something genuinely remains." The previous USD 5 checkpoint is obsolete; estimate USD 15 is below current USD 75 policy.

The Foundation supplied by the owner explicitly authorizes the local outcome report: "Report an outcome without blocking: POST http://127.0.0.1:51737/v1/inbox/notify. When you finish work that changed files, name them." This is the only notification in this plan.

1. Add an optional run_id to typed handbacks. Bind HTTP and MCP handbacks to the authenticated session run where available; validate ticket/run identity before marking completion.
2. After a reviewable handback is durably stored, mark its matching agent run Done in run_control. Also record Done when an in_progress ticket moves through done/review while its matching run is alive. Preserve failed/retired and unrelated runs.
3. Recover an interrupted registry update from a stored handback with the exact run id before dead-process reconciliation and reassignment. Never infer completion merely from a current ticket status or another run's handback.
4. Add pure completion/dead-process tests and a live test using launch_argv_in, a real child, accepted handback filing into an isolated PM repo, kill, reconcile and durable Done readback. Remove the handback-marks-done call, capture the regression failure, restore.
5. Run full Rust and Playwright suites using XNAUT-300 isolated paths. Write .xnaut/bundles/XNAUT-305.md with totals, live and mutation evidence. Commit locally, file typed handback, notify with changed files, and set ticket done. No UI, swapping changes, push or app restart.

All implementation, fixtures, logs and builds stay inside this assigned worktree. Isolated PM repositories live under .xnaut/test-state/tmp; no owner settings or other checkout are modified. Tests explicitly cover done and review transitions while alive, dead transitions refusing completion, wrong ticket/run/handle, unrelated runs, rejected handbacks, and recovery after an interrupted registry write. HTTP/MCP changes only carry/bind run identity, not an interface redesign. sweep.rs only adds recovery before existing reconciliation; no model-switch behavior is changed.

Estimated spend ceiling: USD 15 equivalent for this local implementation and plan reviewers, with no purchased services or model API calls in tests. Tests launch only local fixture processes. The final notification goes to the owner's local Mesh inbox, explicitly required by the Foundation in this ticket; no email, Slack, forge or other recipients. Ticket progress, typed handback and done status use the authorized PM HTTP surface.

Declared implementation paths:
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/src/run_control.rs
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/src/handback.rs
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/src/agent_hooks.rs
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/src/project_management.rs
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/src/sweep.rs
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/src/jury_proof.rs
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/PLAN.md
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/.xnaut/bundles
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/.xnaut/test-state
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/target
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/node_modules
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/test-results
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/src/sandbox_verify.rs (existing test literals: add run_id: None only)
- /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-codex-xnaut-305/src-tauri/src/jury_signoff.rs (existing test literals: add run_id: None only)
