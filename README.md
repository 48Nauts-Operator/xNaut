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

[![Version](https://img.shields.io/badge/version-1.8.6-blue)](CHANGELOG.md)
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

### Project Workspaces *(new in 1.8.1)*
- Orca/CMUX model: each project card on the left is a workspace — the top tab bar shows only that project's tabs
- Global views (Tasks/Automations/PM/Search) live in a shared **Home** workspace
- Selecting a project restores its existing tabs; first open creates a terminal in the project folder (or attaches its zellij session)
- Active project highlight, and a status dot that lights when a project has open tabs
- Right-pane **root picker** — click the Files icon to switch the tree between Home, Project Root, and the current project
- **Open an existing project** *(1.8.2)* — right-click a folder in the tree → "Open as project", or ask the chat agent ("open my X project"); drag a file/folder from the tree onto a terminal to insert its path
- **Opt+B / Opt+M** *(1.8.2)* — split the focused terminal with a browser / markdown pane

### Plan Mode *(new in 1.8.1)*
- Two-pane planning workspace from any PM project: chat (left) + a live `PLAN.md` document (right)
- Solution-architect persona, Engram-grounded; the plan stays in the document pane (not dumped into chat) and the agent extends the current doc each turn
- Doc pane has an **Edit / Preview** toggle; rendered with marked + highlight.js + Mermaid

### Markdown (dependency-free) *(new in 1.8.1)*
- Shared renderer (`markdown-render.js` → `window.xnautMarkdown`) powered by **marked** (UMD) — GFM, raw-HTML passthrough, syntax highlighting (highlight.js), and **Mermaid** diagrams
- Replaces the CDN TipTap editor (which fails to load in this WebKit); open any `.md` with Edit/Preview + `Cmd+S`
- `Cmd +/-/0` zoom works in markdown docs *and* terminals

### Diff Viewer with AI Annotations *(new in 1.8.0)*
- Side-by-side or unified `git diff HEAD` view for any worktree
- Inline note cards floating beside the changed lines — read from `<worktree>/.xnaut/notes.json`
- Annotations follow [hunk](https://github.com/modem-dev/hunk)'s data model: `oldRange + newRange` anchoring, `summary + rationale` two-body pattern, tags, confidence, source, author
- File watcher updates the pane within ~100ms of any change to `notes.json`
- HTTP broker at `127.0.0.1:<port>/v1/notes` with hunk's 11-verb vocabulary (`comment-add`, `comment-apply`, `comment-list`, `comment-rm`, `comment-clear`, plus `review`, `get`, `navigate`, etc.)
- Skill file at `skills/xnaut-review/SKILL.md` tells external agents (Claude Code, Codex, etc.) the protocol
- Three-dock rendering: split-view + new-side note → right; split-view + old-side note → left; off-hunk notes surface as a file-level group

### Browser Panes *(new in 1.8.0)*
- Native Tauri child webview rendered as a pane alongside terminals
- Address bar with back/forward/reload, drag-drop URL support, sandboxed (no Tauri API exposed to loaded pages)
- `Cmd+Alt+B` splits the current pane and adds a browser to the right

### Markdown Editor *(new in 1.8.0)*
- TipTap 2.10 rich editor lazy-loaded from CDN
- Image / link / task-list / table / placeholder extensions + markdown round-trip
- File open/save (Cmd+O / Cmd+S), image paste + drag-drop, bubble toolbar on text selection
- `Cmd+Alt+M` splits with a markdown pane — write notes beside the terminal

### Worktree Manager *(new in 1.8.0)*
- Top-bar button → modal for creating worktrees with [Orca](https://github.com/stablyai/orca)'s recipe: `--no-track` + `push.autoSetupRemote=true` so `git push` and `git status` both behave
- Launch any configured agent (claude, codex, gemini, grok, opencode, custom) directly in the new worktree
- Existing worktrees listed with per-row "launch agent here" + remove (force-remove on dirty trees with confirm)

### Agent Runner *(new in 1.8.0)*
- User-editable registry at `~/.config/xnaut/agents.toml` — 5 prompt-injection strategies cover every coding CLI's quirk
- Top-bar status strip with the Orca state vocabulary: `working` (yellow spinner) / `done` (emerald check) / `blocked`/`waiting`/`permission`/`interrupted` (red filled) / `idle` (gray)
- Output-silence detection fallback; a hook listener at `127.0.0.1:<port>/v1/hook` accepts real-time pushes from agent hook scripts (per-session bearer tokens)
- Click a pill → jump to that agent's tab

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
