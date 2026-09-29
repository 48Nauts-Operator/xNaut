# Jev tool decisions, model selection and agent access — preview

Date: 2026-09-29. Branch: `feat/jev-tool-selection`, based on `2cea331` (the bounded tool-catalog preview). This is a separate preview, not a published release.

## Delivered behavior

- Observatory → **Decisions**: whichskill-inspired table and evidence inspector, search, status filter, 5/10/25 pagination, filtered totals, relevance probabilities, preloaded schemas versus actual calls, latency, estimated cost, raw receipt/copy and explicit connection test.
- Jev **Off / Shadow / Active**. One batched relevance request per chat turn through NautGate. Active preloads up to the configured limit, while search/load keeps every permitted tool reachable. Permissions, required approvals and the 128-schema ceiling stay in code. Empty, invalid, unavailable or timed-out judgments fall back to the bounded catalog.
- Settings → Tasks Mode → **Chat LLM** now has route and model pickers. Chat tabs also have a per-conversation model picker. Local LM Studio/Ollama and NautGate's advertised OpenRouter, OpenAI/Codex and Claude models are grouped. An exact model ID can still be entered in settings.
- Agent Settings → **Chat model** persists a separate chat provider. Changing chat to a gateway model does not change the model/provider passed to the coding CLI. Legacy settings synchronization cannot replace an explicit workspace chat choice.
- Ordinary action requests that stop at prose get one bounded correction. No tool is forced, no action is repeated, and a valid coding-session handoff or a read-only panel is not retried. Clarification and refusal remain possible.
- Removed the contradictory blanket claim that agent chat has no filesystem/network tools. Actual attached schemas and discovery determine availability. A proposed task mentioning “verification” no longer produces a false action-claim warning; a coding-session handoff is not an execution receipt.
- The route probe uses a modern token budget, adapts explicit parameter rejection, and requires a returned `ping` function call. HTTP 200 alone is not proof. Both ordinary HTTP and SSE-wrapped reasoning errors are covered. Authentication/funding failures do not trigger repeated requests.
- Paper is a disabled-by-default local plugin in the public catalog. Codex launches now carry HTTP MCP plugins as well as stdio plugins, with header secrets passed by environment-variable name instead of command-line value.

## Live findings and limitations

| Check | Observed result | Meaning |
| --- | --- | --- |
| GPT-5.6 Sol through configured NautGate | Actual ping call; 1,669 ms | The route can return function calls. No project tool was executed by the probe. |
| OpenRouter / Gemini 3.7 Flash through NautGate | Actual ping call; 3,604 ms | The OpenRouter tool route worked through the required gateway. |
| Claude Fable 5 through NautGate | Anthropic API credit error | This deployed route is using a metered API account. A model appearing in the selector does not prove subscription coverage or available funds. |
| GPT-6 Astra / Chat Completions | Rejects `none`; without it rejects tools + reasoning | Parameter cleanup cannot provide a compatible transport. |
| GPT-6 Astra / standard NautGate Responses endpoint | `Missing required parameter: tools[0].function` | The gateway translates Responses back to Chat Completions. Native Responses support is a separate unfinished integration. Do not label Astra's tool route fixed. |
| Local Paper, xNaut MCP client | Initialize and tools/list; 35 tools, including `paper__get_basic_info` and `paper__list_files` | MCP discovery works. No document was edited. |

The model picker lists configured models; it does **not** create or repair gateway subscription accounts. Subscription routes must be exposed and tested in NautGate. No automatic switch to another model or a different billing route was introduced. The user’s existing model selection was preserved.

A Cortana clean-clone JobUp audit has **not** been executed, verified or signed off by this work. Tool-selection/probe tests are not a security audit. Cortana's persona refers to runtime-only `xnaut_security_audit`/plan tools, which are not necessarily in the chat catalog. Discovery or a clearly marked coding-session handoff is required when the connected chat tools cannot do the work.

## Jev evaluation

The live checks used synthetic requests and the real NautGate → TypeSafe route. Tools were selected but never executed.

| Request | Selected | Catalog → schemas sent | Time | Input tokens | Estimated Jev cost |
| --- | --- | --- | --- | --- | --- |
| Read XNAUT-440 | `list_tickets` | 43 → 3 | 763 ms | 9,259 | $0.000389 |
| Draw frontend/backend | `update_canvas`, `read_canvas` | 43 → 4 | 366 ms | 9,255 | $0.000389 |
| Audit JobUp | evaluation-only audit tool, `write_document` | 43 → 4 | 365 ms | 9,269 | $0.000389 |

The audit case initially failed: its candidate catalog had no audit tool and the test incorrectly expected `list_agents`. That failed run is retained. After adding an explicitly synthetic audit candidate, all three cases passed. This establishes transport and candidate coverage for these examples, not production accuracy, audit completeness or measured workflow savings. Separate fixture tests cover catalogs of 155 and 500 tools, fallback, schema preservation and discovery of omitted tools.

Prices use Jev 1.13.0 input tokens at $0.042/M; unknown model/usage is shown as unknown, not free. Detailed decisions persist in `jev-decisions.sqlite` with the most recent 2,000 completed records retained (in-progress records are protected). Monthly xNaut usage remains in a separate table when detailed receipts are pruned. Other apps' TypeSafe costs are not imported. Recent prose and tool descriptions go to hosted Jev only when enabled; this is not local/private inference.

## Local gateway compatibility deployment

The main Stargate image, `nautgate-uat-core:attestation-receipts-20260928-r7`, returned 404 for `/v1/systemone`. It was not replaced or restarted.

An isolated `nautgate-systemone-xnaut` service uses the previously deployed `nautgate-uat-core:lifecycle-clients-20260923-r1` image with only its native System One router, shared authentication, session limits and usage receipts. It has no chat proxy, background workers or startup migrations. It requires the existing System One database schema.

- Stargate source: `/Users/sg1/.nautgate/systemone-compat/systemone_only.py`.
- Remote bind: `127.0.0.1:18092`; Docker network `nautgate-stargate_default`.
- Local persistent tunnel: `~/Library/LaunchAgents/com.xnaut.jev-nautgate-tunnel.plist`, forwarding `127.0.0.1:18092` to the same loopback port on Stargate.
- Preview Jev endpoint override: `http://127.0.0.1:18092/v1`; uses the existing NautGate client key, never a TypeSafe key in the browser.
- Deployment script copy and tunnel log: `~/xnaut-testing/runs/2026-09-29-jev-tool-selection/gateway/`.

This is a compatibility service, not a full gateway release. Consolidate it into a tested main NautGate build later. To retire it, switch Jev Off or restore the main compatible endpoint first, then unload only this LaunchAgent and stop only this dedicated container. The existing 8090 chat tunnel is unrelated.

## Paper configuration repair on this Mac

Paper's MCP URL was found in `~/.claude.json`, not the Codex global configuration inspected. Paper was listening on 29979. The newest agent, `stark`, had no `plugin:*` capabilities. Added an enabled Paper entry to xNaut's plugin registry and granted only `plugin:paper` to Stark. Other capabilities and profile fields were preserved; protected backups are outside Git in the evidence folder. The running app reads these files on each turn. No process was restarted.

Loopback refers to the machine running the client: this fixes local chat. It does not create a tunnel from exe.dev/GitVM to the Mac. New Codex launches in the preview receive the HTTP connector; already-running CLI processes were not modified.

## Gateway follow-up: native Responses and subscription routing

1. Preserve the requested model, account policy and billing route in NautGate. Do not silently send a subscription request to a paid API key.
2. Provide a native Responses path for Astra or a complete Chat→Responses upstream adapter. Preserve function schemas, call IDs, tool outputs, reasoning-state continuity and streamed events. Do not translate back to the unsupported upstream endpoint.
3. Verify a real two-round tool cycle with Astra, not only HTTP status or a text answer. Cover errors, interruption, repeated tool output and routing receipts. xNaut must use the supported contract end to end; parameter removal alone is insufficient.
4. Expose configured Codex/Claude subscription routes with account/transport metadata. Verify that the chosen subscription route really serves the request. Current Claude failure is API funding, not missing xNaut tools.
5. Retest ordinary Sol, Claude and OpenRouter traffic before replacing the main gateway. Keep the existing app and in-flight sessions intact during preparation.

Sources: [TypeSafe HTTP API](https://docs.typesafe.ai/api), [Noul](https://docs.typesafe.ai/primitives/noul), [OpenAI Responses migration](https://developers.openai.com/api/docs/guides/migrate-to-responses), [function calling](https://developers.openai.com/api/docs/guides/function-calling). Exact local gateway behavior was checked against the deployed service, not inferred only from documentation.

## Verification and package

Final test counts and package path are recorded below after completion. Evidence root: `~/xnaut-testing/runs/2026-09-29-jev-tool-selection/`. Dashboard screenshots render real synthetic-evaluation receipts in a stubbed UI; they are not live audit findings.

### Final checks

- Rust: **1,463 passed, 0 failed, 51 opt-in tests ignored**, `cargo test --bin xnaut -- --test-threads=4`. An earlier unrestricted run intermittently lost one scratch-ledger refusal in `sandbox_verify::tests::a_green_run_on_a_tree_without_the_work_moves_nothing`; no production ledger code was changed to hide that failure.
- Browser: **377 passed** on the full suite. The first run exposed eight standalone voice fixtures missing the new picker module; optional mounting fixed this, then all 36 affected voice/decision/model tests and the complete suite passed. A final provider-registry synchronization correction additionally passed **21 model-settings/Agent Space tests**.
- New JavaScript lint and `git diff --check` passed. Existing repository Rust warnings remain.
- Live Jev candidate-covered evaluation: **3/3** synthetic selections passed. Earlier missing-candidate failure remains in the evidence. Local Paper MCP discovery passed; the installed Codex 0.158.0 also parsed the generated HTTP plugin override successfully.
- Both desktop (1440×1000) and compact (900×760) Decisions screenshots were reviewed.
- Local preview is configured **Shadow**, using the isolated NautGate endpoint. Change to **Active → Save selection settings** to preload recommendations. Off makes no selection calls. There is no bundled owner key or machine-specific default in the public source.
- Preview destination: `/Users/cand0rian/xnaut-testing/builds/2026-09-29-jev-tool-selection/xNAUT Decisions Preview.app`. Packaging/signature status and preserved commit are in the Obsidian handover.
- No public release, security sign-off, app restart, subscription-account repair or main NautGate replacement was performed.
