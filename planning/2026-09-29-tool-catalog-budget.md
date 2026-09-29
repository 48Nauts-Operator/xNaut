# Tool request budget and a possible Jev selector

## Incident

Cortana's JobUp security-audit request failed before executing tools. The gpt-5.6-sol route rejected 155 definitions because it permits at most 128. Changing models is not the primary remedy: xNaut assembled native and connected MCP tools without enforcing a request budget.

This patch repairs request construction. It does not perform a JobUp audit, verify remediations, or grant a security sign-off.

## Implemented behavior

- Catalogs of at most 128 tools retain their existing schemas and order.
- Larger catalogs keep core tools and expose `xnaut_search_tools` / `xnaut_load_tools`. Search matches names and descriptions and supports empty-query enumeration with 20-result pagination. Loading exact names makes their original schemas available on the following model response.
- Loading replaces the previous optional selection; the core remains present. Every request stays at or below 128, including when the native catalog grows. All permitted tools remain discoverable rather than being truncated permanently.
- Discovery is built after document/read-only filtering and from this agent's connected plugin capabilities. Calls must belong to the set advertised for that model round. A load call cannot authorize a sibling call in the same response.
- Discovery/loading are metadata operations and do not count toward work performed. MCP error results are not successful actions.
- A tool-array-size rejection is identified as request construction failure instead of advising the owner to switch models.
- Existing model, gateway, permission checks, maximum-round limit, and execution handlers remain in place. No new inference provider or API credential is required.

## Verification

Catalog tests cover 0/42/128/129/155/500 definitions, all 500 tools remaining discoverable and loadable, original schema preservation, replacement and capacity limits, invalid selections leaving state unchanged, search ranking, filtered-out tools remaining inaccessible, and the per-round advertisement snapshot.

An isolated HTTP/SSE model fixture and HTTP MCP fixture exercise the actual turn loop with 155 combined definitions. They perform discovery, loading, a deliberately premature rejected tool call, then execution of the last optional tool. A second case stops after loading and verifies that no work is recorded as performed. Neither case accesses the owner's projects or external model accounts.

Full-suite and bundle results are recorded in the final verification section below. These tests do not establish live Cortana/JobUp audit success.

## Jev opportunity — proposed, not enabled

The owner suggested using Jev to decide which tools a job needs. This is a useful semantic selection task. It supplements the request budget rather than replacing its deterministic enforcement.

Suggested first experiment:

1. Filter capabilities and permissions in code, producing the permitted catalog.
2. Give Jev the current request, a bounded relevant conversation summary, and candidate names/descriptions. Never include credentials. For a multi-tool job, use independent relevance questions per candidate, batched together; do not force a single mutually exclusive tool choice.
3. Use the ranked results to preload a bounded optional set. Keep the discover/load fallback so omissions do not become permanent capability loss.
4. On timeout, missing key, unknown IDs, invalid output, or an uncertain selection, use the deterministic catalog path. Never let this selector expand permissions or approve deployment/security sign-off.
5. Measure against the deterministic baseline on real xNaut requests before making it the default. Compare essential-tool recall, completed workflows, discovery rounds, model input tokens, end-to-end latency, and actual cost. Include ambiguous requests, multi-step audits, unavailable tools, and adversarial instructions in tool descriptions.
6. Send inference through NautGate according to project routing policy once its TypeSafe contract is verified; record only xNaut-originated Jev usage in the Observatory ledger.

Begin in shadow mode: record proposed choices without affecting execution. Then enable preloading when measured behavior warrants it. No fixed confidence threshold or claimed speed/cost savings is justified yet.

Other candidates include agent matching, execution-location recommendations (local/exe.dev/GitVM after deterministic eligibility filtering), and evidence relevance. Availability, workspace existence, authentication, explicit owner choices, resource limits and approval requirements remain exact checks in code. Security sign-off requires actual verification evidence, not a classifier's confidence.

Sources reviewed 2026-09-29:
- https://docs.typesafe.ai/llms.txt
- https://docs.typesafe.ai/cookbooks/function_calling
- https://docs.typesafe.ai/cookbooks/skill_suggestion
- Local TypeSafe skill: /Users/cand0rian/.agents/skills/typesafe-ai/SKILL.md

## Final verification

- Full Rust suite: **1,452 passed, 0 failed, 48 existing ignored**.
- Focused catalog run: **10 passed**, including eight new regression tests and two existing catalog checks.
- Frontend assets rebuilt successfully; no frontend source changes in this patch.
- Tauri debug application bundle built successfully; copied bundle passed `codesign --verify --deep --strict` after ad-hoc signing with project entitlements.
- `git diff --check` passed.
- Separate preview: `/Users/cand0rian/xnaut-testing/builds/2026-09-29-tool-catalog-budget/xNAUT Tool Catalog Preview.app`.
- Evidence: `/Users/cand0rian/xnaut-testing/runs/2026-09-29-tool-catalog-budget/`.
- Branch: `fix/tool-catalog-budget`, based on previous preview `8ff948b`, so earlier Observatory/jury/voice/session fixes are included.
- The active owner's app was not stopped or replaced. No release/installation, live model audit, remediation verification, or security sign-off was performed. A live Cortana request in the new preview remains the acceptance check.
