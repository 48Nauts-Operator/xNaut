//! XNAUT-467: an explicitly compiled debug harness runs native coordination
//! against a marked scratch profile. Production builds cannot activate it.
use std::path::{Path, PathBuf};
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
    // Keep this a runtime refusal: ordinary builds need not enable previews.
    match crate::FULL_WIKI_PREVIEW && crate::JOURNAL_PREVIEW {
        true => {},
        false => panic!("Loop acceptance requires both full-wiki and journal preview build flags"),
    }
    let root = root();
    // Git must use existing trust without modifying the owner's SSH files or sockets.
    std::env::set_var("GIT_SSH_COMMAND","ssh -o StrictHostKeyChecking=yes -o UpdateHostKeys=no -o ControlMaster=no -o ControlPath=none");
    for name in [
        "config",
        "config-base",
        "data-base",
        "vault",
        "agent-runs",
        "layouts",
        "config/settings.json",
        "config/mobile.json",
        "config/agent-profiles.toml",
    ] {
        checked_child(&root, name).expect("Isolated state path must remain inside acceptance root");
    }
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
        ("XNAUT_VAULT_ROOT", "vault"),
    ] {
        let path = checked_child(&root, name)
            .expect("Isolated environment path must remain inside acceptance root");
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
    let _: crate::settings::Settings = serde_json::from_value(value.clone())
        .expect("Acceptance settings must parse without fallback defaults");
    validate_settings(&root, &value).expect(
        "Acceptance settings must disable unrelated automation and use isolated ports/paths",
    );
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

fn validate_settings(root: &Path, value: &serde_json::Value) -> Result<(), String> {
    let path = value["project_root"]
        .as_str()
        .ok_or("Missing isolated project root")?;
    if !Path::new(path)
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root)
    {
        return Err("Project root escapes acceptance state".into());
    }
    let port = value["mcp_port"]
        .as_u64()
        .ok_or("Distinct MCP port required")?;
    if !(1024..=65535).contains(&port) || port == 51737 {
        return Err("Acceptance must not bind owner's default MCP port".into());
    }
    for key in ["core_team", "foreign_session_reaper", "engram"] {
        if value[key]["enabled"].as_bool() == Some(true) {
            return Err(format!("Acceptance disables unrelated {key} automation"));
        }
    }
    if value["project_management"]["remote_url"]
        .as_str()
        .is_some_and(|v| !v.is_empty())
    {
        return Err("Acceptance PM cannot synchronize a remote board".into());
    }
    let mobile: serde_json::Value = serde_json::from_slice(
        &std::fs::read(checked_child(root, "config/mobile.json")?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mobile_port = mobile["port"]
        .as_u64()
        .ok_or("Distinct mobile port required")?;
    if !(1024..=65535).contains(&mobile_port) || mobile_port == 8931 || mobile_port == port {
        return Err("Acceptance mobile port must be distinct".into());
    }
    if mobile["token"].as_str().is_none_or(|s| s.len() < 24) {
        return Err("Acceptance mobile bridge requires its own token".into());
    }
    Ok(())
}

/// Shared app-owned roots. Provider binaries/auth remain explicit read-only dependencies.
pub(crate) fn platform_config_dir() -> Option<PathBuf> {
    if ENABLED {
        Some(root().join("config-base"))
    } else {
        dirs::config_dir()
    }
}
pub(crate) fn platform_data_dir() -> Option<PathBuf> {
    if ENABLED {
        Some(root().join("data-base"))
    } else {
        dirs::data_dir()
    }
}
pub(crate) fn platform_data_local_dir() -> Option<PathBuf> {
    if ENABLED {
        Some(root().join("data-base"))
    } else {
        dirs::data_local_dir()
    }
}

fn checked_child(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Invalid acceptance state name".into());
    }
    let path = root.join(name);
    let mut ancestor = path.as_path();
    while !ancestor.exists() {
        // Broken links must not become a later escape once their target appears.
        if ancestor.symlink_metadata().is_ok() {
            return Err("Broken acceptance state link".into());
        }
        ancestor = ancestor.parent().ok_or("No existing state ancestor")?;
    }
    if !ancestor
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root)
    {
        return Err("Acceptance state escapes its marked root".into());
    }
    Ok(path)
}

pub(crate) fn refuse_local_worker() -> Result<(), String> {
    if ENABLED {
        Err("Loop acceptance permits only isolated remote repository workers; local CLI/trust writes are refused".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("xnaut-loop-isolation-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn scoped_paths_never_escape_or_follow_owner_links() {
        let tmp = Scratch::new();
        let root = tmp.path().canonicalize().unwrap();
        assert_eq!(
            checked_child(&root, "config-base/xnaut/debug.log").unwrap(),
            root.join("config-base/xnaut/debug.log")
        );
        for bad in ["../owner", "/owner", "", "config/../../owner"] {
            assert!(checked_child(&root, bad).is_err());
        }
        #[cfg(unix)]
        {
            let outside = Scratch::new();
            std::os::unix::fs::symlink(outside.path(), root.join("config")).unwrap();
            assert!(checked_child(&root, "config/settings.json").is_err());
            std::os::unix::fs::symlink(outside.path().join("missing"), root.join("broken"))
                .unwrap();
            assert!(checked_child(&root, "broken/file").is_err());
        }
    }
    #[test]
    fn startup_requires_own_ports_and_disables_unrelated_automation() {
        let tmp = Scratch::new();
        let root = tmp.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/mobile.json"),
            serde_json::json!({"port":18931,"token":"isolated-test-token-32-characters"})
                .to_string(),
        )
        .unwrap();
        let mut value = serde_json::json!({"project_root":root,"mcp_port":52737,"project_management":{"remote_url":""}});
        assert!(validate_settings(&root, &value).is_ok());
        value["mcp_port"] = serde_json::json!(51737);
        assert!(validate_settings(&root, &value).is_err());
        value["mcp_port"] = serde_json::json!(52737);
        value["core_team"] = serde_json::json!({"enabled":true});
        assert!(validate_settings(&root, &value).is_err());
    }
}

/// Native preparation seam for the isolated acceptance driver; approval and
/// dispatch still use the normal confirmed-plan command and admissions.
#[tauri::command]
pub fn loop_acceptance_plan(
    project: String,
    tickets: Vec<String>,
    environment: Option<String>,
) -> Result<crate::swarm_plan::SwarmPlan, String> {
    if !ENABLED {
        return Err("Native loop acceptance is not enabled in this build".into());
    }
    let repo = crate::project_management::repo_now()?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !repo.starts_with(root()) {
        return Err("Acceptance PM escaped its marked root".into());
    }
    let plan = crate::swarm_plan::build_with_environment(&project, &tickets, environment.as_deref())?;
    crate::swarm_plan::remember(plan.clone())?;
    Ok(plan)
}
