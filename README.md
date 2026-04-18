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

[![Version](https://img.shields.io/badge/version-1.5.0-blue)](CHANGELOG.md)
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
- Warp-style tree view with expand/collapse, lazy loading
- Right-click context menu: Send to Terminal, Open in Editor, Copy Path
- Search filter, file type icons
- Position toggle (left or right side)

### Built-in File Editor
- Click files to open with syntax highlighting (25+ languages via highlight.js)
- Line numbers with scroll sync
- Markdown preview mode
- Save with Cmd+S

### AI Integration
- **Local-first** -- Ollama, LM Studio, and AntBot work without cloud APIs
- **AntBot auto-start** -- gateway launches automatically on xNAUT startup
- **AI Explainer** -- "Explain Screen" reads terminal output and explains what's happening
- **AI Theme Generator** -- describe a vibe, AI creates a matching color theme
- **Cloud providers** -- Anthropic, OpenAI, OpenRouter, Perplexity
- Model auto-detection for local providers

### Work Session Logger
- Record all terminal commands with timestamps and duration
- SHA-256 Merkle tree hash chain -- tamper-evident proof
- QR code verification -- scan to verify work session is authentic
- Professional HTML/PDF reports with tool usage summary
- Tool detection: groups Besen, AntBot, Claude Code, Docker, etc. with total duration
- Perfect for billable hours and client documentation

### Privacy Monitor (ClawProxy)
- Transparent LLM API proxy integration
- Detects leaked API keys, credentials, PII in prompts
- Real-time privacy indicator in status bar
- Privacy assessment panel with alerts

### Themes
- 5 curated default themes (Jellybeans, Default Dark, Dracula, Solarized Light, Monokai)
- AI Theme Generator -- describe a mood, get a custom theme
- Import Warp (YAML) and JSON themes
- Full app theming -- terminal, editor, chrome all follow the theme
- 4 bundled Nerd Fonts (JetBrains Mono NF, Fira Code NF, Cascadia Code NF, Source Code Pro NF)

### Command Snippets
- Compact cards with collapsible command lists
- Search bar and A-Z alphabet index
- Favorites (star) with priority sorting
- Explain button -- AI explains any command
- Share button for team collaboration
- Hover actions (Copy, Run, Explain)

### Auto-Update
- Checks GitHub Releases for new versions on startup
- Blue banner notification with one-click update
- Signed releases with Merkle tree verification

### Cross-Platform
- macOS (Apple Silicon + Intel)
- Windows (x64) -- .msi and .exe installers
- Platform-specific directory tracking

### Native macOS Integration
- Native menu bar with About, Edit, View, Window
- Cmd+, for Settings
- Clean 3-icon top bar (sidebar, new tab, 3-dot menu)

### SSH, Triggers, Error Monitor, Ralph Ultra
- SSH profiles with ~/.ssh/config import
- Pattern matching triggers (configurable from Settings)
- Real-time error collection panel
- Ralph Ultra AI orchestrator

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
    ├── pty.rs          PTY sessions + directory tracking
    ├── worklog.rs      Session logging, Merkle proof, QR, reports
    ├── ralph.rs        PRD, CLI detection, AC testing
    ├── ssh.rs          SSH connections
    ├── ai.rs           LLM provider integration
    ├── commands.rs     Tauri commands (AntBot, ClawProxy, file nav, editor)
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

- [ ] Module system -- loadable tool packs with community SDK
- [ ] Community Hub -- GitHub-based package registry for sharing scripts
- [ ] SecondBrain integration -- terminal long-term memory
- [ ] Privacy reports -- integrated with ClawProxy data in work reports
- [ ] Linux support (AppImage, .deb)

---

## License

MIT

---

*Built with Rust, Tauri v2, and too much coffee.*
