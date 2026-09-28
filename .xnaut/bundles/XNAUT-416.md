# XNAUT-416 — Voice conversations: the public continuous route (V1)

Branch: `agent/claude/xnaut-416`, carried forward from the reconciled baseline
`d55221b` (released 1.27.2 + the preserved voice checkpoint `c57b141`).

## What changed

xNAUT gains a **public continuous voice conversation**. One explicit start holds
the whole exchange: the microphone streams to the provider, replies stream back,
the user can talk over them, and every turn is executed by the xNAUT agent
already selected in that conversation. There is no per-turn *Finish & send* and
no *Speak again*.

The route is ported from Bucki's active public configuration
(`48Nauts/Bucky`, `development` @ `629ff06`, `publicVoiceMode=gptLive`), traced
through `BuckiManager.startActiveVoiceIfNeeded` →
`OpenAIRealtimeRunner.handleLiveEvent` / `runCodexDelegation` →
`GPTLiveSession` / `LivePlaybackInterruptionGate`. Attribution and the places we
depart from the source are in each file header.

### New: `src-tauri/src/voice_live/`

Everything that decides *what happens* is a pure state machine with no I/O, so a
whole multi-turn conversation can be driven deterministically in tests.

| File | Responsibility |
| --- | --- |
| `session.rs` | The conversation: events in, actions out. Turn lifecycle, barge-in, delegation, epochs. |
| `turn.rs` | The turn ledger — exactly one committed user message and one authoritative assistant message per turn. |
| `context.rs` | Backend context, including replay of a reopened conversation. |
| `gate.rs` | Barge-in: energy heuristic plus server transcript confirmation. |
| `protocol.rs` | The Live wire contract (Live is **not** Realtime). |
| `mod.rs` | Transport, microphone, speaker, commands, ACL. |
| `conversation_tests.rs` | End-to-end acceptance evidence with synthetic audio. |

### Three deliberate departures from the reference

1. **Delegation goes to the selected xNAUT agent.** Bucki's `CodexVoiceBackend`
   forces `forced_login_method="chatgpt"`, `--ignore-user-config`, and strips
   `OPENAI_API_KEY`/`OPENAI_BASE_URL`. Copying that would bypass NautGate. We
   send `delegation: {"type":"client"}` and run the turn through the pane's
   existing `chat_send_tools` path, so provider, model, permissions and routing
   are the ones the app already resolved.

2. **One message pair per turn.** Bucki's `flushLiveTranscript` appends a
   transcript entry per *flush* (80 chars / terminal punctuation / 1 s idle), so
   one long utterance becomes several chat messages. XNAUT-416 requires exactly
   one committed user message per turn, so fragment grouping stays for live
   captions and a turn-scoped ledger decides what enters the conversation.

3. **Reopening replays context into the backend.** In the reference,
   `resetLiveState()` clears `liveContext` and `BuckiManager.init` restores
   history into *UI state only* — a reopened conversation shows its transcript
   but the assistant cannot refer to it. `Bucky-REFERENCE.md` flags this as
   unresolved. `ConversationContext::restore` seeds context from the persisted
   conversation and `render()` puts it ahead of anything said this session.

### Frontend

- `src/js/voice-live.js` — the continuous surface: overlay, caption ribbon, and
  the dispatch loop that runs each turn on the selected agent and returns its
  real outcome (a failure is reported as a failure).
- `src/js/chat-panel.js` — `dispatchVoiceTurn()`; binding, history and commit
  wiring; typed input during a live session joins the same turn ledger;
  pane teardown releases the microphone and the billable socket.
- `src/css/voice-overlay.css`, `src/index.html` — overlay styling and load order.

The earlier push-to-talk prototype (`voice-conversation.js`, `voice_local/`) is
untouched and remains the private V2 local route.

## Verification

```sh
cargo test --manifest-path src-tauri/Cargo.toml
XNAUT_TEST_PORT=4291 npx playwright test
```

XNAUT_TEST_TOTALS={"rust":[{"passed":1427,"failed":0,"ignored":46}],"ui":[340]}

Of those, 88 Rust tests and 20 Playwright tests are new for this ticket.
`cargo clippy --all-targets` adds no warnings in the changed files.
`git diff --check` is clean. `npx eslint` is clean on the changed JavaScript.

### The acceptance list, and where each line is proved

| V1 acceptance | Evidence |
| --- | --- |
| Start once, many spoken turns | `conversation_tests::one_start_carries_a_five_turn_spoken_conversation`; `voice-live.spec.mjs` "one start carries three spoken turns with no send or speak-again click" |
| Streamed replies, interruption, stale audio fenced | `conversation_tests::talking_over_a_reply_stops_it_and_fences_out_the_audio_behind_it`; `gate.rs` tests |
| Exactly one user + one assistant message per turn | `conversation_tests::a_noisy_provider_still_produces_one_message_pair_per_turn`; all of `turn.rs` |
| Deltas/delegation/retries create no duplicates | `a_repeated_delegation_event_dispatches_once`, `a_duplicate_agent_result_neither_persists_nor_speaks_twice` |
| Reopen with transcript and context, no replayed work | `a_reopened_conversation_answers_from_its_saved_context`; `voice-live.spec.mjs` "reopening a saved conversation replays it as context and does not redo the work" |
| Agent / project / model / permissions preserved | `every_turn_dispatches_to_the_selected_agent_project_and_permissions`; `voice-live.spec.mjs` "each turn runs on this conversation selected agent, model and permissions" |
| Automated multi-turn + synthetic audio | `conversation_tests.rs` throughout — synthetic 24 kHz PCM16 sine frames, not tokens |

## How to verify by hand

The automated evidence above is component-level. To exercise the real route:

1. Write a profile at
   `~/Library/Application Support/xnaut/voice-live.json`, `chmod 600`:
   ```json
   { "api_key": "sk-…", "model": "gpt-live-1" }
   ```
   `endpoint` is optional and defaults to the traced Live URL. The file is
   refused unless it is private, `wss://`, and carries no embedded credentials.
2. `cd src-tauri && cargo tauri dev`
3. Open a Chat pane. The **Voice** button next to the microphone is enabled only
   when the profile loads; without one it says so rather than failing on click.
4. Click **Voice** once, then just talk. Expect: a caption ribbon, a spoken
   reply, and one user/assistant message pair per turn in the conversation.
5. Talk over a reply — playback should stop promptly and the old audio must not
   resume.
6. End voice, close the pane, reopen the same conversation, start voice again
   and ask about something established earlier in it.

## Not done, and deliberately so

- **No live-account run.** `gpt-live-1` and `wss://api.openai.com/v1/live/sessions`
  are findings from the reference source snapshot, not an availability claim,
  and this machine has no Live credential. The transport is implemented and its
  decoding is tested against the traced event shapes; **nobody has yet seen this
  talk to the real service.** That is the single largest open risk.
- **No real microphone, speaker or echo check.** Capture and playback are wired
  to the existing cpal paths, but every audio assertion here is synthetic PCM.
  Speaker echo without hardware cancellation, device switching, sleep/wake and
  the macOS microphone permission prompt are all unverified.
- **Not run in a signed app.** Microphone entitlement behaviour in a
  signed/notarised build is untested.
- **Chat pane only.** Agent Space still has the push-to-talk adapter; extending
  the surface registry to Agent Space, Plan and Vault is XNAUT-424.
- **Full/Summary/Silent is not exposed on this route.** The public route speaks
  the agent's answer. Agent-authored spoken summaries are XNAUT-421.
- **V2 (private Jarvis) untouched**, as scoped.
- Two repository hygiene checks fail on this branch and did so before it:
  three unattributed test fns (`foundation.rs`, `main.rs`, `veto.rs`) and
  `vault-pane.js:491 'path' is not defined`. Neither is in the changed files.
