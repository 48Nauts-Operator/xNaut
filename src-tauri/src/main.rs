// ABOUTME: Entry point for xNAUT Tauri application - initializes state, registers commands, and starts the app.
// ABOUTME: Displays ASCII art on startup and sets up all event handlers for PTY sessions and terminal management.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_hooks;
mod agents;
mod ai;
mod browser;
mod chat;
mod commands;
mod engram;
mod errors;
mod forges;
mod gitops;
mod pty;
mod ralph;
mod scaffold;
mod scheduler;
mod search;
mod settings;
mod ssh;
mod state;
mod status;
mod tasks;
mod triggers;
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
║              Version 1.5.0                                        ║
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
        ])
        .setup(|app| {
            // Build native macOS menu
            let about_metadata = AboutMetadataBuilder::new()
                .version(Some("1.5.0"))
                .short_version(Some("1.3"))
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
            let view_menu = SubmenuBuilder::new(app, "View")
                .fullscreen()
                .separator()
                .item(&split_right)
                .item(&split_down)
                .item(&split_browser)
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
                    _ => {}
                }
            });

            // Kick off the agent-status decay task (Phase 4).
            status::spawn_decay_task(app.handle().clone());

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
