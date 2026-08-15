// The plugin library (XNAUT-147).
//
// A plugin is an MCP server: the way an agent gets a capability xNAUT does
// not ship itself — library docs, a ticket system, a notes vault. The library
// is the single place one enters the system, the same rule the skill library
// follows.
//
// Storage is its OWN file, not settings.json. An older build's settings save
// has stripped keys before (see the `extra` flatten note in settings.rs), and
// a plugin's credential is not something to lose that way. Same reasoning as
// mobile.json.
//
// What "install" means here: the plugin is written into the config the coding
// harness reads at launch (claude via --mcp-config, codex via -c overrides).
// Nothing is installed globally and no other tool's config file is touched —
// a plugin enabled here reaches xNAUT's agents and nothing else on the machine.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// A process xNAUT starts and talks to over stdio.
    Stdio,
    /// An HTTP MCP endpoint.
    Http,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub transport: Transport,
    /// stdio: the binary. Empty for http.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// http: the endpoint. Empty for stdio.
    #[serde(default)]
    pub url: String,
    /// http: headers the endpoint needs, typically Authorization.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Grouping in the library. Free text, so a custom plugin can invent one.
    #[serde(default)]
    pub category: String,
    /// The setup step the owner has to do OUTSIDE xNAUT, if any. Shown next
    /// to the credential fields, because "it just does not connect" is nearly
    /// always one of these.
    #[serde(default)]
    pub note: String,
    /// Env vars the server needs, in order. A value the owner has not filled
    /// in yet stays empty and the plugin cannot be enabled.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Which of `env` must be non-empty before this can run.
    #[serde(default)]
    pub required_env: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
    /// Where to read about it. Shown, never fetched.
    #[serde(default)]
    pub docs_url: String,
    /// A seeded entry the library knows about, as opposed to one the owner
    /// added. Seeds can be reset; custom ones are only ever deleted.
    #[serde(default)]
    pub seeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginStore {
    #[serde(default)]
    pub plugins: Vec<Plugin>,
}

fn store_path() -> PathBuf {
    dirs::config_dir()
        .map(|dir| dir.join("xnaut").join("plugins.json"))
        .unwrap_or_else(|| PathBuf::from(".xnaut-plugins.json"))
}

/// The catalog. Every field is editable in the library: a package name or a
/// hosted endpoint is exactly the sort of thing that moves without notice, so
/// these are starting points, not promises. Each npm package and each hosted
/// URL here answered when the catalog was written (2026-08-15); the ones that
/// looked stale say so in their note.
pub fn seed() -> Vec<Plugin> {
    vec![
        Plugin {
            id: "context7".into(),
            name: "Context7".into(),
            description: "Current library and framework documentation, fetched per question instead of recalled from training data.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@upstash/context7-mcp".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Docs & search".into(),
            note: "Works without a key at a lower rate limit.".into(),
            env: HashMap::from([("CONTEXT7_API_KEY".to_string(), "".to_string())]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://github.com/upstash/context7".into(),
            seeded: true,
        },
        Plugin {
            id: "exa".into(),
            name: "Exa".into(),
            description: "Neural web search built for agents: full page contents, not a list of links.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "exa-mcp-server".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Docs & search".into(),
            note: "".into(),
            env: HashMap::from([("EXA_API_KEY".to_string(), "".to_string())]),
            required_env: vec!["EXA_API_KEY".into()],
            enabled: false,
            docs_url: "https://github.com/exa-labs/exa-mcp-server".into(),
            seeded: true,
        },
        Plugin {
            id: "brave-search".into(),
            name: "Brave Search".into(),
            description: "Independent web index. A second opinion when one search engine keeps returning the same three pages.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-brave-search".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Docs & search".into(),
            note: "Reference server, last published 2024 — check it still runs.".into(),
            env: HashMap::from([("BRAVE_API_KEY".to_string(), "".to_string())]),
            required_env: vec!["BRAVE_API_KEY".into()],
            enabled: false,
            docs_url: "https://github.com/modelcontextprotocol/servers".into(),
            seeded: true,
        },
        Plugin {
            id: "notion".into(),
            name: "Notion".into(),
            description: "Read and write Notion pages and databases.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@notionhq/notion-mcp-server".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Knowledge".into(),
            note: "Create an internal integration, then share the pages with it — an unshared page is invisible to the token.".into(),
            env: HashMap::from([("NOTION_TOKEN".to_string(), "".to_string())]),
            required_env: vec!["NOTION_TOKEN".into()],
            enabled: false,
            docs_url: "https://github.com/makenotion/notion-mcp-server".into(),
            seeded: true,
        },
        Plugin {
            id: "obsidian".into(),
            name: "Obsidian".into(),
            description: "Search and edit an Obsidian vault.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "mcp-obsidian".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Knowledge".into(),
            note: "Needs the Local REST API community plugin running inside Obsidian.".into(),
            env: HashMap::from([("OBSIDIAN_API_KEY".to_string(), "".to_string()), ("OBSIDIAN_HOST".to_string(), "http://127.0.0.1:27123".to_string())]),
            required_env: vec!["OBSIDIAN_API_KEY".into()],
            enabled: false,
            docs_url: "https://github.com/MarkusPfundstein/mcp-obsidian".into(),
            seeded: true,
        },
        Plugin {
            id: "memory".into(),
            name: "Memory".into(),
            description: "A knowledge graph the agent writes to and reads back across runs.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-memory".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Knowledge".into(),
            note: "".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://github.com/modelcontextprotocol/servers".into(),
            seeded: true,
        },
        Plugin {
            id: "engram".into(),
            name: "Engram".into(),
            description: "Your own memory service. Set its URL before enabling — the library does not guess where your services run.".into(),
            transport: Transport::Http,
            command: "".into(),
            args: vec![],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Knowledge".into(),
            note: "".into(),
            env: HashMap::from([("ENGRAM_TOKEN".to_string(), "".to_string())]),
            required_env: vec![],
            enabled: false,
            docs_url: "".into(),
            seeded: true,
        },
        Plugin {
            id: "lineary".into(),
            name: "Lineary".into(),
            description: "48Nauts ticket and issue tracking. Set its URL before enabling.".into(),
            transport: Transport::Http,
            command: "".into(),
            args: vec![],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Work tracking".into(),
            note: "".into(),
            env: HashMap::from([("LINEARY_TOKEN".to_string(), "".to_string())]),
            required_env: vec![],
            enabled: false,
            docs_url: "".into(),
            seeded: true,
        },
        Plugin {
            id: "linear".into(),
            name: "Linear".into(),
            description: "Issues, projects and cycles in Linear.".into(),
            transport: Transport::Http,
            command: "".into(),
            args: vec![],
            url: "https://mcp.linear.app/mcp".into(),
            headers: HashMap::from([("Authorization".to_string(), "".to_string())]),
            category: "Work tracking".into(),
            note: "Authorization: Bearer <token>. The hosted endpoint answered 401 at seed time, meaning it is alive and wants auth.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://linear.app/docs/mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "sentry".into(),
            name: "Sentry".into(),
            description: "Errors and traces from Sentry, including the issue an agent is asked to fix.".into(),
            transport: Transport::Http,
            command: "".into(),
            args: vec![],
            url: "https://mcp.sentry.dev/mcp".into(),
            headers: HashMap::from([("Authorization".to_string(), "".to_string())]),
            category: "Work tracking".into(),
            note: "".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://docs.sentry.io/product/sentry-mcp/".into(),
            seeded: true,
        },
        Plugin {
            id: "gmail".into(),
            name: "Gmail".into(),
            description: "Read, search, draft and send mail.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@gongrzhe/server-gmail-autoauth-mcp".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Comms".into(),
            note: "First run opens a Google consent screen and stores the token in your home directory.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://github.com/gongrzhe/server-gmail-autoauth-mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "google-calendar".into(),
            name: "Google Calendar".into(),
            description: "Read and create calendar events.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@cocal/google-calendar-mcp".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Comms".into(),
            note: "Path to the OAuth client JSON downloaded from Google Cloud.".into(),
            env: HashMap::from([("GOOGLE_OAUTH_CREDENTIALS".to_string(), "".to_string())]),
            required_env: vec!["GOOGLE_OAUTH_CREDENTIALS".into()],
            enabled: false,
            docs_url: "https://github.com/nspady/google-calendar-mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "slack".into(),
            name: "Slack".into(),
            description: "Read channels and post messages as a bot user.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-slack".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Comms".into(),
            note: "Reference server, last published 2025 — check it still runs.".into(),
            env: HashMap::from([("SLACK_BOT_TOKEN".to_string(), "".to_string()), ("SLACK_TEAM_ID".to_string(), "".to_string())]),
            required_env: vec!["SLACK_BOT_TOKEN".into(), "SLACK_TEAM_ID".into()],
            enabled: false,
            docs_url: "https://github.com/modelcontextprotocol/servers".into(),
            seeded: true,
        },
        Plugin {
            id: "github".into(),
            name: "GitHub".into(),
            description: "Issues, pull requests and code search on GitHub. Development here lives on Forgejo, so this is for mirrors and other people's repositories.".into(),
            transport: Transport::Http,
            command: "".into(),
            args: vec![],
            url: "https://api.githubcopilot.com/mcp/".into(),
            headers: HashMap::from([("Authorization".to_string(), "".to_string())]),
            category: "Dev".into(),
            note: "Authorization: Bearer <personal access token>.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://github.com/github/github-mcp-server".into(),
            seeded: true,
        },
        Plugin {
            id: "playwright".into(),
            name: "Playwright".into(),
            description: "Drives a real browser: open a page, click, fill, screenshot, read the console.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@playwright/mcp@latest".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://github.com/microsoft/playwright-mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "filesystem".into(),
            name: "Filesystem".into(),
            description: "Read and write files under paths you name — and only those.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "Add each allowed directory as an argument. With no path it exposes nothing.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://github.com/modelcontextprotocol/servers".into(),
            seeded: true,
        },
        Plugin {
            id: "postgres".into(),
            name: "Postgres".into(),
            description: "Read-only SQL against a Postgres database.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-postgres".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "Add the connection string as an argument. Reference server, last published 2024.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://github.com/modelcontextprotocol/servers".into(),
            seeded: true,
        },
        Plugin {
            id: "sequential-thinking".into(),
            name: "Sequential Thinking".into(),
            description: "A scratchpad for multi-step reasoning the model can revise as it goes.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-sequential-thinking".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "https://github.com/modelcontextprotocol/servers".into(),
            seeded: true,
        },
        Plugin {
            id: "figma".into(),
            name: "Figma".into(),
            description: "Reads Figma files so a build can follow the actual design rather than a screenshot of it.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "figma-developer-mcp".into(), "--stdio".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Design".into(),
            note: "".into(),
            env: HashMap::from([("FIGMA_API_KEY".to_string(), "".to_string())]),
            required_env: vec!["FIGMA_API_KEY".into()],
            enabled: false,
            docs_url: "https://github.com/GLips/Figma-Context-MCP".into(),
            seeded: true,
        },
        Plugin {
            id: "excalidraw".into(),
            name: "Excalidraw".into(),
            description: "Diagrams as data. Runs locally on port 3001.".into(),
            transport: Transport::Http,
            command: "".into(),
            args: vec![],
            url: "http://127.0.0.1:3001/mcp".into(),
            headers: HashMap::from([]),
            category: "Design".into(),
            note: "Start the local Excalidraw MCP server first; nothing here launches it.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            enabled: false,
            docs_url: "".into(),
            seeded: true,
        },
    ]
}

fn load_store() -> PluginStore {
    let path = store_path();
    let mut store: PluginStore = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    // Add seeds the owner has never seen. An existing entry is left ALONE:
    // his edits and his credentials outrank our defaults.
    for candidate in seed() {
        if !store.plugins.iter().any(|item| item.id == candidate.id) {
            store.plugins.push(candidate);
        }
    }
    store
}

fn save_store(store: &PluginStore) -> Result<(), String> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Why a plugin cannot run yet, or None when it is ready. Enabling something
/// that will fail at launch is worse than refusing it here, because the
/// failure lands inside an agent run where nobody reads it.
pub fn blocker(plugin: &Plugin) -> Option<String> {
    match plugin.transport {
        Transport::Http if plugin.url.trim().is_empty() => {
            return Some("needs its URL".into());
        }
        Transport::Stdio if plugin.command.trim().is_empty() => {
            return Some("needs a command".into());
        }
        _ => {}
    }
    for key in &plugin.required_env {
        if plugin.env.get(key).map(|value| value.trim().is_empty()).unwrap_or(true) {
            return Some(format!("needs {key}"));
        }
    }
    None
}

#[tauri::command]
pub fn plugin_catalog() -> Result<Vec<Plugin>, String> {
    let store = load_store();
    save_store(&store)?; // persist newly-seeded entries so ids stay stable
    Ok(store.plugins)
}

#[tauri::command]
pub fn plugin_save(plugin: Plugin) -> Result<Plugin, String> {
    let mut store = load_store();
    let mut plugin = plugin;
    plugin.id = plugin.id.trim().to_lowercase();
    if plugin.id.is_empty() {
        return Err("a plugin needs an id".into());
    }
    if plugin.enabled {
        if let Some(reason) = blocker(&plugin) {
            return Err(format!("{} {reason}", plugin.name));
        }
    }
    match store.plugins.iter_mut().find(|item| item.id == plugin.id) {
        Some(existing) => *existing = plugin.clone(),
        None => store.plugins.push(plugin.clone()),
    }
    save_store(&store)?;
    Ok(plugin)
}

#[tauri::command]
pub fn plugin_delete(id: String) -> Result<(), String> {
    let mut store = load_store();
    store.plugins.retain(|item| item.id != id.trim());
    save_store(&store)
}

/// The enabled plugins, ready to hand to a runtime.
pub fn active() -> Vec<Plugin> {
    load_store()
        .plugins
        .into_iter()
        .filter(|plugin| plugin.enabled && blocker(plugin).is_none())
        .collect()
}

/// `--mcp-config` payload for claude: the same JSON shape its own config uses.
pub fn claude_config(plugins: &[Plugin]) -> serde_json::Value {
    let mut servers = serde_json::Map::new();
    for plugin in plugins {
        let entry = match plugin.transport {
            Transport::Http => serde_json::json!({
                "type": "http",
                "url": plugin.url,
                "headers": plugin.headers.iter().filter(|(_, v)| !v.trim().is_empty()).collect::<HashMap<_, _>>(),
            }),
            Transport::Stdio => serde_json::json!({
                "command": plugin.command,
                "args": plugin.args,
                "env": plugin.env.iter().filter(|(_, v)| !v.trim().is_empty()).collect::<HashMap<_, _>>(),
            }),
        };
        servers.insert(plugin.id.clone(), entry);
    }
    serde_json::json!({ "mcpServers": servers })
}

/// TOML value for one codex `-c mcp_servers.<id>=<value>` override.
fn codex_value(plugin: &Plugin) -> String {
    let quote = |value: &str| format!("{:?}", value);
    match plugin.transport {
        Transport::Http => format!("{{url={}}}", quote(&plugin.url)),
        Transport::Stdio => {
            let args = plugin
                .args
                .iter()
                .map(|arg| quote(arg))
                .collect::<Vec<_>>()
                .join(",");
            let mut env: Vec<String> = plugin
                .env
                .iter()
                .filter(|(_, value)| !value.trim().is_empty())
                .map(|(key, value)| format!("{key}={}", quote(value)))
                .collect();
            env.sort(); // a HashMap would otherwise reorder the flags run to run
            let env = if env.is_empty() {
                String::new()
            } else {
                format!(",env={{{}}}", env.join(","))
            };
            format!("{{command={},args=[{args}]{env}}}", quote(&plugin.command))
        }
    }
}

/// Launch flags that give a runtime the enabled plugins.
///
/// Returns (flags, temp config path to clean up later). Only the two runtimes
/// with a documented switch are wired; the rest get nothing rather than a
/// config file written into someone's home directory behind their back.
pub fn launch_flags(runtime_id: &str, plugins: &[Plugin]) -> Vec<String> {
    if plugins.is_empty() {
        return Vec::new();
    }
    match runtime_id {
        "claude" => {
            let path = std::env::temp_dir().join(format!("xnaut-mcp-{}.json", std::process::id()));
            let text = serde_json::to_string(&claude_config(plugins)).unwrap_or_default();
            if std::fs::write(&path, text).is_err() {
                return Vec::new();
            }
            vec!["--mcp-config".into(), path.to_string_lossy().into_owned()]
        }
        // codex takes stdio servers only here. Handing it a url-shaped entry
        // it may not understand would fail the whole RUN at startup, which is
        // a bad trade for one plugin.
        "codex" => plugins
            .iter()
            .filter(|plugin| plugin.transport == Transport::Stdio)
            .flat_map(|plugin| {
                vec![
                    "-c".to_string(),
                    format!("mcp_servers.{}={}", plugin.id, codex_value(plugin)),
                ]
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio(id: &str) -> Plugin {
        Plugin {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "pkg".into()],
            url: String::new(),
            headers: HashMap::new(),
            category: "test".into(),
            note: String::new(),
            env: HashMap::from([("TOKEN".to_string(), "secret".to_string())]),
            required_env: vec!["TOKEN".into()],
            enabled: true,
            docs_url: String::new(),
            seeded: false,
        }
    }

    #[test]
    fn a_plugin_missing_its_credential_is_refused_not_launched() {
        // Enabling something that dies inside an agent run is worse than
        // refusing it in the library, where the message is actually read.
        let mut plugin = stdio("notion");
        plugin.env.insert("TOKEN".into(), "  ".into());
        assert_eq!(blocker(&plugin).as_deref(), Some("needs TOKEN"));
        assert!(blocker(&stdio("notion")).is_none());
    }

    #[test]
    fn an_http_plugin_without_a_url_is_refused() {
        // Lineary ships with no URL on purpose: it is his own server and the
        // library must not invent an address for it.
        let lineary = seed().into_iter().find(|p| p.id == "lineary").unwrap();
        assert_eq!(blocker(&lineary).as_deref(), Some("needs its URL"));
    }

    #[test]
    fn claude_gets_a_config_file_and_codex_gets_overrides() {
        let plugins = vec![stdio("context7")];
        let flags = launch_flags("claude", &plugins);
        assert_eq!(flags[0], "--mcp-config");
        assert!(std::path::Path::new(&flags[1]).exists());

        let codex = launch_flags("codex", &plugins);
        assert_eq!(codex[0], "-c");
        assert_eq!(
            codex[1],
            r#"mcp_servers.context7={command="npx",args=["-y","pkg"],env={TOKEN="secret"}}"#
        );
    }

    #[test]
    fn every_seeded_plugin_is_reachable_or_says_what_is_missing() {
        // A catalog entry that can neither run nor explain itself is worse
        // than no entry: it looks installed and does nothing.
        for plugin in seed() {
            assert!(!plugin.name.trim().is_empty(), "{} has no name", plugin.id);
            assert!(!plugin.category.trim().is_empty(), "{} has no category", plugin.id);
            match plugin.transport {
                Transport::Stdio => assert!(!plugin.command.trim().is_empty(), "{} has no command", plugin.id),
                // A blank URL is allowed for the owner's OWN services, and
                // then blocker() must say so rather than enabling silently.
                Transport::Http => assert!(
                    !plugin.url.trim().is_empty() || blocker(&plugin).is_some(),
                    "{} has no url and no blocker",
                    plugin.id
                ),
            }
            for key in &plugin.required_env {
                assert!(plugin.env.contains_key(key), "{} requires {key} but never asks for it", plugin.id);
            }
        }
    }

    #[test]
    fn the_catalog_survives_a_round_trip_through_its_file() {
        // The store is JSON on disk and the UI edits it: a field that cannot
        // come back (an enum whose casing does not match, a missing default)
        // would empty the library on the second launch, not the first.
        let store = PluginStore { plugins: seed() };
        let text = serde_json::to_string(&store).unwrap();
        let back: PluginStore = serde_json::from_str(&text).unwrap();
        assert_eq!(back.plugins.len(), seed().len());
        assert!(text.contains("\"stdio\"") && text.contains("\"http\""));
        // And a sparse entry — what a hand-edited file looks like — still loads.
        let sparse: Plugin = serde_json::from_str(
            r#"{"id":"x","name":"X","description":"","transport":"http"}"#,
        )
        .expect("sparse plugin must load");
        assert!(!sparse.enabled && sparse.args.is_empty());
    }

    #[test]
    fn codex_never_receives_an_http_plugin() {
        let http = seed().into_iter().find(|p| p.id == "linear").unwrap();
        assert!(launch_flags("codex", &[http.clone()]).is_empty());
        assert!(!launch_flags("claude", &[http]).is_empty());
    }

    #[test]
    fn a_runtime_with_no_documented_switch_gets_nothing() {
        // Silently writing into ~/.gemini or similar to make a feature work is
        // exactly the kind of surprise this library exists to avoid.
        assert!(launch_flags("gemini", &[stdio("context7")]).is_empty());
        assert!(launch_flags("claude", &[]).is_empty());
    }

    #[test]
    fn empty_credentials_never_reach_the_launch() {
        let mut plugin = stdio("context7");
        plugin.env.insert("EMPTY".into(), String::new());
        let config = claude_config(&[plugin]);
        let env = &config["mcpServers"]["context7"]["env"];
        assert!(env.get("EMPTY").is_none());
        assert_eq!(env["TOKEN"], "secret");
    }
}
