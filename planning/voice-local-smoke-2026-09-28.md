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

The owner granted microphone permission and confirmed the continuous public
conversation works. Formal interruption/echo and live context-resume acceptance
remain outstanding. The automated UI tests use mocked commands; they are not
real audio evidence. The microphone permission description now explicitly
describes public voice transmission, including STT.

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

In Chat, click the single **microphone**, then choose **STS — Speech to Speech**
for continuous conversation or **STT — Speech to Text** for a composer draft.
STT does not send automatically or speak. Public STT still sends microphone
audio to the configured public provider. Jarvis remains V2.
General navigation and Agent Space integration need separate acceptance.

## Follow-up: composer, right pane and live ticket retrieval

The rebuilt native test bundle includes:

- One mic and an STS/STT/Stop submenu; no production Chat Talk/Voice text buttons.
- Copy-draft icon; speech captions saved and displayed in main Chat, alongside
  the complete agent answer.
- Voice status docked in the right pane. Green animation follows actual native
  playback queue activity; blue follows delegated backend work. Reduced-motion
  preferences disable animation.
- Tool activity displayed in Chat while lookups/actions run.
- Exact ticket-ID lookup before the default newest-20 limit.

### Empty-answer root cause

Reproduced through the configured NautGate route with model `auto`: the gateway
returned HTTP 200 and an event-stream content type, but the body was plain JSON
reporting that reasoning was mandatory and could not be disabled. The native
SSE reader ignored the body and reported an empty successful answer.

The parser now detects JSON and SSE provider errors, rejects empty rounds, and
retries once without the reasoning override for that specific validation failure.
Authentication and unrelated failures are not treated as that retry condition.
No NautGate deployment was necessary for this client-side correction.

### Verification

- 27 Playwright tests passed, including STS/STT switching, stale-session events,
  main-chat captions, copy, tool progress, full answers and right-pane states.
- 89 deterministic Rust voice tests passed; the account handshake test remained
  ignored in this run (it passed explicitly in the earlier TLS verification).
- HTTP-200 JSON/SSE error regression and existing stream-parser tests passed.
- Explicit native NautGate read-only tool-loop test passed: it executed ticket
  lookups for both XNAUT-277 and XNAUT-445 and returned both in the answer.
  Live model quality is nondeterministic; this is a successful observed run.
- Modified JavaScript lint and `git diff --check` passed.
- Frontend and native debug app build passed; the updated test app was reopened.

The latest UI state is covered by automated browser tests; final hands-on native
STS/STT/color verification remains with the owner. An attempt to reopen the
saved Chat using coordinate-based console automation sent JavaScript to the
wrong input. That automation was stopped; the user does not need to execute
the accidentally pasted line. Tool progress currently represents live activity;
separate durable action-history reconstruction is outside this patch.

## Follow-up: macOS menu click regression

Owner reported STS and STT selections did nothing. Reproduced both failures in
Playwright WebKit before changing code: STS never invoked voice_live_open, and
STT never opened a session or populated the draft. The previous Chromium-only
suite missed native macOS pointer/focus ordering.

WebKit blurs the focused menu item when a button is clicked; relatedTarget is
outside the menu, so focusout hid the menu before click could activate a mode.
Preventing the menu item's primary pointer-down default preserves focus until
click; keyboard activation remains on the existing click handler.

Both previously failing WebKit tests pass after the fix. The full voice-live
suite passed in Chromium and WebKit: **42/42**. Repeat with:

```sh
npx playwright install chromium webkit
XNAUT_TEST_PORT=4296 npx playwright test --config=playwright.voice.config.mjs
```

These browser tests mock native voice commands; they prove the selected mode
reaches session startup, not microphone/audio acceptance. Native bundle rebuilt
for the operator to try the corrected menu.
