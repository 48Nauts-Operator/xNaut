# Voice conversations: first implementation slice

Date: 2026-09-16. Branch: `feat/xnaut-416-voice-conversations`, based on `dev` at `2f2bef3`.
Tickets: XNAUT-416 (epic), XNAUT-417 (probes), XNAUT-418 (coordinator), XNAUT-419 (overlay).
Status: in development, not released. No ticket's full acceptance gate is complete.

## Implemented

- Shared frontend voice coordinator with a pinned destination, session epochs, turn IDs, playback generations, and provider capabilities. A provider implements `start(captureId, signal)`, `stop(captureId, signal)`, `cancel(captureId)` and, when capable, `speak(text, signal)`. `speak` resolves after playback ends and must immediately stop its renderer when aborted. `cancel` must work during initialization. No automatic provider fallback.
- Full/Summary/Silent speech policy, verified with synthetic providers. Only explicitly classified assistant answer/summary events enter playback. Summary requires an agent capability and is limited to 80 words/1 KiB. No summarizing model/request. Scoped events, segment deduplication, a 32 KiB pending speech budget and a 512-segment per-turn bound reject stale/repeated/overflowing work. Playback interruption does not cancel agent execution.
- Chat and Agent Space's existing local dictation use the shared coordinator. A small overlay shows the pinned destination, microphone/transcription state, Finish and Cancel. Escape cancels. Removing a composer cancels its capture; late transcripts cannot move to a different field. Completed transcription inserts a draft through the existing composer helper and never sends it automatically.
- Native capture ownership uses a window identity supplied by Tauri and an opaque utterance UUID. Stop/cancel cannot consume another capture. Device initialization and shutdown run off async workers; shutdown joins the capture thread before releasing the lease. Native recording ends after two minutes even if the webview stops responding. PCM is bounded to the negotiated rate/duration with a hard memory ceiling.
- Unique temporary audio files with cleanup on success/failure, private Unix file permissions, and serialized first-use model installation. Window destruction releases its capture. `voice_cancel` is registered and allowed by the existing voice-stop permission.

The visible UI remains **push-to-talk local dictation using the existing whisper-cli path**. It is not yet a streaming conversation. Playback modes exist in the coordinator but are not exposed by this dictation-only provider. No System/Local/Cloud selector is shown until those providers are connected. Cancelling while whisper is already transcribing discards the result; terminating the transcription subprocess is still pending. First-use Whisper model download remains existing opt-in dictation behavior, not a new public voice dependency.

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

Current Bucky `response.create` still invokes its conversational LLM. The probe verifies direct engine access, not a ready speech-only endpoint. `/voice/v1/capabilities` and `/voice/v1/session` remain to be implemented by the optional companion.

## Validation

- 12 coordinator tests pass: startup races, one destination, stale transcript rejection, explicit speech classes, summary limits, Silent interruption, old turns/sessions, bounded queues, provider failures and unavailable playback modes.
- 10 focused native voice tests pass, including ownership, capture bounds, audio cleanup and joining the microphone worker; ACL audit passes.
- Six Playwright integration tests pass using real voice modules and mocked native audio: focus changes, competing composers, cancellation during transcription/startup, removed destinations, visible failure/retry and first-use download disclosure.
- Changed JavaScript passes ESLint; dictation wiring smoke and `git diff --check` pass. No new Rust warnings in these changes.
- Repository-wide preflight is not green on the base branch: it references removed `agents-panel.js` and missing `xnautCreatePmPanel`. Whole-tree ESLint reports 57 existing errors in unchanged `app.js`, `right-pane.js` and `vault-pane.js`.
- Hygiene reports existing unattributed-test/undefined-identifier issues. Its nested full Rust suite did not finish after more than five minutes and was stopped; this is **not** a full-suite pass. Focused voice/ACL tests ran separately and passed. No signed-app microphone or Windows build was tested.

## Next increment

Implement the optional speech-only local companion, format/capability negotiation, bounded streaming transport and a native playback consumer with per-block generation checks. Then bind Full/Silent replies to Chat and Agent Space's normal send/history paths, followed by trusted agent-authored summaries. System speech, cloud voice, settings migration and broader field support remain in their planned tickets. Do not present the synthetic coordinator tests as live provider support.

Ticket status updates through the running app were already blocked by unrelated staged control-repository changes. The implementation is recorded here; ticket statuses are not claimed changed, and those unrelated edits remain untouched.
