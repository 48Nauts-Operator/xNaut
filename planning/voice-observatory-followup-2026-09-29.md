# Voice, Observatory and dispatch follow-up — 2026-09-29

Status: implementation on `feat/voice-cost-footer`, based on released v1.28.1 (`73e73ced`). Preview only; no release or merge is implied. The installed app and existing agents were not restarted.

## Delivered behavior

- Observatory has General and Agents tabs. General holds usage, voice accounting, Sessions and Ledger. Agents holds Running agents and Dispatched runs.
- Sessions, Running agents, Dispatched runs and Ledger default to five rows with Previous/Next and 5/10/25 selectors. Cards retain their height and the page scrolls. Pagination covers the loaded recent window, not an unlimited server history: Ledger loads up to 250 lines; dispatched history retains its existing fetch limit.
- Opus appears after Fable. The usage reader accepts a separately reported legacy Opus counter without duplicating newer scoped counters. Absent counters say “Not reported separately”; model API prices are not represented as a new subscription allowance.
- A voice footer and Observatory card appear when a valid voice profile is configured. Current/last connection and month-to-date estimates use the same native accounting feed.
- A quick mute next to each chat microphone controls the active voice conversation, sharing acknowledged state and failure handling with the right pane.
- Chat and Agent Space voice continue when switching tabs. An active agent conversation's view is retained when selecting another agent; its transcript and execution callback remain bound to the original thread. Explicitly closing the conversation/pane still ends the microphone.
- Starting STS on saved history now sends bounded historical context to the voice model as well as to the execution model. Old requests are marked historical and are not automatically executed. STT sends no restored history.
- Agent Settings → Compute offers Local, exe.dev, GitVM and Automatic (configured provider). NautBot's `dispatch_ticket` tool accepts an optional `environment` of `local`, `exe-dev` or `gitvm`, for this ticket only. Explicit missing configurations fail instead of falling back to local.
- Observatory worker rows and Flow Watch's Open session button use the actual terminal session ID. Flow Watch labels a detected Codex sign-in screen “sign-in required”.
- Remote Codex launches check login readiness before pushing source and starting an agent. This detects missing authentication; it does not provision authentication or configure a NautGate tunnel.
- Cleanup scanning and deletion recheck registered project bases, explicit agent workspace paths and launch environments. A registered base or parent containing it is protected, even if Git is clean. Unreadable registry data refuses cleanup. Ordinary unregistered ticket children remain eligible under existing rules.

## Cost and time-saved accounting

GPT-Live-1 is $0.05/minute, billed per connected second. Silence, mute, listening and backend waiting remain connected time. The execution model is billed separately.

The native meter consumes cumulative `session.usage.updated` snapshots and replaces them with `session.closed.usage.seconds`; duplicate snapshots are never added. While connected it interpolates using a monotonic clock. Closing stops microphone/playback immediately, sends `session.close`, and allows up to five seconds for the final duration before closing transport. Missing final reports remain explicitly estimated (`~`). Unknown model rates remain unpriced.

Only session IDs, model/rate, duration, spoken word count and start month are stored in `xnaut/voice-usage.sqlite3` under the OS application-data directory. No audio, transcripts or credentials are written to that ledger. Month totals cover sessions started in the local calendar month, on this device, since accounting was enabled. Prior usage and the short Settings connection-test session are not backfilled or included. This is an estimate, not an OpenAI invoice or account-wide billing report.

Input time saved = spoken words × (1 / typing WPM − 1 / speech WPM), floored at zero. Defaults are the owner's 149 speech WPM and an assumed 60 typing WPM; both are editable. This estimates input time only. It does not measure waiting, listening, corrections, task quality or overall productivity. No hourly earnings claim is inferred.

Sources checked:
- [GPT-Live-1 model](https://developers.openai.com/api/docs/models/gpt-live-1)
- [Voice latency and cost](https://developers.openai.com/api/docs/guides/voice-latency-cost)
- [Live conversations](https://developers.openai.com/api/docs/guides/live-conversations)
- [Opus 5.5 announcement](https://claude.com/blog/claude-opus-5-5-built-for-coding-sessions-that-use-more-context)
- [Opus model documentation](https://platform.claude.com/docs/en/models/opus-5-5/overview)

## Two separate startup incidents

### Missing local base worktree

Cleanup removed the registered `safety-net` base. Live project registration was inspected: XNAUT's `source_path` points there. The dispatcher uses that project registration; it is not inherently a special base unique to ticket 440. The owner described it as Claudi's workspace; that ownership description must not obscure the actual path dependency.

The original dev worktree at `00a0fb24a5ca7aa135fa5a2d740ed41923a5dd1b` was restored, along with 410 archived local entries. Git status is clean and the worktree is locked with a registered-dispatcher reason. Its older baseline was deliberately not advanced. Archives remain intact. Protection in this branch complements the Git lock; external shell cleanup must also respect registrations and locks.

### Local Codex daemon socket

The local Mac's Codex 0.158.0 daemon remained running as PID 98414, holding its protected socket under `/private/tmp/codex-daemon-501`. The advertised socket symlink in `~/.codex/app-server-control` was absent. A second daemon could neither bind the occupied physical socket nor connect through the missing alias.

Confirmed OpenAI's transport implementation: the advertised path is a symlink to a private socket whose name is SHA-256 of the canonical advertised path. Checked ownership, socket type and connectivity, then recreated only the missing deterministic symlink. `codex app-server daemon version` subsequently returned running, CLI/server 0.158.0. A temporary interactive `codex --no-alt-screen` launch reached Folder access and Agent Command Center; only that test client was closed. No model request was sent and the running daemon was not restarted. The cause of the original symlink removal is not established.

[Upstream socket implementation](https://github.com/openai/codex/blob/main/codex-rs/app-server-transport/src/transport/unix_socket.rs).

### Separate remote authentication

Read-only SSH checks on the configured exe.dev VM found Codex installed but `codex login status` unsuccessful. Local authentication is not remote authentication. GitVM is not currently configured on this installation. This branch supplies destination selection and actionable refusal; a paid live remote dispatch was not performed. Existing remote-launch limitations (including NautGate rebinding and some local tool endpoints) remain; selecting a destination does not supply those capabilities automatically.

## Conversational memory: current fix and next architecture

Current full conversation storage remains in the existing Chat and Agent Space localStorage paths. The fix preserves that thread during navigation and supplies recent context on reconnection; it is not a new durable, searchable memory database. Agent Space's execution history still uses its existing bounded recent window; the voice instructions cap restored text at 24,000 characters. Very old details outside those bounds are not guaranteed recall.

The next memory layer should have:

1. A native SQLite conversation store, with stable conversation/message/turn IDs, role, timestamps, agent/project scope, ordered text, tool calls/results, attachments and completion state. Commit each completed event atomically; stream partial drafts separately. Use unique event IDs to prevent replay duplicates.
2. A single app-owned conversation controller independent of visible tabs. Views subscribe to the same conversation, rather than owning its transport or storage. The current retained-view fix is a bounded bridge toward this ownership model.
3. Search by project, agent, ticket, date and full text (FTS), with direct links to the exact original turn. Retrieve existing content before asking the user to repeat it.
4. Bounded model context: recent turns plus a versioned rolling summary and retrieved original excerpts. Keep source message IDs and a summary coverage watermark. Summarize asynchronously through the existing model route, never as a blocking extra local model and never discard originals.
5. Distinguish user statements, agent proposals, verified tool results and approved durable facts. A model claim must not silently become a verified fact. Historical tool actions are context, not permission to repeat them.
6. Explicit per-project sharing and retention. Engram-OSS can index or receive selected durable records later; conversation continuity must work with it offline. Respect deletion across text, indexes and derived summaries. Cloud voice already receives its selected context; local storage alone does not make public voice private.
7. Tests for app restart, crash mid-stream, interrupted speech, tab/agent switches, history beyond the context window, exact recall of a prior ticket, no cross-project leakage, deletion and duplicate event delivery.

TypeSafe's skill and current docs were consulted. Known rules, accounting, persistence and explicit compute choices stay deterministic; no Jev request or additional paid decision model was added. Semantic ranking could be evaluated separately if retrieval quality warrants it.

## Validation and preview acceptance

- Rust: 1,439 passed, zero failed, 48 existing ignored live/integration tests.
- Browser: 67 passed, zero failed (the six affected suites, Chromium fixtures).
- ESLint: changed frontend files clean.
- `git diff --check`: clean.
- Real microphone/speaker acceptance and comparison against the provider's invoice remain manual; browser fixtures and state-machine tests do not prove acoustic behavior.

Preview bundle: `~/xnaut-testing/builds/2026-09-29-voice-observatory/xNAUT Preview.app`. Built in debug mode and signed with the existing Developer ID identity; not notarized or released. It uses a distinct bundle identifier and will have separate webview conversation storage. Native xNaut settings and agent registrations remain shared. No credentials are embedded. Do not run competing dispatcher instances during acceptance.

Manual acceptance:
1. Open preview when convenient; inspect General/Agents tabs and each list's 5/10/25 selector. Confirm existing conversations in the installed app are untouched.
2. Start mic → STS. Speak a ticket reference, switch to Observatory while a reply is pending, return and ask a follow-up. Confirm one connection, continuous audio and both turns in that same thread.
3. Hide the voice pane. Use the composer mute; wait for the acknowledged muted state. A new phrase must not reach transcription. Unmute and confirm the same session resumes. End voice explicitly.
4. Reopen that saved thread and start STS. Ask about the earlier reference; confirm historical requests are not re-executed. Repeat STT navigation and confirm draft text remains in its original composer.
5. While connected, compare footer/card duration and price; mute and silence should keep counting. End and wait up to five seconds for final usage. Restart preview to confirm the local total survives. Treat non-final totals as estimates.
6. Open a running worker from both Observatory and Flow Watch. The actual existing terminal should open, rather than launch a guessed command.
7. Save an agent's Compute setting; a one-ticket environment override should leave that preference unchanged. Attempting an unconfigured destination or unauthenticated remote Codex should fail clearly, without a fake working worker.

## Follow-up idea

The owner proposed Jev-assisted compute selection. See [the routing proposal](jev-compute-routing.md) for eligibility checks, owner precedence, parallel state gathering, evaluation and fallback. It is documented, not enabled in this preview.
