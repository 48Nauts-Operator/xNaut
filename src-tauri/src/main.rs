// ABOUTME: Entry point for xNAUT Tauri application - initializes state, registers commands, and starts the app.
// ABOUTME: Displays ASCII art on startup and sets up all event handlers for PTY sessions and terminal management.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod commands;
mod errors;
mod pty;
mod ralph;
mod ssh;
mod state;
mod triggers;
mod worklog;

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
║              Version 1.4.0                                        ║
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
        ])
        .setup(|app| {
            // Build native macOS menu
            let about_metadata = AboutMetadataBuilder::new()
                .version(Some("1.4.0"))
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

            let view_menu = SubmenuBuilder::new(app, "View").fullscreen().build()?;

            let window_menu = SubmenuBuilder::new(app, "Window").close_window().build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_menu)
                .item(&edit_menu)
                .item(&view_menu)
                .item(&window_menu)
                .build()?;

            app.set_menu(menu)?;

            // Handle menu events
            app.on_menu_event(move |app_handle, event| {
                if event.id().0 == "preferences" {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.eval("toggleSettingsPanel()");
                    }
                }
            });

            println!("✓ State initialized");
            println!("✓ Commands registered");
            println!("✓ Native menu configured");
            println!("✓ Event handlers ready");
            println!("\n🎉 xNAUT is ready!\n");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
