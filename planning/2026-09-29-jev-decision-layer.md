# xNaut decision layer — plan and implementation

## Goal

Use small, typed Jev judgments where meaning matters, while xNaut owns permissions, constraints, execution and evidence. Inspect recommendations and their actual effects under Observatory → Decisions. All inference goes through NautGate; this device's xNaut receipts alone feed its cost card.

## Delivery sequence

| Stage | Judgment | Inputs and fixed checks | Completion evidence |
| --- | --- | --- | --- |
| 1 — this implementation | Which tools should be available first? | Current request, bounded recent conversation, already-permitted tool catalog. Preserve discovery, 128-tool limit, original schemas and execution permissions. | Batched relevance questions, Off/Shadow/Active, deterministic fallback, persistent decisions + actual tool calls, cost receipts, API/error/UI tests and a separate preview. |
| 2 | Which agent fits the task? | Explicit owner assignments take precedence; only registered, enabled agents with valid repos, runtime/auth and capability coverage qualify. | Labeled task set; matching accuracy, reroutes, completed jobs and no unauthorized assignments. |
| 3 | Where should the agent run? | Explicit local/exe.dev/GitVM choice takes precedence; exact availability, repo access, privacy policy and budget filter candidates before ranking. | Resource fixtures and live sandbox lifecycle tests; execution location shown in receipts; no silent provider substitution. |
| 4 | Which evidence/context is relevant? | Candidate history/notes/documents from scoped retrieval; preserve source IDs and provenance. | Recall against labeled evidence, reduced context tokens, recoverable omitted evidence, injection tests. |
| 5 | Which checks/review path does work need? | Repository policy and required security/release checks stay mandatory; Jev may recommend additional checks or flag evidence gaps. | Known failure corpus, false-negative analysis, independent verification. No Jev-only security sign-off or approval. |

## Stage 1 behavior

Each independent tool-relevance question runs in one batched System One request, not one network call per tool. Use Noul per candidate because an audit may need several tools. The value is probability of relevance, not a separate confidence estimate or permission to execute. A configurable threshold and top-K limit determine preloading; they are product policy, not a proven model accuracy claim.

Off makes no Jev calls. Shadow records the proposed set but leaves the existing catalog unchanged. Active preloads selected schemas with search/load still available for every permitted tool. Unknown IDs, malformed/incomplete probabilities, oversized input, timeout, unavailable credentials or gateway failure leave the baseline catalog intact. No direct TypeSafe bypass. No retry that silently spends twice.

Selection is once per agent chat turn, before the tool loop; tool outcomes are recorded afterward under the same decision. No recommendation is reported as executed. Every selected, not-selected, later-discovered, successful, failed and rejected tool can be distinguished in the inspector. Records persist locally in SQLite, with bounded retention; settings default Off for new installations. The owner can choose Shadow or Active on the Decisions page. Experimental thresholds are visible.

## Dashboard reference

Reviewed local whichskills/skill-dash (`public/app.js`, `public/index.html`, MIT, Copyright 2026 48Nauts). Adapt the compact table → inspector pattern, relevance bars and inspectable question/answer/usage receipt. Preserve xNaut's colors and typography. Add search, status filter, 5/10/25 pagination, refresh and a readable empty/error state. Inspecting history never triggers paid inference. Off/Shadow/Active and policy controls belong at the top of this page. A synthetic connection check, if run, must be explicitly initiated and recorded as a test, never as real agent work.

## Verification and rollout

- Unit: request shape, batched independent questions, bounded context, probability validation, stable ranking, threshold/top-K, missing/extra IDs, unknown prices and missing usage, no changes on fallback.
- Transport: authenticated NautGate `/v1/systemone`, explicit xNaut/session attribution, no redirects/retries, timeouts and malformed responses. Never emit credentials to browser or logs.
- Integration: 155-tool selection stays below 128; unselected tools remain discoverable; exact original schemas execute; shadow doesn't change the catalog; actual calls and usage join the right record; filtering never expands permissions.
- Persistence: concurrent records, reload, pagination/filtering, corruption surfaced as unavailable, settings preserving unknown fields, bounded history without deleting monthly usage totals.
- UI: Decisions navigation, keyboard tabs, mode changes, filters/pagination, selected-versus-used, raw receipt, xNaut cost and failure states; screenshot review at desktop and smaller widths.
- Evaluation: synthetic requests for audit, ticket lookup, drawing, plugin repair, unrelated chat and ambiguous follow-ups. Compare required-tool recall, preload sizes and provider latency/cost. A small fixture run is not production reliability or savings proof.
- Package from the preserved preview lineage, push Forgejo branch, record Obsidian handover; do not stop the owner's running app.

## Gateway discovery

The local NautGate source has the shared-key `/v1/systemone` route, but the currently configured gateway returned HTTP 404 on 2026-09-29. Historical UAT docs show that route was previously deployed on Stargate. Investigate deployment separately without overwriting its dirty working tree or interrupting current chat. The app must make this dependency visible and fall back safely until a compatible route is reachable.

## Implementation handover

See [implementation report](2026-09-29-jev-implementation-report.md) for delivered behavior, evaluated cases, local compatibility deployment, Paper repair, model-picker scope and the outstanding Astra/subscription gateway work. These gateway constraints are not hidden by a green UI probe. Stage 1 does not execute a security audit, choose agent permissions or waive verification.
