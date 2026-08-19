// A hook that can say no (XNAUT-132).
//
// Ported from jcode (github.com/1jehuang/jcode, MIT), `docs/HOOKS.md`, whose
// `pre_tool` is the only gating hook of its five: exit 0 permits, exit 2
// blocks, and the stderr comes back to the model as the tool error so it can
// adapt. Everything else in their system is fire and forget, on the stated
// principle that a broken policy script should degrade to "no policy" rather
// than brick every session.
//
// Two departures, both deliberate:
//
//   THE DECISION IS CENTRAL, NOT PER SCRIPT. jcode runs a script the user
//   writes. Here the script is a thin courier that POSTs to the app, because
//   xNAUT already knows which agent is calling, what it owns, and what tier
//   the work is. A rule that needs that context cannot live in a shell script
//   the agent could also read.
//
//   THE POLICY IS DATA. Rules live in `veto.toml`, not in this file. XNAUT-132
//   is explicit that it is the mechanism and not the rules, and a policy in
//   code is a policy that needs a release to change.
//
// What is NOT here, also from jcode: rewriting arguments. A veto is allow or
// refuse. A wrapper that silently edits a call leaves the model believing it
// did something it did not do, and that confusion is worse than a refusal.
//
// FAILS OPEN, EVERYWHERE. No policy file, an unparseable one, a rule that
// matches nothing, the app unreachable, the request malformed: all allow. The
// only thing that blocks is a rule that matched. An agent wedged by its own
// safety rail is a worse outcome than a call that should have been stopped,
// because the first happens every time and the second is rare and recoverable.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct VetoRequest {
    /// Tool the agent is about to run: "Bash", "Write", "Edit", …
    ///
    /// The alias is not decoration. The hook script forwards the harness's own
    /// PreToolUse envelope verbatim, and that envelope calls these `tool_name`
    /// and `tool_input`. Without the alias every field arrived at its
    /// `serde(default)`, so `tool` was "" and `input` was null: no rule could
    /// ever match, the policy was dead on the only path that uses it, and
    /// nothing errored because a request full of defaults deserialises fine.
    /// Found 2026-08-19 while documenting the feature, not by a test: the smoke
    /// test drove the SCRIPT against a stub server, so it never exercised this
    /// struct. Its sibling one route over (`agent_hooks::HookPayload`) had the
    /// harness's names right all along.
    #[serde(default, alias = "tool_name")]
    pub tool: String,
    /// The tool's own input, verbatim. Shapes differ per tool, so rules match
    /// on the flattened text rather than on a schema we would have to track.
    #[serde(default, alias = "tool_input")]
    pub input: serde_json::Value,
    /// Who is calling. The harness does not know, so the hook script adds it
    /// from the environment xNAUT launched the agent with.
    #[serde(default, alias = "agent_handle")]
    pub agent: String,
    #[serde(default, alias = "working_dir")]
    pub cwd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Deny { reason: String },
    /// Hold the call and put the question to the owner (XNAUT-189).
    ///
    /// A policy that can only refuse is a policy nobody leaves switched on:
    /// every rule has to be written for the worst case, so the useful middle
    /// ("this one is fine, but tell me") had nowhere to go. The id is the
    /// inbox item the hook script then waits on; it is empty until
    /// `handle_veto` has created that item.
    Ask { reason: String, id: String },
}

/// One rule. Every field is optional and every present field must match, so a
/// rule with no fields at all matches nothing rather than everything.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Rule {
    /// Tool name, exactly, case-insensitive. Omit to match any tool.
    #[serde(default)]
    pub tool: Option<String>,
    /// Substring of the flattened input, case-insensitive.
    #[serde(default)]
    pub contains: Option<String>,
    /// Path prefix the call must be under to match.
    #[serde(default)]
    pub under: Option<String>,
    /// Agent handle this applies to. Omit for every agent.
    #[serde(default)]
    pub agent: Option<String>,
    /// What the model is told. Written for the model, since this is the text
    /// it reads and reasons about.
    pub reason: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Policy {
    #[serde(default)]
    pub deny: Vec<Rule>,
    /// Rules that hold the call and ask, rather than refusing it. Deny is
    /// checked first: a call that is forbidden is never worth interrupting
    /// the owner about.
    #[serde(default)]
    pub ask: Vec<Rule>,
}

pub fn policy_path() -> PathBuf {
    if let Ok(path) = std::env::var("XNAUT_VETO_POLICY") {
        return PathBuf::from(path);
    }
    dirs::config_dir()
        .map(|dir| dir.join("xnaut").join("veto.toml"))
        .unwrap_or_else(|| PathBuf::from("veto.toml"))
}

/// The policy, or an empty one. A broken file is an empty policy, never an
/// error: a typo in a rule must not stop every agent on the machine.
pub fn load() -> Policy {
    let Ok(text) = std::fs::read_to_string(policy_path()) else {
        return Policy::default();
    };
    match toml::from_str::<Policy>(&text) {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("[veto] {} is not readable, so no policy applies: {error}", policy_path().display());
            Policy::default()
        }
    }
}

/// Everything in the tool input as one lowercase string.
///
/// Rules match text rather than schema because every tool shapes its input
/// differently and new tools arrive without warning. A missed match fails open,
/// which is the direction this whole module leans.
fn flatten(input: &serde_json::Value) -> String {
    fn walk(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::String(text) => {
                out.push_str(text);
                out.push(' ');
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            serde_json::Value::Object(map) => map.values().for_each(|item| walk(item, out)),
            other => {
                out.push_str(&other.to_string());
                out.push(' ');
            }
        }
    }
    let mut out = String::new();
    walk(input, &mut out);
    out.to_lowercase()
}

pub fn decide_with(policy: &Policy, request: &VetoRequest) -> Decision {
    // Deny wins. A call that is forbidden is never worth interrupting the
    // owner about, so the ask list is only consulted once nothing denies.
    if let Some(rule) = first_match(&policy.deny, request) {
        return Decision::Deny { reason: rule.reason.clone() };
    }
    if let Some(rule) = first_match(&policy.ask, request) {
        return Decision::Ask { reason: rule.reason.clone(), id: String::new() };
    }
    Decision::Allow
}

fn first_match<'a>(rules: &'a [Rule], request: &VetoRequest) -> Option<&'a Rule> {
    let haystack = flatten(&request.input);
    let cwd = request.cwd.to_lowercase();
    let agent = request.agent.trim_start_matches('@').to_lowercase();

    for rule in rules {
        // A rule with nothing to match on is a mistake in the policy file, and
        // treating it as "deny everything" would take the machine down.
        if rule.tool.is_none() && rule.contains.is_none() && rule.under.is_none() {
            continue;
        }
        if let Some(tool) = &rule.tool {
            if !tool.eq_ignore_ascii_case(request.tool.trim()) {
                continue;
            }
        }
        if let Some(needle) = &rule.contains {
            if !haystack.contains(&needle.to_lowercase()) {
                continue;
            }
        }
        if let Some(under) = &rule.under {
            let under = under.to_lowercase();
            if !cwd.starts_with(&under) && !haystack.contains(&under) {
                continue;
            }
        }
        if let Some(who) = &rule.agent {
            if !who.trim_start_matches('@').eq_ignore_ascii_case(&agent) {
                continue;
            }
        }
        return Some(rule);
    }
    None
}

pub fn decide(request: &VetoRequest) -> Decision {
    decide_with(&load(), request)
}

/// The endpoint the hook script posts to. Unauthenticated on purpose: it is
/// bound to loopback, and a token check that fails closed would turn a lost
/// token into a machine where no agent can run any tool.
pub async fn handle_veto(
    axum::extract::State(ctx): axum::extract::State<crate::agent_hooks::ServerCtx>,
    axum::Json(request): axum::Json<VetoRequest>,
) -> axum::Json<Decision> {
    // Every tool call passes here, which makes this the one place that sees a
    // write before it happens. Two agents reaching for the same file is worth
    // saying out loud (XNAUT-190); it is not grounds to refuse either of them,
    // so it never touches the decision below.
    if let Some(conflict) = crate::claims::note(&request.agent, &request.tool, &request.input) {
        let req = crate::inbox::PostRequest {
            from: request.agent.trim().to_string(),
            title: format!("Two agents are editing {}", short_path(&conflict.file)),
            body: conflict.description.clone(),
            level: "warn".to_string(),
            context: std::collections::BTreeMap::from([
                ("file".to_string(), conflict.file.clone()),
                ("other agent".to_string(), conflict.other.clone()),
            ]),
            ..Default::default()
        };
        let _ = crate::inbox::create_and_announce(&ctx.app, "notify", req, None);
        crate::ledger::record("conflict", &request.agent, "", &format!("{} with @{}", conflict.file, conflict.other));
    }

    let decision = decide(&request);
    match &decision {
        Decision::Deny { reason } => {
            crate::ledger::record("refused", &request.agent, "", &format!("{}: {reason}", request.tool));
            axum::Json(decision)
        }
        Decision::Ask { reason, .. } => {
            // The answer takes as long as a person takes, and this request is
            // held open by a hook script the harness is waiting on. So the
            // question goes to the inbox now and the id goes back immediately;
            // the script waits on /v1/inbox/wait/:id, where a timeout is
            // already an allow.
            let req = crate::inbox::PostRequest {
                from: if request.agent.trim().is_empty() {
                    "system".to_string()
                } else {
                    request.agent.trim().to_string()
                },
                title: format!("Allow {}?", tool_label(&request)),
                body: reason.clone(),
                level: "warn".to_string(),
                context: ask_context(&request),
                ..Default::default()
            };
            match crate::inbox::create_and_announce(&ctx.app, "approve", req, None) {
                Ok(item) => {
                    crate::ledger::record("asked", &request.agent, "", &format!("{}: {reason}", request.tool));
                    axum::Json(Decision::Ask { reason: reason.clone(), id: item.id })
                }
                // Nothing to wait on means nobody can answer, and a question
                // nobody can answer must not become a block.
                Err(_) => axum::Json(Decision::Allow),
            }
        }
        Decision::Allow => axum::Json(decision),
    }
}

/// The tail of a path, for a title that has to fit on one line.
fn short_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((_, name)) if !name.is_empty() => name.to_string(),
        _ => trimmed.to_string(),
    }
}

/// A one-line name for the call, for the inbox title.
fn tool_label(request: &VetoRequest) -> String {
    let tool = request.tool.trim();
    let tool = if tool.is_empty() { "this call" } else { tool };
    match request.input.get("command").and_then(serde_json::Value::as_str) {
        Some(command) if !command.trim().is_empty() => {
            let command = command.trim();
            let short: String = command.chars().take(60).collect();
            format!("{tool}: {short}{}", if command.chars().count() > 60 { "…" } else { "" })
        }
        _ => tool.to_string(),
    }
}

/// What the owner needs to decide without opening the session.
fn ask_context(request: &VetoRequest) -> std::collections::BTreeMap<String, String> {
    let mut context = std::collections::BTreeMap::new();
    context.insert("tool".to_string(), request.tool.trim().to_string());
    if !request.agent.trim().is_empty() {
        context.insert("agent".to_string(), request.agent.trim().to_string());
    }
    if !request.cwd.trim().is_empty() {
        context.insert("cwd".to_string(), request.cwd.trim().to_string());
    }
    let input = flatten(&request.input);
    if !input.is_empty() {
        context.insert("input".to_string(), input.chars().take(400).collect());
    }
    context
}

/// The policy file as text, for the editor.
#[tauri::command]
pub fn veto_read() -> serde_json::Value {
    let path = policy_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "text": text,
        "rules": load().deny.len(),
    })
}

/// Check text without writing it. The editor calls this as you type.
#[tauri::command]
pub fn veto_validate(text: String) -> serde_json::Value {
    match toml::from_str::<Policy>(&text) {
        Ok(policy) => {
            // A rule with no condition parses and then does nothing, which is
            // the one mistake a syntax check would otherwise wave through.
            let inert = policy
                .deny
                .iter()
                .filter(|rule| rule.tool.is_none() && rule.contains.is_none() && rule.under.is_none())
                .count();
            serde_json::json!({ "ok": true, "rules": policy.deny.len(), "inert": inert })
        }
        Err(error) => serde_json::json!({ "ok": false, "error": error.to_string() }),
    }
}

/// Write the policy, keeping a copy of what was there.
///
/// Text in, text out: comments and grouping are the part that explains WHY a
/// rule exists, and a structured editor that rewrote the file from parsed
/// rules would delete all of it on the first save.
///
/// Nothing broken is ever written. A parse failure returns the message and
/// leaves the file alone, because a policy file that does not parse is a
/// machine with no policy at all, and the failure would be silent.
#[tauri::command]
pub fn veto_write(text: String) -> Result<serde_json::Value, String> {
    let policy: Policy = toml::from_str(&text).map_err(|error| format!("{error}"))?;

    let path = policy_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    // Back up first, and only when there is something to lose.
    let mut backup = None;
    if path.exists() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let target = path.with_extension(format!("toml.{stamp}.bak"));
        std::fs::copy(&path, &target).map_err(|error| format!("could not back up the policy: {error}"))?;
        backup = Some(target.display().to_string());
    }
    std::fs::write(&path, &text).map_err(|error| format!("could not write {}: {error}", path.display()))?;

    // Read back rather than trust the write: this file is a control.
    let in_force = load().deny.len();
    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "rules": policy.deny.len(),
        "in_force": in_force,
        "backup": backup,
    }))
}

/// Every backup, newest first, so a bad edit is one click from undone.
#[tauri::command]
pub fn veto_backups() -> Vec<serde_json::Value> {
    let path = policy_path();
    let Some(dir) = path.parent() else { return Vec::new() };
    let mut found: Vec<(std::time::SystemTime, String, u64)> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.starts_with("veto.toml.") && name.ends_with(".bak")
                })
                .filter_map(|entry| {
                    let meta = entry.metadata().ok()?;
                    Some((meta.modified().ok()?, entry.path().display().to_string(), meta.len()))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found
        .into_iter()
        .take(10)
        .map(|(_, path, size)| serde_json::json!({ "path": path, "bytes": size }))
        .collect()
}

/// Read the policy, for a settings screen.
#[tauri::command]
pub fn veto_rules() -> serde_json::Value {
    let policy = load();
    serde_json::json!({
        "path": policy_path().display().to_string(),
        "exists": policy_path().exists(),
        "rules": policy.deny.iter().map(|rule| serde_json::json!({
            "tool": rule.tool, "contains": rule.contains, "under": rule.under,
            "agent": rule.agent, "reason": rule.reason,
        })).collect::<Vec<_>>(),
    })
}

/// Try a call against the policy without running it. This is how a rule gets
/// checked before it is trusted, rather than by waiting for the call it was
/// meant to stop.
#[tauri::command]
pub fn veto_check(tool: String, input: String, agent: Option<String>, cwd: Option<String>) -> Decision {
    decide(&VetoRequest {
        tool,
        input: serde_json::from_str(&input).unwrap_or(serde_json::Value::String(input.clone())),
        agent: agent.unwrap_or_default(),
        cwd: cwd.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `XNAUT_VETO_POLICY` is process-wide, so the two tests that write a real
    /// file have to take turns. Without this they read each other's policy and
    /// fail as if the veto were broken.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
    }

    fn request(tool: &str, input: serde_json::Value) -> VetoRequest {
        VetoRequest { tool: tool.into(), input, agent: "rudi".into(), cwd: "/f/12-websites/dat-ag-website".into() }
    }

    #[test]
    /// Deserialise what the HARNESS actually sends, not what we wish it sent.
    ///
    /// Claude Code's PreToolUse envelope names these `tool_name` and
    /// `tool_input`. Every field on VetoRequest is `serde(default)`, so the
    /// wrong names did not error: they produced an empty tool and a null input,
    /// which no rule can match. The policy was dead on the only path that uses
    /// it, silently, and the smoke test could not see it because it drove the
    /// SCRIPT against a stub server and never built this struct.
    ///
    /// So this test feeds the real envelope, byte for byte.
    fn the_harness_envelope_deserialises_into_a_request_rules_can_match() {
        let envelope = serde_json::json!({
            "session_id": "abc123",
            "transcript_path": "/tmp/transcript.jsonl",
            "cwd": "/Users/cand0rian/DevHub_Studio/factory/02-Development/xnaut",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": "git push origin main", "description": "push" },
            "agent": "rudi"
        });
        let request: VetoRequest = serde_json::from_value(envelope).expect("the real envelope must parse");
        assert_eq!(request.tool, "Bash", "the tool name never arrived, so no rule can match");
        assert_eq!(request.agent, "rudi");
        assert!(request.cwd.ends_with("xnaut"));

        // And the whole point: a rule written against that tool now fires.
        let policy = policy(
            r#"
            [[deny]]
            tool = "Bash"
            contains = "git push"
            reason = "Not from an agent."
            "#,
        );
        assert!(
            matches!(decide_with(&policy, &request), Decision::Deny { .. }),
            "a rule that matches the real envelope has to fire"
        );
    }

    #[test]
    /// The middle tier (XNAUT-189). A policy that can only refuse is one nobody
    /// leaves switched on, so a rule must be able to hold the call and ask.
    fn an_ask_rule_holds_the_call_instead_of_refusing_it() {
        let policy = policy(
            r#"
            [[ask]]
            tool = "Bash"
            contains = "git push"
            reason = "Pushing to a shared branch. Fine by me?"
            "#,
        );
        let decision = decide_with(&policy, &request("Bash", serde_json::json!({"command": "git push origin main"})));
        match decision {
            Decision::Ask { reason, id } => {
                assert!(reason.contains("Fine by me"), "{reason}");
                // Empty until handle_veto has an inbox item to point at; the
                // decision function itself creates nothing.
                assert!(id.is_empty());
            }
            other => panic!("expected an ask, got {other:?}"),
        }
    }

    #[test]
    /// Deny beats ask. Interrupting the owner to confirm something the policy
    /// already forbids is the worst of both designs.
    fn a_call_that_is_denied_is_never_asked_about() {
        let policy = policy(
            r#"
            [[deny]]
            tool = "Bash"
            contains = "git push"
            reason = "Never from an agent."

            [[ask]]
            tool = "Bash"
            contains = "git push"
            reason = "Fine by me?"
            "#,
        );
        assert!(matches!(
            decide_with(&policy, &request("Bash", serde_json::json!({"command": "git push"}))),
            Decision::Deny { .. }
        ));
    }

    #[test]
    /// The wire shape is what the hook script matches on, and it matches with
    /// `case`, not a JSON parser. If the tag ever changes, the script stops
    /// asking and silently allows.
    fn the_ask_decision_serialises_to_what_the_script_looks_for() {
        let json = serde_json::to_string(&Decision::Ask {
            reason: "check first".into(),
            id: "in-123".into(),
        })
        .unwrap();
        assert!(json.contains(r#""decision":"ask""#), "{json}");
        assert!(json.contains(r#""id":"in-123""#), "{json}");
    }

    #[test]
    /// An ask rule with nothing to match on must not swallow every call, the
    /// same way a deny rule must not.
    fn an_ask_rule_with_nothing_to_match_on_is_ignored() {
        let policy = policy("[[ask]]\nreason = \"why not\"\n");
        assert_eq!(
            decide_with(&policy, &request("Bash", serde_json::json!({"command": "ls"}))),
            Decision::Allow
        );
    }

    fn policy(toml_text: &str) -> Policy {
        toml::from_str(toml_text).expect("policy parses")
    }

    /// The editor writes TEXT, so comments survive; and nothing broken is ever
    /// written, because a policy file that does not parse is a machine with no
    /// policy and the failure would be silent.
    #[test]
    fn the_editor_keeps_comments_backs_up_and_refuses_broken_input() {
        let _guard = lock();
        let path = std::env::temp_dir().join(format!("xnaut-veto-edit-{}/veto.toml", std::process::id()));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        std::env::set_var("XNAUT_VETO_POLICY", &path);

        let first = "# publishing is the line\n[[deny]]\ntool = \"Bash\"\ncontains = \"git push\"\nreason = \"ask first\"\n";
        let saved = veto_write(first.into()).expect("write");
        assert_eq!(saved["rules"], 1);
        assert_eq!(saved["in_force"], 1);
        assert!(saved["backup"].is_null(), "there was nothing to back up yet");

        // The comment is still there, which a structured writer would have eaten.
        let read = veto_read();
        assert!(read["text"].as_str().unwrap().contains("# publishing is the line"));

        // A second save keeps the previous file.
        let second = format!("{first}\n[[deny]]\ntool = \"Bash\"\ncontains = \"git tag\"\nreason = \"ask\"\n");
        let saved = veto_write(second).expect("write");
        assert_eq!(saved["rules"], 2);
        let backup = saved["backup"].as_str().expect("a backup was kept");
        assert!(std::fs::read_to_string(backup).unwrap().contains("git push"));
        assert_eq!(veto_backups().len(), 1);

        // Broken input is refused, and the file on disk is untouched.
        let error = veto_write("[[deny]]\ntool = ".into()).unwrap_err();
        assert!(!error.trim().is_empty());
        assert_eq!(load().deny.len(), 2, "a broken save replaced a working policy");

        // Validation reports inert rules, which parse but do nothing.
        let checked = veto_validate("[[deny]]\nreason = \"no conditions\"\n".into());
        assert_eq!(checked["ok"], true);
        assert_eq!(checked["inert"], 1);
        let bad = veto_validate("nonsense = [".into());
        assert_eq!(bad["ok"], false);
        assert!(bad["error"].as_str().is_some_and(|text| !text.is_empty()));

        std::env::remove_var("XNAUT_VETO_POLICY");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn no_policy_allows_everything() {
        // The default state of the machine. If this ever fails, every agent on
        // it stops working.
        let empty = Policy::default();
        assert_eq!(decide_with(&empty, &request("Bash", serde_json::json!({"command": "rm -rf /"}))), Decision::Allow);
    }

    #[test]
    fn a_rule_blocks_and_says_why_in_words_the_model_can_use() {
        let policy = policy(r#"
            [[deny]]
            tool = "Bash"
            contains = "git push"
            reason = "Pushing a website publishes it. Ask André in the inbox instead."
        "#);
        let blocked = decide_with(&policy, &request("Bash", serde_json::json!({"command": "git push origin main"})));
        match blocked {
            Decision::Deny { reason } => {
                assert!(reason.contains("inbox"), "the reason has to tell the model what to do instead: {reason}");
            }
            other => panic!("a push was not refused: {other:?}"),
        }
        // A different command through the same tool is untouched.
        assert_eq!(
            decide_with(&policy, &request("Bash", serde_json::json!({"command": "git status"}))),
            Decision::Allow
        );
        // And a different tool with the same text is untouched, because the
        // rule named a tool.
        assert_eq!(
            decide_with(&policy, &request("Write", serde_json::json!({"content": "git push origin main"}))),
            Decision::Allow
        );
    }

    #[test]
    fn rules_can_be_scoped_to_a_path_or_an_agent() {
        let policy = policy(r#"
            [[deny]]
            tool = "Write"
            under = "/f/12-websites/dat-ag-website"
            reason = "The DAT AG site is a client site."

            [[deny]]
            agent = "social"
            tool = "Bash"
            reason = "Social does not run commands."
        "#);
        // In scope by cwd.
        assert!(matches!(
            decide_with(&policy, &request("Write", serde_json::json!({"file_path": "/f/12-websites/dat-ag-website/index.html"}))),
            Decision::Deny { .. }
        ));
        // Another site is not covered by that rule.
        let elsewhere = VetoRequest { cwd: "/f/12-websites/48nauts.com".into(), ..request("Write", serde_json::json!({"file_path": "/f/12-websites/48nauts.com/index.html"})) };
        assert_eq!(decide_with(&policy, &elsewhere), Decision::Allow);
        // Agent-scoped rules only bind that agent.
        let social = VetoRequest { agent: "social".into(), ..request("Bash", serde_json::json!({"command": "ls"})) };
        assert!(matches!(decide_with(&policy, &social), Decision::Deny { .. }));
        assert_eq!(decide_with(&policy, &request("Bash", serde_json::json!({"command": "ls"}))), Decision::Allow);
    }

    #[test]
    fn a_path_matches_in_the_arguments_as_well_as_the_cwd() {
        // An agent sitting in one directory can still write into another, so a
        // path rule that only looked at cwd would be trivially avoidable.
        let policy = policy(r#"
            [[deny]]
            under = "/f/12-websites/dat-ag-website"
            reason = "client site"
        "#);
        let from_elsewhere = VetoRequest {
            cwd: "/tmp".into(),
            ..request("Write", serde_json::json!({"file_path": "/f/12-websites/dat-ag-website/index.html"}))
        };
        assert!(matches!(decide_with(&policy, &from_elsewhere), Decision::Deny { .. }));
    }

    #[test]
    fn a_rule_with_nothing_to_match_on_is_ignored_rather_than_matching_all() {
        // The dangerous typo: a rule whose conditions were all removed. Reading
        // it as "deny everything" would stop every agent on the machine at once.
        let policy = policy(r#"
            [[deny]]
            reason = "someone deleted the conditions"
        "#);
        assert_eq!(decide_with(&policy, &request("Bash", serde_json::json!({"command": "ls"}))), Decision::Allow);
    }

    #[test]
    fn a_broken_policy_file_is_no_policy() {
        let _guard = lock();
        let path = std::env::temp_dir().join(format!("xnaut-veto-{}.toml", std::process::id()));
        std::fs::write(&path, "[[deny]]\ntool = \"Bash\"\nreason = ").expect("write");
        std::env::set_var("XNAUT_VETO_POLICY", &path);
        assert_eq!(decide(&request("Bash", serde_json::json!({"command": "anything"}))), Decision::Allow);

        // And a good file at the same path does apply, so the test above is
        // about the parse failure rather than the path being wrong.
        std::fs::write(&path, "[[deny]]\ntool = \"Bash\"\ncontains = \"push\"\nreason = \"no\"").expect("write");
        assert!(matches!(decide(&request("Bash", serde_json::json!({"command": "git push"}))), Decision::Deny { .. }));

        std::env::remove_var("XNAUT_VETO_POLICY");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn input_is_flattened_whatever_shape_the_tool_uses() {
        // Every tool shapes its input differently and new ones arrive without
        // warning, so rules match text rather than a schema we would have to
        // keep up with.
        let nested = serde_json::json!({
            "edits": [{ "old_string": "a", "new_string": "git push --force" }],
            "meta": { "count": 2 }
        });
        assert!(flatten(&nested).contains("git push --force"));
        assert!(flatten(&nested).contains('2'));
    }
}
