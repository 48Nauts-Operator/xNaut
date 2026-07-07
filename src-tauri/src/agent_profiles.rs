use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentAccess {
    pub read: Vec<String>,
    pub write: Vec<String>,
    pub denied: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub status: String,
    pub version: u32,
    pub role: String,
    pub skills: Vec<String>,
    pub access: AgentAccess,
    pub tools: Vec<String>,
    pub constraints: Vec<String>,
    pub outputs: Vec<String>,
    pub body: String,
    pub rel: String,
    pub built_in: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentCatalogItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub version: u32,
    pub role: String,
    pub rel: String,
    pub built_in: bool,
}

fn agent_profiles_dir(root: &Path) -> Result<PathBuf, String> {
    crate::vault::safe_join(root, "System/Agents")
}

pub fn built_in_profiles() -> Vec<AgentProfile> {
    vec![
        built_in_profile(
            "agentfather",
            "AgentFather",
            "agent orchestration",
            vec!["design-agent-profile", "coordinate-specialists"],
            access_preset("conservative"),
            vec!["create_agent_profile", "agent_profile_catalog"],
            vec![
                "Do not inspect source code.",
                "Do not run terminal commands.",
                "Create role profiles through approved storage only.",
            ],
            vec!["agent-profile"],
        ),
        built_in_profile(
            "librarian",
            "Librarian",
            "knowledge curation",
            vec!["organize-vault", "summarize-notes"],
            access_preset("vault_writer"),
            vec!["vault_note_read", "vault_note_write", "vault_search"],
            vec!["Preserve source attribution."],
            vec!["curated-note", "catalog-entry"],
        ),
        built_in_profile(
            "analyst",
            "Analyst",
            "analysis",
            vec!["research", "synthesize-findings"],
            access_preset("vault_reader"),
            vec!["vault_note_read", "vault_search"],
            vec!["Separate facts from assumptions."],
            vec!["analysis-brief"],
        ),
        built_in_profile(
            "pm",
            "PM",
            "project management",
            vec!["plan-roadmap", "manage-tasks"],
            access_preset("vault_writer"),
            vec!["tasks_list", "tasks_create_project", "vault_note_write"],
            vec!["Keep decisions traceable to project goals."],
            vec!["project-plan", "task-list"],
        ),
        built_in_profile(
            "architect",
            "Architect",
            "architecture",
            vec!["create-architecture", "review-design"],
            access_preset("vault_writer"),
            vec!["vault_note_read", "vault_note_write", "graph_scan"],
            vec!["Do not edit implementation code."],
            vec!["architecture-note"],
        ),
        built_in_profile(
            "security",
            "Security",
            "security review",
            vec!["threat-model", "review-controls"],
            access_preset("vault_reader"),
            vec!["vault_note_read", "vault_search"],
            vec!["Treat secrets and credentials as denied content."],
            vec!["security-review"],
        ),
        built_in_profile(
            "planner",
            "Planner",
            "planning",
            vec!["break-down-work", "sequence-tasks"],
            access_preset("vault_writer"),
            vec!["tasks_list", "vault_note_write"],
            vec!["Keep plans executable and scoped."],
            vec!["implementation-plan"],
        ),
        built_in_profile(
            "builder",
            "Builder",
            "implementation",
            vec!["implement-plan", "update-files"],
            access_preset("builder"),
            vec!["read_source", "edit_source", "run_tests"],
            vec!["Stay within assigned files."],
            vec!["code-change"],
        ),
        built_in_profile(
            "reviewer",
            "Reviewer",
            "review",
            vec!["review-implementation", "verify-tests"],
            access_preset("reviewer"),
            vec!["read_source", "run_tests"],
            vec!["Prioritize defects, regressions, and missing tests."],
            vec!["review-report"],
        ),
    ]
}

pub fn profile_rel_for_id(raw: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;

    for ch in raw.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(ch);
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }

    if slug.is_empty() {
        slug = "agent-profile".to_string();
    }

    format!("System/Agents/Custom/{slug}.md")
}

#[tauri::command]
pub fn agent_profiles_seed() -> Result<Vec<AgentProfile>, String> {
    let root = crate::vault::vault_root("work")?;
    for profile in built_in_profiles() {
        let abs = crate::vault::safe_join(&root, &profile.rel)?;
        if abs.exists() {
            continue;
        }
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        fs::write(&abs, render_profile_markdown(&profile))
            .map_err(|e| format!("write {}: {e}", abs.display()))?;
    }
    agent_profiles_list()
}

#[tauri::command]
pub fn agent_profiles_list() -> Result<Vec<AgentProfile>, String> {
    let root = crate::vault::vault_root("work")?;
    let agents = agent_profiles_dir(&root)?;
    let mut profiles = Vec::new();
    read_profiles_dir(&root, &agents, &mut profiles)?;
    profiles.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.rel.cmp(&b.rel)));
    Ok(profiles)
}

#[tauri::command]
pub fn agent_profile_read(rel: String) -> Result<AgentProfile, String> {
    ensure_agent_rel(&rel)?;
    let root = crate::vault::vault_root("work")?;
    let abs = crate::vault::safe_join(&root, &rel)?;
    let body = fs::read_to_string(&abs).map_err(|e| format!("read {}: {e}", abs.display()))?;
    parse_profile_markdown(&rel, &body, is_built_in_rel(&rel))
}

#[tauri::command]
pub fn agent_profile_save(profile: AgentProfile) -> Result<AgentProfile, String> {
    if profile.built_in || is_built_in_id(&profile.id) {
        return Err("built-in profiles cannot be saved".to_string());
    }

    let rel = if profile.rel.trim().is_empty() {
        profile_rel_for_id(&profile.id)
    } else {
        profile.rel.clone()
    };
    ensure_custom_rel(&rel)?;

    let root = crate::vault::vault_root("work")?;
    let abs = crate::vault::safe_join(&root, &rel)?;
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }

    let mut saved = profile;
    saved.rel = rel.clone();
    saved.built_in = false;
    fs::write(&abs, render_profile_markdown(&saved))
        .map_err(|e| format!("write {}: {e}", abs.display()))?;
    agent_profile_read(rel)
}

#[tauri::command]
pub fn agent_profile_delete(rel: String) -> Result<(), String> {
    if is_built_in_rel(&rel) {
        return Err("built-in profiles cannot be deleted".to_string());
    }
    ensure_custom_rel(&rel)?;

    let root = crate::vault::vault_root("work")?;
    let abs = crate::vault::safe_join(&root, &rel)?;
    fs::remove_file(&abs).map_err(|e| format!("delete {}: {e}", abs.display()))
}

#[tauri::command]
pub fn agent_profile_catalog() -> Result<serde_json::Value, String> {
    let items: Vec<AgentCatalogItem> = agent_profiles_list()?
        .into_iter()
        .map(|profile| AgentCatalogItem {
            id: profile.id,
            name: profile.name,
            status: profile.status,
            version: profile.version,
            role: profile.role,
            rel: profile.rel,
            built_in: profile.built_in,
        })
        .collect();
    Ok(serde_json::json!({ "items": items }))
}

#[tauri::command]
pub fn agent_profile_test(
    profile: AgentProfile,
    sample_rel: Option<String>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "reads": profile.access.read,
        "writes": profile.access.write,
        "tools": profile.tools,
        "blocked": profile.access.denied,
        "sample_rel": sample_rel.unwrap_or_default(),
        "summary": "Dry run only. No files changed."
    }))
}

pub fn parse_profile_markdown(
    rel: &str,
    body: &str,
    built_in: bool,
) -> Result<AgentProfile, String> {
    let (frontmatter, body) = frontmatter_block(body)?;
    let mut xnaut_agent = false;
    let mut id: Option<String> = None;
    let mut name: Option<String> = None;
    let mut status: Option<String> = None;
    let mut version: Option<u32> = None;
    let mut role: Option<String> = None;
    let mut skills = Vec::new();
    let mut access = AgentAccess::default();
    let mut tools = Vec::new();
    let mut constraints = Vec::new();
    let mut outputs = Vec::new();
    let mut current_top_list: Option<&str> = None;
    let mut in_access = false;
    let mut current_access_list: Option<&str> = None;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(value) = line.strip_prefix("    - ") {
            match current_access_list {
                Some("read") => access.read.push(clean_scalar(value)),
                Some("write") => access.write.push(clean_scalar(value)),
                Some("denied") => access.denied.push(clean_scalar(value)),
                _ => return Err(format!("unexpected nested list item: {line}")),
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("  - ") {
            match current_top_list {
                Some("skills") => skills.push(clean_scalar(value)),
                Some("tools") => tools.push(clean_scalar(value)),
                Some("constraints") => constraints.push(clean_scalar(value)),
                Some("outputs") => outputs.push(clean_scalar(value)),
                _ => return Err(format!("unexpected list item: {line}")),
            }
            continue;
        }

        if in_access && line.starts_with("  ") {
            let (key, value) = split_key_value(trimmed)?;
            current_top_list = None;
            current_access_list = Some(key);
            match key {
                "read" => access.read = parse_list_scalar(value),
                "write" => access.write = parse_list_scalar(value),
                "denied" => access.denied = parse_list_scalar(value),
                _ => return Err(format!("unknown access key: {key}")),
            }
            continue;
        }

        let (key, value) = split_key_value(line)?;
        current_top_list = None;
        current_access_list = None;
        in_access = false;

        match key {
            "xnaut_agent" => xnaut_agent = value == "true",
            "id" => id = Some(clean_scalar(value)),
            "name" => name = Some(clean_scalar(value)),
            "status" => status = Some(clean_scalar(value)),
            "version" => {
                version = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| format!("invalid version: {value}"))?,
                );
            }
            "role" => role = Some(clean_scalar(value)),
            "skills" => {
                skills = parse_list_scalar(value);
                current_top_list = Some("skills");
            }
            "access" => in_access = true,
            "tools" => {
                tools = parse_list_scalar(value);
                current_top_list = Some("tools");
            }
            "constraints" => {
                constraints = parse_list_scalar(value);
                current_top_list = Some("constraints");
            }
            "outputs" => {
                outputs = parse_list_scalar(value);
                current_top_list = Some("outputs");
            }
            _ => return Err(format!("unknown profile key: {key}")),
        }
    }

    if !xnaut_agent {
        return Err("xnaut_agent must be true".to_string());
    }
    let id = id.unwrap_or_default();
    if id.is_empty() {
        return Err("id must not be empty".to_string());
    }
    let name = name.unwrap_or_default();
    if name.is_empty() {
        return Err("name must not be empty".to_string());
    }
    let status = status.unwrap_or_default();
    if status != "enabled" && status != "disabled" {
        return Err("status must be enabled or disabled".to_string());
    }
    let version = version.ok_or_else(|| "version must be present".to_string())?;
    let role = role.unwrap_or_default();
    if role.is_empty() {
        return Err("role must not be empty".to_string());
    }

    Ok(AgentProfile {
        id,
        name,
        status,
        version,
        role,
        skills,
        access,
        tools,
        constraints,
        outputs,
        body: body.to_string(),
        rel: rel.to_string(),
        built_in,
    })
}

pub fn render_profile_markdown(profile: &AgentProfile) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("xnaut_agent: true\n");
    out.push_str(&format!("id: {}\n", profile.id));
    out.push_str(&format!("name: {}\n", profile.name));
    out.push_str(&format!("status: {}\n", profile.status));
    out.push_str(&format!("version: {}\n", profile.version));
    out.push_str(&format!("role: {}\n", profile.role));
    push_list(&mut out, "skills", &profile.skills);
    out.push_str("access:\n");
    push_nested_list(&mut out, "read", &profile.access.read);
    push_nested_list(&mut out, "write", &profile.access.write);
    push_nested_list(&mut out, "denied", &profile.access.denied);
    push_list(&mut out, "tools", &profile.tools);
    push_list(&mut out, "constraints", &profile.constraints);
    push_list(&mut out, "outputs", &profile.outputs);
    out.push_str("---\n");
    out.push_str(&profile.body);
    out
}

fn frontmatter_block(body: &str) -> Result<(&str, &str), String> {
    let offset = if body.starts_with("---\r\n") {
        "---\r\n".len()
    } else if body.starts_with("---\n") {
        "---\n".len()
    } else {
        return Err("profile markdown must start with frontmatter".to_string());
    };

    let closing = body[offset..]
        .find("\n---\r\n")
        .or_else(|| body[offset..].find("\n---\n"))
        .map(|index| offset + index)
        .ok_or_else(|| "profile markdown missing closing frontmatter".to_string())?;
    let rest_start = closing + "\n---".len();
    let rest = body[rest_start..]
        .strip_prefix("\r\n")
        .or_else(|| body[rest_start..].strip_prefix('\n'))
        .unwrap_or(&body[rest_start..]);

    Ok((&body[offset..closing], rest))
}

fn parse_list_scalar(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        return inner
            .split(',')
            .map(clean_scalar)
            .filter(|value| !value.is_empty())
            .collect();
    }

    vec![clean_scalar(trimmed)]
}

fn split_key_value(line: &str) -> Result<(&str, &str), String> {
    let (key, value) = line
        .split_once(':')
        .ok_or_else(|| format!("expected key/value line: {line}"))?;
    Ok((key.trim(), value.trim()))
}

fn clean_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn push_list(out: &mut String, key: &str, values: &[String]) {
    out.push_str(&format!("{key}:\n"));
    for value in values {
        out.push_str(&format!("  - {value}\n"));
    }
}

fn push_nested_list(out: &mut String, key: &str, values: &[String]) {
    out.push_str(&format!("  {key}:\n"));
    for value in values {
        out.push_str(&format!("    - {value}\n"));
    }
}

fn built_in_profile(
    id: &str,
    name: &str,
    role: &str,
    skills: Vec<&str>,
    access: AgentAccess,
    tools: Vec<&str>,
    constraints: Vec<&str>,
    outputs: Vec<&str>,
) -> AgentProfile {
    AgentProfile {
        id: id.to_string(),
        name: name.to_string(),
        status: "enabled".to_string(),
        version: 1,
        role: role.to_string(),
        skills: strings(skills),
        access,
        tools: strings(tools),
        constraints: strings(constraints),
        outputs: strings(outputs),
        body: format!("# {name}\n\nYou are the xNAUT {name} agent for {role}.\n"),
        rel: format!("System/Agents/{name}.md"),
        built_in: true,
    }
}

fn strings(values: Vec<&str>) -> Vec<String> {
    values.into_iter().map(str::to_string).collect()
}

fn access_preset(name: &str) -> AgentAccess {
    match name {
        "conservative" => AgentAccess {
            read: strings(vec!["agent_profiles", "vault_catalog"]),
            write: strings(vec!["agent_profiles_custom"]),
            denied: strings(vec!["source_code", "terminal", "secrets"]),
        },
        "vault_writer" => AgentAccess {
            read: strings(vec!["vault"]),
            write: strings(vec!["vault"]),
            denied: strings(vec!["source_code", "terminal", "secrets"]),
        },
        "builder" => AgentAccess {
            read: strings(vec!["vault", "source_code"]),
            write: strings(vec!["assigned_files"]),
            denied: strings(vec!["secrets"]),
        },
        "reviewer" => AgentAccess {
            read: strings(vec!["vault", "source_code", "test_output"]),
            write: strings(vec!["review_notes"]),
            denied: strings(vec!["secrets"]),
        },
        _ => AgentAccess {
            read: strings(vec!["vault"]),
            write: Vec::new(),
            denied: strings(vec!["source_code", "terminal", "secrets"]),
        },
    }
}

fn read_profiles_dir(
    root: &Path,
    dir: &Path,
    profiles: &mut Vec<AgentProfile>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            read_profiles_dir(root, &path, profiles)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let abs = crate::vault::safe_join(root, &rel)?;
        let body = fs::read_to_string(&abs).map_err(|e| format!("read {}: {e}", abs.display()))?;
        profiles.push(parse_profile_markdown(&rel, &body, is_built_in_rel(&rel))?);
    }

    Ok(())
}

fn ensure_agent_rel(rel: &str) -> Result<(), String> {
    if rel.starts_with("System/Agents/") && rel.ends_with(".md") {
        Ok(())
    } else {
        Err("profile path must be under System/Agents and end with .md".to_string())
    }
}

fn ensure_custom_rel(rel: &str) -> Result<(), String> {
    if rel.starts_with("System/Agents/Custom/") && rel.ends_with(".md") {
        Ok(())
    } else {
        Err("custom profile path must be under System/Agents/Custom and end with .md".to_string())
    }
}

fn is_built_in_rel(rel: &str) -> bool {
    built_in_profiles().iter().any(|profile| profile.rel == rel)
}

fn is_built_in_id(id: &str) -> bool {
    built_in_profiles().iter().any(|profile| profile.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profile_frontmatter_and_body() {
        let sample = r#"---
xnaut_agent: true
id: architect
name: Architect
status: enabled
version: 1
role: architecture
skills:
  - create-architecture
access:
  read:
    - vault
  write:
    - vault
  denied:
    - source_code
tools:
  - read_vault
constraints:
  - Do not edit implementation code.
outputs:
  - architecture
---
# Persona

You are a systems architect.
"#;

        let profile = parse_profile_markdown("System/Agents/Architect.md", sample, true).unwrap();

        assert_eq!(profile.id, "architect");
        assert_eq!(profile.name, "Architect");
        assert_eq!(profile.access.denied, vec!["source_code"]);
        assert!(profile.body.contains("systems architect"));
        assert!(profile.built_in);
    }

    #[test]
    fn parses_crlf_profile_frontmatter_and_body() {
        let sample = "---\r\nxnaut_agent: true\r\nid: architect\r\nname: Architect\r\nstatus: enabled\r\nversion: 1\r\nrole: architecture\r\nskills:\r\n  - create-architecture\r\naccess:\r\n  read:\r\n    - vault\r\n  write:\r\n    - vault\r\n  denied:\r\n    - source_code\r\ntools:\r\n  - read_vault\r\nconstraints:\r\n  - Do not edit implementation code.\r\noutputs:\r\n  - architecture\r\n---\r\n# Persona\r\n\r\nYou are a systems architect.\r\n";

        let profile = parse_profile_markdown("System/Agents/Architect.md", sample, true).unwrap();

        assert_eq!(profile.id, "architect");
        assert_eq!(profile.access.denied, vec!["source_code"]);
        assert!(profile.body.contains("systems architect"));
    }

    #[test]
    fn render_round_trips_required_fields() {
        let profile = AgentProfile {
            id: "reviewer".to_string(),
            name: "Reviewer".to_string(),
            status: "enabled".to_string(),
            version: 1,
            role: "review".to_string(),
            skills: vec!["review-implementation".to_string()],
            access: AgentAccess {
                read: vec!["vault".to_string()],
                write: vec!["draft_notes".to_string()],
                denied: vec!["secrets".to_string()],
            },
            tools: vec!["read_vault".to_string()],
            constraints: vec!["Do not approve missing tests.".to_string()],
            outputs: vec!["qa-report".to_string()],
            body: "# Persona\n\nYou review implementation readiness.\n".to_string(),
            rel: "System/Agents/Reviewer.md".to_string(),
            built_in: true,
        };

        let rendered = render_profile_markdown(&profile);
        let parsed = parse_profile_markdown(&profile.rel, &rendered, profile.built_in).unwrap();

        assert_eq!(parsed.id, "reviewer");
        assert_eq!(parsed.skills, vec!["review-implementation"]);
        assert_eq!(parsed.access.write, vec!["draft_notes"]);
        assert!(parsed.body.contains("implementation readiness"));
    }

    #[test]
    fn built_in_agentfather_has_conservative_access() {
        let profiles = built_in_profiles();
        let father = profiles.iter().find(|p| p.id == "agentfather").unwrap();
        assert!(father.tools.contains(&"create_agent_profile".to_string()));
        assert!(father.access.denied.contains(&"source_code".to_string()));
        assert!(father.access.denied.contains(&"terminal".to_string()));
    }

    #[test]
    fn profile_filename_is_safe() {
        assert_eq!(
            profile_rel_for_id("SAP Migration Architect"),
            "System/Agents/Custom/sap-migration-architect.md"
        );
        assert_eq!(profile_rel_for_id("../bad"), "System/Agents/Custom/bad.md");
    }

    #[test]
    fn listing_root_uses_safe_join_rel() {
        let root = Path::new("/tmp/xnaut-work");

        assert_eq!(
            agent_profiles_dir(root).unwrap(),
            crate::vault::safe_join(root, "System/Agents").unwrap()
        );
    }

    #[test]
    fn rejects_xnaut_agent_not_true() {
        let sample =
            valid_profile_markdown().replacen("xnaut_agent: true", "xnaut_agent: false", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "xnaut_agent must be true");
    }

    #[test]
    fn rejects_empty_id() {
        let sample = valid_profile_markdown().replacen("id: architect", "id: ", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "id must not be empty");
    }

    #[test]
    fn rejects_empty_name() {
        let sample = valid_profile_markdown().replacen("name: Architect", "name: ", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "name must not be empty");
    }

    #[test]
    fn rejects_invalid_status() {
        let sample = valid_profile_markdown().replacen("status: enabled", "status: paused", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "status must be enabled or disabled");
    }

    #[test]
    fn rejects_missing_version() {
        let sample = valid_profile_markdown().replacen("version: 1\n", "", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "version must be present");
    }

    #[test]
    fn rejects_missing_role() {
        let sample = valid_profile_markdown().replacen("role: architecture\n", "", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "role must not be empty");
    }

    #[test]
    fn rejects_empty_role() {
        let sample = valid_profile_markdown().replacen("role: architecture", "role: ", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "role must not be empty");
    }

    fn valid_profile_markdown() -> String {
        r#"---
xnaut_agent: true
id: architect
name: Architect
status: enabled
version: 1
role: architecture
skills:
  - create-architecture
access:
  read:
    - vault
  write:
    - vault
  denied:
    - source_code
tools:
  - read_vault
constraints:
  - Do not edit implementation code.
outputs:
  - architecture
---
# Persona

You are a systems architect.
"#
        .to_string()
    }
}
