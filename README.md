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

[![Version](https://img.shields.io/badge/version-1.2.0-blue)](CHANGELOG.md)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-v2.0-blue)](https://tauri.app/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

</div>

---

## The Story

I didn't set out to build a terminal. I just got tired of the ones I had.

My day-to-day is cloud infrastructure -- multiple providers, Docker, Terraform, Kubernetes. The kind of work where you have dozens of long commands you run regularly but can never quite remember by heart, where you're constantly flipping between SSH sessions, and where a failing test buries the actual error three screens up.

I tried other tools. I used Warp for a while -- their AI coding features were interesting, but they kept changing direction and pricing, and eventually it just wasn't worth it. I looked at other "smart" terminals, but none of them had exactly what I wanted. The features were always close, but not quite right.

So I figured: why not build my own? These days, with Rust and Tauri, you can build a native app that's 8MB instead of 500MB, and it actually feels fast.

That was over six months ago. Since then, xNAUT has become the terminal I use every day. It started as a weekend project and turned into the tool I reach for first whenever I open my laptop.

**What made me keep using it:**

- **The error panel.** When I run tests, the errors show up in a panel on the right side. I don't have to scroll back or search through output -- they're just there. It sounds simple, but it changed how I work.

- **Command snippets.** I don't remember every `gcloud` or `kubectl` incantation, and I don't want to keep a separate notes file. xNAUT has a built-in command book where I store everything I use regularly. When I need a command, I find it and run it right there. No copy-pasting from a wiki.

- **Split panes that just work.** Up to six terminals in one tab, split any way I want. Opt+D, Shift+Opt+D, done.

- **Ralph Ultra.** I built an AI agent orchestrator directly into the terminal so I wouldn't need yet another tool running alongside it. It reads a PRD, picks the right AI model, runs stories against acceptance criteria, and tracks costs -- all from a sidebar panel.

It just grows on me more and more. It's a personal tool that solves my personal problems, and I happen to think other people who live in the CLI might find it useful too. If that's you, feel free to use it.

---

## Features

### Terminal
- Multiple PTY sessions with tabs
- Split panes (up to 6 per tab) -- vertical, horizontal, grid layouts
- Custom shells (bash, zsh, fish)
- Shells auto-close when you type `exit`
- Full xterm.js rendering with 256-color and truecolor support

### Command Snippets
- Save commands you use often with names and categories
- One-click copy or run directly into the active terminal
- Markdown rendering with syntax-highlighted code blocks
- Handles multi-line commands and quoted arguments correctly

### Error Monitor
- Parses terminal output in real-time
- Collects errors and warnings into a dedicated side panel
- No more scrolling back to find what failed

### SSH
- Connect to remote servers with saved profiles
- Reads your `~/.ssh/config` automatically
- Password and key-based auth

### AI Chat
- Built-in AI assistant (Anthropic, OpenAI, OpenRouter, Perplexity)
- Ask questions, analyze errors, get command suggestions
- Context-aware -- knows your current directory and recent output

### Smart Triggers
- Regex pattern matching on terminal output
- Auto-notify on errors, run commands, or ask AI
- Customizable rules

### Ralph Ultra (v1.2.0)
- AI agent orchestrator built into the terminal
- Reads PRD files with user stories and acceptance criteria
- Auto-detects installed AI CLIs (Claude Code, Aider, Codex)
- Picks optimal models based on task type and budget mode
- Runs stories, tests acceptance criteria, retries on failure
- Tracks costs and learns from past runs
- Three execution modes: Balanced, Super Saver, Fast Delivery

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

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Opt+D` | Split vertical |
| `Shift+Opt+D` | Split horizontal |
| `Opt+Arrow` | Navigate panes |
| `Opt+W` | Close pane |
| `Ctrl+Shift+R` | Toggle Ralph panel |
| `Ctrl+R` | Command history |

## Architecture

```
Frontend (HTML/CSS/JS + xterm.js)
    |  Tauri IPC
Backend (Rust)
    ├── pty.rs          PTY sessions + split panes
    ├── ralph.rs        PRD, CLI detection, AC testing, config
    ├── ssh.rs          SSH connections
    ├── ai.rs           LLM provider integration
    ├── triggers.rs     Pattern matching & automation
    ├── commands.rs     Tauri command handlers
    └── state.rs        Thread-safe shared state

Frontend Modules (ES)
    └── js/ralph/       Orchestrator engine (8 modules)
        ├── ralph-orchestrator.js    Main execution loop
        ├── ralph-capability-matrix.js  Model selection
        ├── ralph-cost-tracker.js    Budget tracking
        └── ...
```

- **Binary size**: ~8MB (release, stripped)
- **Memory per terminal**: ~2MB
- **PTY creation**: <100ms
- **IPC latency**: Sub-millisecond

## Project Status

xNAUT is actively used in production as a daily driver. It's stable for personal use.

See [CHANGELOG.md](CHANGELOG.md) for version history.

### What's Working
- Terminal sessions, tabs, split panes (up to 6)
- SSH connections with config import
- AI chat with multiple providers
- Command snippets with categories
- Error monitoring panel
- Smart triggers
- Ralph Ultra orchestrator (v1.2.0)

### On the Horizon
- Session recording and playback
- Docker/container integration
- Theme customization
- Plugin system

## Contributing

Contributions are welcome. Fork it, build something useful, open a PR.

```bash
# Run tests
cargo test

# Lint
cargo clippy

# Format
cargo fmt
```

## License

MIT -- see [LICENSE](LICENSE).

## Acknowledgments

Built with:
- [Tauri](https://tauri.app/) -- native app framework
- [portable-pty](https://docs.rs/portable-pty/) -- cross-platform PTY
- [xterm.js](https://xtermjs.org/) -- terminal rendering
- [tokio](https://tokio.rs/) -- async runtime

---

<div align="center">

**Built because no terminal had exactly what I needed.**

</div>
