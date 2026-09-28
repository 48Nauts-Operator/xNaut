# Optional local voice companion

Development prototype for XNAUT-420. This is a separately managed service, not
part of the Xnaut application bundle. Public users do not need to install it.
It performs speech recognition and supplied-text synthesis without loading a
conversational LLM. Xnaut's selected execution agent still handles the task.

The initial real engine adapter targets **Apple Silicon** with cached Parakeet
and Qwen3-TTS models. Windows/Linux speech engines, native system speech, cloud
voice and agent-authored summaries are separate work. Do not use this service
as a replacement for Bucky's conversational endpoint: run it separately.

## Private setup

Use a separately installed, compatible `speech-to-speech` environment. The
tested API is 0.2.10 at local revision `0355f39e6597aba381af7698c71250e863cbde5d`,
with `mlx-audio` 0.4.2 and MLX 0.31.1. Its configured upstream is
[Hugging Face speech-to-speech](https://github.com/huggingface/speech-to-speech).
Availability of this exact revision through the public upstream has not been
verified; generic one-command installation is not a release claim yet. The
adapter needs `ParakeetTDTSTTHandler`, `Qwen3TTSHandler`, `VADAudio`, `TTSInput`
and `CancelScope` with the APIs used in `server.py`.

Install the protocol host packages into that optional environment if missing:

```sh
/path/to/speech-runtime/.venv/bin/python -m pip install -r companions/local-voice/requirements.txt
```

Download the chosen models separately, after reviewing their terms. The service
sets `HF_HUB_OFFLINE`, `TRANSFORMERS_OFFLINE` and disables Hub telemetry before
loading engines. An incomplete cache fails; the service does not fetch missing
models or fall back to a cloud API. No packages/models are auto-installed by Xnaut.

Start from the Xnaut source checkout, using your own or an authorized voice:

```sh
/path/to/speech-runtime/.venv/bin/python companions/local-voice/server.py \
  --port 8791 \
  --connection-file "$HOME/Library/Application Support/xnaut/voice-local.json" \
  --stt-model mlx-community/parakeet-tdt-0.6b-v3 \
  --tts-model mlx-community/Qwen3-TTS-12Hz-1.7B-Base-6bit \
  --reference /path/outside-the-repository/my-authorized-voice.wav \
  --reference-text 'The exact words spoken in the reference recording.'
```

`--reference` is required by this Base TTS model. Do not put personal recordings,
weights, persona prompts, memories or credentials in the Xnaut repository.
The speech engine's voice configuration stays external; the app profile stores
only the endpoint and generated bearer token. The file is created with private
permissions and reused on restart. The app rejects group/world-readable profiles
on Unix; use `chmod 600` if needed. Tokens are not returned to the webview.

Launch a build of this feature branch, then click **Talk** beside the microphone
in Chat or Agent Space. **Finish & send** transcribes through Parakeet and submits
through the same send/history/permissions path as typed input. Recording has a
two-minute limit; reaching the limit inserts a draft without submitting a task.
**Speak again** starts the next utterance. The microphone stays off while awaiting
and speaking replies, avoiding a claim of untested echo cancellation.

**Full response prose** reads completed chat replies; fenced code and standalone
structured payloads stay on screen. **Silent** keeps the written answer and makes
no synthesis request. **Stop speaking** interrupts audio, not the agent task.
**End voice** cancels voice operations and releases capture. Changing Chat's
conversation key ends the old voice destination. CLI coding-session output and
spoken Summary are not connected yet; do not infer support from the Talk button.

Audio goes only to the literal loopback endpoint. **This is a voice-only privacy
boundary**: the selected agent may still send text/project context to its provider.
The companion does not store audio/transcripts, and upstream utterance printing
is suppressed. Normal Xnaut conversation history is unchanged.

## Protocol v1

`GET /voice/v1/capabilities` and `WS /voice/v1/session` require
`Authorization: Bearer <token>`. Browser `Origin` headers are rejected; Xnaut
connects from Rust with proxy use disabled, literal loopback IPs only, and no
HTTP redirects. The server binds only `127.0.0.1`.

Capabilities declare `version:1`, `recognition:true`, `synthesis:true`,
`streaming_input:false`, `streaming_output:true`, `format:"pcm_s16le"`,
`sample_rate:16000`, `channels:1`, `max_text_bytes:4096`,
`max_input_seconds:120`, and `synthetic:false`.

One WebSocket carries one operation. Models remain loaded between operations.
The first client text frame is one of:

```json
{"type":"transcribe","id":"<canonical UUID>"}
{"type":"synthesize","id":"<canonical UUID>","text":"The supplied spoken answer."}
```

For transcription, send ordered binary PCM frames then
`{"type":"input.commit","id":"<same UUID>"}`. The server replies with `ready`
(same ID and protocol/audio format), `transcript.final` (same ID and `text`),
then `done` (same ID, `frames:1`). No partial transcript starts an agent task.

For synthesis, `ready` precedes ordered binary PCM, followed by `done` with the
number of audio frames. Every binary frame in either direction is:

| Bytes | Meaning |
| --- | --- |
| 0–15 | UUID bytes in network order |
| 16–19 | unsigned sequence number, big-endian, beginning at zero |
| 20 onward | mono signed PCM16 little-endian; 2–16,384 payload bytes |

Malformed IDs, gaps, repeats, wrong formats, oversized frames and premature
closure fail the operation. There is no retry/replay of submitted agent work.
Errors are `{"type":"error","id":"<UUID or null>","message":"..."}`.

Send `{"type":"cancel","id":"<same UUID>"}` during inference, or close the
socket. The server drops every subsequent engine block, including buffered
blocks yielded after model cancellation. A bounded eight-frame handoff queue
applies backpressure. Only one client may use the model worker at a time; another
gets a visible busy error while a cancelled computation unwinds.

Xnaut independently fences audio using session/turn/playback generations. The
native device callback emits silence for a cancelled generation and its worker
drops the stream. Its queue holds at most two seconds of 16 kHz PCM, with
linear resampling to the output device. Native cancellation does not wait for
the Python model to stop. Hardware interruption latency remains to be measured.

## Tests and measured limits

```sh
/path/to/speech-runtime/.venv/bin/python -m unittest discover -s companions/local-voice -p 'test_*.py' -v
node --test tests/voice-session.test.cjs
npx playwright test tests/voice-overlay.spec.mjs tests/voice-conversation.spec.mjs
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut voice --no-default-features
```

Protocol tests need `httpx` (tested 0.28.1). `--synthetic` starts a test-only
service with silent PCM and a fixed transcript, without importing ML engines.
Xnaut deliberately rejects its `synthetic:true` capability in normal operation.

A real loopback probe on the M1 Max/64 GiB development machine reproduced the
synthetic input transcript and received 82/83 ordered TTS frames without an LLM.
During concurrent development work, cold recognition took 36.53 seconds; cold
TTS first PCM took 27.52 seconds; warm TTS first PCM took 7.06 seconds (31.93
seconds to finish 2.66 seconds of audio). These results are substantially slower
than the direct-engine probe and **do not meet natural-conversation latency**.
The host load averages reached approximately 818/778/666; a later warm sample
still took 5.79 seconds to first PCM. These runs cannot isolate service overhead
from machine contention. Repeat under controlled load before tuning. No audible
latency, voice-quality, microphone-permission or signed-app hardware claim is
made from these file/transport tests. The installed app has not been replaced.

## Source and model attribution

The adapter consumes, rather than copies, the Apache-2.0 speech-to-speech handler
APIs named in `server.py`. The WebSocket host follows the documented
[FastAPI WebSocket interface](https://fastapi.tiangolo.com/advanced/websockets/).
Model cards currently label [MLX Parakeet v3](https://huggingface.co/mlx-community/parakeet-tdt-0.6b-v3)
CC-BY-4.0 and [MLX Qwen3-TTS Base 6-bit](https://huggingface.co/mlx-community/Qwen3-TTS-12Hz-1.7B-Base-6bit)
Apache-2.0. These are separate from Xnaut's code license. No model weights or
voice recordings are distributed here; retain upstream attributions and check
the original model and voice rights for your own distribution.
