# Voice conversations: implementation plan

Status: implementation started; no phase gate complete. Date: 2026-09-16.
Progress and measured probes: [First implementation slice](voice-build-progress.md).
Architecture: [Voice conversations](voice-architecture.md).

## Current delivery order — user decision, 2026-09-28

**V1: Bucki public voice inside xNaut. V2: private Jarvis route.** This supersedes the original local-first delivery sequence. The P0–P6 sections below are retained as the earlier engineering breakdown; their numbering is not the current execution order. The existing local prototype is preserved for V2, not the V1 acceptance baseline.

V1 must reproduce the public Bucki conversation experience in the selected xNaut conversation:

1. Start voice once; sustain multiple spoken turns without Finish & send / Speak again for every turn.
2. Stream microphone audio and spoken replies through the public Bucki-style session route, with turn detection and interruption. Do not substitute recording a whole utterance followed by completed-answer readback.
3. Persist both sides of the transcript into the selected xNaut conversation, with stable identities and no duplicate turns from partial events.
4. Reopen that conversation later and continue with its saved context after explicitly starting voice again. Restoring the text display alone does not establish context restoration to the voice/backend session.
5. Keep task execution bound to the selected xNaut agent and permission path. Connect voice delegation and task results without starting duplicate agent turns from transcript events.
6. Test synthetic multi-turn audio, barge-in, save/reopen/context continuity and reconnection; then verify real hardware. Public voice is explicit opt-in, and V1 requires no local model installation.

Inspect Bucki's actual active public configuration before choosing the transport: its source contains distinct Live and Realtime paths. Reuse the working behavior and event contract, not an assumed interchangeable protocol. Preserve Full/Summary/Silent as requirements; do not claim a mode is available until wired and tested. No additional local summarizer is needed for V1.

V2 adds the private Jarvis/local provider while preserving the same conversation, transcript and resume contract. System speech and broader field support remain later work and do not block V1. Private voice assets and credentials remain external.

## Earlier engineering breakdown (historical ordering)

The following phases describe the original plan and implementation probes. Apply them selectively to the V1/V2 order above. Historical local push-to-talk fallback gates do not satisfy V1 continuous-conversation acceptance.

## P0 — Resolve the implementation risks

Deliver a short capability report with pinned runtime versions and reproducible probes. Use synthetic audio and mock agent responses first; live provider tests are explicit and separate from ordinary CI.

- [x] Inspect the local runtime's recognition and TTS APIs; run one speech-only utterance-to-text and text-to-streamed-audio probe without loading its conversational LLM. Direct engine probes pass; a speech-only companion endpoint is still required (see progress report).
- [ ] Measure cold start, warm first transcript, first audible speech and cancellation for the private local setup. Do not launch/terminate the user's existing Bucky session as part of a probe.
- [ ] Prototype native system recognition and synthesis in signed Tauri macOS and Windows builds. Record supported languages, offline status, microphone entitlements, installed voice/download requirements, and interruption capability.
- [ ] Validate one built-in-agent `voice_emit` and one Codex App Server dynamic-tool event with a mock renderer. Compare time to first speech segment with normal answer completion. Verify tool continuations and actual supported protocol shape.
- [ ] Check safe per-turn authorization for CLI/MCP speech events; do not use a globally writable “speak to active tab” tool.
- [ ] Inventory third-party code/model licenses and where a generic speech bridge will be maintained. Keep model weights and personal voice assets out of the Xnaut bundle.

Gate: choose verified local bridge APIs and system platforms; define supported summary transports. No cloud-voice subscription assumption is required to proceed.

## P1 — Shared session infrastructure

Files: new `src-tauri/src/voice_session/`, `src/js/voice-session.js`, `src/js/voice-overlay.js`, `src/css/voice-overlay.css`; modify `state.rs`, `main.rs`, `settings.rs`, `src/index.html`, and Tauri permissions/capabilities.

- [ ] Define versioned provider capabilities, session commands, normalized events, and the surface registry from the architecture.
- [ ] Implement one microphone lease shared with existing `voice.rs` dictation.
- [ ] Implement session/turn identities, sequence numbers, session epochs, playback generations, bounded queues, and cleanup on end/error/window close.
- [ ] Add the overlay with a pinned destination, input draft, Voice connection, Full/Summary/Silent, microphone mute and End. Support keyboard activation and accessible labels.
- [ ] Distinguish microphone mute, silent output, end voice session and cancel agent task.
- [ ] Add durable global/project preferences and legacy Kokoro preference migration. Preserve unknown settings. No capture starts during migration or application launch.
- [ ] Deliver backend events only to the owning surface. Register all new commands in `generate_handler!`, permission definitions and applicable capabilities.
- [ ] Add a synthetic provider that emits partial/final transcripts and controllable audio for deterministic integration tests.

Gate: mock conversation supports Dictate/Converse, mode changes, interruption, switching and failure recovery. Two windows cannot take the microphone simultaneously; a late event cannot affect another destination.

## P2 — Local speech bridge and first useful conversation

Files: `voice_session/providers/local.rs`, native playback module; generic protocol examples under a version-controlled documentation directory; companion code location chosen in P0. The current `docs/` directory is ignored, so public setup instructions need a deliberate tracked location. Modify `voice-dictate.js`, `chat-panel.js` and `agent-space.js` only through the shared surface client.

- [ ] Implement `/voice/v1/capabilities` and WebSocket protocol v1 in the optional companion. Publish schemas and sample synthetic events.
- [ ] Connect Parakeet input and Qwen3-TTS supplied-text output with explicit LLM bypass. Preserve the existing Bucky full-conversation mode for its current user.
- [ ] Add endpoint, voice selection, health checks, format negotiation, authentication, reconnect and error reporting in Xnaut.
- [ ] Start by attaching to a separately managed local service. Do not add automatic Python/model installation or take ownership of a Bucky-owned process.
- [ ] Connect one Agent Space conversation and one built-in Chat conversation. A submitted transcript goes through the same existing send/permissions/history path as typed input.
- [ ] Implement Full and Silent. Full can initially queue a completed message when the agent does not yet stream; label that limitation. Silent performs no synthesis.
- [ ] Test speaker echo, headphone interruption and device changes. Use push-to-talk or listen-after-playback when simultaneous listening fails the echo test.
- [ ] Add a private external profile for the owner's existing voice only in user settings, never in public examples, fixtures or build resources.

Gate: sustained local conversation with visible text and spoken output; no conversational LLM loaded by the speech bridge; no outbound cloud speech calls; complete cancellation of stale audio; service loss preserves text and never invokes cloud fallback. Public builds remain usable without this companion.

## P3 — Agent-authored Summary and real streaming

Files: `agent_tools.rs`, `chat.rs`, `agent_hooks.rs`, `agents.rs`, new `codex_app_server.rs`, voice speech-policy module; extend `agent-space.js` and `chat-panel.js` adapters.

- [ ] Add a trusted turn event sink to the tool loop. Incrementally parse text and tool-call argument fragments; execute a tool only once its arguments are complete and valid.
- [ ] Preserve provider/model selection, permission handling, plugin tools, plan/document outputs, routing notices and canonical final text. Do not make a failed stream retry already executed actions.
- [ ] Register session-scoped `voice_emit` for supported agent turns. Validate text limits, event identity, segment IDs, and permitted kinds; deduplicate calls and acknowledge without waiting for playback.
- [ ] Add concise speech instructions alongside existing task instructions. Require visible full answers and grounded summaries; do not alter tool/action-only response formats with ad hoc marker text.
- [ ] Expose Summary only on adapters with a verified mechanism. Handle omitted summaries visibly without a second LLM request.
- [ ] Add Codex App Server stdio integration: initialize, start/resume thread, stream assistant text, handle approvals, steer and interrupt. Use the normal Codex authentication flow; do not read or export its credentials.
- [ ] Pin the supported App Server protocol/version and validate against its generated schema. Keep exec JSONL as a compatibility path and preserve its completed-message limitation.
- [ ] Verify resuming an existing exec conversation before offering migration. Do not create two writers for the same Codex thread. Where migration is unavailable, require a new explicitly linked conversation.
- [ ] Add turn-bound MCP summary delivery for other supported CLIs, with minimal per-launch authorization. Unsupported terminals remain Dictate-only or completed-answer readback where structured output exists.
- [ ] Track spoken segments separately from display text. On Full/Summary/Silent changes, clear or continue queues according to the architecture without replaying work.

Gate: an actual execution agent supplies both written answer and speech summary in its task workflow; first eligible speech can play before all display text is complete; no additional summarization request/model; measured latency includes tool continuation overhead. Tests prove action payloads, tool arguments, internal reasoning, and stale summaries do not enter playback.

## P4 — Lightweight public default and surface coverage

Files: `voice_session/providers/system/`, additional shared surface registrations, settings UI in `app.js`.

- [ ] Implement native synthesis and supported recognition for the P0 macOS/Windows matrix. Keep platform-specific dependencies gated so an unavailable speech framework does not break another platform's build.
- [ ] Probe recognition and playback independently, including language availability and whether network recognition would be used. Enforce local-only voice policy before capture leaves the device.
- [ ] Expose a clear setup state when recognition is missing; preserve typed input/readback and the existing opt-in dictation option. Do not claim universal offline dictation or install models silently.
- [ ] Extend registered surfaces to Plan, Vault and ordinary fields. Field insertion respects caret/selection; conversation history owns assistant output. Switching focus never reroutes an in-flight utterance.
- [ ] Add global shortcut preferences without enabling global listening by default.
- [ ] Verify Full/Summary/Silent uses the same policy across System and Local service, and that private profiles remain user-specific.

Gate: a clean supported machine can use system speech without Bucky, Python, another LLM, or API credentials, within the published language/capability matrix. Unsupported combinations show useful setup choices rather than a nonfunctional microphone button.

## P5 — Optional cloud voice and reliable switching

Files: `voice_session/providers/cloud/`, credential references/settings, provider capability UI.

- [ ] Implement GPT-Live client delegation using the existing selected Xnaut execution agent. Keep transcript timeline/context in Xnaut and deduplicate delegation dispatch.
- [ ] Reuse normal agent event routing for work/results; never execute partial transcript fragments as tasks. Process corrections through steering/queued-follow-up semantics.
- [ ] Support natural Summary with grounded agent speech segments. Expose that GPT-Live may paraphrase them.
- [ ] Route exact Full and strict Summary through an explicitly selected direct TTS renderer. Disable overlapping Live audio. Validate any transport change needed when entering these modes.
- [ ] Implement Silent as a hard playback gate; show when cloud input remains connected and billable. End actually closes the provider connection and releases resources.
- [ ] Switch between Cloud, System and the user's local profile without changing the execution model or replaying committed turns. Drop late audio/delegation events from the retired provider while accepting results from still-valid agent work.
- [ ] Keep API keys in backend secret storage. Do not infer API entitlement from Codex login or present API voice as subscription-included.
- [ ] Surface usage/connection duration and reconnect failures. Never switch from Local to Cloud automatically.

Gate: live account testing confirms delegated work, interruptions, exact-vs-natural playback, connection teardown, provider switching and usage reporting. CI uses mocked provider events and incurs no voice charges.

## P6 — Privacy, packaging and public/private documentation

- [ ] Enforce `voice_local_only` across every speech adapter and user-configured fallback. Clearly show when the selected execution agent is cloud-hosted.
- [ ] Audit execution/tools/memory/storage before offering a `project_local_only` guarantee. If broader enforcement is not ready, ship the truthful voice-only privacy label and leave whole-project isolation unavailable.
- [ ] Document system setup, cloud API billing, generic local bridge installation, hardware/language requirements, model licenses, and how to use one's own or an authorized voice.
- [ ] Publish generic placeholders and synthetic fixtures; keep the owner's Jarvis recordings, weights, personal prompts, addresses, credentials and memories outside the repository and release package.
- [ ] Add source/license attribution for ported code; review distribution rights for optional runtimes and any recommended models. Do not bundle model downloads in normal installation/update artifacts.
- [ ] Verify signed/notarized macOS and Windows release builds, microphone prompts, cold start, shutdown, upgrade migration and uninstall behavior.
- [ ] Publish the supported adapter matrix, measured latency, known limitations and setup guide. Do not claim any-field two-way support outside Xnaut; track external app adapters separately.

Gate: public release works without the private setup, and another user can follow the generic guide to connect their own local voice service. Documentation matches tested behavior.

## Acceptance scenarios

| Scenario | Required result |
| --- | --- |
| Speak into an Agent Space thread | Exactly one committed user message; correct agent, project, history and permission context |
| Partial recognition changes | Draft updates; no duplicate submission or execution |
| Full answer contains Markdown/code/tools | Speak only classified assistant presentation text; honor code/read options |
| Summary on a capable agent | Agent-generated short version and complete written answer; no extra summarizer |
| Missing/malformed speech segment | Written answer preserved; visible status; no guessed or automatic full readback |
| Silent selected during speech | Playback and synthesis stop; microphone/text continue |
| User interrupts | Old audio cannot resume; task continues unless separately cancelled |
| Cloud to local switch during a task | Same agent turn continues; no old provider speech or repeated actions |
| Switch active tab mid-utterance | Destination remains pinned until explicit rebind |
| Local service unavailable | Text remains usable; no cloud fallback |
| Agent response queue outruns playback | Bounded memory; explicit paused/resume behavior without silently omitting Full text |
| Two simultaneous conversations | Only the voice-bound turn may emit speech; one microphone owner |
| Reconnect after a tool succeeded | Result restored without rerunning the tool |
| Local voice with cloud agent | Audio stays on approved hosts; UI explicitly identifies remote execution |
| Speaker output reaches microphone | No self-conversation; tested echo handling or disclosed push-to-talk fallback |
| User ends voice / app closes / device disappears | Microphone, streams and cloud connection released; agent task status remains correct |
| Clean public installation | No personal assets, hardcoded private endpoints or mandatory ML runtime |

## Measurement and testing

Record warm and cold runs separately. Time points: speech end, committed transcript, agent start, first assistant text, first speech segment, first synthesized frame, first audible playback, interruption detection, last stale frame rejected. Report p50/p95 by hardware, language, input/output engine, speakers/headphones and execution adapter. Avoid a blanket “subsecond conversation” claim.

Initial engineering targets, not measured promises: interruption-to-playback-stop p95 at most 200 ms; local coordinator overhead from an accepted speech segment to renderer dispatch p95 at most 50 ms; no unbounded queue growth in a 30-minute session. Recognition, model and synthesis latency get separate measured budgets after P0. Maintain metrics without storing raw audio/transcripts by default.

Tests should cover state races and observable effects, not mirror individual helper implementations. Use synthetic audio and mock providers for deterministic tests; microphone echo, voice quality, OS permissions and real cloud access require manual hardware/account validation.

For changed code, run focused Rust tests and relevant Playwright scenarios, then repository checks appropriate to the affected files:

```sh
node scripts/preflight.mjs --fast
node scripts/hygiene-check.mjs
npm run lint
cargo test --manifest-path src-tauri/Cargo.toml
```

Validate platform builds when native dependencies, permissions or packaging change. Keep unrelated existing failures distinguished from regressions. This planning-only change needs link/content review and `git diff --check`, not a runtime build.

## Deferred work

Cross-application accessibility/browser adapters; arbitrary terminal transcript recognition; automatic companion installation and model management; Linux system speech certification; Codex subscription-backed voice entitlement experiments; personal voice training inside Xnaut. None blocks the initial local-service conversation or the public provider interface.

## Tracking tickets

Created in XNAUT on 2026-09-16. Parent: XNAUT-416. All tickets are inbox/unassigned; dependencies are recorded in ticket bodies.

| Ticket | Scope | Depends on |
| --- | --- | --- |
| XNAUT-417 | validate speech-only local engines, system speech and agent summary transports | none; first implementation gate |
| XNAUT-418 | implement the session coordinator, lifecycle and provider contracts | XNAUT-417 |
| XNAUT-419 | add the shared overlay and bind Chat and Agent Space destinations | XNAUT-418 |
| XNAUT-420 | build the optional speech-only local bridge and Jarvis service adapter | XNAUT-417, XNAUT-418, XNAUT-419 |
| XNAUT-421 | stream built-in tool chat and emit agent-authored spoken summaries | XNAUT-418, XNAUT-419, XNAUT-420 |
| XNAUT-422 | add Codex App Server streaming and turn-scoped CLI speech delivery | XNAUT-417, XNAUT-418, XNAUT-419, XNAUT-421 |
| XNAUT-423 | implement lightweight system speech on supported macOS and Windows builds | XNAUT-417, XNAUT-418, XNAUT-419, XNAUT-421 |
| XNAUT-424 | extend destination routing to Plan, Vault and ordinary Xnaut fields | XNAUT-419, XNAUT-421, XNAUT-423 |
| XNAUT-425 | add optional GPT-Live delegation and safe cloud-to-local switching | XNAUT-420, XNAUT-421, XNAUT-422, XNAUT-423 |
| XNAUT-426 | enforce privacy boundaries and publish the generic private-voice setup guide | XNAUT-420, XNAUT-421, XNAUT-422, XNAUT-423, XNAUT-424, XNAUT-425 |

Product feature in the private Obsidian vault: `Business/Engram/XNAUT/03-Product Features/Voice Conversations.md`.
