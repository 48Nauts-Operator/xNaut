// Per-agent capability policy (XNAUT-127 / XNAUT-157).
//
// The rule this module exists to keep: a capability the UI shows as OFF must
// be OFF because the runtime cannot do it, not because the prompt asked
// nicely. The old `access` presets denied `terminal` and `secrets` while
// enforcing nothing; that is the failure this replaces, and it is also the
// thing a compliance buyer would pay for, so an overclaim here is worse than
// a missing feature.
//
// Two layers, per XNAUT-127 and XNAUT-132:
//   launch-time (here) removes a capability outright — the CLI never gets the
//     tool, so there is nothing to argue with;
//   call-time (XNAUT-132's pre_tool veto) handles rules that depend on the
//     arguments, such as which path or which host, and hands the reason back
//     to the model so it can adapt.
// This module is the first half. It fails CLOSED on the capability (a denied
// tool is simply absent) while the veto layer fails open on a broken script.
//
// Every field therefore declares its enforcement level PER RUNTIME:
//   Enforced — translated into a launch flag the CLI itself obeys.
//   Advisory — stated in the prompt; the agent can still choose to ignore it.
// The UI badges each row with the level for the runtime actually selected, so
// nobody is misled about what is holding.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Enforcement {
    Enforced,
    Advisory,
}

impl Enforcement {
    pub fn as_str(self) -> &'static str {
        match self {
            Enforcement::Enforced => "enforced",
            Enforcement::Advisory => "advisory",
        }
    }
}

fn default_filesystem() -> String {
    "workspace-write".to_string()
}
fn default_network() -> String {
    "any".to_string()
}
fn yes() -> bool {
    true
}

/// Defaults reproduce today's behaviour exactly, so an existing profile that
/// has never seen this struct keeps working unchanged.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentPolicy {
    /// read-only | workspace-write | full
    #[serde(default = "default_filesystem")]
    pub filesystem: String,
    /// Roots beyond the project the agent may read, e.g. the vault.
    #[serde(default)]
    pub extra_roots: Vec<String>,
    #[serde(default = "yes")]
    pub shell: bool,
    #[serde(default = "yes")]
    pub web_fetch: bool,
    #[serde(default = "yes")]
    pub web_search: bool,
    /// none | allowlist | any
    #[serde(default = "default_network")]
    pub network: String,
    #[serde(default)]
    pub network_hosts: Vec<String>,
}

impl Default for AgentPolicy {
    fn default() -> Self {
        Self {
            filesystem: default_filesystem(),
            extra_roots: Vec::new(),
            shell: true,
            web_fetch: true,
            web_search: true,
            network: default_network(),
            network_hosts: Vec::new(),
        }
    }
}

/// What actually holds for a given runtime. The UI reads this to badge each
/// row; nothing else may claim enforcement.
pub fn enforcement_for(runtime_id: &str, field: &str) -> Enforcement {
    match (runtime_id, field) {
        // codex takes --sandbox, which the CLI itself obeys.
        ("codex", "filesystem") => Enforcement::Enforced,
        // claude takes --disallowedTools, which removes the tool entirely.
        ("claude", "filesystem") => Enforcement::Enforced,
        ("claude", "shell") => Enforcement::Enforced,
        ("claude", "web_fetch") => Enforcement::Enforced,
        ("claude", "web_search") => Enforcement::Enforced,
        // Network needs a sandbox or an egress proxy; nothing on the CLI
        // enforces it today, so it stays honest about being advisory.
        (_, "network") => Enforcement::Advisory,
        _ => Enforcement::Advisory,
    }
}

/// Tools claude must not be given, derived from the policy.
fn claude_disallowed(policy: &AgentPolicy) -> Vec<&'static str> {
    let mut denied = Vec::new();
    if policy.filesystem == "read-only" {
        denied.extend(["Write", "Edit", "NotebookEdit"]);
    }
    if !policy.shell {
        denied.push("Bash");
    }
    if !policy.web_fetch {
        denied.push("WebFetch");
    }
    if !policy.web_search {
        denied.push("WebSearch");
    }
    denied
}

/// Translate the policy into launch flags. Returns the arguments to append.
/// A runtime with no lever returns nothing rather than pretending.
pub fn launch_flags(runtime_id: &str, policy: &AgentPolicy) -> Vec<String> {
    match runtime_id {
        // codex rejects --sandbox together with --approve-for-me, because
        // --approve-for-me ALREADY means "workspace-write, approvals handled
        // automatically". So each policy maps to the one flag that expresses
        // it, never a pair the CLI refuses to start with.
        "codex" => match policy.filesystem.as_str() {
            "read-only" => vec!["--sandbox".to_string(), "read-only".to_string()],
            "full" => vec!["--dangerously-bypass-approvals-and-sandbox".to_string()],
            _ => vec!["--approve-for-me".to_string()],
        },
        "claude" => {
            let denied = claude_disallowed(policy);
            if denied.is_empty() {
                Vec::new()
            } else {
                vec!["--disallowedTools".to_string(), denied.join(",")]
            }
        }
        _ => Vec::new(),
    }
}

/// The advisory half, appended to the prompt. Only lines that are NOT
/// enforced for this runtime appear here — an enforced rule needs no asking,
/// and repeating it would suggest the agent has a choice.
pub fn advisory_lines(runtime_id: &str, policy: &AgentPolicy) -> Vec<String> {
    let mut lines = Vec::new();
    if enforcement_for(runtime_id, "filesystem") == Enforcement::Advisory {
        match policy.filesystem.as_str() {
            "read-only" => lines.push("Do not write or edit files; this run is read-only.".into()),
            "workspace-write" => {
                lines.push("Write only inside the project you were given.".into())
            }
            _ => {}
        }
    }
    if enforcement_for(runtime_id, "shell") == Enforcement::Advisory && !policy.shell {
        lines.push("Do not run shell commands.".into());
    }
    if policy.network == "none" {
        lines.push("Do not make network requests.".into());
    } else if policy.network == "allowlist" && !policy.network_hosts.is_empty() {
        lines.push(format!(
            "Network access is limited to: {}.",
            policy.network_hosts.join(", ")
        ));
    }
    if !policy.extra_roots.is_empty() {
        lines.push(format!(
            "You may also read: {}.",
            policy.extra_roots.join(", ")
        ));
    }
    lines
}

/// The UI asks Rust which rows genuinely enforce for a runtime, rather than
/// keeping its own copy of the table — a mirrored list would drift, and a
/// wrong "enforced" badge is the exact overclaim this module exists to stop.
#[tauri::command]
pub fn policy_enforcement(runtime_id: String) -> std::collections::HashMap<String, String> {
    ["filesystem", "shell", "web_fetch", "web_search", "network"]
        .into_iter()
        .map(|field| {
            (
                field.to_string(),
                enforcement_for(&runtime_id, field).as_str().to_string(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_only() -> AgentPolicy {
        AgentPolicy {
            filesystem: "read-only".into(),
            ..Default::default()
        }
    }

    #[test]
    fn codex_never_gets_a_flag_pair_it_refuses_to_start_with() {
        // `--sandbox` with `--approve-for-me` is a hard error: the second
        // already implies workspace-write. Passing both broke every codex run.
        for filesystem in ["read-only", "workspace-write", "full"] {
            let policy = AgentPolicy {
                filesystem: filesystem.into(),
                ..Default::default()
            };
            let flags = launch_flags("codex", &policy);
            let sandboxed = flags.iter().any(|flag| flag == "--sandbox");
            let auto = flags.iter().any(|flag| flag == "--approve-for-me");
            assert!(!(sandboxed && auto), "{filesystem} produced {flags:?}");
        }
    }

    #[test]
    fn defaults_keep_codex_writing_inside_the_workspace() {
        let policy = AgentPolicy::default();
        assert_eq!(
            launch_flags("codex", &policy),
            vec!["--approve-for-me".to_string()],
            "approve-for-me is workspace-write with approvals handled"
        );
        assert!(launch_flags("claude", &policy).is_empty());
    }

    #[test]
    fn read_only_denies_writing_on_codex() {
        // The deny path is the product. If this ever returns workspace-write
        // for a read-only policy, we would be selling a claim we do not keep.
        let flags = launch_flags("codex", &read_only());
        assert_eq!(flags, vec!["--sandbox".to_string(), "read-only".to_string()]);
        assert!(!flags.contains(&"workspace-write".to_string()));
        assert!(!flags.contains(&"--approve-for-me".to_string()));
    }

    #[test]
    fn read_only_removes_the_write_tools_on_claude() {
        let flags = launch_flags("claude", &read_only());
        assert_eq!(flags[0], "--disallowedTools");
        for tool in ["Write", "Edit", "NotebookEdit"] {
            assert!(flags[1].contains(tool), "{tool} still reachable: {}", flags[1]);
        }
    }

    #[test]
    fn switching_off_shell_and_web_removes_those_tools() {
        let policy = AgentPolicy {
            shell: false,
            web_fetch: false,
            web_search: false,
            ..Default::default()
        };
        let flags = launch_flags("claude", &policy);
        assert!(flags[1].contains("Bash"));
        assert!(flags[1].contains("WebFetch"));
        assert!(flags[1].contains("WebSearch"));
    }

    #[test]
    fn full_access_is_explicit_never_implied() {
        let policy = AgentPolicy {
            filesystem: "full".into(),
            ..Default::default()
        };
        assert!(launch_flags("codex", &policy)
            .contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
    }

    #[test]
    fn a_runtime_without_levers_claims_nothing() {
        // gemini has no sandbox flag we can rely on: emit no flags rather
        // than a flag that does nothing.
        assert!(launch_flags("gemini", &read_only()).is_empty());
        assert_eq!(
            enforcement_for("gemini", "filesystem"),
            Enforcement::Advisory
        );
    }

    #[test]
    fn advisory_text_appears_only_where_nothing_enforces() {
        // claude enforces the filesystem, so it must not also be asked.
        let claude = advisory_lines("claude", &read_only());
        assert!(!claude.iter().any(|line| line.contains("read-only")));
        // gemini has no lever, so the prompt is the only thing left.
        let gemini = advisory_lines("gemini", &read_only());
        assert!(gemini.iter().any(|line| line.contains("read-only")));
    }

    #[test]
    fn network_is_always_advisory_today() {
        let policy = AgentPolicy {
            network: "none".into(),
            ..Default::default()
        };
        assert_eq!(enforcement_for("codex", "network"), Enforcement::Advisory);
        assert!(advisory_lines("codex", &policy)
            .iter()
            .any(|line| line.contains("network")));
    }
}
