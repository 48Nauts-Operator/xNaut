# XNAUT-300 Phase 1

Owner approved this plan. Build in the dispatched agent/codex/xnaut-300 worktree. Read the design at /Users/zelda/.xnaut-vault/work/xnaut/Development/features/2026-09-06_Run-Registry.md before implementation.

1. Adapt the manifest, atomic persistence and append-only journal from XNAUT-277 commit 18b7632 into run_control.rs. Keep one durable run record; exclude provider resume, swapping, capture-pattern interpretation, NautGate routes and publishing. Credit the requested historical ancestry and the local source.
2. Allocate and persist a run ID before local agent launch; bind runtime, identity, session, ticket, worktree, process and capture evidence. Persist lifecycle transitions and explicit waiting_on/progress timestamps.
3. Add a pure evidence verdict with named failed proofs, startup grace, terminal-state protection and a waiting exemption for progress timeouts. Add a production sweep reconcile seam that consumes the verdict, persists failed records and returns only the ticket still belonging to that run to the board. Preserve failure recovery across partial writes and app restarts.
4. Test with explicit directories inside the worktree. Exercise the same reconcile seam through the sweep tick with an isolated ticket repository and ledger. Remove its reconcile call, capture the failing test output, restore the call and prove green.
5. Run an isolated live process kill test on this machine with one command and the production reconcile path within 180 seconds. Record run ID, timestamps, ticket changes and ledger rows. Do not touch the owner's live registry/control directory or replace the installed app.
6. Run the full Rust and Playwright suites with state paths redirected to worktree test directories. Write .xnaut/bundles/XNAUT-300.md with totals, mutation evidence, live evidence and remaining limitations. Commit on this branch; file the typed handback and set done only after all required gates pass.

The Phase 2 questions about degraded swaps and branch continuation remain out of scope.
