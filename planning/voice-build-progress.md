# Voice conversations: implementation progress

Date: 2026-09-16. Branch: `feat/xnaut-416-voice-conversations`, based on `dev` at `2f2bef3`.
Tickets: XNAUT-416 (epic), XNAUT-417 (probes), XNAUT-418 (coordinator), XNAUT-419 (overlay), XNAUT-420 (local bridge).
Status: in development, not released. No ticket's full acceptance gate is complete.

## Scope correction — 2026-09-28

The user clarified the target as Bucki working inside xNaut: continuous streamed conversation, transcription, saved conversations and the ability to pick them up later. Release order is now **V1 public Bucki route, V2 private Jarvis route**. The prior local-first plan is superseded; existing local work is retained for V2. No runtime correction is implemented by this documentation change.

Source review in `/Users/cand0rian/DevHub_Studio/factory/02-Development/Bucky` found public audio/transcript event handling in `OpenAIRealtimeRunner.swift`, a distinct Live/delegation contract in `GPTLiveSession.swift`, and saved conversation/artifact loading and atomic writes in `WorkspaceHistoryStore.swift` plus `BuckiManager.swift`. The user reports that conversation/resume works today. This review confirms code paths, not a fresh end-to-end run or which public transport is active on the user's machine; backend context replay still needs tracing.

By comparison, xNaut's `src/js/voice-conversation.js` explicitly advertises `streamingInput: false`, waits for playback before recording, and reads completed chat answers. That implementation cannot establish parity with the requested continuous public Bucki route. See the revised implementation plan for V1 acceptance criteria.

## Preservation checkpoint — 2026-09-28

This branch is an unfinished work-in-progress backup. The user confirms that it does not yet work as designed: the current button-driven interaction does not deliver the intended natural, streaming conversation. Earlier focused test results below describe individual components, not acceptance of the overall experience. No new runtime tests were performed for this backup checkpoint.

Resume in worktree `.worktrees/voice-conversations`, branch `feat/xnaut-416-voice-conversations`. Implementation commits `f6a0c7c` and `c1d46dd` preserve the coordinator, overlay, local speech companion and chat readback. The architecture and implementation plan beside this file describe the target behavior and remaining work. This voice feature belongs to xNaut.

The backup covers tracked source, tests and plans. Local credentials, private Jarvis voice assets, installed model weights and generated build/cache files are external or ignored and are not part of the Git backup.

## Current increment: local speech service and chat readback (XNAUT-420)

The optional [local companion and setup guide](../companions/local-voice/README.md) now implement authenticated capabilities and protocol-v1 WebSockets. They call Parakeet and supplied-text Qwen3-TTS directly, keep one model worker, bound audio/text/queues, reject browser origins, and discard buffered engine frames after cancellation. No conversational LLM or private voice asset is included.

Xnaut's Rust adapter reads a separate private endpoint/token profile, permits literal loopback addresses only, and validates capabilities, format, utterance identity and sequence numbers. Local recognition consumes the existing native capture without creating a WAV, using Whisper or downloading its model. Streamed PCM goes to a bounded native CPAL renderer; session/generation checks fence output in the device callback as well as the transport. Closing or reloading a window releases its local session and capture. Another window cannot acquire the conversation's microphone.

Chat and Agent Space now have **Talk**, **Finish & send**, **Speak again**, **Full response prose / Silent**, **Stop speaking**, and **End voice**. Input is explicitly push-to-talk; the microphone stays off during agent work/playback. The two-minute cap inserts a draft rather than submitting a task. Voice submits through the existing composer/history/permission path, then reads a completed normal chat answer. Code stays visible; raw structured/action payloads (including malformed ones) do not enter speech. Changing a Chat conversation key ends its voice destination. The written answer remains complete.

This is a prototype, not completion of P2/P3: continuous partial recognition, agent-authored Summary, CLI coding-session readback, System/Cloud switching, settings UI and signed-app microphone/speaker/echo validation remain pending. No installed app or Bucky session was replaced. The local engine/profile is optional and not bundled.

A real loopback probe reproduced synthetic speech and streamed supplied-text audio (82 and 83 PCM blocks) with no conversational LLM. Cold transcription took 36.53 s; first TTS PCM took 27.52 s cold and 7.06 s warm. A further warm sample reached first PCM in 5.79 s and finished 85 blocks in 17.68 s. These are heavily confounded measurements: host load averages during the run were approximately 818/778/666, and Rust/browser checks were also much slower. They do not establish normal latency or meet the conversation target. Repeat under controlled load before attributing the slowdown to transport, inference or scheduling. Only the temporary probe companion was stopped afterward.

Validation for this increment: 18 Rust tests selected by `voice` (including PCM framing/resampling, loopback exchange and stalled-transport cancellation), the ACL audit, 6 companion protocol tests, 12 coordinator tests, and 12 Playwright tests covering the real Chat send/history path and dictation. Changed JavaScript passes ESLint; expanded voice wiring smoke passes. Repository-wide failures and the full-suite limitation below remain separate. Native microphone/speaker playback has not been exercised in a signed app; Python real-engine tests consumed synthetic files/PCM without playing them.

Next: profile the local transport under controlled load and exercise a separately built signed app on hardware; then wire agent-authored summaries and CLI response adapters. Ticket updates remain subject to the pre-existing control-repository sync blockage.

## Previous increment: coordinator and dictation (XNAUT-418)

- Shared frontend voice coordinator with a pinned destination, session epochs, turn IDs, playback generations, and provider capabilities. A provider implements `start(captureId, signal)`, `stop(captureId, signal)`, `cancel(captureId)` and, when capable, `speak(text, signal)`. `speak` resolves after playback ends and must immediately stop its renderer when aborted. `cancel` must work during initialization. No automatic provider fallback.
- Full/Summary/Silent speech policy, verified with synthetic providers. Only explicitly classified assistant answer/summary events enter playback. Summary requires an agent capability and is limited to 80 words/1 KiB. No summarizing model/request. Scoped events, segment deduplication, a 32 KiB pending speech budget and a 512-segment per-turn bound reject stale/repeated/overflowing work. Playback interruption does not cancel agent execution.
- Chat and Agent Space's existing local dictation use the shared coordinator. A small overlay shows the pinned destination, microphone/transcription state, Finish and Cancel. Escape cancels. Removing a composer cancels its capture; late transcripts cannot move to a different field. Completed transcription inserts a draft through the existing composer helper and never sends it automatically.
- Native capture ownership uses a window identity supplied by Tauri and an opaque utterance UUID. Stop/cancel cannot consume another capture. Device initialization and shutdown run off async workers; shutdown joins the capture thread before releasing the lease. Native recording ends after two minutes even if the webview stops responding. PCM is bounded to the negotiated rate/duration with a hard memory ceiling.
- Unique temporary audio files with cleanup on success/failure, private Unix file permissions, and serialized first-use model installation. Window destruction releases its capture. `voice_cancel` is registered and allowed by the existing voice-stop permission.

That first increment exposed **push-to-talk local dictation using the existing whisper-cli path**. The original microphone button retains that provider alongside the new Talk button described above. No System/Local/Cloud selector is shown until those providers are connected. Cancelling while whisper is already transcribing discards the result; terminating the transcription subprocess is still pending. First-use Whisper model download remains existing opt-in dictation behavior, not a new public voice dependency.

## Speech-only engine probe (XNAUT-417)

The optional `scripts/probe-local-voice.py` consumes the external speech-to-speech handler APIs directly. It does not construct the conversational pipeline, connect to Bucky, create an LLM, open a microphone or play speaker audio. The probe sets Hugging Face/Transformers offline flags before imports. No weights or recordings are added to the repository.

Environment: Apple M1 Max, 64 GiB RAM; speech-to-speech 0.2.10 at clean revision `0355f39e6597aba381af7698c71250e863cbde5d`, mlx-audio 0.4.2, MLX 0.31.1. Speech-to-speech source is Apache-2.0. Model distribution/license review is still pending; this does not authorize bundling model weights or voices.

Input was a synthetic macOS system-voice WAV, 16 kHz mono, with the sentence: “This is a synthetic voice sample for testing. Keep the full written answer and speak a short summary.” The same synthetic file was the TTS reference. No personal voice recording/profile was used. Models were already cached.

| Engine | Load + warmup | First request | Second request |
| --- | ---: | ---: | ---: |
| `mlx-community/parakeet-tdt-0.6b-v3` | 11.93 s | 0.73 s to final transcript | 0.38 s to final transcript |
| `mlx-community/Qwen3-TTS-12Hz-1.7B-Base-6bit` | 12.70 s | 1.14 s to first PCM | 0.92 s to first PCM |

Both STT requests reproduced the synthetic sentence. TTS produced 95 and 113 PCM blocks (3.04 and 3.62 seconds of audio). These are two local observations, not p50/p95, an audio-quality assessment, signed-app microphone verification or end-to-end/audible latency. First request follows the handler's own warmup; load timing is not a cold filesystem-cache measurement.

Cancellation after the first TTS block returned 18 and 8 further buffered blocks before generation ended, taking 0.66 and 0.79 seconds to return. The companion must tag every block with the generation captured **before** synthesis. Xnaut's renderer must drop mismatched generations and clear scheduled playback immediately, independently of model shutdown. Merely cancelling the Python generator does not meet the 200 ms playback-stop target. That target is still unmeasured.

Reproduce with the separately installed runtime's Python interpreter and cached models:

```sh
python scripts/probe-local-voice.py stt \
  --model mlx-community/parakeet-tdt-0.6b-v3 --input /path/to/synthetic-16k-mono.wav
python scripts/probe-local-voice.py tts \
  --model mlx-community/Qwen3-TTS-12Hz-1.7B-Base-6bit \
  --reference /path/to/synthetic-16k-mono.wav --reference-text 'The synthetic reference transcript.'
# Repeat the TTS probe with --cancel-after-blocks 1 for cancellation behavior.
```

Current Bucky `response.create` still invokes its conversational LLM. This initial probe verified direct engine access. The current increment above implements separate `/voice/v1/capabilities` and `/voice/v1/session` endpoints in the optional companion.

## Previous increment validation and repository baseline

- 12 coordinator tests pass: startup races, one destination, stale transcript rejection, explicit speech classes, summary limits, Silent interruption, old turns/sessions, bounded queues, provider failures and unavailable playback modes.
- 10 focused native voice tests pass, including ownership, capture bounds, audio cleanup and joining the microphone worker; ACL audit passes.
- Six Playwright integration tests pass using real voice modules and mocked native audio: focus changes, competing composers, cancellation during transcription/startup, removed destinations, visible failure/retry and first-use download disclosure.
- Changed JavaScript passes ESLint; dictation wiring smoke and `git diff --check` pass. No new Rust warnings in these changes.
- Repository-wide preflight is not green on the base branch: it references removed `agents-panel.js` and missing `xnautCreatePmPanel`. Whole-tree ESLint reports 57 existing errors in unchanged `app.js`, `right-pane.js` and `vault-pane.js`.
- Hygiene reports existing unattributed-test/undefined-identifier issues. Its nested full Rust suite did not finish after more than five minutes and was stopped; this is **not** a full-suite pass. Focused voice/ACL tests ran separately and passed. No signed-app microphone or Windows build was tested.

## Original first-slice follow-up

The companion, transport, native renderer and Full/Silent normal chat adapters are now implemented above. Their signed-app hardware and latency gates remain open. Agent-authored summaries, system/cloud speech, settings migration and broader field support remain in their planned tickets.

Ticket status updates through the running app were already blocked by unrelated staged control-repository changes. The implementation is recorded here; ticket statuses are not claimed changed, and those unrelated edits remain untouched.
