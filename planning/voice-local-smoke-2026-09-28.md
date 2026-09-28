# Local public voice verification — 2026-09-28

Baseline: Forgejo dev `8b22d977e01f3557d821bb877fd1e0b5adaec110`.
Branch: `test/xnaut-416-local-voice`. Worktree: `.worktrees/voice-public-local`.

## Result

- Frontend build passed.
- 88 deterministic Rust voice tests passed.
- 20 Playwright voice UI tests passed on isolated port 4296.
- A real authenticated Live session accepted the session-start protocol using
  the existing local Bucki account. No microphone audio was used for this probe.
- Native app initially failed connecting: tokio-tungstenite 0.21 had no TLS
  feature enabled. The Python probe succeeded but the native wss connection
  could not. Enabled `native-tls` and updated the lockfile.
- Added an opt-in account smoke test through the same Rust connection helper
  used by the app. All 89 voice tests passed with that test explicitly enabled.
- Built and opened `xNAUT Voice Test.app` with a separate bundle identifier
  `com.nautcode.xnaut.voice-test`. The native Voice control now gets through
  connection setup and reaches the macOS microphone consent dialog.
- NautGate's configured local endpoint is reachable. The opened test Chat uses
  provider `nautgate`, model `auto`, and conversation key
  `local-voice-test-20260928`.

## Remaining acceptance

Microphone permission awaits the owner. Real multi-turn speech, playback,
interruption/echo and reopening with live context are not yet verified. The
automated UI tests use mocked commands; they are not real audio evidence.
The app's existing microphone permission text incorrectly describes only local
dictation; it should describe public voice transmission too.

## Repeatable native account probe

Normal tests skip the account probe. To opt in, with the private
`~/Library/Application Support/xnaut/voice-live.json` profile already configured:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --manifest-path src-tauri/Cargo.toml \
  live_account_accepts_session_start_over_tls -- --ignored
```

This opens a short billable connection; it sends only session configuration,
never microphone audio. Credentials remain outside the repository and output.

## Local app

Bundle: `src-tauri/target/debug/bundle/macos/xNAUT Voice Test.app`.
The installed `/Applications/xNAUT.app` was not replaced. The test app has
separate webview storage but shares the existing native xNaut configuration;
it is not a fully isolated user profile. This host's role is workstation.
The hook listener automatically selected an unused port because the installed
app already owns the configured port.

In the opened Chat pane, use **Voice** for the continuous public route.
**Talk** is the older local prototype. The test Chat was opened through the
existing `xnautAttachChatTab` function in the native developer console; general
navigation and Agent Space integration still need separate acceptance.
