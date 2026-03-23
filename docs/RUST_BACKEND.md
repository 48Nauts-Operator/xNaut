# XNAUT Rust Backend Architecture

This document describes the Rust backend implementation for XNAUT, a native AI-powered terminal application built with Tauri v2.

## Overview

XNAUT uses a Rust backend for high performance, memory safety, and native system integration, while maintaining a web-based frontend using HTML/CSS/JS with xterm.js.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Frontend (Web)                           │
│              HTML + CSS + JS + xterm.js                     │
└───────────────────────┬─────────────────────────────────────┘
                        │ Tauri IPC
                        ▼
┌─────────────────────────────────────────────────────────────┐
│                  Tauri Commands Layer                       │
│                  (commands.rs)                              │
└───────┬─────────────┬──────────────┬──────────────┬─────────┘
        │             │              │              │
        ▼             ▼              ▼              ▼
    ┌───────┐   ┌─────────┐   ┌──────────┐   ┌──────────┐
    │  PTY  │   │   SSH   │   │    AI    │   │ Triggers │
    │ (pty) │   │  (ssh)  │   │   (ai)   │   │(triggers)│
    └───┬───┘   └────┬────┘   └─────┬────┘   └─────┬────┘
        │            │              │              │
        └────────────┴──────────────┴──────────────┘
                        │
                        ▼
                ┌───────────────┐
                │  App State    │
                │   (state)     │
                └───────────────┘
```

## Module Structure

### `main.rs` - Application Entry Point
- Initializes Tauri app
- Registers all commands
- Sets up application state
- Displays ASCII art banner on startup
- Configures event handlers

### `state.rs` - Application State Management
Thread-safe state container using `Arc<Mutex<>>` for:
- **PtySession**: Active PTY sessions with process handles
- **SshSession**: Active SSH connections
- **Trigger**: Pattern-based triggers
- **SharedSession**: Session sharing state

Key functions:
- `AppState::new()` - Creates empty state
- `generate_session_id()` - UUID generation
- `generate_share_code()` - Short shareable codes

### `pty.rs` - PTY Session Management
Handles pseudo-terminal sessions using `portable-pty`:

**Key Functions:**
- `create_pty_session()` - Creates new PTY with shell process
- `write_to_pty()` - Sends input to terminal
- `resize_pty()` - Updates terminal dimensions
- `close_pty()` - Kills session and cleanup
- `spawn_pty_reader()` - Background task for output streaming

**Features:**
- Async output reading with tokio
- Base64-encoded output emission to frontend
- Automatic session cleanup on process exit
- Support for custom shells and working directories

### `commands.rs` - Tauri Command Handlers
Exposes Rust functions to frontend via Tauri IPC:

**Terminal Commands:**
- `create_terminal_session` - Start new PTY
- `write_to_terminal` - Send input
- `resize_terminal` - Update size
- `close_terminal` - End session
- `list_terminal_sessions` - Get all active sessions

**Trigger Commands:**
- `create_trigger` - Add new pattern trigger
- `list_triggers` - Get all triggers
- `delete_trigger` - Remove trigger
- `toggle_trigger` - Enable/disable trigger

**Session Sharing:**
- `share_session` - Create shareable link
- `join_shared_session` - Connect to shared session
- `unshare_session` - Stop sharing

**AI Commands:**
- `ask_ai` - Send prompt to AI
- `analyze_output` - AI-powered output analysis

**SSH Commands:**
- `create_ssh_session` - Connect to remote host
- `list_ssh_sessions` - Get SSH connections

### `ssh.rs` - SSH Connection Management
Remote terminal sessions using `ssh2`:

**Key Components:**
- `SshConfig` - Connection parameters
- `SshSessionHandle` - Session wrapper
- `connect()` - Establishes SSH connection
- `open_shell()` - Opens interactive shell
- `execute_command()` - Runs single command

**Authentication:**
- Password-based auth
- Public key authentication
- Passphrase-protected keys

### `ai.rs` - AI Integration
LLM integration for terminal assistance:

**Supported Providers:**
- OpenAI (GPT-3.5/4)
- Anthropic (Claude)
- Custom endpoints

**Features:**
- `ask()` - General AI queries
- `analyze_output()` - Error analysis
- `suggest_command()` - Natural language to commands
- Command extraction from responses
- Context-aware suggestions

### `triggers.rs` - Pattern Matching & Actions
Automated responses to terminal output:

**Trigger Actions:**
- `Notify` - Desktop notifications
- `RunCommand` - Execute commands
- `AiAssist` - Request AI help

**Common Patterns:**
- Error detection
- Warning detection
- Permission denied
- Connection errors
- Syntax errors

**Functions:**
- `process_output()` - Checks output against triggers
- `execute_trigger_action()` - Runs triggered actions
- `create_default_triggers()` - Helpful default triggers

### `errors.rs` - Error Handling
Custom error types using `thiserror`:

**Error Variants:**
- `PtySessionNotFound` - Invalid session ID
- `PtyCreationFailed` - PTY spawn error
- `SshConnectionFailed` - SSH connection error
- `SshAuthFailed` - Authentication error
- `AiServiceError` - AI API errors
- `TriggerNotFound` - Invalid trigger ID

## Data Flow

### Creating a Terminal Session
```
Frontend → create_terminal_session()
         ↓
    commands.rs
         ↓
    pty::create_pty_session()
         ↓
    portable-pty creates PTY + shell process
         ↓
    spawn_pty_reader() starts background task
         ↓
    Session stored in AppState
         ↓
    Returns session_id to frontend
```

### Terminal Input/Output
```
User types → Frontend
           ↓
    write_to_terminal(session_id, data)
           ↓
    pty::write_to_pty()
           ↓
    Data written to PTY master
           ↓
    Shell process receives input

Shell output → PTY reader task
             ↓
    Base64 encode
             ↓
    app.emit("terminal-output:session_id", data)
             ↓
    Frontend receives event
             ↓
    xterm.js displays output
```

### Trigger Processing
```
PTY output → trigger::process_output()
           ↓
    Regex pattern matching
           ↓
    Match found?
           ↓
    execute_trigger_action()
           ↓
    Emit event to frontend OR execute command
```

## Thread Safety

All shared state uses:
- `Arc<Mutex<T>>` for thread-safe shared ownership
- `tokio::sync::Mutex` for async-aware locking
- No unsafe code in core functionality

## Event System

Tauri events emitted from Rust:
- `terminal-output:{session_id}` - PTY output
- `terminal-closed:{session_id}` - Session ended
- `ssh-connected` - SSH connection established
- `trigger-notification` - Trigger fired
- `trigger-command` - Command execution requested
- `trigger-ai-assist` - AI assistance requested

## Building and Testing

### Build
```bash
cd src-tauri
cargo build --release
```

### Run Tests
```bash
cargo test
```

### Run in Development
```bash
cargo tauri dev
```

### Build Production Binary
```bash
cargo tauri build
```

## Dependencies

Core dependencies in `Cargo.toml`:
- `tauri` - Framework for native apps
- `tokio` - Async runtime
- `portable-pty` - Cross-platform PTY
- `ssh2` - SSH client
- `reqwest` - HTTP client for AI APIs
- `serde/serde_json` - Serialization
- `anyhow/thiserror` - Error handling
- `uuid` - Session ID generation
- `regex` - Pattern matching
- `base64` - Data encoding

## Performance Characteristics

- **PTY Output**: Streamed in 8KB chunks
- **Memory**: Minimal per session (~2MB PTY buffers)
- **Concurrency**: Async I/O allows many sessions
- **Latency**: Sub-millisecond IPC between Rust and frontend

## Security Considerations

1. **PTY Isolation**: Each session runs in separate process
2. **No Shell Injection**: Commands validated before execution
3. **SSH Key Protection**: Keys never exposed to frontend
4. **API Keys**: Stored securely, not in frontend code
5. **Session IDs**: Cryptographically random UUIDs

## Future Enhancements

Planned features:
- Session recording and playback
- Container/Docker integration
- Plugin system for custom triggers
- Multi-user collaboration
- End-to-end encryption for shared sessions
- Terminal multiplexing (tabs/splits in Rust)

## Frontend Integration

The frontend communicates with Rust via Tauri's `invoke()`:

```javascript
// Create terminal session
const sessionId = await invoke('create_terminal_session', {
  config: {
    shell: '/bin/bash',
    cols: 80,
    rows: 24
  }
});

// Send input
await invoke('write_to_terminal', {
  sessionId,
  data: 'ls -la\n'
});

// Listen for output
await listen(`terminal-output:${sessionId}`, (event) => {
  const data = atob(event.payload.data); // Base64 decode
  terminal.write(data);
});
```

## Troubleshooting

### PTY Sessions Not Starting
- Check shell path is valid
- Verify working directory exists
- Check process permissions

### SSH Connection Failures
- Verify network connectivity
- Check SSH key permissions (should be 600)
- Validate username and hostname

### AI Integration Issues
- Verify API key is set
- Check network access to AI provider
- Review rate limits and quotas

## Contributing

When adding new features:
1. Add ABOUTME comments to new files
2. Use proper error handling with `Result<T, E>`
3. Write unit tests for new functions
4. Update this documentation
5. Follow existing code patterns
