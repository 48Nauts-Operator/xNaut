# 🍎 Building xNAUT on macOS

## One-Time Setup (15 minutes)

### 1. Install Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Install Xcode Command Line Tools
```bash
xcode-select --install
```

## Build the App (5 minutes)

### 1. Navigate to Project
```bash
cd ~/Downloads/xnaut/src-tauri
```

### 2. Build for macOS
```bash
cargo tauri build
```

This will:
- Download Rust dependencies (~2 minutes)
- Compile Rust backend (~2 minutes)
- Create macOS .app bundle (~1 minute)

### 3. Find Your App
```bash
open target/release/bundle/macos/
```

You'll see: **xNAUT.app**

### 4. Install
Drag `xNAUT.app` to your Applications folder!

## Run the App

1. Open **Launchpad** or **Applications** folder
2. Click **xNAUT** icon
3. App opens as a native macOS application!
4. No browser, no Node.js - pure native Mac app

## First Run

When you first open xNAUT:
1. macOS may show "xNAUT cannot be opened" (unsigned app)
2. Go to **System Preferences > Privacy & Security**
3. Click **"Open Anyway"** next to xNAUT
4. App will open and work normally from then on

## Development Mode (optional)

To run in development mode:
```bash
cd ~/Downloads/xnaut/src-tauri
cargo tauri dev
```

This opens the app with hot-reload for testing.

## What You Get

- ✅ Native macOS app (not Electron!)
- ✅ ~5-10MB binary size
- ✅ Uses system WebKit (no bundled Chromium)
- ✅ Instant startup
- ✅ Low memory (~20-30MB RAM)
- ✅ All features: AI, SSH, session sharing, triggers
- ✅ Feels like a real Mac app

## Troubleshooting

**"cargo: command not found"**
```bash
source $HOME/.cargo/env
```

**"xcode-select: error"**
- Install Xcode from App Store first
- Or install Command Line Tools manually

**Build errors**
- Make sure you're in `xnaut/src-tauri` directory
- Try: `cargo clean` then `cargo tauri build` again

**App won't open**
- Check Privacy & Security settings
- Click "Open Anyway" for xNAUT

## File Size Comparison

- **xNAUT**: ~5-10MB
- **VS Code** (Electron): ~500MB
- **iTerm2**: ~15MB
- **Terminal.app**: Built-in

xNAUT is incredibly small for what it does!

## Next Steps

1. Build the app (follow steps above)
2. Open xNAUT
3. Click ⚙️ Settings
4. Add your AI API keys (Anthropic, OpenAI, etc.)
5. Start using your AI-powered terminal!

---

**Made with ❤️ in pure Rust + Tauri**
