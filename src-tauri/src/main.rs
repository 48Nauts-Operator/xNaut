// ABOUTME: Entry point for xNAUT Tauri application - initializes state, registers commands, and starts the app.
// ABOUTME: Displays ASCII art on startup and sets up all event handlers for PTY sessions and terminal management.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_hooks;
mod agent_notes_broker;
mod agents;
mod ai;
mod browser;
mod chat;
mod commands;
mod debug_log;
mod diff;
mod docsgen;
mod engram;
mod errors;
mod forges;
mod gitops;
mod graph;
mod notes;
mod plow;
mod pm;
mod project_todos;
mod pty;
mod ralph;
mod scaffold;
mod scheduler;
mod search;
mod settings;
mod skills;
mod ssh;
mod state;
mod status;
mod tasks;
mod triggers;
mod vault;
mod worklog;
mod worktree;
mod zellij;

use state::AppState;
use tauri::menu::{AboutMetadataBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::Manager;

const XNAUT_ASCII: &str = r#"
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
║              Version 1.7.0                                        ║
║                                                                   ║
║  Features:                                                        ║
║    ✓ Multiple PTY Sessions                                       ║
║    ✓ SSH Connection Support                                      ║
║    ✓ AI Terminal Assistant                                       ║
║    ✓ Smart Triggers & Notifications                              ║
║    ✓ Session Sharing                                             ║
║                                                                   ║
╚═══════════════════════════════════════════════════════════════════╝
"#;

fn print_startup_banner() {
    println!("{}", XNAUT_ASCII);
    println!("🚀 xNAUT is starting...\n");
}

#[tokio::main]
async fn main() {
    // Print startup banner
    print_startup_banner();

    // Initialize application state
    let app_state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .manage(browser::BrowserPaneRegistry::new())
        .manage(notes::NotesWatcher::new())
        .manage(vault::VaultManager::default())
        .invoke_handler(tauri::generate_handler![
            // Terminal session management
            commands::create_terminal_session,
            commands::create_command_session,
            commands::write_to_terminal,
            commands::resize_terminal,
            commands::close_terminal,
            commands::list_terminal_sessions,
            // Trigger management
            commands::create_trigger,
            commands::list_triggers,
            commands::delete_trigger,
            commands::toggle_trigger,
            // Session sharing
            commands::share_session,
            commands::join_shared_session,
            commands::unshare_session,
            // AI integration
            commands::ask_ai,
            commands::analyze_output,
            // AntBot local AI
            commands::check_antbot,
            commands::ask_antbot,
            commands::start_antbot_gateway,
            // ClawProxy privacy monitor
            commands::check_clawproxy,
            commands::start_clawproxy,
            commands::get_privacy_alerts,
            commands::get_privacy_stats,
            // SSH support
            commands::create_ssh_session,
            commands::write_to_ssh,
            commands::list_ssh_sessions,
            commands::get_ssh_config_hosts,
            // File Navigator + Editor
            commands::list_directory,
            commands::read_file,
            commands::write_file,
            commands::get_home_directory,
            commands::get_current_directory,
            commands::get_git_info,
            // Ralph Ultra integration
            ralph::ralph_read_prd,
            ralph::ralph_write_prd,
            ralph::ralph_backup_prd,
            ralph::ralph_list_backups,
            ralph::ralph_restore_backup,
            ralph::ralph_detect_clis,
            ralph::ralph_check_cli_health,
            ralph::ralph_run_ac_test,
            ralph::ralph_read_config,
            ralph::ralph_write_config,
            ralph::ralph_write_temp_file,
            ralph::ralph_cleanup_temp_file,
            // Work session logging & proof
            worklog::worklog_start,
            worklog::worklog_log,
            worklog::worklog_stop,
            worklog::worklog_status,
            worklog::worklog_summary,
            worklog::worklog_qr,
            worklog::worklog_verify,
            worklog::worklog_list,
            worklog::worklog_export_html,
            worklog::worklog_save_report,
            // Worktree-per-agent (Phase 2 of Orca port)
            worktree::worktree_list,
            worktree::worktree_add,
            worktree::worktree_remove,
            worktree::worktree_suggest_path,
            // Agent registry + launch dispatch (Phase 3 of Orca port)
            agents::agent_list,
            agents::agent_launch,
            agents::agent_registry_path,
            // Agent status overlay (Phase 4 of Orca port)
            status::agent_sessions_list,
            status::agent_session_interrupt,
            // Agent hook listener (Phase 5 of Orca port)
            agent_hooks::agent_hooks_url,
            // Browser panes (Phase 6 of Orca port)
            browser::browser_pane_create,
            browser::browser_pane_set_bounds,
            browser::browser_pane_set_visible,
            browser::browser_pane_navigate,
            browser::browser_pane_back,
            browser::browser_pane_forward,
            browser::browser_pane_reload,
            browser::browser_pane_destroy,
            browser::browser_pane_list,
            // Phase 8a — diff viewer + file-watched notes (hunk port)
            diff::diff_for_worktree,
            diff::diff_for_commit,
            diff::diff_against_ref,
            notes::notes_read,
            notes::notes_write,
            notes::notes_add,
            notes::notes_remove,
            notes::notes_clear,
            notes::notes_watch_start,
            notes::notes_watch_stop,
            // Phase 8c — bundled skill locator (mirror of `hunk skill path`)
            skills::skill_path,
            skills::skill_list,
            // Tasks Mode v1.6 — settings
            settings::settings_get,
            settings::settings_set,
            // Tasks Mode v1.6 — chat panel
            chat::chat_send,
            chat::chat_check_endpoint,
            chat::net_probe,
            chat::net_fetch_json,
            // Tasks Mode v1.6 — Engram brain
            engram::engram_status,
            // Tasks Mode v1.6 — forges (Forgejo/GitHub/GitLab)
            forges::forge_list_issues,
            forges::forge_get_issue,
            forges::forge_create_pr,
            forges::forge_hosts,
            // Tasks Mode v1.6 — zellij
            zellij::zellij_check,
            zellij::zellij_sessions,
            // Tasks Mode v1.6 — text search (rg + git-grep fallback)
            search::search_text,
            // Tasks Mode v1.6 — git pane
            gitops::git_ahead_behind,
            gitops::git_outgoing_files,
            gitops::git_uncommitted_files,
            gitops::git_outgoing_commits,
            gitops::git_file_diff,
            gitops::git_stage,
            gitops::git_unstage,
            gitops::git_commit,
            gitops::git_push,
            gitops::git_branches,
            gitops::git_ai_commit_message,
            // Tasks Mode v1.6 — automations
            scheduler::automation_list,
            scheduler::automation_save,
            scheduler::automation_delete,
            scheduler::automation_fire_now,
            // Tasks Mode v1.6 — task registry + scaffold
            tasks::tasks_list,
            tasks::tasks_create_project,
            tasks::task_remove,
            scaffold::scaffold_init_project,
            scaffold::scaffold_init_task,
            scaffold::scaffold_promote_task,
            scaffold::scaffold_task_from_issue,
            // PM Space v1.7 — external projects + financials
            pm::pm_list,
            pm::pm_get,
            pm::pm_save,
            pm::pm_delete,
            pm::pm_financials,
            // Vault knowledge-graph + code dependency graph
            graph::graph_scan,
            graph::code_scan,
            vault::vault_init,
            vault::vault_open,
            vault::vault_close,
            vault::vault_tree,
            vault::vault_note_read,
            vault::vault_note_write,
            vault::vault_note_create,
            vault::vault_note_move,
            vault::vault_note_delete,
            vault::vault_note_rename,
            vault::vault_backlinks,
            vault::vault_tags,
            vault::vault_tag_notes,
            vault::vault_search,
            vault::vault_sync,
            // App-wide debug log
            debug_log::debug_log_append,
            debug_log::debug_log_path,
            debug_log::debug_log_clear,
            // Per-project to-do / reminders
            project_todos::project_todos_list,
            project_todos::project_todos_add,
            project_todos::project_todos_toggle,
            project_todos::project_todos_remove,
            // PM Space v1.7 — Plow (lead tool) read-only bridge
            plow::plow_list_opportunities,
            plow::plow_get_opportunity,
            plow::plow_status,
            // PM Space v1.7 — client document generation
            docsgen::docgen_templates,
            docsgen::docgen_generate,
        ])
        .setup(|app| {
            // Build native macOS menu
            let about_metadata = AboutMetadataBuilder::new()
                .version(Some("1.7.0"))
                .short_version(Some("1.7"))
                .copyright(Some("© 2024-2026 48Nauts"))
                .website(Some("https://github.com/48Nauts-Operator/xNaut"))
                .website_label(Some("GitHub"))
                .build();

            let preferences = MenuItemBuilder::with_id("preferences", "Settings...")
                .accelerator("CmdOrCtrl+,")
                .build(app)?;

            let mut app_menu_builder = SubmenuBuilder::new(app, "xNAUT")
                .about(Some(about_metadata))
                .separator()
                .item(&preferences)
                .separator();

            #[cfg(target_os = "macos")]
            {
                app_menu_builder = app_menu_builder.hide().hide_others().show_all().separator();
            }

            let app_menu = app_menu_builder.quit().build()?;

            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;

            // View menu — splits land here with CmdOrCtrl+D / CmdOrCtrl+Shift+D
            // (iTerm2 convention). Frontend handlers are global in app.js.
            let split_right = MenuItemBuilder::with_id("split_right", "Split Right")
                .accelerator("CmdOrCtrl+D")
                .build(app)?;
            let split_down = MenuItemBuilder::with_id("split_down", "Split Down")
                .accelerator("CmdOrCtrl+Shift+D")
                .build(app)?;
            let split_browser = MenuItemBuilder::with_id("split_browser", "Split → Browser")
                .accelerator("CmdOrCtrl+Alt+B")
                .build(app)?;
            let split_markdown = MenuItemBuilder::with_id("split_markdown", "Split → Markdown")
                .accelerator("CmdOrCtrl+Alt+M")
                .build(app)?;
            let view_menu = SubmenuBuilder::new(app, "View")
                .fullscreen()
                .separator()
                .item(&split_right)
                .item(&split_down)
                .item(&split_browser)
                .item(&split_markdown)
                .build()?;

            // Window menu — CmdOrCtrl+W closes the *tab*, not the window.
            // CmdOrCtrl+Shift+W is the escape hatch for closing the window itself.
            // Without these explicit items Tauri's default close_window() grabs
            // Cmd+W and exits the app (one window == close window == quit).
            let close_tab = MenuItemBuilder::with_id("close_tab", "Close Tab")
                .accelerator("CmdOrCtrl+W")
                .build(app)?;
            let close_window = MenuItemBuilder::with_id("close_window_xnaut", "Close Window")
                .accelerator("CmdOrCtrl+Shift+W")
                .build(app)?;
            let window_menu = SubmenuBuilder::new(app, "Window")
                .item(&close_tab)
                .item(&close_window)
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_menu)
                .item(&edit_menu)
                .item(&view_menu)
                .item(&window_menu)
                .build()?;

            app.set_menu(menu)?;

            // Handle menu events
            app.on_menu_event(move |app_handle, event| {
                let Some(window) = app_handle.get_webview_window("main") else { return };
                match event.id().0.as_str() {
                    "preferences" => {
                        let _ = window.eval("toggleSettingsPanel()");
                    }
                    "close_tab" => {
                        // closeTab is global in app.js; activeTabId is its module-level state.
                        let _ = window.eval(
                            "if (typeof activeTabId !== 'undefined' && activeTabId) { closeTab(activeTabId); }",
                        );
                    }
                    "close_window_xnaut" => {
                        let _ = window.close();
                    }
                    "split_right" => {
                        let _ = window.eval("if (typeof splitPane === 'function') splitPane('vertical');");
                    }
                    "split_down" => {
                        let _ = window.eval("if (typeof splitPane === 'function') splitPane('horizontal');");
                    }
                    "split_browser" => {
                        let _ = window.eval("if (typeof splitPane === 'function') splitPane('vertical', 'browser');");
                    }
                    "split_markdown" => {
                        let _ = window.eval("if (typeof splitPane === 'function') splitPane('vertical', 'markdown');");
                    }
                    _ => {}
                }
            });

            // Kick off the agent-status decay task (Phase 4).
            status::spawn_decay_task(app.handle().clone());

            // Tasks Mode v1.6: automation scheduler tick.
            scheduler::spawn_scheduler_task(app.handle().clone());

            // Phase 5: start the local hook listener so agents can push state.
            let app_for_hooks = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use std::collections::HashMap;
                let tokens: agent_hooks::HookTokenMap =
                    std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                match agent_hooks::start_server(app_for_hooks.clone(), tokens.clone()).await {
                    Ok(url) => {
                        if let Some(s) = app_for_hooks.try_state::<AppState>() {
                            let mut slot = s.hook_server.lock().await;
                            *slot = Some(agent_hooks::HookServerInfo {
                                url: url.clone(),
                                tokens,
                            });
                            println!("✓ Agent hook listener at {url}");
                        }
                    }
                    Err(e) => eprintln!("[agent_hooks] failed to start: {e}"),
                }
            });

            println!("✓ State initialized");
            println!("✓ Commands registered");
            println!("✓ Native menu configured");
            println!("✓ Event handlers ready");
            println!("✓ Agent status decay task running");
            println!("\n🎉 xNAUT is ready!\n");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
