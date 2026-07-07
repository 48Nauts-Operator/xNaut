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

pub fn agent_profiles_root(root: &Path) -> PathBuf {
    root.join("System").join("Agents")
}

pub fn parse_profile_markdown(
    rel: &str,
    body: &str,
    built_in: bool,
) -> Result<AgentProfile, String> {
    let (frontmatter, body) = frontmatter_block(body)?;
    let mut xnaut_agent = false;
    let mut id = String::new();
    let mut name = String::new();
    let mut status = String::new();
    let mut version = 0;
    let mut role = String::new();
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
            "id" => id = clean_scalar(value),
            "name" => name = clean_scalar(value),
            "status" => status = clean_scalar(value),
            "version" => {
                version = value
                    .parse::<u32>()
                    .map_err(|_| format!("invalid version: {value}"))?;
            }
            "role" => role = clean_scalar(value),
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
    if id.is_empty() {
        return Err("id must not be empty".to_string());
    }
    if name.is_empty() {
        return Err("name must not be empty".to_string());
    }
    if status != "enabled" && status != "disabled" {
        return Err("status must be enabled or disabled".to_string());
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
