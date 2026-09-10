# XNAUT-317: parallel integration builds on private clones

Continue branch agent/codex/xnaut-317 at 2ed03c3. No existing implementation or handback was found. XNAUT-315 instrumentation is present in e9c0a9d.

Choose the ticket's explicitly allowed parallel-clone alternative. Production edits are confined to src-tauri/src/jury_signoff.rs. Keep existing merge journaling, compare-and-swap publication, compensation commits, and returning red tickets to their authors.

1. Limit sign-off admission locking to reading/deduplicating and persisting the new review job; release it before reviewer execution and builds. Serialize integration ref preparation/publication and compensation with a separate mutex, released before running npm/cargo in each job's existing private clone. Avoid concurrent publication races without serializing builds.
2. Serialize promotion with publication/compensation. Do not promote a successful historical snapshot over a newer integration tip; record that promotion awaits the current tip's verification. Preserve rollback semantics and owner revocation.
3. Add real Git fixture tests proving two approved jobs enter integration commands concurrently on different clones, both merges remain on the integration branch, and a red concurrent job is reverted while the independent green change remains. Assert no lock is held across the integration commands. Add a mutation check restoring build serialization and capture the expected failing concurrency test, restore and rerun green.
4. Run full Rust and UI suites, record exact own-run totals and mutation evidence in .xnaut/bundles/XNAUT-317.md. Update the linked work-vault design document (create and link a ticket document if the existing links are missing), commit, file typed handback and set done.

The ticket's one-build/two-signoffs and red-batch tests describe its batching alternative. For the parallel alternative, use two overlapping builds with independent red compensation as the equivalent acceptance proof; no batching implementation or batch claims. This plan explicitly requests review of that acceptance interpretation before any production edit.


## Review clarifications

Declared local paths: src-tauri/src/jury_signoff.rs, .xnaut/bundles/XNAUT-317.md, PLAN.md. The ticket separately authorizes the work-vault document API. Its linked Markdown was confirmed missing and its HTML link is unsupported by that API; create and link work:xnaut/Development/features/2026-09-10_XNAUT-317.md after implementation, with the required shipped section. No other production files will change.

Estimated whole-run model/review spend: USD 15, capped below the ticket's USD 20 limit. No paid external test infrastructure or deployment is planned.

Own baseline results: cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut AND cargo test --manifest-path src-tauri/Cargo.toml each passed 1076, failed 0, ignored 45. XNAUT_TEST_PORT=4291 npx playwright test passed 150 after npm ci. Mutation is still pending implementation.

Measurement: the live doctor is version 1.26.3 and provides no throughput field. Applying the existing throughput.rs recent_commits counting rule to local dev for 24 hours found 14 commits and zero NautBot merges, or 0/hour. This limited local observation does not establish a machine-wide bottleneck or a speedup. The serialized build bottleneck is established structurally by the SIGNOFF_LOCK lifetime; concurrency fixtures will prove its removal.

Extend verification to stale-snapshot promotion (a late green cannot promote an obsolete snapshot), compensation/publication interleaving (the newer independent commit survives), approval revoked before publication, and revocation during concurrent verification. Keep the existing red-integration and restart/revocation tests.

The automatic review could not run its Claude reviewer because its credential expires before the review deadline. Owner approval remains required if that reviewer is unavailable or to settle the parallel-alternative acceptance interpretation. No production implementation will start on pending.


## Approved continuation, 2026-09-11

The owner approved this plan on 2026-09-10 (in-b0ce7e5c-0784-4c20-82dc-436bd32fe589), and authorized rebasing after the integration conflict. Rebased original implementation 596772a onto origin/dev a25182b. The only conflict was resolved by retaining XNAUT-319 swarm refusal before XNAUT-317 admission reuse. No change to approved scope or acceptance interpretation. Fresh suite and repeated mutation results are recorded in .xnaut/bundles/XNAUT-317.md.
