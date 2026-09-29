---
Author: Codex
Last modified: 2026-09-29
Status: preview, not released
---
# Observatory, voice corrections and remote session repair

Based on released 1.28.2 (`d91eb8b`), isolated branch `fix/observatory-jury-voice`.

## What changed

- Retired jury asks are reconciled against the authoritative job when listed. Only the exact retired job's open ask is archived; an in-flight review, missing job, another job on the same ticket, and revocable notification are retained. Plan/PoC cards no longer offer the sign-off-only Re-review action.
- Compact Observatory summary cards are 205px with 8px gaps and wrap. Voice monthly details and speed assumptions expand on demand; current cost and estimated time saved remain visible.
- Jev has an xNaut-only cost card, gated by a configured TypeSafe credential or recorded usage. It reads only the native application's reserved `jev-usage.json` receipts, deduplicates request IDs, and estimates Jev 1.13.0 input at $0.042/M tokens. Output is free; unknown models are explicitly unpriced; unreadable accounting never becomes a zero bill. No account-wide or external-tool history is imported. **No xNaut Jev inference caller exists yet**: the truthful current state is “No xNaut calls recorded.” Adding the routing engine and writing its actual usage receipts remains separate work; this change does not enable it.
- Typed STS corrections use `session.thinking.append` for Live context and the same selected xNaut agent dispatch as spoken work. They stop old playback, preserve/commit earlier speech, and create a new complete user message. Local typed-turn IDs never become provider delegation IDs. The microphone/session remain active. Long context is chunked using the existing bounded commentary helper.
- Adopted remote rows are attached through `agent_remote_attach` before opening a terminal. The request specifies the exact session name and resolves the environment from the row. An unavailable/finished session produces a visible error instead of an empty tab. Concurrent clicks deduplicate. Local adopted rows use the existing Zellij opener. Ambiguous GitVM workspace records are refused rather than attaching a guessed workspace.

## Live diagnosis and repair

XNAUT-402 job `a74c3ab0-3827-469e-84ad-f33dec706951` and XNAUT-404 job `12938593-ebae-4d76-8e60-a87ac3807355` were already `superseded`. Their stale inbox asks remained `open`, so approval correctly refused. Archived only those two asks using append-only status events, preserving the job/ticket evidence and backing up each inbox file under `~/xnaut-testing/runs/2026-09-29-observatory-jury-voice/`. The running app's `/api/inbox` subsequently no longer returned either ask. Nothing was approved, merged or dispatched.

A separate short GPT-Live probe (silence only, no microphone capture) reproduced the keyboard problem: `response.item.create` returned **“response.item.create requires Responses delegation.”** The replacement `session.thinking.append` returned `session.thinking.appended` and no errors. The user's voice session was not touched.

exe.dev still has `xnaut-cortana-0340c4d0` and `xnaut-cortana-e35d8b58`, both with live Codex processes. A separate read-only SSH/tmux viewport onto the first returned 4,101 terminal bytes. Only that probe's local SSH viewer was terminated; remote agents were not interrupted.

## Validation

- Rust full suite: **1,444 passed, 0 failed, 48 existing ignored**.
- Full browser suite: **368 passed**; three new focused tests cover Jev cost/error states and exact remote attachment/deduplication/failure. Final affected browser pass: **11 passed** after compacting the voice details.
- Live protocol rejection/acceptance probe described above.
- exe.dev session discovery and read-only terminal stream described above.
- Observatory desktop screenshot inspected; the mock accounting screenshot is UI evidence, not a usage claim.
- `git diff --check` passed. Repository-wide ESLint reports 56 pre-existing errors; comparison against the base found no added app.js issues, and the other changed UI modules pass.
- Separate macOS debug preview bundle; not installed, launched or released. User's running app remains on 1.28.2.

Remaining human acceptance: use the preview, start STS, speak, submit a typed correction, hear its reply, then speak another request without restarting voice. Open each Cortana row and verify it shows the selected run. No claim of a fresh microphone/speaker acceptance test is made.

## Sources

- [GPT-Live session context events](https://developers.openai.com/api/docs/guides/live-conversations): session-wide `session.thinking.append`, null delegation ID, bounded context, acknowledgment.
- [TypeSafe API](https://docs.typesafe.ai/api.md): provider input/output token usage.
- [TypeSafe models](https://docs.typesafe.ai/models.md): Jev 1.13.0 input-only pricing, verified 2026-09-29.
