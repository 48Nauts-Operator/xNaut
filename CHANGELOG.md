# Changelog

All notable changes to xNAUT are documented in this file.

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
