# Changelog

All notable changes to xNAUT are documented in this file.

## [1.8.0] - 2026-05-28

### Added — Orca + hunk port (Phases 1–8)

A multi-day port of features mined from two reference apps: [stablyai/orca](https://github.com/stablyai/orca) (the agent IDE) and [modem-dev/hunk](https://github.com/modem-dev/hunk) (the agent-aware diff viewer).

**Design system (Phase 1)** — Orca's paired token model (every `--background` ships with its `--foreground`), monochrome chrome with state-only color, dark + light themes via `data-theme` attribute, 3-elevation rule, agent state dot vocabulary, git decoration palette matching VS Code.

**Worktree-per-agent (Phase 2)** — `worktree.rs` with Orca's exact recipe (`--no-track` + `push.autoSetupRemote=true` in the new worktree, platform-aware path comparison, preflight clean check before non-force removal). Top-bar Worktrees button opens a manager with create / list / launch-agent / remove.

**Agent registry + launch (Phase 3)** — `agents.rs` with 5 prompt-injection strategies (`argv`, `flag-prompt`, `flag-prompt-interactive`, `flag-interactive`, `stdin-after-start`) covering every coding CLI's quirk. User-editable TOML at `~/.config/xnaut/agents.toml` auto-seeded with claude, codex, gemini, grok, opencode. Stubbed `preflight_trust` for cursor/copilot/codex.

**Agent status overlay (Phase 4)** — `status.rs` tracks per-session state using Orca's vocabulary (`working / blocked / waiting / done` + UI-only `idle / permission / interrupted`). Top-bar pill strip with the literal Orca rendering rules (yellow spinner / emerald check / red filled / gray-40%). Output-silence decay: 2s of PTY silence → idle; 30 min stale → dropped. Click a pill to jump to the agent's tab.

**Agent hook listener (Phase 5)** — `agent_hooks.rs` runs an axum HTTP listener on `127.0.0.1:<random>` with 1 MB body cap, 5s timeout, per-session bearer tokens. Spawned agents receive `XNAUT_HOOK_URL` + `XNAUT_HOOK_TOKEN` env vars so their hook scripts can push state changes. Infrastructure-only — per-agent hook script writers deferred.

**Browser panes (Phase 6)** — `browser.rs` uses Tauri 2's child-webview API (`unstable` feature) to float native webviews over a DOM placeholder. Address bar with back/forward/reload/URL, drag-drop support, sandboxed (no Tauri API injected). Three menu items: New Browser Tab button, Split → Browser (`Cmd+Alt+B`). Includes the macOS chrome-offset Y fix for child webview positioning.

**Markdown editor (Phase 7)** — TipTap 2.10 lazy-loaded from jsdelivr ESM. Extensions: starter-kit, image, link, task-list (nested), table (resizable), placeholder, `tiptap-markdown` for round-trip markdown ↔ HTML. File open/save via existing Tauri commands, image paste + drag-drop, bubble toolbar on selection with bold/italic/strike/code/H1–H3/lists/blockquote/link. Top-bar Markdown button and Split → Markdown (`Cmd+Alt+M`).

**Diff viewer with inline annotations (Phase 8)** — the killer feature ported from hunk:
- `diff.rs` parses `git diff HEAD` (or `show`/`against-ref`) into structured JSON (file → hunks → lines with side-aware numbering)
- `notes.rs` reads/writes `<worktree>/.xnaut/notes.json` in hunk's `agent-context` shape: `{ version, summary, files: [{ path, annotations: [{ oldRange, newRange, summary, rationale, tags, confidence, source, author, createdAt }] }] }`. Range-on-both-sides anchoring renders correctly in split or unified
- `notify` crate watches the file; changes emit `notes-changed` and the pane re-renders in <100ms
- `agent_notes_broker.rs` extends Phase 5's listener with hunk's 11-verb vocabulary on `/v1/notes`: `list / get / review / comment-add / comment-apply / comment-list / comment-rm / comment-clear`. `reveal:true` emits a `diff-reveal` event so the viewer scrolls to the new note
- Three-dock render pattern from hunk: split-view notes dock right (new-side) or left (old-side); notes that don't match any hunk surface as a file-level group at the top of the file section
- `skills/xnaut-review/SKILL.md` instructs external agents on the action vocabulary, payload shapes, and the "don't comment on every hunk" / "navigate before commenting" soft rules
- `skill_path` / `skill_list` commands (mirror of `hunk skill path`) so agents can locate the bundled skill files

### Fixed
- Tauri 2 ACL: every command added in Phases 2–8 needed an entry in `permissions/default.toml` plus the `allow-all-commands` set. Without these, custom commands returned "Command not found" at runtime even when registered in `invoke_handler!`. Documented in the `xnaut-tauri-acl-recipe` memory.
- Tauri child webview Y coords on macOS counted from NSWindow top (including title bar), so address bars were painted over. `getChromeOffsetY()` measures `outerHeight − innerHeight` and adds it to viewport coords; fallback constant 28.
- Cmd+W now closes the active tab (was: closed the whole window because the default Window menu intercepted it). Cmd+Shift+W closes the window. Cmd+D / Cmd+Shift+D split right/down via View-menu items with `CmdOrCtrl` accelerators.
- Removed `transparent: true` from window config — without `macOSPrivateApi` it produced invisible / black / white windows in dev.

## [1.7.0] - 2026-06-13

### Added — PM Space
- PM section (sidebar): external client projects alongside internal ones
- Project intake with Plow (lead tool) opportunity picker — client, contacts, value pulled from the lead, never retyped
- Financials computed from the Merkle worklog: hours, burn (hours x project rate), margin vs offer
- Per-project rate (CHF/h) set at intake
- Client document generation from editable German templates (Offerte, SLA, Architektur, Meeting-Notes) via the configured LLM, written to <project>/client/
- Project-scoped chat: chat panel grounded in the project's intake data

## [1.6.0] - 2026-06-12

### Added — Tasks Mode
- Native Chat panel wired to any OpenAI-compatible endpoint (LM Studio, Ollama, NautGate, cloud) with streaming, Engram (Brain) memory grounding, and chat-driven project/task scaffolding
- Project creation end-to-end: folder under the configured factory root, git init, repo on Forgejo/GitHub/GitLab, baseline prompt to CLAUDE.md/AGENTS.md, agent launched in a named Zellij session via NautGate
- Forge Tasks panel: browse issues/PRs on Forgejo/GitHub/GitLab, one-keystroke "Start" into a Create Worktree modal (issue context preloaded for the agent)
- Automations: scheduled agent runs with precheck command, grace window, fresh/reuse sessions
- Project pane (right): Files tree, ripgrep search (git-grep fallback), full Git source control (outgoing commits, AI commit messages, push split-button with Create PR, side-by-side diffs)
- Projects sidebar (left) with pinning and plan-usage strip
- Settings: new "Tasks Mode" section (LLM endpoint with save-and-test, Engram toggle, project root and categories, forge hosts)
- Pi joins the agent registry; agents launch with NautGate routing env

### Removed
- Legacy file-navigator tree (superseded by the project pane Files view)

## [1.5.0] - 2026-04-18

### Added

**Work Session Logger**
- Record terminal commands with timestamps and duration tracking
- SHA-256 Merkle tree hash chain for tamper-evident proof
- QR code verification (scan to verify work is authentic)
- Professional HTML/PDF reports with tool usage summary
- Tool detection: groups Besen, AntBot, Claude Code, Docker, Terraform, etc.
- Duration per command and per tool

**AI Explainer**
- "Explain Screen" in 3-dot menu — AI reads terminal output and explains what's happening
- Uses AntBot (local-first) with fallback to configured provider

**AI Theme Generator**
- Describe a vibe (e.g., "ocean blue", "cyberpunk neon") and AI creates a matching color theme
- Mini terminal preview showing generated colors
- Auto-saves to custom themes

**Privacy Monitor (ClawProxy Integration)**
- Transparent LLM API proxy integration
- Detects leaked API keys, credentials, PII in prompts
- Real-time privacy indicator in status bar (green/yellow/red)
- Privacy panel with API call stats, cost, and alert details
- Routes AI traffic through ClawProxy when available

**AntBot Auto-Start**
- Settings toggle to auto-start AntBot gateway on xNAUT launch
- "Start Now" button in Settings > AI

**Cross-Platform**
- Windows x64 support (.msi and .exe installers)
- macOS Intel support
- Platform-specific directory tracking (lsof/proc/PowerShell)

**Theme System v2**
- 5 curated default themes (Jellybeans, Default Dark, Dracula, Solarized Light, Monokai)
- Separated sections: Default, AI Generated, Imported (with delete on custom)
- Import from Warp YAML and JSON theme files
- Full app theming — editor, chrome, borders all follow theme
- Color pickers with live preview

**Bundled Nerd Fonts**
- JetBrains Mono NF, Fira Code NF, Cascadia Code NF, Source Code Pro NF
- Ligature toggle in Settings

**UI Improvements**
- Binary tree split panes (proper close + resize)
- Clean 3-icon top bar (sidebar toggle, new tab, 3-dot menu)
- Redesigned command snippets (compact cards, favorites, search, A-Z index, explain)
- File browser position toggle (left/right)
- Sidebar toggle icon (SVG, Warp-style)

**Auto-Update**
- Tauri updater plugin with signed releases
- GitHub Actions release workflow (macOS + Windows)
- Blue banner notification with one-click update

### Fixed
- Split pane close now uses binary tree collapse (siblings promote correctly)
- Directory tracking via lsof (no more shell hook flashing)
- ACL permissions for all new commands
- WebView CSP — AI calls routed through Rust backend
- Theme generator handles AntBot's line-wrapped JSON responses

---

## [1.3.0] - 2026-04-15

### Added

**Warp-Style Settings Panel**
- Full-screen settings page (Cmd+,) with nav menu and search
- AI section: configure local providers (Ollama, LM Studio, AntBot) and cloud providers (Anthropic, OpenAI, OpenRouter, Perplexity)
- Model auto-detection: fetches available models from local LLM endpoints
- Appearance section: 12 built-in themes with full ANSI palettes and live preview
- Keyboard Shortcuts section: click-to-rebind with conflict detection
- Nautify section: shell and SSH configuration
- Triggers section: pattern matching on terminal output

**Built-in File Editor**
- Click files in navigator to open in syntax-highlighted editor panel
- Syntax highlighting via highlight.js for 25+ languages
- Line numbers with scroll sync in both view and edit modes
- Markdown preview mode with rendered HTML
- Save with Cmd+S, unsaved changes indicator, close confirmation
- Toggle between Edit and Preview modes

**Warp-Style File Navigator**
- Left-side tree view with expand/collapse folders
- Lazy-loaded directory contents
- File type icons and search filter
- Right-click context menu: Send to Terminal, Open in Editor, Copy Path, cd into folder
- Current directory shown in status bar footer

**AI Integration**
- AntBot integration: local-first AI agent via CLI (no cloud needed)
- Direct Ollama and LM Studio chat routing (no backend proxy)
- Model selection per provider in Settings

**Terminal Improvements**
- URL detection: clickable links in terminal output (addon-web-links)
- Split panes: up to 16 panes, iTerm2-style with hover close buttons
- Autocomplete: history-based command suggestions (Tab to accept)
- Shell integration: OSC 133 prompt detection foundation
- Directory tracking: status bar shows current CWD and git branch (via lsof)
- Startup banner: shows on first terminal, clears after 3 seconds

**Native macOS Integration**
- Native menu bar: About xNAUT, Edit (Cmd+C/V/X), View, Window
- Cmd+, opens Settings (standard macOS shortcut)
- About dialog with version, copyright, GitHub link

**Infrastructure**
- CI/CD pipeline: reusable GitHub Actions workflows (clippy, fmt, test, audit)
- Organization-wide workflow templates at 48Nauts-Operator/ci-workflows

### Changed
- Removed AI chat panel (redundant with terminal-based AI tools like AntBot, Claude Code)
- Removed LLM dropdown from status bar (moved to Settings > AI)
- Cleaned up status bar: only File Navigator, Error Monitor, Snippets, Ralph, SSH icons
- Split pane close button uses hover-to-show (no per-pane headers)

### Fixed
- ACL permissions for read_file, write_file, check_antbot, ask_antbot commands
- File path insertion targets focused pane in split view
- Context menu click handlers (mouseup instead of onclick to prevent race)
- Directory tracking updates shared status bar directly
- Shell hooks injected with 2-second delay to prevent startup interference
- Multiple null reference crashes from removed UI elements
- CSS grid overflow on split panes (min-height:0 cascade)
- Pane refit after close with multiple passes

---

## [1.2.0] - 2026-03-23

### Added — Ralph Ultra Integration (Phase 1-3)

**Backend (Rust)**
- `ralph.rs` module with 12 new Tauri commands:
  - PRD management: `ralph_read_prd`, `ralph_write_prd`, `ralph_backup_prd`, `ralph_list_backups`, `ralph_restore_backup`
  - CLI detection: `ralph_detect_clis`, `ralph_check_cli_health`
  - AC test execution: `ralph_run_ac_test`
  - Config persistence: `ralph_read_config`, `ralph_write_config`
  - Temp file management: `ralph_write_temp_file`, `ralph_cleanup_temp_file`
- `create_command_session` command for spawning AI CLIs in PTY (non-interactive program execution)
- Exit code capture on PTY close — `terminal-closed:{id}` events now include `exitCode` field

**Frontend (JavaScript)**
- Ralph orchestrator engine (8 ES modules in `src/js/ralph/`):
  - Task type detection (14 categories via keyword scoring)
  - Model capability matrix with 3 execution modes (balanced, super-saver, fast-delivery)
  - Cost tracking with estimated vs actual, persisted to disk
  - Learning recorder with weighted scoring (reliability 40%, efficiency 35%, speed 25%)
  - Execution planner with mode comparisons
  - Prompt builder for Claude, Aider, and Codex CLIs
  - Main orchestrator: load project, detect CLIs, plan, execute stories, test ACs, retry/advance
- Ralph UI panel with project path input, execution mode selector, run/pause/stop/test controls, story list, cost summary, and live log monitor
- Tauri bridge module for ES module access to `invoke`/`listen`

**UI**
- Ralph panel toggle via robot icon in status bar or `Ctrl+Shift+R` keyboard shortcut
- `ralph.css` stylesheet matching xNAUT dark design tokens

### Fixed

- **Split pane exit handling** — typing `exit` in a split pane now properly closes that pane and re-layouts remaining panes (was going stale/unresponsive)
- **Snippet copy/run with quotes** — commands containing double quotes (e.g., `gcloud compute ssh --zone "europe-west6-a"`) no longer get truncated at the first quote when using copy or run buttons
- **Claude Code nested session error** — PTY now strips `CLAUDECODE` environment variable so Claude Code can run inside xNAUT without false nested-session detection
- **PtyConfig deserialization** — added `#[serde(default)]` so `cols`/`rows` fields default to 80x24 when not provided by frontend

### Changed

- Split pane limit increased from 4 to 6 — new 5-pane (3+2) and 6-pane (3x2 grid) layouts
- Version bumped to 1.2.0 across `Cargo.toml`, `tauri.conf.json`, and UI badge

## [1.1.0] - 2025-10-06

### Initial Release

- Multiple PTY sessions with tab management
- Split pane support (up to 4 panes with vertical/horizontal splits)
- SSH connection support with config file integration
- AI chat integration (Anthropic, OpenAI, OpenRouter, Perplexity)
- Workflow recording and playback
- Smart triggers and notifications
- Session sharing capabilities
- xterm.js v5.5.0 terminal rendering
- Tauri v2 ACL security model with proper permissions
- macOS native app bundle (~8MB binary)
