# 🧪 xNAUT Testing Checklist

## Quick Test (5 minutes)

### 1. ✅ Rebuild with Enhanced Logging
```bash
cd ~/Downloads/xnaut/src-tauri
cargo tauri build
```

### 2. ✅ Open the App
```bash
open target/release/bundle/macos/xNAUT.app
```

### 3. ✅ Open DevTools
Press: `Cmd+Option+I` (⌘⌥I)

OR click the 🐛 Debug button in the top-right

### 4. ✅ Create Terminal Tab
Click the ➕ button in the tab bar

### 5. ✅ Watch Console Output

**You should see:**
```
🔄 Attempting to create terminal session...
📦 Terminal session result: {session_id: "xyz123"}
✅ Terminal session created: xyz123
📡 Setting up listener for: terminal-output-xyz123
⌨️ Setting up input handler...
🧪 Sending test echo command...
✅ Test command sent
```

**Critical Question:**
Do you see `📥 Received terminal output:` logs?

- **YES** → PTY is working, frontend receiving data ✅
- **NO** → PTY not emitting events ❌

### 6. ✅ Check Terminal Window

**Do you see colored messages?**
- 🔄 Yellow: "Connecting to backend..."
- ✅ Green: "Connected! Session: xyz123"
- ⌨️ Cyan: "Terminal ready for input"

- **YES** → Frontend is working, PTY might be issue ✅
- **NO** → Terminal creation failed completely ❌

## 📸 What to Screenshot

1. **DevTools Console** - showing all the logs above
2. **Terminal Window** - even if it's black
3. **Debug Button Output** - click 🐛 and screenshot the alert

## 📋 Report Template

Copy/paste this and fill it in:

```
## Test Results

**Build Status:**
- [ ] Build succeeded with no errors
- [ ] Build had warnings (list them)
- [ ] Build failed (paste error)

**Console Logs:**
(paste full console output here)

**Visual Status:**
- [ ] Saw yellow "Connecting..." message
- [ ] Saw green "Connected" message
- [ ] Saw cyan "Terminal ready" message
- [ ] Terminal stayed completely black
- [ ] Other (describe):

**Event Reception:**
- [ ] YES - I see "📥 Received terminal output" logs
- [ ] NO - Never saw output reception logs

**Screenshots:**
(attach console + terminal screenshots)

**Additional Notes:**
(any other observations)
```

## 🎯 Next Steps Based on Results

### If you see "Received terminal output" logs:
→ PTY is working! Issue is with rendering or xterm.js

### If you DON'T see "Received terminal output" logs:
→ PTY not emitting events. Check Rust backend.

### If you see colored messages in terminal:
→ Frontend working! Backend might not be spawning shell.

### If terminal is completely black (no colored messages):
→ Terminal creation failing earlier. Check Tauri invoke errors.

---

**Just fill out the report template above and we can pinpoint the exact issue!**
