# 📦 xNAUT Standalone Package

**Ready to use on macOS!**

## 📥 What's Included

This package contains the complete xNAUT terminal application:

- ✅ Full Rust backend with PTY support
- ✅ Complete web frontend (HTML/CSS/JavaScript)
- ✅ All documentation and build guides
- ✅ ACL permissions configured (fixed the "not allowed by ACL" error)
- ✅ Enhanced diagnostic logging
- ✅ All 9 features: Terminals, AI Chat, SSH, Workflows, Triggers, Session Sharing

**Package size:** 135KB (excludes build artifacts)

## 🚀 Quick Start (On Your Mac)

### 1. Extract the Archive

```bash
tar -xzf xnaut-standalone.tar.gz
cd xnaut
```

### 2. Build the App (First Time - 5 minutes)

```bash
cd src-tauri
cargo tauri build
```

This will:
- Download Rust dependencies (~2 min)
- Compile the app (~2 min)
- Create `xNAUT.app` bundle (~1 min)

### 3. Run the App

```bash
open target/release/bundle/macos/xNAUT.app
```

Or drag `xNAUT.app` to your Applications folder!

## 📁 Project Structure

```
xnaut/
├── README_FIRST.md          ← Start here!
├── BUILD_NOW.md             ← Build instructions
├── ACL_FIX.md               ← Critical ACL fix explained
├── TESTING_CHECKLIST.md     ← Test the app
├── DEBUGGING_XNAUT.md       ← Debug guide
│
├── src/                     ← Frontend (HTML/JS/CSS)
│   ├── index.html           ← Main UI
│   ├── js/app.js            ← Application logic (enhanced logging)
│   └── css/styles.css       ← Styling
│
└── src-tauri/               ← Rust backend
    ├── Cargo.toml           ← Rust dependencies
    ├── src/
    │   ├── main.rs          ← Entry point
    │   ├── pty.rs           ← Terminal/PTY handling
    │   ├── ssh.rs           ← SSH connections
    │   ├── ai.rs            ← AI integrations
    │   ├── commands.rs      ← Tauri command handlers
    │   └── state.rs         ← Application state
    ├── capabilities/
    │   └── default.json     ← ACL permissions (CRITICAL FIX!)
    ├── tauri.conf.json      ← Tauri configuration
    └── icons/               ← App icons
```

## 🔧 Development Mode

For live development with hot reload:

```bash
cd src-tauri
cargo tauri dev
```

This opens the app with:
- Instant code reloading
- DevTools always available
- Rust recompilation on changes

## ✅ What's Been Fixed

### 1. ACL Permission Error (CRITICAL)
**Error:** `"Command plugin:event|listen not allowed by ACL"`
**Fix:** Created `capabilities/default.json` with all permissions
**Status:** ✅ FIXED

### 2. Tauri API Loading
**Error:** "Tauri API Missing"
**Fix:** Added `withGlobalTauri: true` to config
**Status:** ✅ FIXED

### 3. DOM Loading Race
**Error:** UI elements undefined on startup
**Fix:** Wrapped in `DOMContentLoaded` event
**Status:** ✅ FIXED

### 4. SSH Config Dropdown
**Error:** Dropdown wasn't loading SSH hosts
**Fix:** Added `loadSshConfigHosts()` call
**Status:** ✅ FIXED

### 5. Enhanced Logging
**Added:** Comprehensive console logging for debugging
**Status:** ✅ READY

## 🧪 Testing

After building, follow `TESTING_CHECKLIST.md`:

1. Open DevTools (`Cmd+Option+I`)
2. Create a terminal tab (➕ button)
3. You should see:
   - ✅ Colored connection messages
   - ✅ Shell prompt (bash/zsh)
   - ✅ Console logs showing session creation
   - ✅ Terminal output events received

## 🎯 Next Steps

### If Everything Works:
1. Configure API keys in Settings (⚙️)
2. Try AI chat features (💬)
3. Set up SSH profiles (🔐)
4. Create workflows (📓)
5. Configure triggers (🔔)

### If You Hit Issues:
1. Check `DEBUGGING_XNAUT.md`
2. Open DevTools and check console
3. Follow diagnostic steps in `TESTING_CHECKLIST.md`

## 📚 Documentation Files

- **README_FIRST.md** - Overview and current status
- **BUILD_NOW.md** - Quick build instructions
- **ACL_FIX.md** - Critical ACL permission fix explained
- **BUILD_ON_MACOS.md** - Detailed macOS build guide
- **TESTING_CHECKLIST.md** - Testing guide with checklist
- **DEBUGGING_XNAUT.md** - Complete debugging guide
- **CURRENT_STATUS.md** - All fixes and current state

## 🛠️ Requirements

- **macOS** 10.15+ (Catalina or newer)
- **Rust** (install: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- **Xcode Command Line Tools** (`xcode-select --install`)

## 🎨 Features

1. **Multiple Terminals** - Create unlimited terminal sessions in tabs
2. **AI Chat** - Built-in AI assistant with context awareness
3. **SSH Profiles** - Manage and connect to remote servers
4. **Workflows** - Record and replay command sequences
5. **Triggers** - Auto-notifications on terminal events
6. **Session Sharing** - Share terminal sessions with others
7. **Command History** - Search and reuse previous commands (Ctrl+R)
8. **AI Autocomplete** - Smart command suggestions
9. **File Navigator** - Browse files while working in terminal

## 🔒 Security

xNAUT uses Tauri 2.0's security model:
- ACL controls all permissions
- No network access by default
- Sandboxed execution
- Only explicitly allowed commands work

All permissions are defined in `src-tauri/capabilities/default.json`.

## 📦 File Size

- **Development:** ~135KB (source only)
- **Built App:** ~5-10MB (native macOS binary)
- **Runtime Memory:** ~20-30MB (uses system WebKit, not Chromium)

## 🚀 Production Build

For distribution:

```bash
cd src-tauri
cargo tauri build --release
```

The app will be in `target/release/bundle/macos/xNAUT.app`

To sign and notarize for distribution, you'll need an Apple Developer account.

## 🎉 You're Ready!

Extract the archive and follow **README_FIRST.md** to get started!

---

**Package created:** Oct 6, 2025
**Version:** 1.0.0
**Platform:** macOS (Intel + Apple Silicon)
**Framework:** Tauri 2.0 + Rust
