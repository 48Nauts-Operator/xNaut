// ABOUTME: Entry point for xNAUT Tauri application - initializes state, registers commands, and starts the app.
// ABOUTME: Displays ASCII art on startup and sets up all event handlers for PTY sessions and terminal management.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_hook_setup;
mod claims;
mod tool_support;
mod ledger;
mod veto;
mod agent_hooks;
mod agent_notes_broker;
mod agent_profiles;
mod agent_tools;
mod agents;
mod ai;
mod audit;
mod beacon;
mod browser;
mod build_dag;
mod flow_context;
mod flow_drift;
mod canvas;
mod build_log;
mod chat;
mod codex_spend;
mod commands;
mod core_team;
mod composer;
mod debug_log;
mod decisions;
mod designer;
#[cfg(unix)]
mod designer_local;
mod diff;
mod docsgen;
mod engram;
mod errors;
mod evidence;
mod forges;
mod foundation;
mod gate_score;
mod gitops;
mod graph;
mod handback;
mod heartbeat;
mod housekeeper;
mod incidents;
mod throughput;
mod subdivide;
mod memory;
mod swarm;
mod swarm_plan;
mod inbox;
mod loops;
mod markers;
mod mcp;
mod mcp_client;
mod mobile;
mod nautloom;
mod notes;
mod merge_gate;
mod release_gate;
mod nudge;
mod spend;
mod sweep;
mod run_control;
mod run_signals;
mod jury;
mod jury_runtime;
mod jury_signoff;
#[cfg(test)]
mod jury_proof;
mod switches;
mod xfusion;
mod plan_review;
mod plateau;
mod preflight;
mod plugins;
mod policy;
mod pm;
mod project_management;
mod project_todos;
mod push;
mod pty;
mod repo_check;
mod research;
mod sandbox;
mod dispatch;
mod sandbox_verify;
mod scaffold;
mod scheduler;
mod search;
mod seal;
mod secrets;
mod review_gate;
mod settings;
mod shared_notes;
mod skills;
mod slice_diff;
mod ssh;
mod state;
mod status;
mod tasks;
mod ticket_triage;
mod transcripts;
mod usage;
mod vault;
mod vault_workflows;
mod voice;
mod delivery;
mod vault_tools;
mod worklog;
mod worklog_sources;
mod workspace;
mod writer_lease;
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
║              Version {version}                                        ║
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
    println!(
        "{}",
        XNAUT_ASCII.replace("{version}", env!("CARGO_PKG_VERSION"))
    );
    println!("🚀 xNAUT is starting...\n");
}

#[tokio::main]
async fn main() {
    // A release app launched by launchd/open can hold a CLOSED stdout, and
    // Rust's print! panics on the broken pipe — with panic=abort that killed
    // the whole app on its first log line (tron, 2026-08-31; same signature
    // in rust-panics.log since 08-18). Dev builds keep their pipes: cargo
    // tauri dev reads them.
    #[cfg(not(debug_assertions))]
    unsafe {
        if libc::isatty(1) == 0 {
            let devnull = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
            if devnull >= 0 {
                libc::dup2(devnull, 1);
                libc::dup2(devnull, 2);
            }
        }
    }

    // A panic in a spawned tokio task kills that task SILENTLY (since
    // panic=abort was removed in 1.8.9, the app keeps running with dead
    // tasks — the "frozen but alive" state seen 2026-07-13: dead IPC bridge,
    // idle threads, hook server accepting but never answering). Record every
    // panic with thread + file:line to rust-panics.log so the next freeze
    // names its culprit. Writes directly to disk — immune to a dead bridge.
    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".into());
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "<non-string panic payload>".into()
        };
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        let line = format!(
            "{} PANIC [thread {}] at {}: {}\n",
            chrono::Utc::now().to_rfc3339(),
            thread,
            loc,
            msg
        );
        eprintln!("{line}");
        if let Some(dir) = dirs::data_dir() {
            let p = dir.join("xnaut");
            let _ = std::fs::create_dir_all(&p);
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p.join("rust-panics.log"))
            {
                use std::io::Write;
                let _ = f.write_all(line.as_bytes());
            }
        }
    }));

    // Print startup banner
    print_startup_banner();

    // Initialize application state
    let app_state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Gives the update banner a working "Restart now" (XNAUT-70).
        .plugin(tauri_plugin_process::init())
        .manage(app_state)
        .manage(browser::BrowserPaneRegistry::new())
        .manage(notes::NotesWatcher::new())
        .manage(vault::VaultManager::default())
        .invoke_handler(tauri::generate_handler![
            // The hook that can refuse a tool call (XNAUT-132).
            ledger::ledger_recent,
            sweep::sweep_fleet_report,
            evidence::evidence_arguments,
            evidence::evidence_sessions,
            evidence::evidence_records,
            evidence::evidence_verify,
            evidence::evidence_rotate_kek,
            evidence::evidence_kek_label,
            evidence::evidence_shred,
            evidence::ticket_evidence,
            evidence::unattributed_sessions,
            chat::chat_send_tools,
            tool_support::model_tool_support,
            tool_support::model_tool_support_reset,
            veto::veto_rules,
            veto::veto_check,
            veto::veto_read,
            veto::veto_validate,
            veto::veto_write,
            veto::veto_backups,
            voice::voice_start,
            voice::voice_stop,
            voice::voice_model_ready,
            // Terminal session management
            commands::create_terminal_session,
            commands::create_command_session,
            commands::write_to_terminal,
            commands::resize_terminal,
            commands::create_durable_command_session,
            commands::close_terminal,
            commands::list_terminal_sessions,
            commands::terminal_output_snapshot,
            // AI integration
            commands::ask_ai,
            commands::analyze_output,
            // ClawProxy privacy monitor
            commands::check_clawproxy,
            commands::start_clawproxy,
            commands::get_privacy_alerts,
            commands::get_privacy_stats,
            // SSH support
            commands::create_ssh_session,
            commands::write_to_ssh,
            commands::resize_ssh,
            commands::close_ssh_session,
            commands::list_ssh_sessions,
            commands::get_ssh_config_hosts,
            // File Navigator + Editor
            commands::list_directory,
            commands::read_file,
            commands::write_file,
            commands::get_home_directory,
            commands::get_current_directory,
            commands::get_git_info,
            commands::repo_web_url,
            usage::max_usage,
            usage::max_accounts,
            usage::codex_usage,
            // Ralph Ultra integration
            // Work session logging & proof
            worklog::worklog_start,
            worklog::worklog_log,
            worklog::worklog_stop,
            worklog::worklog_status,
            worklog::worklog_orphans,
            worklog::worklog_resume,
            worklog::worklog_discard,
            worklog::worklog_summary,
            worklog::worklog_qr,
            worklog::worklog_verify,
            worklog::worklog_list,
            worklog::worklog_export_html,
            worklog::worklog_save_report,
            worklog::worklog_report_range,
            // Worktree-per-agent (Phase 2 of Orca port)
            worktree::worktree_list,
            worktree::worktree_add,
            worktree::worktree_remove,
            worktree::worktree_suggest_path,
            worktree::repo_bootstrap,
            // Reclaiming the disk those worktrees fill (XNAUT-264)
            housekeeper::housekeeper_report,
            housekeeper::housekeeper_reclaim,
            housekeeper::housekeeper_disk,
            // Agent registry + launch dispatch (Phase 3 of Orca port)
            agents::agent_list,
            agents::agent_headless_command,
            agents::agent_launch,
            agents::nautgate_max_launch_register,
            agents::agent_run_output,
            agents::agent_session_attach,
            agents::agent_session_alive,
            agents::agent_registry_path,
            agents::agent_registry_rollback,
            agents::agent_registry_drift,
            agent_profiles::agent_profiles_seed,
            agent_profiles::agent_profiles_list,
            agent_profiles::agent_profile_read,
            agent_profiles::agent_profile_save,
            agent_profiles::agent_profile_delete,
            agent_profiles::agent_profile_catalog,
            agent_profiles::agent_profile_test,
            agent_profiles::agent_profile_list,
            agent_profiles::agent_profile_notes,
            agent_profiles::agent_profile_get,
            agent_profiles::agent_profile_create,
            agent_profiles::agent_profile_update,
            agent_profiles::agent_profile_duplicate,
            agent_profiles::agent_profile_launch,
            agent_profiles::agent_remote_sessions,
            agent_profiles::agent_remote_attach,
            agent_profiles::agent_chat_turn,
            agent_profiles::agent_build_workspace,
            research::research_status,
            research::research_brief,
            core_team::core_team_status,
            core_team::core_team_scan,
            plugins::plugin_catalog,
            plugins::plugin_save,
            plugins::plugin_connect,
            canvas::canvas_get,
            canvas::canvas_set,
            canvas::canvas_undo,
            canvas::document_get,
            canvas::document_set,
            canvas::document_save_to_vault,
            plugins::plugin_delete,
            plugins::exe_machines,
            agent_profiles::agent_project_prepare,
            agent_profiles::agent_scratch_workspace,
            foundation::foundation_prompt,
            foundation::foundation_set_override,
            // Mesh — the human inbox (XNAUT-156)
            inbox::inbox_list,
            inbox::inbox_answer,
            inbox::inbox_decide,
            inbox::inbox_set_status,
            inbox::inbox_bulk,
            inbox::inbox_post,
            // Agent status overlay (Phase 4 of Orca port)
            status::agent_sessions_list,
            status::agent_session_interrupt,
            // Agent hook listener (Phase 5 of Orca port)
            agent_hooks::agent_hooks_url,
            agent_hooks::project_mcp_info,
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
            policy::policy_enforcement,
            skills::skill_path,
            skills::skill_catalog,
            skills::skill_for_artifact,
            skills::skill_artifact_map,
            skills::skill_write,
            skills::skill_import,
            skills::skill_delete,
            skills::skill_read,
            skills::skill_favourite,
            skills::skill_favourites,
            skills::skill_list,
            // Tasks Mode v1.6 — settings
            settings::settings_get,
            settings::settings_set,
            // Mobile companion bridge (XNAUT-32)
            mobile::mobile_info,
            // Tasks Mode v1.6 — chat panel
            chat::chat_send,
            chat::chat_send_model,
            chat::chat_send_provider,
            chat::chat_check_endpoint,
            chat::chat_list_models,
            chat::chat_list_provider_models,
            chat::net_probe,
            chat::net_fetch_json,
            mcp::mcp_list_tools,
            mcp::mcp_call_tool,
            mcp::mcp_start_local_excalidraw,
            mcp::mcp_stop_local_excalidraw,
            // Tasks Mode v1.6 — Engram brain
            engram::engram_status,
            engram::engram_store_learning,
            engram::engram_run_learning_loop,
            // Tasks Mode v1.6 — forges (Forgejo/GitHub/GitLab)
            forges::forge_list_issues,
            forges::forge_get_issue,
            forges::forge_add_issue_comment,
            forges::forge_create_pr,
            forges::forge_hosts,
            // Tasks Mode v1.6 — zellij
            zellij::zellij_check,
            zellij::zellij_sessions,
            zellij::zellij_live_sessions,
            codex_spend::codex_spend,
            build_dag::dag_step,
            build_dag::dag_validate,
            flow_context::flow_context,
            flow_context::flow_context_data,
            flow_drift::flow_drift,
            review_gate::review_gate,
            build_dag::slice_outputs_check,
            build_log::build_log_append,
            build_log::build_log_read,
            build_log::build_log_list,
            decisions::decision_log_append,
            decisions::decision_log_brief,
            decisions::decision_log_summarize,
            slice_diff::slice_changes,
            slice_diff::slice_file_diff,
            gate_score::gate_score_run,
            plateau::plateau_check,
            preflight::preflight_checks,
            markers::resolve_marker,
            shared_notes::shared_notes_link,
            shared_notes::shared_notes_list,
            zellij::zellij_delete_session,
            zellij::zellij_prune_exited,
            zellij::zellij_sessions_info,
            zellij::zellij_open_command,
            // Tasks Mode v1.6 — text search (rg + git-grep fallback)
            search::search_text,
            // Tasks Mode v1.6 — git pane
            gitops::git_ahead_behind,
            gitops::git_outgoing_files,
            gitops::git_uncommitted_files,
            gitops::git_worktree_list,
            gitops::git_commit_diff,
            gitops::git_ticket_files,
            gitops::git_outgoing_commits,
            gitops::git_commit_log,
            gitops::git_release_history,
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
            audit::audit_list,
            repo_check::repo_preflight,
            repo_check::project_facts,
            repo_check::projects_activity,
            tasks::tasks_list,
            tasks::tasks_create_project,
            tasks::project_create,
            tasks::task_remove,
            scaffold::scaffold_init_project,
            scaffold::scaffold_init_task,
            scaffold::scaffold_promote_task,
            scaffold::scaffold_task_from_issue,
            project_management::pm_module_status,
            project_management::pm_module_initialize,
            project_management::pm_module_connect,
            project_management::pm_module_sync,
            project_management::pm_project_list,
            project_management::pm_project_import_existing,
            project_management::pm_project_create,
            project_management::pm_project_update,
            project_management::pm_ticket_list,
            project_management::pm_ticket_create,
            project_management::pm_ticket_update,
            project_management::pm_ticket_delete,
            project_management::pm_event_list,
            project_management::pm_ticket_owner_history,
            // Loops — versioned visual workflow runtime
            loops::loops_workflow_validate,
            loops::loops_workflow_audit,
            loops::loops_permissions_evaluate,
            loops::loops_workflow_seed_delivery,
            loops::loops_workflow_list,
            loops::loops_workflow_get,
            loops::loops_workflow_save,
            loops::loops_workflow_activate,
            loops::loops_workflow_deactivate,
            loops::loops_workflow_clone,
            loops::loops_workflow_record_review,
            loops::loops_run_start,
            loops::loops_run_list,
            loops::loops_run_get,
            loops::loops_run_events,
            loops::loops_run_claim_node,
            loops::loops_run_complete_node,
            loops::loops_run_fail_node,
            loops::loops_run_approve,
            loops::loops_run_resume,
            loops::loops_run_cancel,
            loops::loops_run_reconcile,
            // Local-model ticket triage workflow
            ticket_triage::ticket_triage_run,
            ticket_triage::ticket_triage_decide,
            ticket_triage::ticket_triage_records,
            // Sandbox verify (XNAUT-19)
            dispatch::pm_ticket_dispatch,
            swarm_plan::swarm_plan_dispatch,
            project_management::pm_ticket_tag,
            project_management::pm_ticket_release,
            sandbox_verify::sandbox_verify_start,
            sandbox_verify::sandbox_verify_records,
            gitops::git_release_notes,
            delivery::delivery_lifecycle,
            run_control::run_detail,
            run_control::run_registry_list,
            memory::memory_index_list,
            memory::memory_find_cmd,
            memory::memory_note_read,
            memory::memory_recall_for_ticket,
            memory::memory_stats,
            sandbox_verify::loops_run_sandbox_node,
            // Vault knowledge-graph + code dependency graph
            graph::graph_scan,
            graph::code_scan,
            // Designer (XNAUT-61) — real builds in a sandbox
            designer::designer_list,
            designer::designer_create,
            designer::designer_get,
            designer::designer_rename,
            designer::designer_set_archived,
            designer::designer_append_message,
            designer::designer_publish,
            designer::designer_set_session,
            designer::designer_spin_up,
            designer::designer_adopt_local,
            designer::designer_renew,
            designer::designer_set_runtime,
            designer::designer_stop,
            vault::vault_init,
            vault::vault_open,
            vault::vault_close,
            vault::vault_tree,
            workspace::workspace_agentic_items,
            workspace::workspace_sessions,
            nautloom::looms_list,
            nautloom::loom_read,
            nautloom::loom_write,
            nautloom::looms_seed_defaults,
            nautloom::loom_run_record,
            nautloom::loom_runs_list,
            nautloom::loom_run_mark,
            nautloom::loom_run,
            nautloom::loom_run_stop,
            nautloom::loom_run_alive,
            nautloom::agent_alive_in,
            nautloom::static_serve,
            nautloom::loom_report,
            nautloom::loom_ship,
            nautloom::loom_sandbox_stats,
            vault::vault_note_read,
            vault::vault_note_write,
            vault_workflows::vault_generate_diagram,
            vault_workflows::vault_document_workflow,
            vault::vault_note_create,
            vault::vault_folder_create,
            vault::vault_folder_move,
            vault::vault_folder_delete,
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
            debug_log::debug_log_reveal,
            debug_log::debug_log_tail,
            // Per-project to-do / reminders
            nudge::agent_nudge,
            spend::spend_ceiling_get,
            spend::spend_ceiling_set,
            switches::kill_switches_get,
            switches::kill_switches_set,
            project_todos::project_todos_list,
            project_todos::project_todos_add,
            project_todos::project_todos_toggle,
            project_todos::project_todos_remove,
            // PM Space v1.7 — client document generation
            docsgen::docgen_templates,
            docsgen::docgen_generate,
        ])
        .setup(|app| {
            // Credentials and evidence live here; nobody else on this machine
            // needs read access. Idempotent, and it also closes files written
            // by earlier versions (XNAUT-213).
            if let Some(dir) = dirs::config_dir().map(|d| d.join("xnaut")) {
                secrets::harden(&dir);
            }

            // Say why NautBot will not work on THIS machine, now, rather than
            // one silent refusal at a time over the following days. Off-thread
            // because it reads settings and the agent registry and the window
            // has no reason to wait for either.
            std::thread::spawn(preflight::run_at_boot);

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

            // `mut` is only used by the macOS-only reassignment below; on other
            // targets that cfg block is stripped, leaving it unused.
            #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
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
            // Refresh the `open` shim at startup, not only when an agent
            // launches. A stale copy on disk is why a markdown file still went
            // to Xcode after the shim learned to handle documents.
            let _ = agents::browser_shim_dir();

            // Tasks Mode v1.6: automation scheduler tick.
            nudge::set_app(app.handle().clone());
            scheduler::spawn_scheduler_task(app.handle().clone());
            // The durable sweep (XNAUT-239): the board is worked on its own
            // clock, not only inside a chat turn.
            // The safety net (XNAUT-264): correct the records a dead app left
            // claiming to be running, and hand those tickets to the sweep so
            // it finishes them before starting anything new.
            sweep::queue_retries(sandbox_verify::reap_orphaned_runs());
            sweep::spawn_sweep_task(app.handle().clone());

            // Runs that outlived the last app (XNAUT-242): put them back on
            // the board before anything else asks "who is working".
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = tauri::Manager::state::<state::AppState>(&handle);
                    status::adopt_surviving_runs(&state.agent_sessions, &handle).await;
                    // …and the ones that outlived it on ANOTHER machine
                    // (XNAUT-266). Local first because it is instant; the
                    // remote pass shells ssh per environment and must not
                    // hold the board empty while it does.
                    status::adopt_remote_runs(&state.agent_sessions, &handle).await;
                });
            }

            // Daily consolidation of verified ticket learnings for all agents.
            engram::spawn_daily_learning_task(app.handle().clone());
            memory::spawn_backfill();

            // Optional local-model triage for configured forge repositories.
            ticket_triage::spawn_auto_triage_task(app.handle().clone());

            // Phase 5: start the local hook listener so agents can push state.
            let app_for_hooks = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use std::collections::HashMap;
                let tokens: agent_hooks::HookTokenMap =
                    std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));

                // Persistent MCP coords: a fixed port + a stable token minted
                // once and saved, so the URL/token pasted into claude/codex
                // configs keep working across app restarts.
                let mut settings = crate::settings::load_or_default();
                if settings.mcp_token.trim().is_empty() {
                    settings.mcp_token = uuid::Uuid::new_v4().to_string();
                    if let Err(e) = crate::settings::save(&settings) {
                        eprintln!("[agent_hooks] could not persist mcp_token: {e}");
                    }
                }
                let mcp_port = settings.mcp_port;
                let mcp_token = settings.mcp_token.clone();
                match agent_hooks::start_server(
                    app_for_hooks.clone(),
                    tokens.clone(),
                    mcp_port,
                    mcp_token,
                )
                .await
                {
                    Ok((url, mcp_token)) => {
                        if let Some(s) = app_for_hooks.try_state::<AppState>() {
                            let mut slot = s.hook_server.lock().await;
                            *slot = Some(agent_hooks::HookServerInfo {
                                url: url.clone(),
                                tokens,
                                mcp_token,
                            });
                            println!("✓ Agent hook listener at {url}");
                        }
                    }
                    Err(e) => eprintln!("[agent_hooks] failed to start: {e}"),
                }
            });

            // XNAUT-32: mobile companion bridge on a fixed, persisted port.
            // Config lives in mobile.json (NOT settings.json — see mobile.rs).
            let app_for_mobile = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let cfg = mobile::load_or_init_config();
                if !cfg.enabled {
                    return;
                }
                match mobile::start_server(app_for_mobile.clone(), cfg.port, cfg.token).await {
                    Ok(port) => println!("✓ Mobile bridge on port {port}"),
                    Err(e) => eprintln!("[mobile] failed to start: {e}"),
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if matches!(event, tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }) {
                let _ = mcp::stop_local_excalidraw_process();
            }
        });
}

#[cfg(test)]
mod acl_audit {
    /// Every permission block must be granted somewhere, or its commands are
    /// dead on arrival.
    ///
    /// `allow-evidence` was the only one of 92 blocks in neither the
    /// allow-all-commands set nor capabilities/default.json, so the evidence
    /// chain's five commands were the only five of 349 the ACL refused, while
    /// delivery-panel.js rendered an Evidence tab calling four of them. Every
    /// call failed at the ACL rather than in the code, which is silent by
    /// design and exactly the failure this project keeps paying for.
    ///
    /// The voice trio is granted directly in capabilities/default.json rather
    /// than through the set, which is why this checks both.
    #[test]
    fn every_declared_permission_is_actually_granted() {
        let toml = include_str!("../permissions/default.toml");
        let caps = include_str!("../capabilities/default.json");
        let set_body = toml
            .split_once("identifier = \"allow-all-commands\"")
            .and_then(|(_, rest)| rest.split_once("permissions = ["))
            .and_then(|(_, rest)| rest.split_once(']'))
            .map(|(body, _)| body.to_string())
            .expect("the allow-all-commands set exists");

        let mut stranded = Vec::new();
        for line in toml.lines() {
            let Some(id) = line
                .strip_prefix("identifier = \"")
                .and_then(|rest| rest.strip_suffix('"'))
            else {
                continue;
            };
            if !id.starts_with("allow-") || id == "allow-all-commands" {
                continue;
            }
            let quoted = format!("\"{id}\"");
            if !set_body.contains(&quoted) && !caps.contains(&quoted) {
                stranded.push(id.to_string());
            }
        }
        assert!(
            stranded.is_empty(),
            "these permissions are declared but granted nowhere, so their commands \
             are blocked by the ACL at runtime with no compile-time sign: {stranded:?}"
        );
    }
}
