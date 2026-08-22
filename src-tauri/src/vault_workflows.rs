use serde::Serialize;
use tauri::State;

use crate::settings::LlmSettings;

const MAX_AGENTS: usize = 6;
const MAX_DOCUMENT_CHARS: usize = 80_000;

#[derive(Clone, Serialize)]
pub struct VaultAgentResult {
    pub agent: String,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct VaultWorkflowResult {
    pub workflow: String,
    pub results: Vec<VaultAgentResult>,
    pub synthesis: String,
}

fn read_document(
    manager: &crate::vault::VaultManager,
    vault: &str,
    rel: &str,
) -> Result<String, String> {
    let indexes = manager
        .indexes
        .lock()
        .map_err(|_| "vault index lock poisoned")?;
    let index = indexes.get(vault).ok_or("vault not open")?;
    if !index.notes.contains_key(rel) {
        return Err("note not found".into());
    }
    let path = crate::vault::safe_join(&index.root, rel)?;
    std::fs::read_to_string(path).map_err(|error| format!("failed to read note: {error}"))
}

fn bounded_document(content: &str) -> &str {
    if content.len() <= MAX_DOCUMENT_CHARS {
        return content;
    }
    let mut end = MAX_DOCUMENT_CHARS;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    &content[..end]
}

fn clean_mermaid(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let without_open = trimmed
        .strip_prefix("```mermaid")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim();
    let clean = without_open
        .strip_suffix("```")
        .unwrap_or(without_open)
        .trim();
    let first = clean.lines().next().unwrap_or_default().trim();
    const ALLOWED: &[&str] = &[
        "flowchart",
        "graph",
        "sequenceDiagram",
        "stateDiagram",
        "stateDiagram-v2",
        "classDiagram",
        "erDiagram",
        "journey",
        "mindmap",
        "timeline",
    ];
    if clean.is_empty() || !ALLOWED.iter().any(|kind| first.starts_with(kind)) {
        return Err("the model did not return a supported Mermaid diagram".into());
    }
    Ok(clean.to_string())
}

fn workflow_instruction(workflow: &str) -> &'static str {
    match workflow {
        "research" => "Identify claims that need verification, missing evidence, and concrete research questions.",
        "architecture" => "Review boundaries, components, dependencies, data flow, failure modes, and implementation gaps.",
        "security" => "Threat-model the document. Identify assets, trust boundaries, abuse cases, mitigations, and verification steps.",
        "delivery" => "Turn the document into an executable delivery review: dependencies, acceptance criteria, risks, and next actions.",
        _ => "Review the document for contradictions, omissions, unclear decisions, and the highest-value improvements.",
    }
}

async fn run_agent(
    llm: LlmSettings,
    agent: String,
    workflow: String,
    document: String,
) -> VaultAgentResult {
    let system = format!(
        "You are {agent}, one member of a document workflow in xNAUT Vault. \
         Work independently and ground every observation in the supplied document. \
         Be concise, specific, and preserve the author's intent. Do not rewrite the document."
    );
    let user = format!(
        "WORKFLOW: {workflow}\nTASK: {}\n\nDOCUMENT:\n{}",
        workflow_instruction(&workflow),
        bounded_document(&document)
    );
    match crate::chat::complete_oneshot(&llm, Some(&system), &user).await {
        Ok(output) => VaultAgentResult {
            agent,
            output,
            error: None,
        },
        Err(error) => VaultAgentResult {
            agent,
            output: String::new(),
            error: Some(error),
        },
    }
}

#[tauri::command]
pub async fn vault_generate_diagram(
    settings: State<'_, crate::state::AppState>,
    manager: State<'_, crate::vault::VaultManager>,
    vault: String,
    rel: String,
    diagram_type: String,
) -> Result<String, String> {
    let document = read_document(&manager, &vault, &rel)?;
    let llm = settings.settings.lock().await.llm.clone();
    let system = "Convert documents into accurate Mermaid diagrams. Return only Mermaid source without markdown fences or commentary. Never invent components or relationships that the document does not support.";
    let user = format!(
        "Create a {} diagram from this document. Use concise labels, readable grouping, and a top-to-bottom flow unless the document clearly needs another direction.\n\nDOCUMENT:\n{}",
        diagram_type.trim(),
        bounded_document(&document)
    );
    clean_mermaid(&crate::chat::complete_oneshot(&llm, Some(system), &user).await?)
}

#[tauri::command]
pub async fn vault_document_workflow(
    settings: State<'_, crate::state::AppState>,
    manager: State<'_, crate::vault::VaultManager>,
    vault: String,
    rel: String,
    workflow: String,
    agents: Vec<String>,
) -> Result<VaultWorkflowResult, String> {
    let document = read_document(&manager, &vault, &rel)?;
    let llm = settings.settings.lock().await.llm.clone();
    let selected = agents
        .into_iter()
        .map(|agent| agent.trim().to_string())
        .filter(|agent| !agent.is_empty())
        .take(MAX_AGENTS)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("select at least one agent".into());
    }

    let runs = selected
        .into_iter()
        .map(|agent| run_agent(llm.clone(), agent, workflow.clone(), document.clone()));
    let results = futures_util::future::join_all(runs).await;
    let successful = results
        .iter()
        .filter(|result| result.error.is_none())
        .map(|result| format!("## {}\n{}", result.agent, result.output))
        .collect::<Vec<_>>();
    let synthesis = if successful.is_empty() {
        String::new()
    } else {
        let system = "Synthesize independent document reviews. Preserve disagreements, remove repetition, and produce concrete edits or decisions. Return Markdown only.";
        let user = format!(
            "Workflow: {workflow}\n\nDocument reviews:\n\n{}",
            successful.join("\n\n")
        );
        crate::chat::complete_oneshot(&llm, Some(system), &user).await?
    };

    Ok(VaultWorkflowResult {
        workflow,
        results,
        synthesis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_mermaid_fences() {
        assert_eq!(
            clean_mermaid("```mermaid\nflowchart TD\n A --> B\n```").unwrap(),
            "flowchart TD\n A --> B"
        );
    }

    #[test]
    fn rejects_prose_instead_of_diagram() {
        assert!(clean_mermaid("Here is your diagram").is_err());
    }

    #[test]
    fn truncation_stays_on_utf8_boundary() {
        let text = "é".repeat(MAX_DOCUMENT_CHARS);
        assert!(bounded_document(&text).is_char_boundary(bounded_document(&text).len()));
    }
}
