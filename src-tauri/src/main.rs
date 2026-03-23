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

use state::AppState;
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
║              Version 1.2.0                                        ║
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
            // SSH support
            commands::create_ssh_session,
            commands::write_to_ssh,
            commands::list_ssh_sessions,
            commands::get_ssh_config_hosts,
            // File Navigator
            commands::list_directory,
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
        ])
        .setup(|_app| {
            println!("✓ State initialized");
            println!("✓ Commands registered");
            println!("✓ Event handlers ready");
            println!("\n🎉 xNAUT is ready!");
            println!("💡 Right-click anywhere in the app and select 'Inspect Element' to open DevTools\n");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
