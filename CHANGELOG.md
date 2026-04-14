# Changelog

All notable changes to xNAUT are documented in this file.

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
