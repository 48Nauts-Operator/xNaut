# XNAUT Frontend-Backend Integration Guide

## Overview

This guide explains how the frontend JavaScript communicates with the Rust/Tauri backend.

## Communication Pattern

```
Frontend (JS) ──invoke()──> Tauri Bridge ──> Rust Backend
Frontend (JS) <──listen()─── Tauri Bridge <── Rust Backend (events)
```

## Complete API Contract

### Terminal Commands

#### 1. Create Terminal Session
```javascript
// Frontend
const result = await invoke('create_terminal_session');
// Returns: { session_id: "session-xyz" }

// Rust Signature
#[tauri::command]
async fn create_terminal_session() -> Result<SessionResponse, String>
```

#### 2. Write to Terminal
```javascript
// Frontend
await invoke('write_to_terminal', {
  sessionId: 'session-xyz',
  data: 'ls -la\n'
});

// Rust Signature
#[tauri::command]
async fn write_to_terminal(
  session_id: String,
  data: String
) -> Result<(), String>
```

#### 3. Resize Terminal
```javascript
// Frontend
await invoke('resize_terminal', {
  sessionId: 'session-xyz',
  cols: 80,
  rows: 24
});

// Rust Signature
#[tauri::command]
async fn resize_terminal(
  session_id: String,
  cols: u16,
  rows: u16
) -> Result<(), String>
```

#### 4. Close Terminal
```javascript
// Frontend
await invoke('close_terminal', {
  sessionId: 'session-xyz'
});

// Rust Signature
#[tauri::command]
async fn close_terminal(session_id: String) -> Result<(), String>
```

### SSH Commands

#### 1. Create SSH Session
```javascript
// Frontend
const result = await invoke('create_ssh_session', {
  config: {
    host: 'example.com',
    port: 22,
    username: 'ubuntu',
    password: 'secret123',      // Optional
    privateKey: '-----BEGIN...' // Optional (alternative to password)
  }
});
// Returns: { session_id: "ssh-session-xyz" }

// Rust Signature
#[derive(Serialize, Deserialize)]
struct SSHConfig {
  host: String,
  port: u16,
  username: String,
  password: Option<String>,
  private_key: Option<String>,
}

#[tauri::command]
async fn create_ssh_session(
  config: SSHConfig
) -> Result<SessionResponse, String>
```

#### 2. Execute SSH Command
```javascript
// Frontend
await invoke('ssh_execute', {
  sessionId: 'ssh-session-xyz',
  command: 'docker ps'
});

// Rust Signature
#[tauri::command]
async fn ssh_execute(
  session_id: String,
  command: String
) -> Result<(), String>
```

#### 3. Close SSH Session
```javascript
// Frontend
await invoke('close_ssh_session', {
  sessionId: 'ssh-session-xyz'
});

// Rust Signature
#[tauri::command]
async fn close_ssh_session(session_id: String) -> Result<(), String>
```

### AI Commands

#### 1. AI Suggest
```javascript
// Frontend
const response = await invoke('ai_suggest', {
  prompt: 'How do I list docker containers?',
  context: 'User is trying to work with Docker...'
});
// Returns: "To list Docker containers, use: docker ps..."

// Rust Signature
#[tauri::command]
async fn ai_suggest(
  prompt: String,
  context: String
) -> Result<String, String>
```

#### 2. AI Analyze Error
```javascript
// Frontend
const analysis = await invoke('ai_analyze_error', {
  errorText: 'Permission denied...',
  context: 'User ran: sudo systemctl restart nginx'
});
// Returns: "This error indicates..."

// Rust Signature
#[tauri::command]
async fn ai_analyze_error(
  error_text: String,
  context: String
) -> Result<String, String>
```

### Session Sharing Commands

#### 1. Create Shared Session
```javascript
// Frontend
const result = await invoke('create_shared_session', {
  sessionId: 'session-xyz'
});
// Returns: { share_code: "ABC123" }

// Rust Signature
#[tauri::command]
async fn create_shared_session(
  session_id: String
) -> Result<ShareCodeResponse, String>
```

#### 2. Join Shared Session
```javascript
// Frontend
const result = await invoke('join_shared_session', {
  shareCode: 'ABC123'
});
// Returns: { session_id: "session-xyz", status: "connected" }

// Rust Signature
#[tauri::command]
async fn join_shared_session(
  share_code: String
) -> Result<JoinSessionResponse, String>
```

### Trigger Commands

#### 1. Add Trigger
```javascript
// Frontend
await invoke('add_trigger', {
  pattern: 'error|failed|exception',
  patternType: 'regex',  // or 'keyword'
  action: 'notify',
  message: 'Error detected!'
});

// Rust Signature
#[tauri::command]
async fn add_trigger(
  pattern: String,
  pattern_type: String,
  action: String,
  message: String
) -> Result<TriggerId, String>
```

#### 2. List Triggers
```javascript
// Frontend
const triggers = await invoke('list_triggers');
// Returns: [{ id: 1, pattern: "...", type: "regex", ... }, ...]

// Rust Signature
#[tauri::command]
async fn list_triggers() -> Result<Vec<Trigger>, String>
```

#### 3. Remove Trigger
```javascript
// Frontend
await invoke('remove_trigger', { id: 1 });

// Rust Signature
#[tauri::command]
async fn remove_trigger(id: u32) -> Result<(), String>
```

## Events from Backend

### Terminal Output Event
```rust
// Rust Backend (emit event)
app_handle.emit_all(
  &format!("terminal-output-{}", session_id),
  OutputPayload { data: output_string }
)?;
```

```javascript
// Frontend (listen)
await listen(`terminal-output-${sessionId}`, (event) => {
  const data = event.payload.data;
  term.write(data);
});
```

### SSH Output Event
```rust
// Rust Backend
app_handle.emit_all(
  &format!("ssh-output-{}", session_id),
  OutputPayload { data: output_string }
)?;
```

```javascript
// Frontend
await listen(`ssh-output-${sessionId}`, (event) => {
  term.write(event.payload.data);
});
```

### Trigger Matched Event
```rust
// Rust Backend
app_handle.emit_all(
  "trigger-matched",
  TriggerPayload {
    trigger_id: 1,
    matched_text: "error found",
    message: "Error detected!"
  }
)?;
```

```javascript
// Frontend
await listen('trigger-matched', (event) => {
  showNotification('Trigger', event.payload.message);
});
```

## Rust Data Structures

```rust
// Response types
#[derive(Serialize)]
struct SessionResponse {
  session_id: String,
}

#[derive(Serialize)]
struct ShareCodeResponse {
  share_code: String,
}

#[derive(Serialize)]
struct JoinSessionResponse {
  session_id: String,
  status: String,
}

// Event payload types
#[derive(Serialize, Clone)]
struct OutputPayload {
  data: String,
}

#[derive(Serialize)]
struct TriggerPayload {
  trigger_id: u32,
  matched_text: String,
  message: String,
}

// Config types
#[derive(Deserialize)]
struct SSHConfig {
  host: String,
  port: u16,
  username: String,
  password: Option<String>,
  private_key: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Trigger {
  id: u32,
  pattern: String,
  pattern_type: String,
  action: String,
  message: String,
  enabled: bool,
}
```

## Tauri Configuration

Ensure all commands are registered in `tauri.conf.json`:

```json
{
  "tauri": {
    "allowlist": {
      "all": false,
      "shell": {
        "all": false,
        "execute": false,
        "sidecar": false,
        "open": false
      }
    }
  },
  "build": {
    "beforeDevCommand": "",
    "beforeBuildCommand": "",
    "devPath": "../src",
    "distDir": "../src"
  }
}
```

And in `main.rs`:

```rust
fn main() {
  tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![
      create_terminal_session,
      write_to_terminal,
      resize_terminal,
      close_terminal,
      create_ssh_session,
      ssh_execute,
      close_ssh_session,
      ai_suggest,
      ai_analyze_error,
      create_shared_session,
      join_shared_session,
      add_trigger,
      list_triggers,
      remove_trigger,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
```

## Error Handling

### Backend Errors
```rust
#[tauri::command]
async fn create_terminal_session() -> Result<SessionResponse, String> {
  match pty::create_session() {
    Ok(session) => Ok(SessionResponse { session_id: session.id }),
    Err(e) => Err(format!("Failed to create session: {}", e))
  }
}
```

### Frontend Handling
```javascript
try {
  const result = await invoke('create_terminal_session');
  console.log('Session created:', result.session_id);
} catch (error) {
  console.error('Error:', error);
  term.writeln(`\x1b[1;31mError: ${error}\x1b[0m`);
}
```

## Testing Integration

### 1. Manual Test
```javascript
// In browser console
await window.xnaut.invoke('create_terminal_session')
  .then(r => console.log('✅ Success:', r))
  .catch(e => console.error('❌ Error:', e));
```

### 2. Check Tauri API
```javascript
// Verify Tauri is loaded
if (window.__TAURI__) {
  console.log('✅ Tauri API available');
} else {
  console.error('❌ Tauri API missing');
}
```

### 3. Test Events
```javascript
// Listen for test event
await listen('test-event', (event) => {
  console.log('Received:', event.payload);
});
```

## Common Issues

### 1. Command Not Found
```
Error: Command create_terminal_session not found
```
**Solution**: Ensure command is registered in `invoke_handler![]` in `main.rs`

### 2. Event Not Received
```
Event listener not firing
```
**Solution**: Check event name matches exactly (case-sensitive)

### 3. Type Mismatch
```
Error: Failed to deserialize
```
**Solution**: Ensure JS object structure matches Rust struct

### 4. Permission Denied
```
Error: Not allowed to invoke command
```
**Solution**: Check Tauri allowlist in `tauri.conf.json`

## Best Practices

1. **Always use try/catch** around `invoke()` calls
2. **Validate input** before sending to backend
3. **Clean up listeners** when components unmount
4. **Use TypeScript** for better type safety (optional)
5. **Log errors** to help with debugging
6. **Show user feedback** for async operations
7. **Handle disconnections** gracefully

## Example: Complete Terminal Creation Flow

```javascript
// Frontend: app.js
async function createTerminal(tabId) {
  const term = new Terminal({ /* config */ });
  term.open(terminalDiv);

  try {
    // 1. Create session via Rust
    const result = await invoke('create_terminal_session');
    const sessionId = result.session_id;

    // 2. Listen for output from Rust
    await listen(`terminal-output-${sessionId}`, (event) => {
      term.write(event.payload.data);
    });

    // 3. Send input to Rust
    term.onData(async (data) => {
      await invoke('write_to_terminal', { sessionId, data });
    });

    // 4. Handle resize
    term.onResize(async ({ cols, rows }) => {
      await invoke('resize_terminal', { sessionId, cols, rows });
    });

    return { sessionId, term };
  } catch (error) {
    console.error('Failed to create terminal:', error);
    term.writeln(`\x1b[1;31mError: ${error}\x1b[0m`);
    return null;
  }
}
```

```rust
// Backend: main.rs
#[tauri::command]
async fn create_terminal_session(
  app_handle: tauri::AppHandle,
  state: tauri::State<'_, AppState>,
) -> Result<SessionResponse, String> {
  let session_id = generate_session_id();

  // Create PTY
  let pty = create_pty()?;

  // Store session
  state.sessions.lock().unwrap().insert(session_id.clone(), pty);

  // Start output thread
  spawn_output_thread(app_handle, session_id.clone(), pty);

  Ok(SessionResponse { session_id })
}

fn spawn_output_thread(
  app_handle: tauri::AppHandle,
  session_id: String,
  pty: PTY,
) {
  tokio::spawn(async move {
    loop {
      match pty.read_output().await {
        Ok(data) => {
          app_handle.emit_all(
            &format!("terminal-output-{}", session_id),
            OutputPayload { data }
          ).ok();
        }
        Err(_) => break,
      }
    }
  });
}
```

## Next Steps

1. Implement all Rust backend commands
2. Test each command individually
3. Test event flow (backend → frontend)
4. Add error handling and logging
5. Optimize performance (debouncing, buffering)
6. Add integration tests

---

**Last Updated**: 2025-10-06
