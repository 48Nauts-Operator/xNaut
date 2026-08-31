# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**xNAUT** is an AI-powered native terminal application built with Tauri v2, combining Rust backend performance with a modern web-based frontend. Originally converted from "Naiterm" (Node.js terminal app) to a native macOS application for better performance and smaller binary size (~5-10MB vs Electron's ~500MB).

### Key Features
- Multiple PTY sessions with tab management
- SSH connection support with config file integration
- AI chat integration (Anthropic, OpenAI, OpenRouter, Perplexity)
- Workflow recording and playback
- Smart triggers and notifications
- Session sharing capabilities

### Current Status
✅ Build-ready with critical ACL permissions fix applied
⚠️ Terminal functionality needs verification after ACL fix

## Grep before you call a `window.*` global

An undefined global in JavaScript is a **silent no-op, not a crash**. It survives
`node --check`, a clean build, a green test suite and a code read — and then the
feature just quietly does nothing, forever, with no error to notice.

Hit for real 2026-08-08: the roster panel called `window.xnautActiveProjectKey`,
which had never existed. The expression short-circuited to `null`, so the
per-project scope silently fell back to global. Nothing errored.

The trap has a second floor. Exporting the missing global as a **function** while
the call site treats it as a **value** is worse than the original bug: a function
object is truthy, so it gets used as data — in that case as a localStorage key,
writing settings to `xnaut-agent-roster:function () { return activeProjectId…`.
Still no error.

So: **before calling `window.something`, grep for where it is assigned.** If it
does not exist, export it properly and fix the call site to match its shape. This
is the same failure class as the gate-path bug the same day — passes every
automated check, silently wrong — and it is worth the ten seconds every time.

## Borrowed ideas must be credited in the code

We build xNAUT by reading other people's solutions to the same problems. That is
the method, not a shortcut — three of the build stage's four upgrades on
2026-08-08 came from other projects, and one of them corrected a design we were a
day from shipping.

Two rules follow, and they are not optional:

1. **Any mechanism taken from another project names its source in the file
   header** — project, the specific file or symbol, and the licence. See
   `gate_score.rs`, `plateau.rs`, `shared_notes.rs` for the form:

   ```rust
   // Ported from CORAL (Apache 2.0), `coral/agent/heartbeat.py::streak_for_epsilon`.
   ```

   The header also records where we **departed** from the original and why —
   `shared_notes.rs` scopes notes per-project where CORAL scopes them per-run,
   and that difference is the interesting part.

2. **Never present a borrowed idea as invented here** — not in release notes, not
   on the website, not in a commit message. The public post at
   `website/blog/we-read-other-peoples-code.html` is the standing example.

Reimplementing rather than copying is also better engineering, not just better
manners. Every port on 2026-08-08 improved on its source precisely because
rewriting forced us to understand which details were load-bearing.

## Architecture

xNAUT uses a hybrid architecture:
- **Backend**: Rust (Tauri v2) - handles PTY sessions, SSH connections, AI integration, and system calls
- **Frontend**: HTML/CSS/JavaScript with xterm.js - provides the terminal UI
- **Communication**: Tauri IPC (async invoke/emit pattern)

```
Frontend (HTML/JS/xterm.js)
    ↕ Tauri IPC
Backend (Rust modules)
    ├─ pty.rs         - PTY session management
    ├─ ssh.rs         - SSH connections
    ├─ ai.rs          - AI provider integration
    ├─ triggers.rs    - Pattern matching & automation
    ├─ state.rs       - Thread-safe app state
    ├─ commands.rs    - Tauri command handlers
    └─ errors.rs      - Error types
```

## Development Commands

### Building and Running

```bash
# Development mode (with hot reload) - RECOMMENDED for testing
cd src-tauri
cargo tauri dev

# Build Rust backend only
cargo build

# Release build
cargo build --release

# Full application bundle for macOS
cargo tauri build
# Output: target/release/bundle/macos/xNAUT.app

# Clean build (if having issues)
cargo clean
cargo tauri build
```

### Opening the Built App

```bash
# Launch the bundled macOS app
open target/release/bundle/macos/xNAUT.app

# Access DevTools
# Press Cmd+Option+I or click the 🐛 button in the app
```

### Testing

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test --lib pty::tests
cargo test --lib state::tests
cargo test --lib ai::tests
cargo test --lib triggers::tests

# Run with coverage
cargo tarpaulin --out Html --output-dir coverage
```

### Code Quality

```bash
# Linting (must pass with no warnings)
cargo clippy -- -D warnings

# Format check
cargo fmt -- --check

# Auto-format
cargo fmt

# Security audit
cargo audit
```

## 🚨 CRITICAL: Tauri 2.0 ACL Configuration

**THE MOST IMPORTANT THING TO KNOW:** Tauri 2.0 has a security model called ACL (Access Control List) that blocks ALL commands and event listeners by default.

### The Problem
Without proper ACL configuration:
- Frontend cannot call Rust commands: `"Command not allowed by ACL"`
- Frontend cannot listen to events: `"plugin:event|listen not allowed by ACL"`
- Terminal output won't display (black screen)
- All features will be blocked

### The Solution
The app includes `src-tauri/capabilities/default.json` which grants all necessary permissions:

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:event:allow-listen",    // ← CRITICAL for terminal output
    "core:event:allow-emit",
    "core:event:default",
    // ... + all 17 custom commands
  ]
}
```

And `src-tauri/tauri.conf.json` must reference it:

```json
{
  "app": {
    "withGlobalTauri": true  // ← Makes Tauri API available globally
  },
  "identifier": "com.xnaut.app",
  "windows": [{
    "label": "main",  // ← Must match capabilities
    "title": "xNAUT"
  }],
  "security": {
    "capabilities": ["default"]  // ← References capabilities/default.json
  }
}
```

### Verification
If terminals show black screens or commands fail:
1. Verify `src-tauri/capabilities/default.json` exists
2. Check `tauri.conf.json` has `"capabilities": ["default"]`
3. Check window has `"label": "main"`
4. Rebuild: `cargo clean && cargo tauri build`

## Key Architectural Patterns

### Thread-Safe State Management
All shared state uses `Arc<Mutex<T>>` wrapping:
- PTY sessions stored in `HashMap<String, PtySession>`
- SSH sessions stored in `HashMap<String, SshSession>`
- Triggers and shared sessions similarly managed
- Session IDs are UUIDs generated via `uuid::Uuid::v4()`

### Async Event-Driven Output
PTY output is streamed via background tokio tasks. Understanding this flow is **CRITICAL** for debugging:

**Complete Data Flow:**
```
User clicks ➕ button
  ↓
Frontend: invoke('create_terminal_session')
  ↓
Backend commands.rs: create_terminal_session()
  ↓
Backend pty.rs: spawn PTY + shell process (/bin/zsh or /bin/bash)
  ↓
Backend pty.rs: spawn_pty_reader() starts background thread
  ↓
Reader thread continuously reads PTY output
  ↓
Output is base64-encoded
  ↓
Backend: app.emit('terminal-output-{session_id}', data)
  ↓
Frontend: listen('terminal-output-{session_id}', callback)  ← REQUIRES ACL PERMISSION
  ↓
Frontend app.js: base64 decode
  ↓
Frontend: term.write(data)
  ↓
xterm.js renders in terminal element
```

**If ANY step fails, terminal stays black!** The ACL permission issue blocked the `listen()` call, which is why terminals showed black screens before the fix.

### Frontend-Backend Communication
All Rust functions exposed via `#[tauri::command]` macro:
- Commands return `Result<T, String>` for error handling
- Frontend calls via `invoke('command_name', { params })`
- Events emitted from Rust via `app.emit(event_name, payload)`

## Module Responsibilities

### `main.rs`
- Application entry point
- Registers all Tauri commands
- Initializes AppState
- Displays ASCII banner on startup

### `state.rs`
- Defines `AppState` with all shared state
- Session ID generation (`generate_session_id()`, `generate_share_code()`)
- Thread-safe access patterns

### `pty.rs`
- `create_pty_session()` - Creates new PTY with portable-pty
- `write_to_pty()` - Sends input to PTY master
- `resize_pty()` - Updates terminal dimensions
- `close_pty()` - Cleanup and process kill
- `spawn_pty_reader()` - Background output streaming

### `ssh.rs`
- `SshConfig` - Connection parameters
- `connect()` - Establishes ssh2 connection
- Supports password and public key auth
- `open_shell()` - Interactive shell session
- `execute_command()` - Single command execution

### `ai.rs`
- `ask()` - Send prompts to LLM providers
- `analyze_output()` - Error analysis
- `suggest_command()` - Natural language to shell commands
- Supports OpenAI, Anthropic, custom endpoints

### `triggers.rs`
- `process_output()` - Regex pattern matching on terminal output
- `execute_trigger_action()` - Runs actions (Notify, RunCommand, AiAssist)
- `create_default_triggers()` - Helpful defaults (error detection, etc.)

### `commands.rs`
- Exposes all backend functions to frontend
- Handles serialization/deserialization
- Error conversion to String for Tauri

## Frontend Structure

### Main Files
- `src/index.html` - UI structure with modals for settings, SSH, triggers, workflows
- `src/js/app.js` - Main application logic (if exists)
- `src/css/styles.css` - Styling (if exists)

### UI Components
- Top bar with LLM provider switcher
- Tab-based terminal management
- AI chat panel (collapsible right sidebar)
- Settings modal for API keys
- SSH profiles modal
- Triggers & notifications modal
- Workflows & notebooks modal

### Terminal Integration
Frontend uses xterm.js (v5.5.0) via CDN:
- Terminal instances managed per tab
- Input sent via `write_to_terminal` command
- Output received via `terminal-output:{session_id}` events
- Resize handled via `resize_terminal` command

## Important Implementation Details

### Base64 Encoding
All PTY I/O is base64-encoded for safe transport:
- Output: Encoded in Rust, decoded in JavaScript (`atob()`)
- Input: Can be sent raw or base64 (backend handles both)

### Session Lifecycle
1. Create: `create_terminal_session()` returns UUID
2. Use: Frontend listens for events, sends writes
3. Close: `close_terminal()` kills process and removes from state
4. Cleanup: Background reader task exits on PTY closure

### Error Handling
Custom error types in `errors.rs`:
- `PtySessionNotFound` - Invalid session ID
- `PtyCreationFailed` - PTY spawn error
- `SshConnectionFailed` - SSH connection error
- `SshAuthFailed` - Authentication error
- `AiServiceError` - AI API errors
- `TriggerNotFound` - Invalid trigger ID

All errors implement `thiserror::Error` for good error messages.

## Dependencies

### Core Rust Crates
- `tauri` (v2.0) - Framework
- `tokio` (v1) - Async runtime (features = ["full"])
- `portable-pty` (v0.8) - Cross-platform PTY
- `ssh2` (v0.9) - SSH client
- `reqwest` (v0.11) - HTTP client for AI APIs
- `serde/serde_json` - Serialization
- `uuid` (v1.0, features = ["v4", "serde"]) - Session IDs
- `base64` (v0.21) - Encoding
- `regex` (v1.10) - Pattern matching

### Frontend Dependencies
- xterm.js (v5.5.0) via CDN

## System Requirements

### Linux Dependencies (Ubuntu/Debian)
```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  libssl-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

### macOS
```bash
xcode-select --install
```

### Windows
- Visual Studio 2022 with C++ tools
- WebView2 (pre-installed on Windows 11)

## Common Development Workflows

### Adding a New Tauri Command
1. Add function with `#[tauri::command]` in `commands.rs` or relevant module
2. Add to `invoke_handler![]` macro in `main.rs`
3. Call from frontend via `invoke('command_name', { params })`

### Adding a New AI Provider
1. Extend `ai.rs` with new provider logic
2. Update `ask()` function to handle new provider
3. Add API key field to settings modal
4. Update frontend LLM switcher dropdown

### Adding a New Trigger Action
1. Add variant to `TriggerAction` enum in `state.rs`
2. Implement execution logic in `execute_trigger_action()` in `triggers.rs`
3. Update frontend trigger creation modal

### Debugging PTY Issues
```bash
# Enable debug logging
RUST_LOG=debug cargo tauri dev

# Module-specific logging
RUST_LOG=xnaut::pty=trace cargo tauri dev
```

## Performance Characteristics

- **PTY Creation**: <100ms per session
- **Memory per PTY**: ~2MB (mostly buffer overhead)
- **Concurrent Sessions**: 50+ tested successfully
- **IPC Latency**: Sub-millisecond
- **Binary Size**: ~8MB (release, stripped)

## Security Considerations

1. **No Shell Injection**: All commands validated before execution
2. **PTY Isolation**: Each session runs in separate process
3. **SSH Key Protection**: Private keys never exposed to frontend
4. **API Keys**: Stored in frontend localStorage (user's responsibility to secure)
5. **Session IDs**: Cryptographically random UUIDs
6. **Memory Safety**: All Rust code is memory-safe (no unsafe blocks in core)

## Troubleshooting

### Issue 1: "Command not allowed by ACL" ⚠️ MOST COMMON

**Error Message:**
```
❌ Failed to create terminal session
"Command plugin:event|listen not allowed by ACL"
```

**Root Cause:** Tauri 2.0 ACL blocking commands/events (see critical section above)

**Solution:**
1. Verify `src-tauri/capabilities/default.json` exists
2. Verify `tauri.conf.json` has `"capabilities": ["default"]`
3. Verify window has `"label": "main"`
4. Rebuild: `cargo clean && cargo tauri build`

### Issue 2: Black Terminal Screen (No Prompt)

**Previous Cause:** ACL blocking events (should be fixed now)

**If still happening:**

**Scenario A: See console logs but no prompt**
- Backend PTY creating but shell not spawning
- Check `pty.rs` line ~90 for shell detection logic
- Verify `/bin/zsh` or `/bin/bash` exists

**Scenario B: No console logs at all**
- ACL still blocking or Tauri invoke failing
- Open DevTools and check for errors
- Test manually: `await invoke('create_terminal_session')`

**Scenario C: Console logs stop at "Setting up listener"**
- Backend not emitting events
- Check `pty.rs` spawn_reader thread
- Add debug logging in `spawn_pty_reader()`

**Scenario D: See "Received terminal output" but terminal black**
- xterm.js rendering issue
- Check `term.write()` calls in `app.js`
- Verify xterm.js loaded from CDN

### Issue 3: Tauri API Not Available

**Error Message:**
```
❌ Tauri API Missing
```

**Solution:** Already fixed with `"withGlobalTauri": true` in tauri.conf.json

**If still happening:**
1. Check browser console for `window.__TAURI__`
2. Verify `tauri.conf.json` has `"withGlobalTauri": true`
3. Verify `Cargo.toml` has `features = ["devtools"]`
4. Try dev mode: `cargo tauri dev`

### Issue 4: base64 Compilation Errors

**Error Message:**
```
error[E0425]: cannot find function `encode` in crate `base64`
```

**Root Cause:** base64 crate v0.21+ changed API

**Solution:** Already fixed in all files using:
```rust
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
STANDARD.encode(data)  // instead of base64::encode(data)
```

### Issue 5: SSH Config Hosts Not Loading

**Problem:** SSH modal doesn't show saved hosts

**Solution:** Already fixed - `loadSshConfigHosts()` now called in `showNewSSHProfile()`

### Build Failures
- Ensure all system dependencies installed
- Run `cargo clean` then rebuild
- Check Rust version: `rustc --version` (requires 1.70+)

### PTY Not Starting
- Verify shell exists: `which bash` or `which zsh`
- Check working directory is accessible
- Enable debug logging: `RUST_LOG=xnaut::pty=debug cargo tauri dev`

### SSH Connection Issues
- Test manually: `ssh user@host`
- Check key permissions: `chmod 600 ~/.ssh/id_rsa`
- Verify network: `ping host`

### Frontend Not Loading
- Check browser console for errors (DevTools: Cmd+Option+I)
- Verify xterm.js CDN is accessible
- Check Tauri IPC is working: look for command invocation errors

### Quick Diagnostic Script

Paste in DevTools Console to check system health:

```javascript
console.log('=== xNAUT Diagnostic ===');
console.log('Tauri API:', window.__TAURI__ ? '✅' : '❌');
console.log('App State:', window.xnaut ? '✅' : '❌');
console.log('Tabs:', window.xnaut?.tabs?.length || 0);

try {
  const result = await invoke('create_terminal_session');
  console.log('Backend:', '✅', result);
} catch (e) {
  console.log('Backend:', '❌', e);
}
```

## Testing Strategy

- **Unit Tests**: In each module's `#[cfg(test)]` section
- **Integration Tests**: In `tests/` directory
- **Manual Testing**: Via `cargo tauri dev`
- **Coverage Goal**: >80% for core modules

### Essential Testing Checklist

After building, verify these work:

**Terminal Functionality (CRITICAL):**
- [ ] App launches without errors
- [ ] DevTools can be opened (Cmd+Option+I or 🐛 button)
- [ ] Status shows "✓ Tauri API Ready"
- [ ] Can create terminal tab (➕ button)
- [ ] Console shows "🔄 Attempting to create terminal session"
- [ ] Console shows "✅ Terminal session created: [id]"
- [ ] Console shows "📡 Setting up listener for: terminal-output-[id]"
- [ ] Console shows "📥 Received terminal output"
- [ ] Terminal shows colored status messages (yellow, green, cyan)
- [ ] Terminal shows shell prompt (bash$ or zsh%)
- [ ] Can type in terminal
- [ ] Commands execute and show output

**Expected Console Logs:**
```
🎯 app.js loaded successfully!
🚀 DOM Ready, initializing xNAUT...
✅ Tauri API available after X attempts
🔄 Attempting to create terminal session...
📦 Terminal session result: {session_id: "abc123"}
✅ Terminal session created: abc123
📡 Setting up listener for: terminal-output-abc123
⌨️ Setting up input handler...
📥 Received terminal output: [data]
```

**Expected Terminal Visual Feedback:**
- 🔄 Yellow: "Connecting to backend..."
- ✅ Green: "Connected! Session: [id]"
- ⌨️ Cyan: "Terminal ready for input"
- Shell prompt appears

**Other Features:**
- [ ] Can create multiple tabs
- [ ] Can close tabs
- [ ] Settings modal opens
- [ ] SSH modal opens and shows saved hosts
- [ ] Chat panel toggles
- [ ] Workflows modal opens
- [ ] Triggers modal opens
- [ ] SSH connections work
- [ ] AI chat responds
- [ ] Workflow recording works
- [ ] Triggers fire correctly

## Documentation

### Essential Documentation Files

**Start Here:**
- `docs/handover.md` - Complete handover document with problem history and solutions
- `EXTRACT_FIRST.md` - Package overview
- `README_FIRST.md` - Current status and overview

**Technical Documentation:**
- `QUICKSTART.md` - Quick start guide
- `RUST_BACKEND.md` - Detailed backend architecture
- `TESTING.md` - Testing guide with examples
- `FRONTEND_README.md` - Frontend documentation
- `INTEGRATION_GUIDE.md` - Integration patterns
- `UI_OVERVIEW.md` - UI component details

**Problem-Specific Guides:**
- `ACL_FIX.md` - ACL permission problem and solution
- `BUILD_NOW.md` - Quick build instructions
- `TESTING_CHECKLIST.md` - Testing guide
- `DEBUGGING_XNAUT.md` - Full debug guide (if exists)
- `CURRENT_STATUS.md` - All fixes applied

**If You Encounter Issues:**
1. Read `docs/handover.md` for known problems and solutions
2. Check `ACL_FIX.md` if getting permission errors
3. Review `TESTING_CHECKLIST.md` for verification steps

## Release Build Optimization

The `Cargo.toml` includes release optimizations:
- `panic = "abort"` - Smaller binary
- `lto = "thin"` - Link-time optimization
- `opt-level = "z"` - Optimize for size
- `strip = true` - Remove debug symbols

## Current Status & Known Issues

### ✅ What Works (Verified)
- App compiles successfully on macOS
- App launches and displays UI
- Tauri API bridge loads correctly
- Can create terminal tabs
- Can close terminal tabs
- All modals open (Settings, SSH, Workflows, Triggers, Chat)
- UI elements render correctly
- No JavaScript errors after fixes applied

### ⚠️ What Needs Testing (After ACL Fix)
The ACL permission fix was just applied. These features **should** now work but need verification:
- Terminal shows shell prompt
- Terminal accepts input
- Terminal shows command output
- SSH connections work
- AI chat functionality
- Workflow recording/playback
- Triggers fire correctly
- Session sharing

### 🔧 Recent Fixes Applied
1. **ACL Permissions**: Created `capabilities/default.json` to allow all commands and events
2. **Tauri API Loading**: Added `"withGlobalTauri": true` to tauri.conf.json
3. **base64 API**: Updated to v0.21+ Engine pattern
4. **DOM Loading**: Wrapped initialization in DOMContentLoaded
5. **SSH Config Loading**: Fixed `loadSshConfigHosts()` function call

### 🎯 Next Steps
1. Build on macOS: `cd src-tauri && cargo tauri build`
2. Open app: `open target/release/bundle/macos/xNAUT.app`
3. Test terminal functionality (see testing checklist above)
4. Verify ACL fix resolved the black screen issue
5. Test remaining features (SSH, AI, workflows, triggers)

### 📋 Future Enhancements
- Session recording/playback
- Container/Docker integration
- Plugin system for custom triggers
- Multi-user collaboration features
- End-to-end encryption for shared sessions

---

# Operational facts — read this before asking Andre anything

Added 2026-07-28 after one session's worth of avoidable questions. If you are
about to ask "which branch / where does it run / can I use X" — the answer is
almost certainly here.

## 1. The running app is NOT the repo

Features Andre says xNAUT "has" may exist only as **uncommitted work in the
worktree the dev app was launched from**. Never conclude "it doesn't exist"
from `git log`. Find the live one first:

```bash
for p in $(pgrep -x xnaut); do lsof -p $p 2>/dev/null | grep -m1 cwd | awk '{print $9}'; done
git worktree list
```

`cargo tauri dev` serves the frontend from that worktree's `src/`, uncommitted
changes included.

## 2. Version lineage

Releases ship from the **loops-platform lineage**, not `main` and not the
tasks-mode branches. As of 2026-07-28 the live line is v1.10.1 on
`feature/bamt-methodology`. Older: 1.9.3 `feature/loops-platform`, 1.8.10
`feature/tasks-mode-v1.6` (stale — do not branch new work off it).
Always branch new work off the **current live lineage**, verified with
`grep '^version' src-tauri/Cargo.toml`.

## 3. Git: Forgejo only. Never GitHub.

`forgejo` (cosmos NAS, Tailscale) is the source of truth. The `origin` remote
points at GitHub and is a mirror — **never push there**. On every new branch:

```bash
git config branch.<branch>.pushRemote forgejo
```

Feature work goes in a worktree under `.worktrees/<name>`, branched with
`--no-track` off the live lineage. Never force-push and never bypass the git
hooks.

## 4. GitVM sandboxes are available — just use them

GitVM is configured and working. Do not ask permission, do not ask whether it
is set up. Use the seam in `src-tauri/src/sandbox.rs`:
`SandboxDriver::for_settings(provider)` -> `create` / `wait_ready` / `exec` /
`destroy`, returning `SandboxHandle { id, public_url }` (exposed port 80, so a
sandbox gives you a real clickable domain).

Rules learned the hard way:
- **Teardown destroys `/workspace`** — pull the source back BEFORE `destroy`
  (XNAUT-40). Never destroy after a failed pull.
- **One sandbox per directory** — key sandboxes by directory/slug.
- Default lease is `timeout_secs = 3600`; renew while a surface is open.

## 5. Paths (macOS)

| What | Where |
|---|---|
| App settings | `~/Library/Application Support/xnaut/settings.json` — **not** `~/.config/xnaut` |
| Mobile bridge config | `~/Library/Application Support/xnaut/mobile.json` (own file so other versions cannot strip the token) |
| Work vault | `~/.xnaut-vault/work/<Project>/Development/…` — **project first**, no `Development/` wrapper before it |
| Design docs | `~/.xnaut-vault/work/<Project>/Development/features/YYYY-MM-DD_Title.md`, YAML frontmatter `Author` + `Last modified` |
| PM control repo | `~/.xnaut-control/projects/<KEY>/tickets/<KEY>-N.json` |
| Panic log | `~/Library/Application Support/xnaut/rust-panics.log` |

## 6. Editing the PM control repo

A valid change is **three things**: the ticket JSON + an event file in
`events/` (`ticket.created` / `ticket.updated`) + a git commit (`feat(pm): …`).
Bump `revision` and `updated_at`. `documentation` references the vault as
`work:<Project>/Development/features/….md`.

Never write `null` into a string field — `ProjectRecord.task_id` and friends are
`String`, and `#[serde(default)]` does not cover an explicit null. One null used
to blank the entire Projects board.

## 7. Adding a Tauri command — the ACL trap

Registering in `main.rs`'s `invoke_handler!` is **not enough**. The command must
also be listed in `src-tauri/permissions/default.toml`, or the frontend gets
"not allowed by ACL" at runtime. `capabilities/default.json` references the
permission set; it is not where individual commands go.

## Pipes eat exit codes: never chain through them

Three times on 2026-08-31 alone, `git <state-changing> | tail` (or `| grep`)
swallowed a non-zero exit and let `&&` march on: a commit landed on a broken
build, and an agent's merge landed on whatever branch happened to be checked
out. The pipeline's exit status is the LAST command's, so `tail` reports
success for a failed checkout.

The rule, now structure: a git command that changes state (checkout, merge,
commit, reset, rebase, push) is NEVER piped. Run it bare, check it, and only
then trim output. When a pipeline is unavoidable, `set -o pipefail` first or
test `PIPESTATUS`. An agent that wants pretty output runs the command twice:
once bare for the verdict, once piped for the summary.

## 8. Do not

- **Do not kill or restart the app Andre is using.** Check what is running
  first (section 1). A dev app started by him is his, not a test fixture.
- **Do not type into terminals/sessions he has on screen** — that includes
  probe commands landing in an agent's input box.
- **Do not edit the worktree that is serving his running app** unless he asked
  for it there: Rust *and* JS edits trigger a `cargo tauri dev` rebuild and
  restart his session.
- **Do not let one version's settings save strip another's keys** — `Settings`
  keeps unknown keys via `#[serde(flatten)] extra`; keep it that way.

## 9. Build / run

```bash
cd <worktree>/src-tauri && cargo tauri dev      # UI work: JS is served live
cargo test --bin xnaut                          # full suite
```

Release builds embed the frontend: a JS-only change ships stale unless you
`rm target/release/xnaut` first. Use `cargo tauri dev` for UI iteration.

## 10. Agents

`just -g cc` (Claude), `just -g codex` (Codex), `justpi` (pi) — persistent
Zellij sessions. Live sessions: `zellij list-sessions` (rows without `EXITED`).
