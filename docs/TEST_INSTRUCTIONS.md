# XNAUT Frontend Testing Instructions

## Quick Start Test (Without Backend)

You can test the frontend UI and interactions even before the Rust backend is complete.

### 1. Open in Browser

Simply open the HTML file in a browser:

```bash
cd /home/jarvis/projects/NautCode/xnaut/src
# Open index.html in your browser
# OR use a simple HTTP server:
python3 -m http.server 8080
# Then visit: http://localhost:8080
```

**Expected Result**: You'll see an error in console: "Tauri API not available" - this is normal without Tauri.

### 2. Test UI Components (No Backend Needed)

These features work without the backend:

✅ **Open/Close Modals**
- Click Settings (⚙️) → Modal opens
- Click × → Modal closes
- Press Escape → Modal closes

✅ **Create Workflows**
- Click Workflows (📓)
- Click "+ New Workflow"
- Fill in name, description, commands
- Click "Save Workflow"
- Should appear in list

✅ **Create SSH Profiles**
- Click SSH (🔐)
- Click "+ New Profile"
- Fill in host, username, password
- Click "Save Profile"
- Should appear in list

✅ **Create Triggers**
- Click Triggers (🔔)
- Click "+ New Trigger"
- Fill in pattern, message
- Click "Save Trigger"
- Should appear in list

✅ **Settings**
- Click Settings (⚙️)
- Enter fake API key
- Click "Save"
- Reload page → Settings persist

✅ **LLM Provider Switching**
- Change provider dropdown
- Model dropdown updates

✅ **Toggle Chat Panel**
- Click Chat (💬)
- Panel slides in/out

### 3. Test with Mock Backend (JavaScript Simulation)

Add this to browser console to simulate Tauri API:

```javascript
// Mock Tauri API
window.__TAURI__ = {
  core: {
    invoke: async (cmd, args) => {
      console.log('MOCK invoke:', cmd, args);

      // Simulate responses
      switch(cmd) {
        case 'create_terminal_session':
          return { session_id: 'mock-session-' + Date.now() };

        case 'write_to_terminal':
          console.log('Mock writing:', args.data);
          return null;

        case 'ai_suggest':
          return 'This is a mock AI response to: ' + args.prompt;

        case 'ai_analyze_error':
          return 'Mock error analysis: This looks like a permission issue.';

        case 'create_ssh_session':
          return { session_id: 'mock-ssh-' + Date.now() };

        case 'create_shared_session':
          return { share_code: 'ABC123' };

        default:
          return { status: 'ok' };
      }
    }
  },
  event: {
    listen: async (event, callback) => {
      console.log('MOCK listen:', event);

      // Simulate terminal output
      if (event.includes('terminal-output')) {
        setTimeout(() => {
          callback({ payload: { data: 'Mock terminal output\r\n' } });
        }, 1000);
      }

      return () => console.log('Unlisten:', event);
    }
  }
};

// Reload page to apply mock
location.reload();
```

Now try:
1. Click "New Tab" → Should create terminal with mock output
2. Type in terminal → Should log to console
3. Click AI Chat → Type message → Should get mock response

## Full Test with Tauri (Requires Backend)

### 1. Build Tauri App

```bash
cd /home/jarvis/projects/NautCode/xnaut
npm install
npm run tauri dev
```

### 2. Test Terminal Creation

1. App should open with XNAUT banner
2. Terminal should show ASCII art
3. Should connect to actual shell
4. Type `ls` → Should see real output

### 3. Test AI Features

**Prerequisites**: Set API key in Settings

1. Click Settings → Enter Anthropic API key → Save
2. Click Chat panel (💬)
3. Type: "How do I list files?"
4. Should get real AI response

### 4. Test SSH

**Prerequisites**: Have SSH server to connect to

1. Click SSH (🔐)
2. Click "+ New Profile"
3. Fill in real SSH details
4. Click "Save Profile"
5. Click "Connect"
6. Should open new tab with SSH session

### 5. Test Workflows

1. Click Workflows (📓)
2. Click "+ New Workflow"
3. Enter name: "Test Workflow"
4. Enter commands:
   ```
   echo "Step 1"
   echo "Step 2"
   echo "Step 3"
   ```
5. Save
6. Click "▶ Run"
7. Should execute all commands in terminal

### 6. Test Triggers

1. Click Triggers (🔔)
2. Click "+ New Trigger"
3. Name: "Error Detection"
4. Type: "keyword"
5. Pattern: "error,failed"
6. Message: "Error detected!"
7. Save
8. In terminal, type: `echo "error occurred"`
9. Should see desktop notification

### 7. Test Command History

1. Run some commands:
   ```
   ls -la
   pwd
   whoami
   date
   ```
2. Press Ctrl+R
3. Type "ls"
4. Should filter to show only `ls -la`
5. Click on it → Should execute again

### 8. Test Session Sharing

1. Click Share (🔗)
2. Should show share code (e.g., "ABC123")
3. Click "Copy to Clipboard"
4. Share code should be in clipboard
5. On another instance, use code to join

### 9. Test Multiple Tabs

1. Click "+ New Tab"
2. Should create Terminal 2
3. Switch between tabs
4. Each should maintain its own session
5. Close tab with ×
6. Should cleanly destroy session

## Automated Test Script

Create this file: `test-frontend.js`

```javascript
// XNAUT Frontend Automated Test
console.log('🧪 Starting XNAUT Frontend Tests...\n');

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    console.log(`✅ ${name}`);
    passed++;
  } catch (e) {
    console.error(`❌ ${name}: ${e.message}`);
    failed++;
  }
}

// Test 1: Tauri API availability
test('Tauri API available', () => {
  if (!window.__TAURI__) throw new Error('Tauri API not found');
});

// Test 2: XNAUT global available
test('XNAUT global available', () => {
  if (!window.xnaut) throw new Error('xnaut not found');
});

// Test 3: Initial tab created
test('Initial tab created', () => {
  if (window.xnaut.tabs.length === 0) throw new Error('No tabs');
});

// Test 4: Settings loaded
test('Settings loaded', () => {
  if (!window.xnaut.settings) throw new Error('Settings not loaded');
});

// Test 5: LocalStorage working
test('LocalStorage working', () => {
  localStorage.setItem('test', 'value');
  if (localStorage.getItem('test') !== 'value') throw new Error('LocalStorage broken');
  localStorage.removeItem('test');
});

// Test 6: Modal functions exist
test('Modal functions exist', () => {
  if (typeof showModal !== 'function') throw new Error('showModal not found');
  if (typeof closeModal !== 'function') throw new Error('closeModal not found');
});

// Test 7: Terminal creation function
test('createNewTab function exists', () => {
  if (typeof createNewTab !== 'function') throw new Error('createNewTab not found');
});

// Test 8: Chat functions
test('Chat functions exist', () => {
  if (typeof sendChatMessage !== 'function') throw new Error('sendChatMessage not found');
  if (typeof addChatMessage !== 'function') throw new Error('addChatMessage not found');
});

// Test 9: SSH functions
test('SSH functions exist', () => {
  if (typeof showSSHModal !== 'function') throw new Error('showSSHModal not found');
  if (typeof connectSSH !== 'function') throw new Error('connectSSH not found');
});

// Test 10: Workflow functions
test('Workflow functions exist', () => {
  if (typeof showWorkflowsModal !== 'function') throw new Error('showWorkflowsModal not found');
  if (typeof executeWorkflow !== 'function') throw new Error('executeWorkflow not found');
});

console.log(`\n📊 Results: ${passed} passed, ${failed} failed`);

if (failed === 0) {
  console.log('✨ All tests passed!');
} else {
  console.log('⚠️ Some tests failed');
}
```

Run in browser console after app loads.

## Performance Testing

### 1. Terminal Output Performance

```javascript
// Flood terminal with output
const terminal = window.xnaut.getActiveTerminal();
if (terminal) {
  for (let i = 0; i < 1000; i++) {
    terminal.term.writeln(`Line ${i}: Lorem ipsum dolor sit amet`);
  }
  console.log('✅ Terminal handled 1000 lines');
}
```

### 2. Memory Usage

```javascript
// Check before
console.log('Memory before:', performance.memory?.usedJSHeapSize);

// Create 10 tabs
for (let i = 0; i < 10; i++) {
  createNewTab();
}

// Check after
console.log('Memory after:', performance.memory?.usedJSHeapSize);
```

### 3. LocalStorage Size

```javascript
// Check storage usage
let total = 0;
for (let key in localStorage) {
  if (localStorage.hasOwnProperty(key)) {
    total += localStorage[key].length;
  }
}
console.log('LocalStorage used:', (total / 1024).toFixed(2), 'KB');
```

## Common Issues & Solutions

### Issue 1: Tauri API not found
**Solution**: Make sure you're running in Tauri (not regular browser)

### Issue 2: Terminal not rendering
**Solution**: Check xterm.js loaded from CDN

### Issue 3: Settings not persisting
**Solution**: Check browser allows localStorage

### Issue 4: Modals not closing
**Solution**: Press Escape or click × button

### Issue 5: Commands not executing
**Solution**: Make sure Rust backend commands are implemented

## Browser Console Debug Commands

```javascript
// Get current state
window.xnaut.tabs              // All tabs
window.xnaut.settings          // Current settings
window.xnaut.workflows         // All workflows
window.xnaut.sshProfiles       // All SSH profiles
window.xnaut.triggers          // All triggers

// Get active terminal
window.xnaut.getActiveTerminal()

// Test Tauri invoke
await window.xnaut.invoke('create_terminal_session')

// Clear all LocalStorage
localStorage.clear()
location.reload()

// Enable verbose logging
window.xnaut.debug = true
```

## Integration Test Checklist

- [ ] Create terminal session
- [ ] Send input to terminal
- [ ] Receive output from terminal
- [ ] Create multiple tabs
- [ ] Close tab
- [ ] Create SSH session
- [ ] Send SSH command
- [ ] AI suggest command
- [ ] AI analyze error
- [ ] Create shared session
- [ ] Join shared session
- [ ] Add trigger
- [ ] Trigger fires on pattern match
- [ ] Save/load settings
- [ ] Save/load workflows
- [ ] Save/load SSH profiles
- [ ] Save/load triggers
- [ ] Command history persists
- [ ] All modals open/close
- [ ] All keyboard shortcuts work
- [ ] Desktop notifications work

---

**Ready to test?** Start with the UI tests, then add the mock backend, then test with real Tauri.
