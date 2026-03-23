# XNAUT - Quick Start Guide

## What is XNAUT?

**XNAUT** is an AI-powered native terminal application built with Tauri v2, combining the performance of Rust with the flexibility of web technologies.

### Key Features
- 🖥️ **Multiple PTY Sessions** - Run multiple terminals in one window
- 🔐 **SSH Support** - Connect to remote servers
- 🤖 **AI Assistant** - Get command suggestions and error analysis
- ⚡ **Smart Triggers** - Automated responses to terminal patterns
- 🔗 **Session Sharing** - Collaborate in real-time
- 🎨 **Beautiful UI** - Modern terminal with xterm.js

## Project Structure

```
xnaut/
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── main.rs         # Entry point
│   │   ├── state.rs        # App state management
│   │   ├── pty.rs          # Terminal sessions
│   │   ├── commands.rs     # Tauri IPC commands
│   │   ├── ssh.rs          # SSH connections
│   │   ├── ai.rs           # AI integration
│   │   ├── errors.rs       # Error types
│   │   └── triggers.rs     # Pattern matching
│   ├── Cargo.toml          # Rust dependencies
│   └── tauri.conf.json     # Tauri config
├── src/                    # Frontend (to be added)
├── RUST_BACKEND.md         # Backend architecture docs
├── TESTING.md              # Testing guide
└── QUICKSTART.md           # This file
```

## Prerequisites

### Required
- **Rust** (1.70+) - `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** (18+) - For frontend build tools
- **Git** - Version control

### System Dependencies (Linux)

Ubuntu/Debian:
```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  libssl-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Fedora/RHEL:
```bash
sudo dnf install \
  webkit2gtk4.1-devel \
  openssl-devel \
  gtk3-devel \
  libappindicator-gtk3-devel \
  librsvg2-devel
```

Arch Linux:
```bash
sudo pacman -S \
  webkit2gtk-4.1 \
  gtk3 \
  libayatana-appindicator \
  librsvg
```

### macOS
```bash
# Install Xcode Command Line Tools
xcode-select --install
```

### Windows
- Install Visual Studio 2022 with C++ tools
- Install WebView2 (usually pre-installed on Windows 11)

## Installation

### 1. Clone or Navigate to Project

```bash
cd /home/jarvis/projects/NautCode/xnaut
```

### 2. Install Rust Dependencies

```bash
cd src-tauri
cargo build
```

This will download and compile all Rust dependencies (~500 crates, takes 5-10 minutes first time).

### 3. Verify Installation

Run the verification script:
```bash
./verify-rust-code.sh
```

You should see:
- ✓ All required files present
- ✓ ~1500 lines of Rust code
- ✓ All modules with ABOUTME comments
- ✓ Documentation files

## Development

### Run in Development Mode

```bash
cd src-tauri
cargo tauri dev
```

This will:
1. Compile the Rust backend
2. Start the frontend dev server
3. Open the application window
4. Enable hot reload for frontend changes

### First Launch

When you run XNAUT for the first time, you'll see:

```
╔═══════════════════════════════════════════════════════════════════╗
║                                                                   ║
║  ██╗  ██╗███╗   ██╗ █████╗ ██╗   ██╗████████╗                   ║
║  ╚██╗██╔╝████╗  ██║██╔══██╗██║   ██║╚══██╔══╝                   ║
║   ╚███╔╝ ██╔██╗ ██║███████║██║   ██║   ██║                      ║
║   ██╔██╗ ██║╚██╗██║██╔══██║██║   ██║   ██║                      ║
║  ██╔╝ ██╗██║ ╚████║██║  ██║╚██████╔╝   ██║                      ║
║  ╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝ ╚═════╝    ╚═╝                      ║
║                                                                   ║
║              AI-Powered Native Terminal                          ║
║              Version 1.0.0                                        ║
╚═══════════════════════════════════════════════════════════════════╝

🚀 XNAUT is starting...
✓ State initialized
✓ Commands registered
✓ Event handlers ready
🎉 XNAUT is ready!
```

## Testing

### Run All Tests

```bash
cd src-tauri
cargo test
```

### Run Specific Module Tests

```bash
# Test PTY module
cargo test --lib pty::tests

# Test state module
cargo test --lib state::tests

# Test AI module
cargo test --lib ai::tests
```

### Code Quality

```bash
# Linting
cargo clippy

# Format check
cargo fmt -- --check

# Auto-format
cargo fmt
```

## Building Production Binary

### Development Build
```bash
cargo build
```

Binary location: `src-tauri/target/debug/xnaut`

### Release Build
```bash
cargo build --release
```

Binary location: `src-tauri/target/release/xnaut`

### Full Application Package
```bash
cargo tauri build
```

Creates platform-specific installers in `src-tauri/target/release/bundle/`

## Usage Examples

### Creating a Terminal Session (Rust API)

```rust
use xnaut::pty::{create_pty_session, PtyConfig};

let config = PtyConfig {
    shell: Some("/bin/bash".to_string()),
    working_dir: Some("/home/user".to_string()),
    cols: 80,
    rows: 24,
    env: None,
};

let session_id = create_pty_session(app, state, config).await?;
```

### Frontend Integration (JavaScript)

```javascript
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Create terminal session
const sessionId = await invoke('create_terminal_session', {
  config: {
    cols: terminal.cols,
    rows: terminal.rows
  }
});

// Listen for output
await listen(`terminal-output:${sessionId}`, (event) => {
  const data = atob(event.payload.data);
  terminal.write(data);
});

// Send input
terminal.onData(async (data) => {
  await invoke('write_to_terminal', {
    sessionId,
    data: btoa(data)
  });
});

// Resize terminal
terminal.onResize(async ({ cols, rows }) => {
  await invoke('resize_terminal', {
    sessionId,
    cols,
    rows
  });
});
```

### SSH Connection

```javascript
const sessionId = await invoke('create_ssh_session', {
  host: 'server.example.com',
  port: 22,
  username: 'user',
  password: 'secret'  // or keyPath: '/path/to/key'
});
```

### AI Assistant

```javascript
// Ask AI for help
const response = await invoke('ask_ai', {
  prompt: 'How do I list all files in a directory?',
  context: 'Linux terminal'
});

console.log(response.response);
console.log(response.suggestions);
console.log(response.commands);
```

### Creating Triggers

```javascript
// Notify on errors
const triggerId = await invoke('create_trigger', {
  pattern: '(?i)error',
  action: {
    type: 'Notify',
    message: 'Error detected!'
  }
});

// AI assistance on permission errors
await invoke('create_trigger', {
  pattern: '(?i)permission denied',
  action: {
    type: 'AiAssist',
    prompt: 'How do I fix this permission error?'
  }
});
```

### Session Sharing

```javascript
// Share a session
const shareCode = await invoke('share_session', {
  sessionId: 'abc-123',
  readOnly: false
});

console.log(`Share code: ${shareCode}`);

// Join shared session
const sharedSession = await invoke('join_shared_session', {
  shareCode: 'XyZ123'
});
```

## Configuration

### AI Provider Setup

Edit your config file or environment variables:

```bash
# OpenAI
export XNAUT_AI_PROVIDER=openai
export XNAUT_AI_API_KEY=sk-...
export XNAUT_AI_MODEL=gpt-4

# Anthropic Claude
export XNAUT_AI_PROVIDER=anthropic
export XNAUT_AI_API_KEY=sk-ant-...
export XNAUT_AI_MODEL=claude-3-opus-20240229
```

### Default Shell

Set your preferred shell:
```bash
export XNAUT_DEFAULT_SHELL=/bin/zsh
```

## Troubleshooting

### Build Errors

**Problem**: Missing system libraries
```
error: failed to run custom build command for `glib-sys`
```

**Solution**: Install system dependencies (see Prerequisites section)

### PTY Not Starting

**Problem**: Terminal sessions fail to create

**Solution**:
1. Check shell exists: `which bash`
2. Verify permissions: `ls -la /bin/bash`
3. Check logs: `RUST_LOG=debug cargo tauri dev`

### SSH Connection Fails

**Problem**: Cannot connect to remote server

**Solution**:
1. Test manually: `ssh user@host`
2. Check key permissions: `chmod 600 ~/.ssh/id_rsa`
3. Verify network: `ping host`

### AI Not Working

**Problem**: AI commands return errors

**Solution**:
1. Verify API key is set
2. Check network connectivity
3. Test API with curl
4. Review rate limits

## Next Steps

### For Users
1. ✅ Install prerequisites
2. ✅ Build and run XNAUT
3. ⬜ Create your first terminal session
4. ⬜ Try SSH to a remote server
5. ⬜ Ask the AI assistant for help
6. ⬜ Set up custom triggers

### For Developers
1. ✅ Review RUST_BACKEND.md for architecture
2. ✅ Read TESTING.md for test strategy
3. ⬜ Add frontend implementation
4. ⬜ Implement AI provider integration
5. ⬜ Add session recording feature
6. ⬜ Create plugin system

## Contributing

1. Fork the repository
2. Create feature branch: `git checkout -b feature/amazing-feature`
3. Write tests for new features
4. Run tests: `cargo test`
5. Run clippy: `cargo clippy`
6. Format code: `cargo fmt`
7. Commit changes: `git commit -m 'Add amazing feature'`
8. Push branch: `git push origin feature/amazing-feature`
9. Open Pull Request

## Resources

- **Architecture**: See RUST_BACKEND.md
- **Testing**: See TESTING.md
- **Tauri Docs**: https://tauri.app/
- **Portable PTY**: https://docs.rs/portable-pty/
- **xterm.js**: https://xtermjs.org/

## License

MIT License - See LICENSE file for details

## Support

- Report bugs: GitHub Issues
- Ask questions: Discussions
- Documentation: /docs folder

---

**Made with ❤️ using Rust and Tauri**
