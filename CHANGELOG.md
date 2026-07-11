# Changelog

All notable changes to xNAUT are documented in this file.

## [Unreleased]

### Added - Frontier models and MCP drawing
- **Remote model discovery.** Agent Chat can enumerate models from the OpenAI-compatible endpoint selected in Settings, including OpenAI and OpenRouter, while retaining per-conversation model overrides.
- **Provider-aware Agent Chat.** The right-pane model selector groups models discovered from configured local, OpenAI, and OpenRouter providers and routes each request through the provider associated with the selected model.
- **Live workspace context.** Right-pane Agents receive a fresh, non-persisted snapshot of the currently visible NAUT-Flow document, so references such as "the document I have open" resolve without manually restating the Vault path.
- **Local Excalidraw MCP.** xNAUT clones, builds, and starts the official MIT-licensed Excalidraw MCP server locally, binds it to loopback, and connects over Streamable HTTP. A hosted endpoint and bearer key remain optional overrides.
- **Capability-aware tools.** xNAUT discovers the drawing tools exposed by the configured local or remote server instead of assuming a fixed tool set.

### Added - Optional Project Management module
- **Opt-in setup.** Project Management stays disabled by default. Enabling it in Settings guides the user through creating a local Git control repository, optionally creating a private repository on a configured Forge, or connecting an existing xNaut control repository.
- **Git-backed records.** The module stores versioned project and ticket JSON, append-only workflow events, and machine-readable schemas outside source repositories. Every mutation creates a scoped Git commit without including unrelated files.
- **Agent-ready commands.** Project and ticket list/create/update commands provide the initial constrained service boundary for future xNaut agents and MCP access. Ticket updates use optimistic revisions to prevent silent overwrites.

### Fixed - Project Management setup
- **Explicit Forge credentials and ownership.** Private remote setup now distinguishes organization repositories from repositories owned by the token's personal account, accepts a setup token, and explains the required write scopes.
- **Existing repository recovery.** Connect Existing can attach an already-created local control repository to an existing SSH or HTTPS remote, including recovery after Forge API creation failed.

### Added - Project and ticket workspace
- **Unified Projects navigation.** The existing Projects/PM entry opens the Git-backed workspace when the module is enabled, eliminating the duplicate PM and Tickets screens while retaining the lightweight legacy view when the module is disabled.
- **Project registry and ticket board.** Create control projects and tickets, filter by project or text, switch between board and table views, and drag tickets through Inbox, Ready, In Progress, Review, Blocked, and Done.
- **Ticket details.** Edit type, priority, status, owner, description, and linked Vault documents with optimistic revision checks. Ticket creation, updates, status moves, and deletion remain individual Git commits.
- **Traceability and synchronization.** Ticket activity is loaded from append-only workflow events. The workspace shows branch/commit/ahead/behind state and can pull/rebase/push the private control repository, including the first push to an empty remote.
- **Vault handoff.** Ticket document references can open the requested work or personal Vault note directly in xNaut.
- **Existing-project and legacy migration.** The workspace automatically imports xNaut's current project registry with stable project keys and source paths. Legacy client scope/contact records are attached to matching projects, legacy todos become tickets, and unmatched todos are preserved under a Legacy Migration project. Original local stores remain untouched as a recovery copy.

## [1.8.12] - 2026-07-06

### Added — Markdown Vault
- **Vault Librarian workspace.** Opening the Vault now switches the far-right pane to Librarian Conversations, with conversation history and an in-pane **+** action for starting a new Librarian thread.
- **Manual notes from templates.** The Vault create panel can create notes from `Templates/*.md`, with title/date substitutions for reusable Concept, Business, Development, and future templates.
- **Direct Markdown import.** Readable Obsidian links and absolute `.md` paths can be imported deterministically into `_inbox/` without waiting for model-generated JSON.

### Fixed — Markdown Vault
- **Reliable note creation.** Explicit `vault_create` / `vault_write` JSON pasted into the Librarian executes directly, and agent-created notes refresh the visible Vault tree immediately instead of requiring an app restart.
- **Local-model action handling.** Qwen/LM Studio chat calls disable hidden reasoning for action requests, always end Qwen repair/follow-up prompts with a user query, cap chat output, and use a stream idle timeout so Vault actions do not hang indefinitely.
- **Settings consistency.** AI Settings save back into the Rust chat settings store so the Librarian uses the model selected in Settings.
- **Preview readability.** Vault Preview hides YAML frontmatter while preserving it in Edit mode, adds proper spacing before headings, and renders polished Markdown tables with headers, borders, striping, and hover feedback.
- **Vault document scrolling.** The note preview and editor have independent scroll containers so long documents remain usable in the right pane.

## [1.8.11] - 2026-07-05

### Fixed — Markdown Vault
- **Vault tree actions.** Replaced fragile menu dialogs with an inline action strip so note/folder actions stay visible and clickable.
- **Vault tree context menus.** Kept tree menus interactive while preserving drag/move behavior for organizing notes.
- **Vault controls.** Restored visible create and refresh controls in the Vault rail.

## [1.8.10] - 2026-07-03

### Added — Knowledge Graph ("the orb")
- **Vault graph.** ⋯ menu → **Knowledge Graph** opens a tab that scans a folder of `.md` notes, parses `[[wikilinks]]` into a force-directed graph, and renders it as an Obsidian-style orb (2D and 3D, slow auto-rotating nebula). Rust `graph_scan` walks every subfolder (skipping dotfiles, 6k cap); notes match by filename stem like Obsidian.
- **Code graph.** The **Vault / Code** selector switches the same pane to scan a codebase — files = nodes, **relative imports** (`./`, `../`, `#include "…"`) = edges (bare packages skipped) — colored **by file type** with a legend. Rust `code_scan`.
- **Cosmic styling.** Per-cluster coloring (union-find, golden-angle hues starting in blue so no red flood), a layered nebula-fog backdrop with a transparent canvas, a dim twinkling starfield of orphan/unlinked nodes, and gentle cyan "signal" particles travelling along backlinks.
- **Timeline build-up.** ▶ Timeline reveals notes/files in file-date (mtime) order so the universe assembles itself; a scrubber lets you scrub through it.

## [1.8.9] - 2026-06-25

### Fixed
- **App-crash (SIGABRT) while working in a terminal.** The app-wide debug-log trimmer sliced its buffer as a UTF-8 `String` at a raw byte offset; once the log passed 2 MB and the cut landed mid-character (terminal output contains emoji/multi-byte chars), it panicked — and with `panic = "abort"` that aborted the whole process. The trimmer now slices on bytes (safe at any offset). Caught via the crash report's WebKit→Rust `didPostMessage` backtrace.
- **Removed `panic = "abort"` from the release profile** — a panic in any `#[tauri::command]` is now caught and returned as an error instead of crashing the entire app. Defense-in-depth against this whole class of crash.

## [1.8.8] - 2026-06-25

### Added
- **Snippet Export / Import** in the Command Snippets panel. `↑ Export` writes a dated `xnaut-snippets-YYYY-MM-DD.json` (version, categories, snippets); `↓ Import` merges from a JSON file, skipping duplicates by id and unioning categories. The feature shipped on `main` (v1.5.1) but had never been merged into the tasks-mode branch — this brings it back.

## [1.8.7] - 2026-06-24

### Fixed
- **Modal Cancel/close actually closes.** The Project-details (PM intake) and Worktree modals styled their overlay `display:flex`, which overrode the `hidden` attribute — so Cancel/X/Escape set the flag but the modal stayed up (you had to quit the app). Added `[hidden] { display:none !important }` to both overlays.
- Removed an undeclared `workflows` reference in the `window.xnaut` debug export that threw a `ReferenceError` at the end of app.js load. (Surfaced by the new app-wide debug log.)

## [1.8.6] - 2026-06-24

### Fixed
- **New projects land in the right folder.** Creating a project via the **+** button now places it at `<project_root>/<Development category folder>/<name>` (e.g. `factory/02-Development/<name>`), matching the chat scaffold flow — instead of directly under the project root. "Open as project" still registers a folder where it already lives.
- **Removing projects works.** The sidebar "Remove from list" (right-click) no longer no-ops on the native `confirm()`; and **Internal** projects in the Projects panel now have a two-click "Remove from list". Both only drop the registry entry — the folder stays on disk.

## [1.8.5] - 2026-06-24

### Added
- **App-wide debug log** — every frontend `console.{log,info,warn,error}` plus uncaught errors and unhandled promise rejections are captured to `~/Library/Application Support/xnaut/debug.log` (size-capped). One readable file for diagnosing issues without opening DevTools; `xnautDebugLogPath()` / `xnautDebugLogClear()` hooks.

### Fixed
- **Browser address bar** — pressing Enter to navigate now `stopPropagation()`s so the keystroke isn't swallowed by global keyboard handlers; navigation errors surface on the input instead of being silently dropped.

## [1.8.4] - 2026-06-24

### Added — Unified Projects & per-project tasks
- **One project area.** The PM panel is now **"Projects"** and lists every project (from the Tasks registry), badged **Internal** (gray) or **Client** (mint). A project opened via "Open as project" lands here as Internal automatically — no separate list.
- **Per-project tasks/reminders** — add, check off, delete; backed by a central store (`project-todos.json`, keyed by task id). Available in two places sharing the same data: the **Projects detail** and the **right-pane Task List** (4th icon), which now has an add box and a Plan Mode shortcut.
- **Plan Mode for any project** — the chat + live `PLAN.md` workspace is reachable from every project, not just Client/PM ones.
- **"Move to external →"** button on Internal projects opens the intake (company/rate/contacts) and flips them to Client; "Remove from PM" reverts a Client project to Internal (tasks intact).
- **Projects top-bar icon** — a clipboard-check icon opens the Projects panel directly.

## [1.8.3] - 2026-06-24

### Added
- **Browser panes open local files** — the address bar now accepts `file://` URLs and bare absolute / `~/` paths (e.g. `~/…/preflight-report.html`), so local HTML (reports, docs) renders inside xNaut instead of only http(s).
- **Pre-Flight Check harness** (`scripts/preflight.mjs`, `just preflight`) — a standalone health/regression check that verifies build, tests, ACL command coverage, JS syntax, version consistency, CSP, and live services (CDN/LLM/Engram/Forge), then writes `preflight-report.html`.

## [1.8.2] - 2026-06-24

### Added
- **Open existing project** — right-click a folder in the right-pane tree → "Open as project" (registers it and switches to its workspace); the chat agent also gained an `open_project` action.
- **Drag a file/folder** from the right-pane tree onto a terminal to insert its path (folder single-click no longer auto-injects).
- **Opt+B / Opt+M** keybindings to split the focused terminal with a browser / markdown pane (the Cmd+Alt menu accelerators weren't firing through the webview).

### Fixed
- Forge Tasks 404 — `forge_list_issues`/`get_issue` now normalize the repo input (full clone URL, `owner/repo`, or `repo.git`) to a bare repo name before building the API path.

## [1.8.1] - 2026-06-24

### Added — Project workspaces, Plan Mode & a dependency-free Markdown stack

**Project-scoped tabs (Orca/CMUX model)** — every tab now belongs to a project workspace. Selecting a project card on the left shows only that project's tabs at the top; the global views (Tasks/Automations/PM/Search) live in a shared **Home** workspace. Clicking a project switches to its existing tabs (restoring the last-active one) and only creates a tab the first time — a terminal `cd`'d into the project folder, or an attach to its zellij session. The selected project card gets a mint highlight, the nav highlight clears while a project is active, and the status dot now means "has open tabs in this session."

**Plan Mode** — a two-pane planning workspace launched from a PM project ("Plan Mode" button): chat on the left (solution-architect persona, Engram-grounded), a live `PLAN.md` document on the right. The agent maintains the document only in the right pane (wrapped in `===PLAN DOCUMENT===` sentinels so embedded code/diagrams survive); the chat stays conversational. The doc pane has an **Edit/Preview** toggle and is fed back to the agent each turn so it extends the current content instead of restarting.

**Dependency-free Markdown** — the markdown editor and Plan doc no longer use the CDN TipTap editor (which fails to load in this WebKit). New shared renderer `markdown-render.js` (`window.xnautMarkdown`) powered by **marked** (UMD): GFM tables/task-lists, **raw-HTML passthrough**, **highlight.js** syntax highlighting, and **Mermaid** diagrams. Opening a `.md` file shows a rendered document with an Edit/Preview toggle and `Cmd+S` save.

**Quality-of-life**
- `Cmd +/-/0` font zoom — terminals *and* markdown docs (context-aware).
- Neon-mint pulsing-X "thinking" spinner, shown in chat while the model generates and on agent status pills.
- Persistent chat history (localStorage, keyed by project) surviving tab close + restart.
- Copy button on every chat message.
- Right-pane root picker — click the Files icon to switch the tree between Home, Project Root, and the current project.
- New-tab (+) moved to the far left of the tab bar as a mint circle.
- PM: create a project inline from the intake dropdown ("+ New project…"), dialog-free two-click "Remove from PM", briefcase icon for the PM nav row, dedup-by-name on create.
- New "Generate Plan" German Projektplan document template.

### Fixed
- LLM streaming aborted long generations after 60s — removed the overall request timeout on the streaming client (kept a 15s connect timeout); raised doc-generation to a 300s cap.
- Native `prompt()` / `confirm()` are no-ops in Tauri's WebKit — replaced the PM new-project and remove flows with inline UI.
- Stale project path after a folder move (right pane "Path does not exist") and manual projects opening a `~` shell instead of their folder.

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
