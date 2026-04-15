# xNAUT - AI-Powered Native Terminal

<div align="center">

```
  ██╗  ██╗███╗   ██╗ █████╗ ██╗   ██╗████████╗
  ╚██╗██╔╝████╗  ██║██╔══██╗██║   ██║╚══██╔══╝
   ╚███╔╝ ██╔██╗ ██║███████║██║   ██║   ██║
   ██╔██╗ ██║╚██╗██║██╔══██║██║   ██║   ██║
  ██╔╝ ██╗██║ ╚████║██║  ██║╚██████╔╝   ██║
  ╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝ ╚═════╝    ╚═╝
```

**A native terminal built for people who actually live in the CLI.**

[![Version](https://img.shields.io/badge/version-1.3.0-blue)](CHANGELOG.md)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-v2.0-blue)](https://tauri.app/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

</div>

---

## Why xNAUT?

xNAUT is a native macOS terminal that combines the power of Rust with a modern UI. Built with Tauri v2, it's ~8MB instead of Electron's ~500MB, and it actually feels fast.

It's designed for developers who work with multiple AI tools (Claude Code, Codex, AntBot), manage infrastructure, and want a terminal that adapts to their workflow -- not the other way around.

---

## Features

### Terminal Core
- Multiple PTY sessions with tabs
- Split panes (up to 16 per tab) -- iTerm2-style with hover close buttons
- Custom shells (zsh, bash, fish) with auto-detection
- Full xterm.js rendering with 256-color and truecolor support
- URL detection -- clickable links in terminal output
- Shell integration (OSC 133) for prompt detection

### Warp-Style Settings Panel
- Full-screen settings page (Cmd+,)
- **AI** -- Configure local (Ollama, LM Studio, AntBot) and cloud (Anthropic, OpenAI, OpenRouter, Perplexity) providers with API keys and model selection
- **Appearance** -- 12 built-in themes with full ANSI palettes (Catppuccin, Tokyo Night, Nord, Dracula, Gruvbox, and more) with live preview
- **Keyboard Shortcuts** -- Click-to-rebind with conflict detection
- **Nautify** -- Shell and SSH configuration
- **Triggers** -- Pattern matching on terminal output

### File Navigator
- Warp-style tree view on the left side
- Expand/collapse folders with lazy loading
- File type icons (code, config, images, etc.)
- Search filter
- Single click files to insert path into terminal
- Double-click folders to insert path
- Drag-and-drop to terminal

### AI Integration
- **Local-first** -- Ollama, LM Studio, and AntBot work without cloud APIs
- **AntBot integration** -- Privacy-first AI agent that runs entirely on local LLMs
- **Cloud providers** -- Anthropic, OpenAI, OpenRouter, Perplexity
- Model auto-detection for local providers (fetches available models from API)
- AI context includes terminal output for domain-aware assistance

### Autocomplete
- History-based command suggestions as you type
- Tab to accept, Escape to dismiss
- Debounced at 150ms for smooth performance

### Native macOS Integration
- Native menu bar (About xNAUT, Edit, View, Window)
- Cmd+, for Settings (standard macOS shortcut)
- Cmd+C/V/X for copy/paste/cut
- About page with version info

### Command Snippets
- Save commands you use often with names and categories
- One-click copy or run directly into the active terminal
- Markdown rendering with syntax-highlighted code blocks

### Error Monitor
- Parses terminal output in real-time
- Collects errors and warnings into a dedicated side panel

### SSH
- Connect to remote servers with saved profiles
- Reads your `~/.ssh/config` automatically
- Password and key-based auth

### Smart Triggers
- Regex pattern matching on terminal output
- Auto-notify on errors, run commands
- Configurable from Settings panel

### Ralph Ultra
- AI agent orchestrator built into the terminal
- Reads PRD files with user stories and acceptance criteria
- Auto-detects installed AI CLIs (Claude Code, Aider, Codex)
- Three execution modes: Balanced, Super Saver, Fast Delivery

---

## Getting Started

### Prerequisites

- **Rust 1.70+** -- [rustup.rs](https://rustup.rs/)
- **macOS**: `xcode-select --install`
- **Linux**: `sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libgtk-3-dev`

### Build & Run

```bash
git clone https://github.com/48Nauts-Operator/xNaut.git
cd xNaut/src-tauri

# Development (with hot reload)
cargo tauri dev

# Release build
cargo tauri build

# Launch
open target/release/bundle/macos/xNAUT.app
```

### Install from DMG

Download the latest DMG from [Releases](https://github.com/48Nauts-Operator/xNaut/releases), open it, and drag xNAUT to Applications.

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+,` | Settings |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Opt+D` | Split vertical |
| `Shift+Opt+D` | Split horizontal |
| `Opt+Arrow` | Navigate panes |
| `Opt+W` | Close pane |
| `Ctrl+Shift+R` | Toggle Ralph panel |
| `Ctrl+R` | Command history |
| `Escape` | Close Settings / modals |

All shortcuts are rebindable in Settings > Keyboard Shortcuts.

---

## Architecture

```
Frontend (HTML/CSS/JS + xterm.js)
    |  Tauri IPC
Backend (Rust)
    ├── pty.rs          PTY sessions + shell integration
    ├── ralph.rs        PRD, CLI detection, AC testing
    ├── ssh.rs          SSH connections
    ├── ai.rs           LLM provider integration
    ├── commands.rs     Tauri command handlers (incl. AntBot, file nav)
    ├── triggers.rs     Pattern matching & automation
    ├── state.rs        Thread-safe shared state
    └── main.rs         Native menu, updater, app setup

Frontend
    ├── js/app.js       Main app (settings panel, file tree, autocomplete, keybindings)
    └── js/ralph/       Orchestrator engine (8 modules)
```

- **Binary size**: ~8MB (release, stripped)
- **Memory per terminal**: ~2MB
- **PTY creation**: <100ms
- **IPC latency**: Sub-millisecond

---

## CI/CD

xNAUT uses reusable GitHub Actions workflows from [48Nauts-Operator/ci-workflows](https://github.com/48Nauts-Operator/ci-workflows):

- `cargo fmt --check` -- formatting
- `cargo clippy -D warnings` -- linting
- `cargo test` -- unit tests
- `cargo audit` -- security vulnerabilities

Every PR must pass all checks before merge.

---

## Roadmap

- [ ] Module system -- loadable tool packs with community SDK (Docker, K8s, Terraform, Security, Crypto)
- [ ] Auto-update -- check for new versions and update in-app
- [ ] Built-in file editor -- drag files to open in editor pane (Zed-style)
- [ ] Binary tree split model -- proper pane resize on close
- [ ] MCP server integration in Settings

---

## License

MIT

---

*Built with Rust, Tauri v2, and too much coffee.*
