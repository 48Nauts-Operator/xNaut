# XNAUT Frontend - Quick Reference Card

## File Locations

```
/home/jarvis/projects/NautCode/xnaut/src/
├── index.html              # Main UI (15 KB)
├── css/styles.css          # Styling (13 KB)
└── js/app.js               # Logic (40 KB)
```

## Key Functions (app.js)

### Terminal Management
```javascript
createNewTab()                    // Create new terminal tab
createTerminal(tabId)             // Create terminal instance
closeTab(tabId)                   // Close tab and cleanup
switchTab(tabId)                  // Switch active tab
```

### AI Features
```javascript
sendChatMessage()                 // Send message to AI
analyzeTerminalOutput()           // AI analyze recent output
addChatMessage(role, content)     // Add message to chat
```

### SSH Management
```javascript
showSSHModal()                    // Show SSH profiles
connectSSH(profileId)             // Connect to SSH server
disconnectSSH(profileId)          // Disconnect SSH
saveSSHProfile()                  // Save SSH profile
```

### Workflows
```javascript
showWorkflowsModal()              // Show workflows
executeWorkflow(workflowId)       // Run workflow
saveWorkflow()                    // Save workflow
toggleWorkflowRecording()         // Start/stop recording
```

### Triggers
```javascript
showTriggersModal()               // Show triggers
saveTrigger()                     // Save trigger
checkTriggers(output)             // Check output for matches
```

### Settings
```javascript
loadSettings()                    // Load from localStorage
saveSettings()                    // Save to localStorage
updateLLMModels()                 // Update model dropdown
```

## Tauri Commands

### Terminal
```javascript
invoke('create_terminal_session')
invoke('write_to_terminal', { sessionId, data })
invoke('resize_terminal', { sessionId, cols, rows })
invoke('close_terminal', { sessionId })
```

### SSH
```javascript
invoke('create_ssh_session', { config })
invoke('ssh_execute', { sessionId, command })
invoke('close_ssh_session', { sessionId })
```

### AI
```javascript
invoke('ai_suggest', { prompt, context })
invoke('ai_analyze_error', { errorText, context })
```

### Session Sharing
```javascript
invoke('create_shared_session', { sessionId })
invoke('join_shared_session', { shareCode })
```

### Triggers
```javascript
invoke('add_trigger', { pattern, patternType, action, message })
invoke('list_triggers')
invoke('remove_trigger', { id })
```

## Events to Listen

```javascript
listen(`terminal-output-${sessionId}`, callback)
listen(`ssh-output-${sessionId}`, callback)
listen('trigger-matched', callback)
```

## Modal IDs

```javascript
'settings-modal'           // Settings
'workflows-modal'          // Workflow list
'workflow-edit-modal'      // Create/edit workflow
'ssh-modal'                // SSH profiles list
'ssh-profile-modal'        // Create/edit SSH profile
'triggers-modal'           // Triggers list
'trigger-edit-modal'       // Create/edit trigger
'share-modal'              // Share session
'history-modal'            // Command history (Ctrl+R)
```

## LocalStorage Keys

```javascript
'xnaut-settings'           // User settings and API keys
'xnaut-history'            // Command history
'xnaut-workflows'          // Saved workflows
'xnaut-ssh-profiles'       // SSH connection profiles
'xnaut-triggers'           // Notification triggers
```

## Global Access (Debug)

```javascript
window.xnaut.invoke               // Direct Tauri invoke
window.xnaut.tabs                 // All tabs
window.xnaut.settings             // Current settings
window.xnaut.workflows            // All workflows
window.xnaut.sshProfiles          // All SSH profiles
window.xnaut.triggers             // All triggers
window.xnaut.getActiveTerminal()  // Get active terminal
```

## Keyboard Shortcuts

```
Ctrl+T        New terminal tab
Ctrl+W        Close current tab
Ctrl+R        Command history search
Escape        Close any modal
Enter         Send chat message (in chat input)
Shift+Enter   New line (in chat input)
```

## CSS Classes (Main)

```css
.top-bar              // Top navigation bar
.tab-bar              // Terminal tab bar
.terminal-area        // Main terminal container
.terminal-pane        // Individual terminal
.side-panel           // Chat/sidebar panel
.modal                // Modal overlay
.btn                  // Button
.btn-primary          // Primary button (blue)
.form-group           // Form field wrapper
```

## Color Variables

```css
--bg-primary          #0a0a0f
--bg-secondary        #1a1a1f
--bg-tertiary         #2a2a2f
--text-primary        #e0e0e0
--text-secondary      #a0a0a0
--accent              #3b82f6
--success             #10b981
--danger              #ef4444
--warning             #f59e0b
```

## Common Patterns

### Show Modal
```javascript
showModal('modal-id');
```

### Close Modal
```javascript
closeModal('modal-id');
// or press Escape
```

### Execute Terminal Command
```javascript
const terminal = window.xnaut.getActiveTerminal();
await invoke('write_to_terminal', {
  sessionId: terminal.sessionId,
  data: 'ls -la\n'
});
```

### Add Chat Message
```javascript
addChatMessage('user', 'Hello!');
addChatMessage('assistant', 'Hi there!');
```

### Show Notification
```javascript
showNotification('Title', 'Message body');
```

### Copy to Clipboard
```javascript
navigator.clipboard.writeText('text to copy');
```

## Error Handling Pattern

```javascript
try {
  const result = await invoke('command_name', { args });
  // Success handling
} catch (error) {
  console.error('Error:', error);
  // User-friendly message
  updateStatus(`Error: ${error.message}`);
}
```

## Data Structure Examples

### Tab Object
```javascript
{
  id: 'tab-123',
  name: 'Terminal 1',
  terminals: [{ sessionId, term, pane, handleResize }],
  isSSH: false,           // Optional
  sshSessionId: 'ssh-1'   // Optional
}
```

### Workflow Object
```javascript
{
  id: 'workflow-123',
  name: 'Deploy',
  description: 'Deploy to production',
  commands: ['git pull', 'npm install', 'npm run build']
}
```

### SSH Profile Object
```javascript
{
  id: 'ssh-123',
  name: 'Production Server',
  host: 'example.com',
  port: 22,
  username: 'ubuntu',
  authMethod: 'password',
  password: 'secret',
  privateKey: '-----BEGIN RSA PRIVATE KEY-----...'
}
```

### Trigger Object
```javascript
{
  id: 'trigger-123',
  name: 'Error Alert',
  type: 'regex',          // or 'keyword'
  pattern: 'error|failed',
  message: 'Error detected!',
  enabled: true
}
```

### Command History Object
```javascript
{
  command: 'ls -la',
  timestamp: '2025-10-06T14:32:15.000Z',
  sessionId: 'session-123'
}
```

## Testing Commands

### Mock Tauri API
```javascript
// Paste in console before app loads
window.__TAURI__ = {
  core: { invoke: async (cmd, args) => { /* mock */ } },
  event: { listen: async (event, callback) => { /* mock */ } }
};
```

### Create Test Terminal
```javascript
createNewTab();
```

### Send Test Output
```javascript
const term = window.xnaut.getActiveTerminal();
term.term.writeln('Test output');
```

### Test AI (requires API key)
```javascript
await invoke('ai_suggest', {
  prompt: 'How do I list files?',
  context: 'User is learning terminal'
});
```

## Common Debugging

### Check if Tauri loaded
```javascript
console.log('Tauri:', !!window.__TAURI__);
```

### Check active terminal
```javascript
const t = window.xnaut.getActiveTerminal();
console.log('Active terminal:', t?.sessionId);
```

### Check settings
```javascript
console.log('Settings:', window.xnaut.settings);
```

### Check LocalStorage
```javascript
Object.keys(localStorage)
  .filter(k => k.startsWith('xnaut'))
  .forEach(k => console.log(k, localStorage[k].length));
```

### Clear all data
```javascript
localStorage.clear();
location.reload();
```

## Performance Tips

1. Terminal buffer: Limited to 5000 chars
2. Command history: Capped at 1000 items
3. Scrollback: 10,000 lines
4. Event listeners: Cleaned up on tab close
5. Modal overlays: Display none when hidden

## Browser Support

- **Tauri WebView**: Chromium-based
- **JavaScript**: ES6+ (async/await, arrow functions)
- **CSS**: Modern (Grid, Flexbox, CSS Variables)
- **APIs**: Clipboard, Notifications

---

**Quick Links:**
- Full Docs: FRONTEND_README.md
- Integration: INTEGRATION_GUIDE.md
- Testing: TEST_INSTRUCTIONS.md
- UI Design: UI_OVERVIEW.md
