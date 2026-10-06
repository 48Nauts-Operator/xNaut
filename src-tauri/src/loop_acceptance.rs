//! XNAUT-467: an explicitly compiled debug harness runs native coordination
//! against a marked scratch profile. Production builds cannot activate it.
use std::path::PathBuf;
pub(crate) const ENABLED: bool =
    cfg!(debug_assertions) && option_env!("XNAUT_LOOP_ACCEPTANCE").is_some();
pub(crate) fn root() -> PathBuf {
    let root = std::env::var_os("XNAUT_LOOP_ACCEPTANCE_ROOT")
        .map(PathBuf::from)
        .expect("Loop acceptance requires XNAUT_LOOP_ACCEPTANCE_ROOT");
    assert!(root.is_absolute(), "Acceptance root must be absolute");
    let root = root
        .canonicalize()
        .expect("Acceptance root must already exist");
    assert!(
        root.join(".xnaut-loop-acceptance").is_file(),
        "Acceptance root must contain its explicit marker"
    );
    root
}
pub(crate) fn config() -> PathBuf {
    root().join("config")
}
pub(crate) fn initialize() {
    if !ENABLED {
        return;
    }
    let root = root();
    for (key, name) in [
        ("XNAUT_REGISTRY_DIR", "registry"),
        ("XNAUT_LEDGER_PATH", "ledger.jsonl"),
        ("XNAUT_SWITCHES_DIR", "switches"),
        ("XNAUT_AGENTS_PATH", "agents.json"),
        ("XNAUT_LEASE_DIR", "leases"),
        ("XNAUT_SPEND_DIR", "spend"),
        ("XNAUT_WORKLOG_DIR", "worklog"),
        ("XNAUT_WORKLOG_ROOT", "worklog"),
        ("XNAUT_EVIDENCE_DIR", "evidence"),
        ("XNAUT_PLUGINS_PATH", "plugins.json"),
        ("XNAUT_AUTOMATIONS_PATH", "automations.json"),
        ("XNAUT_INBOX_DIR", "inbox"),
        ("XNAUT_VERIFY_DIR", "verify"),
        ("ZELLIJ_SOCKET_DIR", "sockets"),
    ] {
        let path = root.join(name);
        if !name.contains('.') {
            std::fs::create_dir_all(&path).expect("Create isolated state");
        }
        std::env::set_var(key, path);
    }
    let config = config();
    let value: serde_json::Value = serde_json::from_slice(
        &std::fs::read(config.join("settings.json")).expect("Prepare acceptance settings first"),
    )
    .expect("Valid acceptance settings");
    let pm = value["project_management"]["repo_path"]
        .as_str()
        .expect("Isolated PM path required");
    assert!(
        std::path::Path::new(pm)
            .canonicalize()
            .expect("PM exists")
            .starts_with(&root),
        "Acceptance PM must be inside scratch root"
    );
    assert!(
        value["project_management"]["enabled"] == true,
        "Acceptance PM must be enabled"
    );
}
