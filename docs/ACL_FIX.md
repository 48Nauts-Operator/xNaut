# 🔒 ACL Permission Fix - CRITICAL

## ❌ The Error You Saw

```
Failed to create terminal session: "Command plugin:event|listen not allowed by ACL"
```

## ✅ What I Fixed

**Root Cause:** Tauri 2.0 has a security feature called ACL (Access Control List) that blocks all commands by default. Your app couldn't listen to events or call custom commands.

**The Fix:** Created proper permissions configuration

### Files Created/Modified:

1. **`src-tauri/capabilities/default.json`** (NEW)
   - Grants permission for event listening (`core:event:allow-listen`)
   - Grants permission for event emitting (`core:event:allow-emit`)
   - Grants permission for all 17 custom commands

2. **`src-tauri/tauri.conf.json`** (MODIFIED)
   - Added `"label": "main"` to window configuration
   - Added `"capabilities": ["default"]` to security section

## 🔑 What This Enables

Now your app can:
- ✅ Listen to terminal output events (`listen()`)
- ✅ Emit events from backend to frontend
- ✅ Call all custom commands:
  - `create_terminal_session`
  - `write_to_terminal`
  - `resize_terminal`
  - `close_terminal`
  - `list_terminal_sessions`
  - `create_trigger`
  - `list_triggers`
  - `delete_trigger`
  - `toggle_trigger`
  - `share_session`
  - `join_shared_session`
  - `unshare_session`
  - `ask_ai`
  - `analyze_output`
  - `create_ssh_session`
  - `list_ssh_sessions`
  - `get_ssh_config_hosts`

## 🚀 Next Step

**Rebuild the app on your Mac:**

```bash
cd ~/Downloads/xnaut/src-tauri
cargo tauri build
open target/release/bundle/macos/xNAUT.app
```

This should now fix the black terminal issue! The ACL was blocking event listeners, which is why you weren't seeing any terminal output.

## 🧪 Test It

After rebuilding:

1. Open the app
2. Press `Cmd+Option+I` to open DevTools
3. Create a terminal tab (➕ button)
4. You should now see:
   - Console logs for terminal session creation
   - Terminal output events being received
   - **A working shell prompt!** 🎉

The terminal should now work because `listen()` is no longer blocked!

## 📚 Understanding Tauri 2.0 ACL

Tauri 2.0 is security-first. By default:
- ❌ No commands allowed
- ❌ No events allowed
- ❌ No file system access
- ❌ No network access

You must explicitly grant permissions in `capabilities/*.json` files.

This is a **good thing** - it means your app is secure by default and only has the permissions it needs.

---

**This was the missing piece!** Rebuild and test - the terminal should now work properly.
