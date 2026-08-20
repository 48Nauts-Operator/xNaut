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
use serde_json::Value;
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
    /// Skills that ship with this plugin, by name in the Skill library. A
    /// connector on its own is a set of tools with no instructions; the skills
    /// are how an agent knows when to reach for them. Empty until an addon
    /// bundle brings some (XNAUT-160).
    #[serde(default)]
    pub skills: Vec<String>,
    /// True once the owner changed something that matters: a credential, an
    /// endpoint, a command. Guessing this from "does any field have a value"
    /// was wrong — a seeded default URL looked like his edit and froze a
    /// broken command in place. Provenance has to be recorded, not inferred.
    #[serde(default)]
    pub owner_edited: bool,
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
    // Overridable so a test never writes into the real library. A test that
    // enabled a plugin in the owner's own config is not a test, it is an edit
    // with extra steps — and one of them did exactly that before this existed.
    if let Ok(path) = std::env::var("XNAUT_PLUGINS_PATH") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            note: "Needs gcp-oauth.keys.json in ~/.gmail-mcp before it will start — a Google Cloud OAuth client, not just a key. Verified 2026-08-15: without it the server exits immediately.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            note: "Add the connection string as an ARGUMENT (it exits with 'Please provide a database URL' otherwise). Reference server, last published 2024.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
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
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "".into(),
            seeded: true,
        },
        Plugin {
            id: "forgejo".into(),
            name: "Forgejo".into(),
            description: "Issues and pull requests on the Forgejo host where development actually lives. GitHub is only a mirror here.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "gitea-mcp".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "Forgejo speaks the Gitea API, so the client is gitea-mcp. The npm package forgejo-mcp ships no executable — npx cannot run it, which verification catches. Token: the one in ~/.config/forgejo/token.".into(),
            env: HashMap::from([
                ("GITEA_HOST".to_string(), "http://cosmos.tail138398.ts.net:3000".to_string()),
                ("GITEA_ACCESS_TOKEN".to_string(), "".to_string()),
            ]),
            required_env: vec!["GITEA_ACCESS_TOKEN".into()],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/seepine/gitea-mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "codebase-memory".into(),
            name: "Codebase Memory".into(),
            description: "The code knowledge graph: who calls what, trace a path, find dead code, impact of a change.".into(),
            transport: Transport::Stdio,
            command: "codebase-memory-mcp".into(),
            args: vec![],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "Already installed here at ~/.local/bin/codebase-memory-mcp. Index a repository first, then ask it structural questions instead of grepping.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "".into(),
            seeded: true,
        },
        Plugin {
            id: "nautdocs".into(),
            name: "NautDocs".into(),
            description: "Search the vault, the knowledge base, tickets and memories in one place.".into(),
            transport: Transport::Stdio,
            command: "nautdocs-mcp".into(),
            args: vec![],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Knowledge".into(),
            note: "Already installed here at ~/.local/bin/nautdocs-mcp. Reach for it before grepping for a document.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "".into(),
            seeded: true,
        },
        Plugin {
            id: "kubernetes".into(),
            name: "Kubernetes".into(),
            description: "Inspect and operate a cluster through your existing kubeconfig.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "mcp-server-kubernetes".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "Uses the current kubectl context. Point it at the right cluster before enabling.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/Flux159/mcp-server-kubernetes".into(),
            seeded: true,
        },
        Plugin {
            id: "supabase".into(),
            name: "Supabase".into(),
            description: "Query and manage a Supabase project: tables, migrations, edge functions.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@supabase/mcp-server-supabase".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "".into(),
            env: HashMap::from([("SUPABASE_ACCESS_TOKEN".to_string(), "".to_string())]),
            required_env: vec!["SUPABASE_ACCESS_TOKEN".into()],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/supabase-community/supabase-mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "magic-21st".into(),
            name: "21st.dev Magic".into(),
            description: "Generates React UI components from a description, in the style of 21st.dev.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@21st-dev/magic".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Design".into(),
            note: "".into(),
            env: HashMap::from([("API_KEY".to_string(), "".to_string())]),
            required_env: vec!["API_KEY".into()],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/21st-dev/magic-mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "firecrawl".into(),
            name: "Firecrawl".into(),
            description: "Scrapes and crawls sites into clean markdown, including pages that need JavaScript.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "firecrawl-mcp".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Docs & search".into(),
            note: "".into(),
            env: HashMap::from([("FIRECRAWL_API_KEY".to_string(), "".to_string())]),
            required_env: vec!["FIRECRAWL_API_KEY".into()],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/mendableai/firecrawl-mcp-server".into(),
            seeded: true,
        },
        Plugin {
            id: "tavily".into(),
            name: "Tavily".into(),
            description: "Search and extract, tuned for agents rather than humans.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "tavily-mcp".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Docs & search".into(),
            note: "".into(),
            env: HashMap::from([("TAVILY_API_KEY".to_string(), "".to_string())]),
            required_env: vec!["TAVILY_API_KEY".into()],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/tavily-ai/tavily-mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "todoist".into(),
            name: "Todoist".into(),
            description: "Tasks, projects and due dates in Todoist.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@doist/todoist-mcp".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Work tracking".into(),
            note: "".into(),
            env: HashMap::from([("TODOIST_API_KEY".to_string(), "".to_string())]),
            required_env: vec!["TODOIST_API_KEY".into()],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/Doist/todoist-mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "atlassian".into(),
            name: "Jira & Confluence".into(),
            description: "Atlassian's own hosted server: Jira issues and Confluence pages.".into(),
            transport: Transport::Http,
            command: "".into(),
            args: vec![],
            url: "https://mcp.atlassian.com/v1/sse".into(),
            headers: HashMap::from([("Authorization".to_string(), "".to_string())]),
            category: "Work tracking".into(),
            note: "OAuth. The hosted endpoint answered 401 at seed time, meaning it is alive and wants auth.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://support.atlassian.com/atlassian-rovo-mcp-server/".into(),
            seeded: true,
        },
        Plugin {
            id: "google-drive".into(),
            name: "Google Drive".into(),
            description: "Search Drive and read documents.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-gdrive".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Knowledge".into(),
            note: "Needs a Google OAuth client file, like Gmail. Reference server, last published 2025; it exits at once without one.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/modelcontextprotocol/servers".into(),
            seeded: true,
        },
        Plugin {
            id: "airtable".into(),
            name: "Airtable".into(),
            description: "Read and write Airtable bases as structured records.".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "airtable-mcp-server".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Knowledge".into(),
            note: "".into(),
            env: HashMap::from([("AIRTABLE_API_KEY".to_string(), "".to_string())]),
            required_env: vec!["AIRTABLE_API_KEY".into()],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/domdomegg/airtable-mcp-server".into(),
            seeded: true,
        },
        Plugin {
            id: "stripe".into(),
            name: "Stripe".into(),
            description: "Payments, customers and subscriptions. Read-only until you give it a key that can do more.".into(),
            transport: Transport::Http,
            command: "".into(),
            args: vec![],
            url: "https://mcp.stripe.com".into(),
            headers: HashMap::from([("Authorization".to_string(), "".to_string())]),
            category: "Business".into(),
            note: "Authorization: Bearer <restricted key>. Use a RESTRICTED key, never the live secret.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://docs.stripe.com/mcp".into(),
            seeded: true,
        },
        Plugin {
            id: "time".into(),
            name: "Time".into(),
            description: "Current time and timezone conversion, so an agent stops guessing what 'today' is.".into(),
            transport: Transport::Stdio,
            command: "uvx".into(),
            args: vec!["mcp-server-time".into()],
            url: "".into(),
            headers: HashMap::from([]),
            category: "Dev".into(),
            note: "Python server via uvx. Verified 2026-08-15: the cached build failed to import on this machine — try `uvx --refresh mcp-server-time` if it will not start.".into(),
            env: HashMap::from([]),
            required_env: vec![],
            skills: vec![],
            owner_edited: false,
            enabled: false,
            docs_url: "https://github.com/modelcontextprotocol/servers".into(),
            seeded: true,
        },
    ]
}

/// A catalog compiled from the public registries (Claude's marketplace and
/// cursor/plugins), shipped as an asset and merged in at load.
///
/// Why an asset rather than more `seed()` entries: the hand-written seeds are
/// the ones we have actually run and can vouch for, notes and all. The catalog
/// is bulk — hundreds of entries whose only claim is "this is what the registry
/// says" — and mixing the two would lose that distinction. An entry the owner
/// has touched is never overwritten by either.
#[derive(Debug, Clone, Deserialize)]
struct CatalogEntry {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    transport: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    required_env: Vec<String>,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    docs_url: String,
    #[serde(default)]
    note: String,
    /// false means the registry says its package ships no executable — the
    /// forgejo-mcp trap. Those are kept but marked, never silently offered.
    #[serde(default)]
    runnable: Option<bool>,
}

fn catalog_asset() -> Vec<Plugin> {
    const CATALOG: &str = include_str!("../assets/plugin-catalog.json");
    let entries: Vec<CatalogEntry> = serde_json::from_str(CATALOG).unwrap_or_default();
    entries
        .into_iter()
        .filter(|entry| !entry.id.trim().is_empty())
        .map(|entry| {
            let note = match entry.runnable {
                Some(false) => {
                    let extra = "The registry says this package ships no executable, so it will not start as written.";
                    if entry.note.trim().is_empty() { extra.to_string() } else { format!("{} {extra}", entry.note.trim()) }
                }
                _ => entry.note,
            };
            Plugin {
                id: entry.id,
                name: entry.name,
                description: entry.description,
                transport: if entry.transport == "http" { Transport::Http } else { Transport::Stdio },
                command: entry.command,
                args: entry.args,
                url: entry.url,
                headers: HashMap::new(),
                category: if entry.category.trim().is_empty() { "Other".into() } else { entry.category },
                note,
                env: entry.env,
                required_env: entry.required_env,
                skills: entry.skills,
                owner_edited: false,
                enabled: false,
                docs_url: entry.docs_url,
                seeded: true,
            }
        })
        .collect()
}

fn load_store() -> PluginStore {
    let path = store_path();
    let mut store: PluginStore = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let mut migrate = false;
    for plugin in &mut store.plugins {
        migrate |= has_plaintext_secret(&plugin.env) || has_plaintext_secret(&plugin.headers);
        unstash_secrets(&mut plugin.env);
        unstash_secrets(&mut plugin.headers);
    }
    // Add seeds the owner has never seen, and REFRESH the ones he has never
    // touched. A seeded command can turn out to be wrong — forgejo-mcp ships no
    // executable, so `npx -y forgejo-mcp` could never run — and without this
    // the broken version outlives the fix in everyone's library. An entry that
    // is switched on, or that carries any value he typed, is left alone.
    // Hand-written seeds first: where both describe the same server, the one
    // we have run wins.
    let catalog = seed()
        .into_iter()
        .chain(catalog_asset().into_iter())
        .fold(Vec::new(), |mut all: Vec<Plugin>, plugin| {
            if !all.iter().any(|kept| kept.id == plugin.id) {
                all.push(plugin);
            }
            all
        });
    for candidate in catalog {
        let Some(existing) = store.plugins.iter_mut().find(|item| item.id == candidate.id) else {
            store.plugins.push(candidate);
            continue;
        };
        // Refresh only what the owner has not made his own.
        if !existing.seeded || existing.owner_edited {
            continue;
        }
        // Refresh the MECHANICS, keep what was configured. The first version
        // replaced the whole entry, which threw away the credential connect()
        // had just discovered — so a plugin reported connected, and the next
        // read found it switched off with an empty token again.
        let was_enabled = existing.enabled;
        let kept_env: Vec<(String, String)> = existing
            .env
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let kept_headers: Vec<(String, String)> = existing
            .headers
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        *existing = candidate;
        for (key, value) in kept_env {
            if existing.env.contains_key(&key) {
                existing.env.insert(key, value);
            }
        }
        for (key, value) in kept_headers {
            if existing.headers.contains_key(&key) {
                existing.headers.insert(key, value);
            }
        }
        // Carry the switch over only if the refreshed entry can actually run.
        // The forgejo entry was enabled while pointing at npx -y forgejo-mcp,
        // a package with no executable: a plugin marked connected that could
        // never start is worse than one plainly switched off.
        existing.enabled = was_enabled && blocker(existing).is_none();
    }
    // XNAUT-214: migrating only on save meant "migrates the next time you
    // happen to edit a plugin", and nothing had edited the library since the
    // migration shipped, so the live install still held three plaintext
    // secrets. Reading is the one thing that always happens.
    if migrate && keychain_enabled() {
        let _ = save_store(&store);
    }
    store
}

/// One plugin's environment, keychain sentinels already resolved. For code that
/// needs a configured credential without going through a launch: `seal` reaches
/// the HSM with the same TSB details the owner typed into the plugin panel.
pub fn plugin_env(id: &str) -> Option<HashMap<String, String>> {
    load_store().plugins.into_iter().find(|plugin| plugin.id == id).map(|plugin| plugin.env)
}

/// A credential-shaped value sitting in the file as itself, not as a sentinel.
fn has_plaintext_secret(values: &HashMap<String, String>) -> bool {
    values.iter().any(|(key, value)| {
        crate::secrets::is_secret_key(key)
            && !value.trim().is_empty()
            && !value.starts_with(crate::secrets::PREFIX)
    })
}

fn save_store(store: &PluginStore) -> Result<(), String> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    // Every write is also the migration: a plaintext credential already in the
    // file goes to the keychain the first time anything saves after upgrade,
    // and the file keeps the sentinel. No separate one-shot to forget to run.
    let mut store = store.clone();
    for plugin in &mut store.plugins {
        stash_secrets(&plugin.id.clone(), &mut plugin.env);
        stash_secrets(&plugin.id.clone(), &mut plugin.headers);
    }
    let text = serde_json::to_string_pretty(&store).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    // The library holds credentials even as sentinels-plus-endpoints; nobody
    // else on the machine needs to read it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// In the test binary the keychain is only touched by a test that asked for a
/// scratch service; otherwise `cargo test` would write items into the owner's
/// own login keychain and leave them there.
#[cfg(test)]
fn keychain_enabled() -> bool {
    std::env::var_os("XNAUT_KEYCHAIN_SERVICE").is_some()
}

#[cfg(not(test))]
fn keychain_enabled() -> bool {
    true
}

/// Replace credential-shaped values with their keychain sentinel, in place.
fn stash_secrets(id: &str, values: &mut HashMap<String, String>) {
    if !keychain_enabled() {
        return;
    }
    for (key, value) in values.iter_mut() {
        if !crate::secrets::is_secret_key(key) {
            continue;
        }
        if let Some(sentinel) = crate::secrets::stash(&format!("plugin/{id}/{key}"), value) {
            *value = sentinel;
        }
    }
}

/// Turn keychain sentinels back into values, in place.
///
/// Done on load rather than at each launch seam so that everything downstream
/// — the panel, `blocker`, `verify`, `claude_config`, `codex_value` — sees
/// exactly what it saw before this existed. Only the FILE changes.
///
/// ponytail: one `security(1)` spawn per secret per read, a few ms each at
/// this count. Cache it if the panel ever feels slow.
fn unstash_secrets(values: &mut HashMap<String, String>) {
    for value in values.values_mut() {
        if value.starts_with(crate::secrets::PREFIX) {
            *value = crate::secrets::resolve(value);
        }
    }
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
    // A hosted endpoint that declares an auth header and has none is missing a
    // credential, not broken. The survey of 2026-08-15 filed Linear, Sentry,
    // GitHub, Jira and Stripe under "tried and failed" with a 401, which reads
    // as our fault when the truth is "he has not signed in yet".
    if matches!(plugin.transport, Transport::Http) {
        for (name, value) in &plugin.headers {
            if value.trim().is_empty() {
                return Some(format!("needs {name}"));
            }
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
    // Did he change anything load-bearing, or just flip the switch? Only the
    // former makes the entry his and stops future seed refreshes.
    if let Some(seeded) = seed().into_iter().find(|item| item.id == plugin.id) {
        let same_mechanics = plugin.command == seeded.command
            && plugin.args == seeded.args
            && plugin.url == seeded.url
            && plugin.env == seeded.env
            && plugin.headers == seeded.headers;
        if !same_mechanics {
            plugin.owner_edited = true;
        }
    } else {
        plugin.owner_edited = true; // one he added himself
    }
    if let Some(existing) = store.plugins.iter().find(|item| item.id == plugin.id) {
        plugin.owner_edited |= existing.owner_edited; // sticky
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

/// Credentials xNAUT already holds, for a plugin that needs one.
///
/// "It may require your Forgejo URL and an access token" is a bad answer when
/// the token has been sitting in ~/.config/forgejo/token the whole time. Look
/// before asking. Two sources only, both explicit: this process's environment,
/// and the forge xNAUT is already configured against. Shell rc files are NOT
/// scraped — guessing at someone's dotfiles to find a secret is how a tool
/// ends up reading things it was never pointed at.
///
/// Returns (filled values, where each came from). Values are never logged.
pub fn discover(plugin: &Plugin) -> (HashMap<String, String>, Vec<String>) {
    let mut found = HashMap::new();
    let mut sources = Vec::new();
    let needed: Vec<String> = plugin
        .env
        .iter()
        .filter(|(_, value)| value.trim().is_empty())
        .map(|(key, _)| key.clone())
        .collect();

    for key in &needed {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                found.insert(key.clone(), value);
                sources.push(format!("{key} from this process's environment"));
            }
        }
    }

    if plugin.id == "forgejo" {
        let settings = crate::settings::load_or_default();
        let forge = settings.forges.iter().find(|forge| forge.kind == "forgejo");
        for key in ["FORGEJO_URL", "GITEA_HOST"] {
            if needed.iter().any(|needed| needed == key) && !found.contains_key(key) {
                if let Some(base) = forge.map(|forge| forge.base_url.trim()).filter(|base| !base.is_empty()) {
                    found.insert(key.to_string(), base.to_string());
                    sources.push(format!("{key} from the forge configured in Settings"));
                }
            }
        }
        for key in ["FORGEJO_TOKEN", "GITEA_ACCESS_TOKEN"] {
        if needed.iter().any(|needed| needed == key) && !found.contains_key(key) {
            let from_settings = forge
                .and_then(|forge| forge.token.clone())
                .filter(|token| !token.trim().is_empty());
            if let Some(token) = from_settings {
                found.insert(key.to_string(), token);
                sources.push(format!("{key} from Settings"));
            } else if let Some(home) = dirs::home_dir() {
                let path = home.join(".config").join("forgejo").join("token");
                if let Ok(token) = std::fs::read_to_string(&path) {
                    if !token.trim().is_empty() {
                        found.insert(key.to_string(), token.trim().to_string());
                        sources.push(format!("{key} from ~/.config/forgejo/token"));
                    }
                }
            }
        }
        }
    }

    (found, sources)
}

/// Does this plugin actually come up? "Connected" has to mean something.
///
/// http: one request, and any answer at all counts — a 401 proves the endpoint
/// is there and wants credentials, which is a different problem from a dead
/// host. stdio: start the server and give it a moment. An MCP server over
/// stdio waits for a request, so still running IS the success case; exiting
/// immediately is the failure, and its stderr says why.
pub async fn verify(plugin: &Plugin) -> Result<String, String> {
    match plugin.transport {
        Transport::Http => {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(12))
                .build()
                .map_err(|e| e.to_string())?;
            let mut request = client.get(plugin.url.trim());
            for (key, value) in &plugin.headers {
                if !value.trim().is_empty() {
                    request = request.header(key, value);
                }
            }
            match request.send().await {
                Ok(response) => Ok(format!("endpoint answered {}", response.status().as_u16())),
                Err(error) => Err(format!("endpoint unreachable: {error}")),
            }
        }
        Transport::Stdio => {
            let command = plugin.command.trim().to_string();
            let args = plugin.args.clone();
            let env: Vec<(String, String)> = plugin
                .env
                .iter()
                .filter(|(_, value)| !value.trim().is_empty())
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            tokio::task::spawn_blocking(move || {
                use std::process::{Command, Stdio};
                if crate::agents::resolve_binary(&command).is_none() {
                    return Err(format!("{command} is not on the PATH"));
                }
                let mut child = Command::new(&command)
                    .args(&args)
                    .envs(env)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("could not start {command}: {e}"))?;
                // npx may download the package first, so give it real time.
                for _ in 0..60 {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let mut stderr = String::new();
                            if let Some(mut pipe) = child.stderr.take() {
                                use std::io::Read;
                                let _ = pipe.read_to_string(&mut stderr);
                            }
                            let tail = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" ");
                            return Err(format!("the server exited ({status}): {tail}"));
                        }
                        Ok(None) => {}
                        Err(error) => return Err(format!("could not watch the server: {error}")),
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
                Ok("the server started and stayed up".to_string())
            })
            .await
            .map_err(|e| format!("verification task failed: {e}"))?
        }
    }
}

/// What an npm package actually is, before we point a plugin at it.
///
/// `npx -y forgejo-mcp` failed with "could not determine executable to run"
/// because that package ships no bin at all. One registry lookup says so in
/// advance, and an agent that can run this lookup can diagnose its own failure
/// instead of handing the error back.
pub async fn npm_package(name: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("https://registry.npmjs.org/{}", name.trim().replace('/', "%2f"));
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Ok(serde_json::json!({ "name": name, "exists": false }));
    }
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    let latest = body
        .pointer("/dist-tags/latest")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let version = body.pointer(&format!("/versions/{latest}")).cloned().unwrap_or(Value::Null);
    let bin = version.get("bin").cloned().unwrap_or(Value::Null);
    Ok(serde_json::json!({
        "name": name,
        "exists": true,
        "version": latest,
        "runnable_with_npx": !bin.is_null(),
        "executables": bin,
        "description": version.get("description").cloned().unwrap_or(Value::Null),
        "repository": version.get("repository").cloned().unwrap_or(Value::Null),
        "published": body.pointer(&format!("/time/{latest}")).cloned().unwrap_or(Value::Null),
    }))
}

/// Search npm, with the one fact that decides usability: does it ship a bin.
pub async fn npm_search(query: &str, limit: usize) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!(
        "https://registry.npmjs.org/-/v1/search?text={}&size={}",
        urlencoding_lite(query),
        limit.clamp(1, 10)
    );
    let body: Value = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for object in body.get("objects").and_then(Value::as_array).cloned().unwrap_or_default() {
        let name = object
            .pointer("/package/name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let detail = npm_package(&name).await.unwrap_or(Value::Null);
        out.push(serde_json::json!({
            "name": name,
            "description": object.pointer("/package/description").cloned().unwrap_or(Value::Null),
            "runnable_with_npx": detail.get("runnable_with_npx").cloned().unwrap_or(Value::Null),
            "version": detail.get("version").cloned().unwrap_or(Value::Null),
        }));
    }
    Ok(serde_json::json!({ "results": out }))
}

/// Minimal percent-encoding for a search term. A whole crate for two
/// characters would be the wrong trade.
fn urlencoding_lite(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

/// Repair a plugin's connector. Used by an agent that has just watched one
/// fail and worked out what it should have been.
pub fn set_command(id: &str, command: &str, args: Vec<String>) -> Result<Plugin, String> {
    let mut store = load_store();
    let plugin = store
        .plugins
        .iter_mut()
        .find(|item| item.id == id.trim())
        .ok_or_else(|| format!("no plugin called {id:?} in the library"))?;
    plugin.command = command.trim().to_string();
    plugin.args = args;
    let updated = plugin.clone();
    save_store(&store)?;
    Ok(updated)
}

/// What the owner called it → the id in the library.
///
/// A model will say "Forgejo", "forgejo" or "Forgejo plugin", and a tool that
/// only accepts the exact id turns a working request into silence.
pub fn resolve_id(text: &str) -> Option<String> {
    let needle = text.trim().to_lowercase();
    let needle = needle.trim_end_matches(" plugin").trim();
    if needle.is_empty() {
        return None;
    }
    let plugins = load_store().plugins;
    plugins
        .iter()
        .find(|plugin| plugin.id.to_lowercase() == needle || plugin.name.to_lowercase() == needle)
        .or_else(|| {
            plugins.iter().find(|plugin| {
                plugin.name.to_lowercase().replace(' ', "-") == needle
                    || plugin.id.replace('-', " ") == needle
            })
        })
        .map(|plugin| plugin.id.clone())
}

/// Fill in what we can find, switch it on, and PROVE it runs.
///
/// One call, because "add Forgejo" is one intention. Only a credential that
/// genuinely cannot be found comes back as a question.
pub async fn connect(id: &str) -> Result<Value, String> {
    let plugin = load_store()
        .plugins
        .into_iter()
        .find(|item| item.id == id.trim())
        .ok_or_else(|| format!("no plugin called {id:?} in the library"))?;

    let (found, sources) = discover(&plugin);
    let mut candidate = plugin.clone();
    for (key, value) in found {
        candidate.env.insert(key, value);
    }
    if let Some(reason) = blocker(&candidate) {
        // Structured, not just prose: the chat renders a sign-in card from
        // this, the way a person expects to be asked for a login — inline,
        // next to the request, not as instructions to go somewhere else.
        return Err(serde_json::json!({
            "needs_auth": {
                "id": candidate.id,
                "name": candidate.name,
                "description": candidate.description,
                "reason": reason,
                "missing": candidate
                    .required_env
                    .iter()
                    .filter(|key| candidate.env.get(*key).map(|value| value.trim().is_empty()).unwrap_or(true))
                    .cloned()
                    .collect::<Vec<_>>(),
                "needs_url": matches!(candidate.transport, Transport::Http) && candidate.url.trim().is_empty(),
            },
            "message": format!("{} {reason}", candidate.name),
        })
        .to_string());
    }

    let detail = verify(&candidate).await?;
    candidate.enabled = true;
    let mut store = load_store();
    match store.plugins.iter_mut().find(|item| item.id == candidate.id) {
        Some(existing) => *existing = candidate.clone(),
        None => store.plugins.push(candidate.clone()),
    }
    save_store(&store)?;
    Ok(serde_json::json!({
        "ok": true,
        "id": candidate.id,
        "name": candidate.name,
        "verified": detail,
        "credentials_found": sources,
    }))
}

/// Write the values the owner typed, verify the thing actually starts, switch
/// it on, and hand it to an agent — in ONE call.
///
/// The UI used to do this as save-then-grant from JavaScript, which had two
/// failure modes, two half-applied states, and an alert() for anything that
/// went wrong. A token typed three times and lost each time is what that costs.
#[tauri::command]
pub async fn plugin_connect(
    id: String,
    values: HashMap<String, String>,
    url: Option<String>,
    agent: Option<String>,
) -> Result<Value, String> {
    let resolved = resolve_id(&id).ok_or_else(|| format!("no plugin called {id:?} in the library"))?;
    {
        let mut store = load_store();
        let plugin = store
            .plugins
            .iter_mut()
            .find(|item| item.id == resolved)
            .ok_or_else(|| format!("no plugin called {resolved:?} in the library"))?;
        for (key, value) in values {
            if value.trim().is_empty() {
                continue; // never blank out a stored credential with an empty box
            }
            if plugin.headers.contains_key(&key) {
                plugin.headers.insert(key, value);
            } else {
                plugin.env.insert(key, value);
            }
        }
        if let Some(url) = url.as_ref().map(|url| url.trim()).filter(|url| !url.is_empty()) {
            plugin.url = url.to_string();
        }
        // Typed values make the entry his, so a later seed refresh cannot
        // reset it.
        plugin.owner_edited = true;
        save_store(&store)?;
    }

    let mut report = connect(&resolved).await?;
    if let Some(handle) = agent.as_ref().map(|handle| handle.trim()).filter(|handle| !handle.is_empty()) {
        let held = crate::agent_profiles::set_plugin_grant(handle, &resolved, true)?;
        report["handed_to"] = serde_json::json!(handle);
        report["agent_now_holds"] = serde_json::json!(held);
    }
    Ok(report)
}

/// A compact view of the library for an agent: enough to decide, not so much
/// that a credential could ride along in the answer.
pub fn catalog_snapshot() -> Vec<serde_json::Value> {
    load_store()
        .plugins
        .into_iter()
        .map(|plugin| {
            serde_json::json!({
                "id": plugin.id,
                "name": plugin.name,
                "category": plugin.category,
                "description": plugin.description,
                "enabled": plugin.enabled,
                "skills": plugin.skills,
                "blocked_by": blocker(&plugin),
            })
        })
        .collect()
}

/// Switch one plugin on or off. Refuses, with the reason, when it could not
/// actually run — the same gate the library applies.
pub fn set_enabled(id: &str, enabled: bool) -> Result<Plugin, String> {
    let mut store = load_store();
    let plugin = store
        .plugins
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("no plugin called {id:?} in the library"))?;
    if enabled {
        if let Some(reason) = blocker(plugin) {
            return Err(format!("{} {reason}", plugin.name));
        }
    }
    plugin.enabled = enabled;
    let updated = plugin.clone();
    save_store(&store)?;
    Ok(updated)
}

/// Everything the library has switched on and that can actually run.
pub fn active() -> Vec<Plugin> {
    load_store()
        .plugins
        .into_iter()
        .filter(|plugin| plugin.enabled && blocker(plugin).is_none())
        .collect()
}

/// The plugins ONE agent gets: the library's working set, narrowed to what
/// that agent was given.
///
/// Two gates on purpose. The library is where a plugin is configured once, with
/// its credential; the agent's own list is where it is handed out. A planner
/// has no business holding a payments server just because the owner connected
/// Stripe for something else.
pub fn active_for(capabilities: &[String]) -> Vec<Plugin> {
    let selected: Vec<&str> = capabilities
        .iter()
        .filter_map(|entry| entry.strip_prefix("plugin:"))
        .collect();
    if selected.is_empty() {
        return Vec::new();
    }
    active()
        .into_iter()
        .filter(|plugin| selected.iter().any(|id| *id == plugin.id))
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
            // This file carries every enabled plugin's credential in the clear,
            // in a directory every account on the machine can list. Taking the
            // secrets out of plugins.json and then writing them here at mode
            // 644 would have moved the exposure, not removed it.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
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

/// XNAUT_PLUGINS_PATH is process-global and cargo runs tests in parallel, so
/// two tests pointing the library at different scratch files raced and one
/// failed at random. Anything that redirects the store takes this first.
#[cfg(test)]
pub(crate) static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A scratch library for a test, plus the lock that keeps it to itself.
#[cfg(test)]
pub(crate) fn scratch_store(name: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
    let guard = STORE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = std::env::temp_dir().join(format!("xnaut-plugins-{name}.json"));
    let _ = std::fs::remove_file(&path);
    std::env::set_var("XNAUT_PLUGINS_PATH", &path);
    (guard, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Store isolation, plus the keychain. The service name is process-global
    /// and a plugin's env can hold a secret, so a test that only took the store
    /// lock could stash under one service and read back under another, or fall
    /// through to the owner's real keychain.
    struct Scratch {
        _store: std::sync::MutexGuard<'static, ()>,
        _keychain: std::sync::MutexGuard<'static, ()>,
    }

    fn scratch_store_local(name: &str) -> (Scratch, PathBuf) {
        let keychain =
            crate::secrets::tests::KEYCHAIN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::set_var("XNAUT_KEYCHAIN_SERVICE", "xnaut-test-plugins");
        let (store, path) = super::scratch_store(name);
        (Scratch { _store: store, _keychain: keychain }, path)
    }

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
            skills: vec![],
            owner_edited: false,
            enabled: true,
            docs_url: String::new(),
            seeded: false,
        }
    }

    #[tokio::test]
    #[ignore = "hits the network and starts a real server; run with --ignored"]
    async fn forgejo_connects_from_the_credentials_already_on_this_machine() {
        // The reply that started this: "it may require your Forgejo URL and an
        // access token" — while the token sat in ~/.config/forgejo/token and
        // the URL sat in Settings. Discovery has to find both without asking.
        let (_store_lock, scratch) = scratch_store_local("connect");
        let plugin = seed().into_iter().find(|p| p.id == "forgejo").unwrap();
        let (found, sources) = discover(&plugin);
        println!("discovered: {sources:?}");
        assert!(
            found.contains_key("GITEA_ACCESS_TOKEN"),
            "the token on this machine was not found: {sources:?}"
        );
        let report = connect("forgejo").await;
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);
        println!("connect: {report:?}");
        assert!(report.is_ok(), "connect failed: {report:?}");
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

    #[tokio::test]
    #[ignore = "starts a real MCP server; run with --ignored"]
    async fn a_token_typed_in_the_ui_survives_everything_that_runs_after_it() {
        // Reported three times: the Forgejo token would not stick. This is the
        // whole path the UI now takes — type it, connect, then let the reads
        // that happen afterwards (opening the panel, another turn) run.
        let (_store_lock, scratch) = scratch_store_local("token");
        let token = std::fs::read_to_string(
            dirs::home_dir().unwrap().join(".config/forgejo/token"),
        )
        .expect("a real token to type")
        .trim()
        .to_string();

        let report = plugin_connect(
            "Forgejo".into(),
            HashMap::from([("GITEA_ACCESS_TOKEN".to_string(), token.clone())]),
            None,
            None,
        )
        .await;
        assert!(report.is_ok(), "connect failed: {report:?}");

        // Everything that reads afterwards must leave it alone.
        let _ = catalog_snapshot();
        let _ = plugin_catalog();
        let _ = active();
        let stored = load_store()
            .plugins
            .into_iter()
            .find(|plugin| plugin.id == "forgejo")
            .unwrap();

        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        assert_eq!(stored.env.get("GITEA_ACCESS_TOKEN"), Some(&token), "the token was lost");
        assert!(stored.enabled, "it did not stay switched on");
        assert!(stored.owner_edited, "a typed credential must mark the entry as his");
    }

    #[test]
    fn a_credential_never_reaches_the_file_in_the_clear() {
        // XNAUT-213: plugins.json held a TSB JWT and an API key that anyone on
        // the machine could read. The value now lives in the keychain and the
        // file keeps a pointer; everything downstream must not notice.
        let (_store_lock, scratch) = scratch_store_local("keychain");
        if !crate::secrets::tests::keychain_usable() {
            return;
        }

        let mut plugin = stdio("securosys-attest");
        plugin.env.insert("SECUROSYS_JWT".into(), "eyJ0eXAi.demo.jwt".into());
        plugin.env.insert("SECUROSYS_TSB_URL".into(), "https://tsb.example".into());
        plugin.required_env = vec!["SECUROSYS_JWT".into()];
        plugin_save(plugin).expect("save");

        let on_disk = std::fs::read_to_string(&scratch).expect("read back");
        let reloaded = load_store()
            .plugins
            .into_iter()
            .find(|item| item.id == "securosys-attest")
            .expect("still in the library");
        let launched = claude_config(&[reloaded.clone()]).to_string();

        crate::secrets::forget("plugin/securosys-attest/SECUROSYS_JWT");
        crate::secrets::forget("plugin/securosys-attest/TOKEN");
        std::env::remove_var("XNAUT_KEYCHAIN_SERVICE");
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        assert!(!on_disk.contains("eyJ0eXAi.demo.jwt"), "the secret is still in the file");
        assert!(on_disk.contains("keychain:plugin/securosys-attest/SECUROSYS_JWT"));
        // Not a credential by name, so it stays readable where it is useful.
        assert!(on_disk.contains("https://tsb.example"));
        assert_eq!(
            reloaded.env.get("SECUROSYS_JWT").map(String::as_str),
            Some("eyJ0eXAi.demo.jwt"),
            "the panel must still see the real value"
        );
        assert!(launched.contains("eyJ0eXAi.demo.jwt"), "the agent must be launched with the real value");
    }

    #[test]
    fn a_plain_read_migrates_a_credential_the_owner_never_re_saved() {
        // XNAUT-214. Measured on the real library: 3 plaintext secrets, 0
        // keychain references, months after the migration shipped, because it
        // only ran on save and nothing had saved. Opening the file is the one
        // event that always happens.
        let (_store_lock, scratch) = scratch_store_local("migrate-on-load");
        if !crate::secrets::tests::keychain_usable() {
            return;
        }

        // Written the way a pre-migration install left it: the value itself.
        std::fs::write(
            &scratch,
            r#"{"plugins":[{"id":"securosys-attest","name":"a","description":"","transport":"stdio",
               "command":"npx","args":[],"url":"","headers":{},"category":"test","note":"",
               "env":{"SECUROSYS_JWT":"eyJ0eXAi.demo.jwt"},"required_env":["SECUROSYS_JWT"],
               "skills":[],"owner_edited":true,"enabled":false,"docs_url":"","seeded":false}]}"#,
        )
        .expect("seed the old shape");

        let read = load_store().plugins.into_iter().find(|item| item.id == "securosys-attest").unwrap();
        let on_disk = std::fs::read_to_string(&scratch).expect("read back");

        crate::secrets::forget("plugin/securosys-attest/SECUROSYS_JWT");
        std::env::remove_var("XNAUT_KEYCHAIN_SERVICE");
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        assert!(!on_disk.contains("eyJ0eXAi.demo.jwt"), "reading left the secret in the clear");
        assert!(on_disk.contains("keychain:plugin/securosys-attest/SECUROSYS_JWT"));
        assert_eq!(
            read.env.get("SECUROSYS_JWT").map(String::as_str),
            Some("eyJ0eXAi.demo.jwt"),
            "the caller must still get the real value"
        );
    }

    #[test]
    fn a_typed_api_key_survives_the_next_read() {
        // Reported: "the API keys in the Plugins are not persistent." This is
        // the exact round trip the UI does — save the plugin with a key, then
        // read the catalog again the way opening the panel does.
        let (_store_lock, scratch) = scratch_store_local("persist");

        let mut tavily = seed().into_iter().find(|plugin| plugin.id == "tavily").unwrap();
        tavily.env.insert("TAVILY_API_KEY".into(), "tvly-typed-by-hand".into());
        tavily.enabled = true;
        plugin_save(tavily).expect("save");

        let after_catalog = plugin_catalog().expect("catalog");
        let stored = load_store().plugins.into_iter().find(|plugin| plugin.id == "tavily").unwrap();

        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        assert_eq!(
            stored.env.get("TAVILY_API_KEY").map(String::as_str),
            Some("tvly-typed-by-hand"),
            "the key did not survive"
        );
        assert!(stored.enabled, "the switch did not survive");
        assert!(
            after_catalog.iter().any(|plugin| plugin.id == "tavily" && plugin.enabled),
            "the catalog does not report it as enabled"
        );
    }

    #[test]
    fn a_plugin_can_be_named_the_way_a_person_says_it() {
        // The model called connect_plugin with "Forgejo", not "forgejo", and a
        // strict lookup turned a working request into a no-op.
        let (_store_lock, scratch) = scratch_store_local("resolve");
        assert_eq!(resolve_id("Forgejo").as_deref(), Some("forgejo"));
        assert_eq!(resolve_id(" forgejo plugin ").as_deref(), Some("forgejo"));
        assert_eq!(resolve_id("Google Calendar").as_deref(), Some("google-calendar"));
        assert_eq!(resolve_id("nothing like this"), None);
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);
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
    fn the_compiled_catalog_loads_and_never_shadows_a_hand_seed() {
        // The asset is compiled from the public registries. A malformed one
        // must not empty the library, and it must never override an entry we
        // have actually run — the hand seeds carry notes the registries do not.
        let catalog = catalog_asset();
        assert!(catalog.len() > 50, "the catalog looks empty: {}", catalog.len());
        for plugin in &catalog {
            assert!(!plugin.id.trim().is_empty());
            match plugin.transport {
                Transport::Stdio => assert!(!plugin.command.trim().is_empty(), "{} has no command", plugin.id),
                Transport::Http => assert!(!plugin.url.trim().is_empty(), "{} has no url", plugin.id),
            }
        }
        // Where both describe the same server, ours wins: forgejo is seeded by
        // hand with the gitea-mcp command that actually runs.
        let (_lock, scratch) = scratch_store("catalog");
        let merged = load_store().plugins;
        let _ = std::fs::remove_file(&scratch);
        let forgejo = merged.iter().find(|plugin| plugin.id == "forgejo").expect("forgejo");
        assert_eq!(forgejo.args, vec!["-y".to_string(), "gitea-mcp".to_string()]);
        assert!(merged.len() > catalog.len(), "the merge lost entries");
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
    fn an_agent_only_gets_the_plugins_it_was_given() {
        // Configuring Stripe once must not hand a payments server to every
        // agent in the roster.
        let selected = vec!["skill:code-review".to_string(), "plugin:context7".to_string()];
        let ids: Vec<String> = active_for(&selected).into_iter().map(|p| p.id).collect();
        assert!(!ids.contains(&"stripe".to_string()));
        // And an agent given nothing gets nothing, rather than everything.
        assert!(active_for(&["skill:code-review".to_string()]).is_empty());
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

