# Native Responses and mandatory NautGate routing

Status: implemented, verified, and packaged as a separate ad-hoc-signed preview. Public release remains 1.28.2.

Branch: `fix/astra-native-responses`, based on `4599228` (`fix/chat-history-repo-access`). This includes the preceding durable conversation history, repository read tools, Jev selection and model picker fixes. Worktree: `.worktrees/astra-native-responses`.

## Owner's routing rule

When NautGate is enabled, xNaut's chat and agent model requests must use NautGate. A gateway error must not trigger a direct-provider fallback. When disabled, xNaut must perform the provider integration itself.

The explicit `llm_providers` NautGate `enabled` switch is authoritative. With no registry row, the primary `llm.provider = nautgate` is the legacy enabled state. Historical key import can establish a gateway configuration, but cannot override an explicit disabled row. The older AI settings sync follows the same rule; merely having a seeded default localhost URL no longer enables a gateway on a fresh install.

Backend routing replaces the endpoint and credential with the enabled gateway's values, preserving the chosen model and prompt. A provider does not need a separate direct API credential when NautGate owns the route. No automatic direct fallback or HTTP redirect is allowed for inference requests. Disabled providers are not revived through primary settings or environment fallback.

Covered entry points: ordinary chat, agent chat and its tools, voice-delegated agent work, one-shot generation, terminal assistance, Researcher model calls, model discovery and tool-support probes. The legacy terminal/Researcher AI client also honors gateway priority. Discovery lists gateway models while enabled, and enabled direct providers while disabled.

This change concerns model inference owned by xNaut. It does not reconfigure third-party coding CLI authentication, MCP servers, or the separate GPT-Live speech connection. Jev retains its existing `/systemone` gateway integration; it falls back to deterministic tool discovery if its gateway is disabled. A generic gateway switch does not create unsupported Realtime/WebSocket routes.

## Native Astra transport

Astra and its dated/namespaced model IDs select `/responses`; classic models retain their existing Chat Completions transport. xNaut uses the same native client with a gateway base URL or a direct OpenAI-compatible base URL.

- Native `input`, flattened function tools, `max_output_tokens` and `reasoning.effort`.
- Unspecified or `none` effort becomes `low` for Astra; explicit effort is forwarded. No obsolete `max_tokens` or `reasoning_effort: none` is sent.
- Responses can reason with tools in the same request; the old separate tool-free thinking pass is skipped.
- Tool results use `function_call_output` and the original `call_id`. Each later round sends only new input and `previous_response_id` to preserve the model's prior output, including reasoning items.
- This continuation relies on provider-side response state under the provider's retention policy. It is not a stateless or local-only transport. Durable xNaut chat history remains separately stored locally; a new user turn starts a new Responses session with the supplied history.
- Streamed text continues to use the existing chat events. Function calls are executed only after a valid `response.completed` payload. Canonical completed output supplies final arguments, avoiding partially streamed execution.
- Disconnects, incomplete/failed responses, malformed arguments, missing IDs and unsupported output items fail explicitly. Duplicate call IDs in one response are deduplicated only when identical; conflicting duplicates fail. A repeated ID from an already executed round stops the turn before executing the new batch.
- Bounded payload buffers, body/stream idle timeouts, and no automatic transport retries or silent Chat Completions downgrade. Astra failure is returned to the conversation rather than making a second prose-only request.
- Existing schema advertisement, capability checks, repository scopes, MCP execution, Jev tool selection and action evidence remain the execution boundary.
- NautGate decision/receipt IDs, transport header and token usage are retained in tool-turn evidence. Dollar pricing for Astra remains unknown; no price was invented.

## Settings and use

Open Settings → Chat LLM. The new **Route model requests through NautGate** checkbox controls the gateway registry switch.

Gateway on: enter/retain the NautGate URL and its client key, and select a model available on that gateway. For Astra, use `gpt-6-astra`; the deployed gateway must preserve native Responses. The gateway owns provider credentials and account routing.

Gateway off: choose a direct provider. OpenAI defaults to `https://api.openai.com/v1`, with its own API key. OpenRouter defaults to `https://openrouter.ai/api/v1`. Local and custom compatible endpoints remain available. Direct mode does not require NautGate, but the endpoint must support the selected model's protocol; unsupported Responses routes produce an explicit error. Arbitrary direct providers have not all been live-tested.

OpenAI API billing is separate from a Codex subscription. This does not add subscription OAuth to the direct API client. Existing coding CLI/subscription support is a separate integration.

Gateway-enabled requests retain the requested model ID. Moving an old direct-provider selection onto a gateway may require choosing that gateway's advertised model ID; xNaut does not silently substitute a different model to make the request succeed.

## Evidence

Evidence directory: `/Users/cand0rian/xnaut-testing/runs/2026-09-29-astra-native-responses/`.

- Full Rust suite: **1,476 passed, 0 failed, 53 ignored**. Ignored live/process tests were not counted as passing.
- Final Responses-focused run after adding the direct-provider credential guard: **8 passed, 1 ignored**. The additional test confirms an Anthropic configuration cannot send its key to the default OpenAI endpoint.
- Native HTTP/SSE fixtures: schema conversion, function result/response-ID continuity, fragmented UTF-8, token usage, interrupted/failed streams, HTTP errors, duplicate call IDs, real scoped repository reads, and refusal to follow a gateway redirect to a direct provider.
- Full browser run: **381 passed, 1 failed**. The failure identified the legacy NautGate credential import regression. After correction, the final seven affected browser checks all passed, including the failed test, direct-provider selection, disabled-switch persistence, and gateway model settings. The full suite was not rerun after that small correction.
- New/affected model-picker focused run before the broad suite: six passed.
- Changed small JavaScript modules pass ESLint. `app.js` retains exactly the same **55 pre-existing diagnostics** as the base revision; no new diagnostics were introduced. Frontend assets build successfully; `git diff --check` passes.
- Live paid synthetic tool round trip through the configured `http://localhost:8090/v1/responses`: function call → `pong` result → streamed **PONG VERIFIED**. No external action was executed by the synthetic ping. The gateway returned `X-NautGate-Transport: openai-responses` on both rounds.
- Round one: decision `84e03f26-9386-4f84-9943-9ad84ded7af8`, receipt `515a97ce-f908-43c3-8562-2b7440d6046c`, **51 input + 14 output tokens**.
- Continuation: decision `104e96c1-e4bb-4a4a-a6d8-f8636eb63346`, receipt `ca83cbb2-008f-467d-9efe-56f88c548c9b`, **77 input + 7 output tokens**.
- Both receipt bundles were read back and report requested, selected and observed model **gpt-6-astra**. This xNaut run did not independently reverify their cryptographic signatures; the separate NautGate deployment handover records signature verification of its own earlier probes.
- Direct-provider operation was verified with isolated HTTP fixtures, not an additional paid direct OpenAI account call. Actual microphone/speaker acceptance remains an owner test; this change does not claim another complete voice acceptance run.

## Preview and rollout

Built and copied bundle: `/Users/cand0rian/xnaut-testing/builds/2026-09-29-astra-native-responses/xNAUT Responses Preview.app`.

The copied bundle passed `codesign --verify --deep --strict`. It is ad-hoc signed, not a notarized public release, and has not been launched.

Do not stop the owner's running app to install it. When ready, the owner can quit the existing preview and open this separate bundle. The previews share normal native settings and conversation storage; avoid running them concurrently when editing settings. For acceptance, select Astra, run **Check this route**, then ask the agent to read a named repository document. Confirm the transcript contains the response and the tool evidence names the read operation.

No public release, installed-app replacement, gateway deployment change, worktree cleanup or unrelated agent restart is part of this handover. Roll back the preview by closing it and returning to the prior bundle. Any explicit routing settings changes persist across bundles.

## References

- NautGate deployed handover: `/Users/cand0rian/DevHub_Studio/factory/02-Development/NautGate-worktrees/native-openai-responses/docs/handovers/2026-09-29-native-openai-responses.md`.
- [OpenAI Responses migration](https://developers.openai.com/api/docs/guides/migrate-to-responses)
- [Function calling](https://developers.openai.com/api/docs/guides/function-calling)
- [Streaming Responses](https://developers.openai.com/api/docs/guides/streaming-responses)
