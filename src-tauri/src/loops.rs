//! Versioned workflow definitions and durable execution state for xNAUT Loops.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

const SCHEMA_VERSION: u32 = 1;
const EVENT_NAME: &str = "loops://run-event";

fn mutation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    #[default]
    Draft,
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Trigger,
    Agent,
    Action,
    Decision,
    HumanApproval,
    Transform,
    Retry,
    Parallel,
    Subflow,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowPort {
    pub id: String,
    #[serde(default = "default_port_type")]
    pub data_type: String,
    #[serde(default)]
    pub required: bool,
}

fn default_port_type() -> String {
    "any".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionRule {
    pub resource: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelPolicyKind {
    Local,
    Balanced,
    Frontier,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPolicy {
    pub kind: ModelPolicyKind,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    #[serde(default)]
    pub inputs: Vec<WorkflowPort>,
    #[serde(default)]
    pub outputs: Vec<WorkflowPort>,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub permissions: Vec<PermissionRule>,
    #[serde(default)]
    pub model_policy: Option<ModelPolicy>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConnection {
    pub id: String,
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowLimits {
    pub max_duration_seconds: u64,
    pub max_node_executions: u32,
    #[serde(default)]
    pub max_agent_calls: Option<u32>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub max_cost_usd: Option<f64>,
}

impl Default for WorkflowLimits {
    fn default() -> Self {
        Self {
            max_duration_seconds: 1800,
            max_node_executions: 100,
            max_agent_calls: None,
            max_tokens: None,
            max_cost_usd: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePresentation {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowPresentation {
    #[serde(default)]
    pub nodes: BTreeMap<String, NodePresentation>,
    #[serde(default)]
    pub viewport_x: f64,
    #[serde(default)]
    pub viewport_y: f64,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
}

fn default_zoom() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub status: WorkflowStatus,
    #[serde(default)]
    pub limits: WorkflowLimits,
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub connections: Vec<WorkflowConnection>,
    #[serde(default)]
    pub presentation: WorkflowPresentation,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub severity: ValidationSeverity,
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub connection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub findings: Vec<ValidationFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowIndex {
    id: String,
    name: String,
    latest_version: u32,
    active_version: Option<u32>,
    project: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub latest_version: u32,
    pub active_version: Option<u32>,
    pub project: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    WaitingForApproval,
    WaitingForInput,
    Paused,
    Failed,
    Cancelled,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeRunStatus {
    Pending,
    Ready,
    Running,
    WaitingForApproval,
    Completed,
    Failed,
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRunState {
    pub node_id: String,
    pub status: NodeRunStatus,
    pub attempts: u32,
    #[serde(default)]
    pub generation: u32,
    #[serde(default)]
    pub operation_key: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: String,
    pub workflow_id: String,
    pub workflow_version: u32,
    #[serde(default)]
    pub project: Option<String>,
    pub status: RunStatus,
    pub input: Value,
    pub nodes: BTreeMap<String, NodeRunState>,
    pub node_executions: u32,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub cancelled_reason: Option<String>,
    pub next_event_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub sequence: u64,
    pub run_id: String,
    pub event: String,
    pub timestamp: String,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub details: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartRunRequest {
    pub workflow_id: String,
    #[serde(default)]
    pub workflow_version: Option<u32>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloneWorkflowRequest {
    pub source_id: String,
    #[serde(default)]
    pub source_version: Option<u32>,
    pub new_id: String,
    pub new_name: String,
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompleteNodeRequest {
    pub run_id: String,
    pub node_id: String,
    #[serde(default)]
    pub output: Value,
    #[serde(default)]
    pub outcomes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FailNodeRequest {
    pub run_id: String,
    pub node_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalRequest {
    pub run_id: String,
    pub node_id: String,
    pub actor: String,
    pub approved: bool,
    #[serde(default)]
    pub comment: String,
}

fn loops_root() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .ok_or_else(|| "local application data directory is unavailable".to_string())
        .map(|root| root.join("xnaut").join("loops"))
}

fn validate_id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(format!(
            "{label} must use 1-80 ASCII letters, numbers, hyphens, or underscores"
        ));
    }
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(&tmp, bytes)
        .map_err(|error| format!("failed to write {}: {error}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|error| format!("failed to replace {}: {error}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))
}

fn finding(
    severity: ValidationSeverity,
    code: &str,
    message: impl Into<String>,
    node_id: Option<&str>,
    connection_id: Option<&str>,
) -> ValidationFinding {
    ValidationFinding {
        severity,
        code: code.into(),
        message: message.into(),
        node_id: node_id.map(str::to_string),
        connection_id: connection_id.map(str::to_string),
    }
}

pub fn validate_definition(definition: &WorkflowDefinition) -> ValidationReport {
    let mut findings = Vec::new();
    if definition.schema_version != SCHEMA_VERSION {
        findings.push(finding(
            ValidationSeverity::Error,
            "schema.unsupported",
            format!(
                "schema version {} is unsupported; expected {SCHEMA_VERSION}",
                definition.schema_version
            ),
            None,
            None,
        ));
    }
    if validate_id(&definition.id, "workflow id").is_err() {
        findings.push(finding(
            ValidationSeverity::Error,
            "workflow.invalid_id",
            "workflow id is invalid",
            None,
            None,
        ));
    }
    if definition.name.trim().is_empty() {
        findings.push(finding(
            ValidationSeverity::Error,
            "workflow.missing_name",
            "workflow name is required",
            None,
            None,
        ));
    }
    if definition.version == 0 {
        findings.push(finding(
            ValidationSeverity::Error,
            "workflow.invalid_version",
            "workflow version must be greater than zero",
            None,
            None,
        ));
    }
    if definition.limits.max_duration_seconds == 0 || definition.limits.max_node_executions == 0 {
        findings.push(finding(
            ValidationSeverity::Error,
            "workflow.missing_limits",
            "duration and node-execution limits must be greater than zero",
            None,
            None,
        ));
    }
    if definition
        .limits
        .max_cost_usd
        .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        findings.push(finding(
            ValidationSeverity::Error,
            "workflow.invalid_cost_limit",
            "cost limit must be a finite positive number",
            None,
            None,
        ));
    }

    let mut node_ids = HashSet::new();
    let mut nodes = HashMap::new();
    let mut trigger_count = 0;
    let mut output_count = 0;
    for node in &definition.nodes {
        if !node_ids.insert(node.id.as_str()) {
            findings.push(finding(
                ValidationSeverity::Error,
                "node.duplicate_id",
                format!("duplicate node id: {}", node.id),
                Some(&node.id),
                None,
            ));
            continue;
        }
        if validate_id(&node.id, "node id").is_err() {
            findings.push(finding(
                ValidationSeverity::Error,
                "node.invalid_id",
                "node id is invalid",
                Some(&node.id),
                None,
            ));
        }
        if node.name.trim().is_empty() {
            findings.push(finding(
                ValidationSeverity::Error,
                "node.missing_name",
                "node name is required",
                Some(&node.id),
                None,
            ));
        }
        if node.kind == NodeKind::Trigger {
            trigger_count += 1;
        }
        if node.kind == NodeKind::Output {
            output_count += 1;
        }
        let mut ports = HashSet::new();
        for port in node.inputs.iter().chain(node.outputs.iter()) {
            if !ports.insert(port.id.as_str()) {
                findings.push(finding(
                    ValidationSeverity::Error,
                    "port.duplicate_id",
                    format!("duplicate port id on node {}: {}", node.id, port.id),
                    Some(&node.id),
                    None,
                ));
            }
        }
        if node.kind != NodeKind::Trigger
            && node.kind != NodeKind::Output
            && node.timeout_seconds.unwrap_or(0) == 0
        {
            findings.push(finding(
                ValidationSeverity::Error,
                "node.missing_timeout",
                "executable nodes require a timeout",
                Some(&node.id),
                None,
            ));
        }
        if node.model_policy.as_ref().is_some_and(|policy| {
            policy.kind == ModelPolicyKind::Fixed
                && (policy.provider.as_deref().unwrap_or("").trim().is_empty()
                    || policy.model.as_deref().unwrap_or("").trim().is_empty())
        }) {
            findings.push(finding(
                ValidationSeverity::Error,
                "node.incomplete_fixed_model",
                "fixed model policy requires both provider and model",
                Some(&node.id),
                None,
            ));
        }
        nodes.insert(node.id.as_str(), node);
    }
    if definition.nodes.is_empty() {
        findings.push(finding(
            ValidationSeverity::Error,
            "workflow.empty",
            "workflow requires at least one node",
            None,
            None,
        ));
    }
    if trigger_count == 0 {
        findings.push(finding(
            ValidationSeverity::Error,
            "workflow.missing_trigger",
            "workflow requires a trigger node",
            None,
            None,
        ));
    }
    if output_count == 0 {
        findings.push(finding(
            ValidationSeverity::Error,
            "workflow.missing_output",
            "workflow requires an output node",
            None,
            None,
        ));
    }

    let mut connection_ids = HashSet::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for connection in &definition.connections {
        if !connection_ids.insert(connection.id.as_str()) {
            findings.push(finding(
                ValidationSeverity::Error,
                "connection.duplicate_id",
                format!("duplicate connection id: {}", connection.id),
                None,
                Some(&connection.id),
            ));
        }
        let Some(from) = nodes.get(connection.from_node.as_str()) else {
            findings.push(finding(
                ValidationSeverity::Error,
                "connection.missing_source",
                format!("source node does not exist: {}", connection.from_node),
                None,
                Some(&connection.id),
            ));
            continue;
        };
        let Some(to) = nodes.get(connection.to_node.as_str()) else {
            findings.push(finding(
                ValidationSeverity::Error,
                "connection.missing_target",
                format!("target node does not exist: {}", connection.to_node),
                None,
                Some(&connection.id),
            ));
            continue;
        };
        let source = from
            .outputs
            .iter()
            .find(|port| port.id == connection.from_port);
        let target = to.inputs.iter().find(|port| port.id == connection.to_port);
        if source.is_none() {
            findings.push(finding(
                ValidationSeverity::Error,
                "connection.missing_source_port",
                format!("source port does not exist: {}", connection.from_port),
                Some(&connection.from_node),
                Some(&connection.id),
            ));
        }
        if target.is_none() {
            findings.push(finding(
                ValidationSeverity::Error,
                "connection.missing_target_port",
                format!("target port does not exist: {}", connection.to_port),
                Some(&connection.to_node),
                Some(&connection.id),
            ));
        }
        if let (Some(source), Some(target)) = (source, target) {
            if source.data_type != "any"
                && target.data_type != "any"
                && source.data_type != target.data_type
            {
                findings.push(finding(
                    ValidationSeverity::Error,
                    "connection.incompatible_types",
                    format!(
                        "cannot connect {} to {}",
                        source.data_type, target.data_type
                    ),
                    Some(&connection.to_node),
                    Some(&connection.id),
                ));
            }
        }
        if connection.from_node == connection.to_node {
            findings.push(finding(
                ValidationSeverity::Error,
                "connection.self_cycle",
                "a node cannot connect directly to itself",
                Some(&connection.from_node),
                Some(&connection.id),
            ));
        }
        adjacency
            .entry(connection.from_node.as_str())
            .or_default()
            .push(connection.to_node.as_str());
    }

    let mut reachable = HashSet::new();
    let mut queue: VecDeque<&str> = definition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Trigger)
        .map(|node| node.id.as_str())
        .collect();
    while let Some(id) = queue.pop_front() {
        if !reachable.insert(id) {
            continue;
        }
        for next in adjacency.get(id).into_iter().flatten() {
            queue.push_back(next);
        }
    }
    for node in &definition.nodes {
        if !reachable.contains(node.id.as_str()) {
            findings.push(finding(
                ValidationSeverity::Error,
                "node.unreachable",
                "node is not reachable from a trigger",
                Some(&node.id),
                None,
            ));
        }
    }

    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for connection in &definition.connections {
        if nodes.contains_key(connection.from_node.as_str())
            && nodes.contains_key(connection.to_node.as_str())
        {
            reverse
                .entry(connection.to_node.as_str())
                .or_default()
                .push(connection.from_node.as_str());
        }
    }
    let mut reaches_output = HashSet::new();
    let mut queue: VecDeque<&str> = definition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Output)
        .map(|node| node.id.as_str())
        .collect();
    while let Some(id) = queue.pop_front() {
        if !reaches_output.insert(id) {
            continue;
        }
        for previous in reverse.get(id).into_iter().flatten() {
            queue.push_back(previous);
        }
    }
    for node in &definition.nodes {
        if reachable.contains(node.id.as_str()) && !reaches_output.contains(node.id.as_str()) {
            findings.push(finding(
                ValidationSeverity::Error,
                "node.no_output_path",
                "node has no path to an output",
                Some(&node.id),
                None,
            ));
        }
    }

    for cycle in cyclic_components(&definition.nodes, &adjacency) {
        if !cycle.iter().any(|id| {
            nodes
                .get(id.as_str())
                .is_some_and(|node| node.kind == NodeKind::Retry)
        }) {
            findings.push(finding(
                ValidationSeverity::Error,
                "workflow.unbounded_cycle",
                "each cycle requires a Retry node and workflow execution limit",
                cycle.first().map(String::as_str),
                None,
            ));
        }
    }

    ValidationReport {
        valid: !findings
            .iter()
            .any(|item| item.severity == ValidationSeverity::Error),
        findings,
    }
}

fn cyclic_components(
    nodes: &[WorkflowNode],
    adjacency: &HashMap<&str, Vec<&str>>,
) -> Vec<BTreeSet<String>> {
    fn visit<'a>(
        id: &'a str,
        adjacency: &HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        stack: &mut Vec<&'a str>,
        stack_positions: &mut HashMap<&'a str, usize>,
        cycles: &mut BTreeMap<String, BTreeSet<String>>,
    ) {
        if visited.contains(id) {
            return;
        }
        stack_positions.insert(id, stack.len());
        stack.push(id);
        for next in adjacency.get(id).into_iter().flatten() {
            if let Some(start) = stack_positions.get(next).copied() {
                let cycle: BTreeSet<String> = stack[start..]
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect();
                let key = cycle.iter().cloned().collect::<Vec<_>>().join("\u{1f}");
                cycles.insert(key, cycle);
            } else {
                visit(next, adjacency, visited, stack, stack_positions, cycles);
            }
        }
        stack.pop();
        stack_positions.remove(id);
        visited.insert(id);
    }
    let mut visited = HashSet::new();
    let mut stack = Vec::new();
    let mut stack_positions = HashMap::new();
    let mut cycles = BTreeMap::new();
    for node in nodes {
        visit(
            &node.id,
            adjacency,
            &mut visited,
            &mut stack,
            &mut stack_positions,
            &mut cycles,
        );
    }
    cycles.into_values().collect()
}

fn workflow_dir(root: &Path, id: &str) -> PathBuf {
    root.join("workflows").join(id)
}

fn workflow_index_path(root: &Path, id: &str) -> PathBuf {
    workflow_dir(root, id).join("index.json")
}

fn workflow_version_path(root: &Path, id: &str, version: u32) -> PathBuf {
    workflow_dir(root, id).join(format!("v{version}.json"))
}

fn run_path(root: &Path, id: &str) -> PathBuf {
    root.join("runs").join(id).join("run.json")
}

fn save_definition_at(
    root: &Path,
    mut definition: WorkflowDefinition,
) -> Result<WorkflowDefinition, String> {
    let report = validate_definition(&definition);
    if !report.valid {
        return Err(serde_json::to_string(&report).unwrap_or_else(|_| "invalid workflow".into()));
    }
    let now = Utc::now().to_rfc3339();
    let index_path = workflow_index_path(root, &definition.id);
    let existing: Option<WorkflowIndex> = index_path
        .exists()
        .then(|| read_json(&index_path))
        .transpose()?;
    if let Some(index) = &existing {
        if definition.version != index.latest_version + 1 {
            return Err(format!(
                "next workflow version must be {}",
                index.latest_version + 1
            ));
        }
    } else if definition.version != 1 {
        return Err("first workflow version must be 1".into());
    }
    if definition.created_at.is_empty() {
        definition.created_at = existing
            .as_ref()
            .and_then(|index| {
                read_json::<WorkflowDefinition>(&workflow_version_path(root, &index.id, 1))
                    .ok()
                    .map(|item| item.created_at)
            })
            .unwrap_or_else(|| now.clone());
    }
    definition.updated_at = now.clone();
    definition.status = WorkflowStatus::Draft;
    write_json_atomic(
        &workflow_version_path(root, &definition.id, definition.version),
        &definition,
    )?;
    let index = WorkflowIndex {
        id: definition.id.clone(),
        name: definition.name.clone(),
        latest_version: definition.version,
        active_version: existing.and_then(|item| item.active_version),
        project: definition.project.clone(),
        updated_at: now,
    };
    write_json_atomic(&index_path, &index)?;
    Ok(definition)
}

fn list_definitions_at(root: &Path, project: Option<&str>) -> Result<Vec<WorkflowSummary>, String> {
    let base = root.join("workflows");
    let Ok(entries) = std::fs::read_dir(base) else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
        let path = entry.path().join("index.json");
        if !path.is_file() {
            continue;
        }
        let index: WorkflowIndex = read_json(&path)?;
        if project.is_some_and(|key| index.project.as_deref() != Some(key)) {
            continue;
        }
        result.push(WorkflowSummary {
            id: index.id,
            name: index.name,
            latest_version: index.latest_version,
            active_version: index.active_version,
            project: index.project,
            updated_at: index.updated_at,
        });
    }
    result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(result)
}

fn get_definition_at(
    root: &Path,
    id: &str,
    version: Option<u32>,
) -> Result<WorkflowDefinition, String> {
    validate_id(id, "workflow id")?;
    let index: WorkflowIndex = read_json(&workflow_index_path(root, id))?;
    let version = version.unwrap_or(index.latest_version);
    read_json(&workflow_version_path(root, id, version))
}

fn activate_definition_at(root: &Path, id: &str, version: u32) -> Result<WorkflowSummary, String> {
    validate_id(id, "workflow id")?;
    let mut index: WorkflowIndex = read_json(&workflow_index_path(root, id))?;
    let definition: WorkflowDefinition = read_json(&workflow_version_path(root, id, version))?;
    let report = validate_definition(&definition);
    if !report.valid {
        return Err("workflow has blocking validation findings".into());
    }
    index.active_version = Some(version);
    index.updated_at = Utc::now().to_rfc3339();
    write_json_atomic(&workflow_index_path(root, id), &index)?;
    Ok(WorkflowSummary {
        id: index.id,
        name: index.name,
        latest_version: index.latest_version,
        active_version: index.active_version,
        project: index.project,
        updated_at: index.updated_at,
    })
}

fn clone_definition_at(
    root: &Path,
    request: CloneWorkflowRequest,
) -> Result<WorkflowDefinition, String> {
    validate_id(&request.new_id, "new workflow id")?;
    if workflow_index_path(root, &request.new_id).exists() {
        return Err(format!("workflow already exists: {}", request.new_id));
    }
    if request.new_name.trim().is_empty() {
        return Err("new workflow name is required".into());
    }
    let mut definition = get_definition_at(root, &request.source_id, request.source_version)?;
    definition.id = request.new_id;
    definition.name = request.new_name.trim().into();
    definition.project = request.project.or(definition.project);
    definition.version = 1;
    definition.status = WorkflowStatus::Draft;
    definition.created_at.clear();
    definition.updated_at.clear();
    save_definition_at(root, definition)
}

fn append_event_at(
    root: &Path,
    run: &mut WorkflowRun,
    event: &str,
    node_id: Option<&str>,
    details: Value,
) -> Result<RunEvent, String> {
    let record = RunEvent {
        sequence: run.next_event_sequence,
        run_id: run.id.clone(),
        event: event.into(),
        timestamp: Utc::now().to_rfc3339(),
        node_id: node_id.map(str::to_string),
        details,
    };
    run.next_event_sequence += 1;
    let path = root
        .join("runs")
        .join(&run.id)
        .join("events")
        .join(format!("{:010}.json", record.sequence));
    write_json_atomic(&path, &record)?;
    Ok(record)
}

fn persist_run(root: &Path, run: &WorkflowRun) -> Result<(), String> {
    write_json_atomic(&run_path(root, &run.id), run)
}

fn emit_event(app: Option<&AppHandle>, event: &RunEvent) {
    if let Some(app) = app {
        let _ = app.emit(EVENT_NAME, event);
    }
}

fn start_run_at(
    root: &Path,
    request: StartRunRequest,
    app: Option<&AppHandle>,
) -> Result<WorkflowRun, String> {
    validate_id(&request.workflow_id, "workflow id")?;
    let index: WorkflowIndex = read_json(&workflow_index_path(root, &request.workflow_id))?;
    let version = request
        .workflow_version
        .or(index.active_version)
        .ok_or("workflow has no active version")?;
    let definition = get_definition_at(root, &request.workflow_id, Some(version))?;
    let report = validate_definition(&definition);
    if !report.valid {
        return Err("workflow has blocking validation findings".into());
    }
    let now = Utc::now().to_rfc3339();
    let mut nodes = BTreeMap::new();
    for node in &definition.nodes {
        nodes.insert(
            node.id.clone(),
            NodeRunState {
                node_id: node.id.clone(),
                status: if node.kind == NodeKind::Trigger {
                    NodeRunStatus::Completed
                } else {
                    NodeRunStatus::Pending
                },
                attempts: 0,
                generation: 0,
                operation_key: None,
                started_at: None,
                completed_at: (node.kind == NodeKind::Trigger).then(|| now.clone()),
                output: (node.kind == NodeKind::Trigger).then(|| request.input.clone()),
                error: None,
            },
        );
    }
    let mut run = WorkflowRun {
        id: uuid::Uuid::new_v4().to_string(),
        workflow_id: definition.id.clone(),
        workflow_version: version,
        project: request.project.or(definition.project.clone()),
        status: RunStatus::Running,
        input: request.input,
        nodes,
        node_executions: 0,
        created_at: now.clone(),
        updated_at: now,
        completed_at: None,
        cancelled_reason: None,
        next_event_sequence: 1,
    };
    let event = append_event_at(
        root,
        &mut run,
        "run.started",
        None,
        serde_json::json!({ "workflow_version": version }),
    )?;
    emit_event(app, &event);
    for trigger in definition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Trigger)
    {
        schedule_downstream(&definition, &mut run, &trigger.id, &[], root, app)?;
    }
    refresh_run_status(&definition, &mut run);
    persist_run(root, &run)?;
    Ok(run)
}

fn read_run_at(root: &Path, id: &str) -> Result<WorkflowRun, String> {
    validate_id(id, "run id")?;
    read_json(&run_path(root, id))
}

fn list_runs_at(root: &Path, workflow_id: Option<&str>) -> Result<Vec<WorkflowRun>, String> {
    let base = root.join("runs");
    let Ok(entries) = std::fs::read_dir(base) else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
        let path = entry.path().join("run.json");
        if !path.is_file() {
            continue;
        }
        let run: WorkflowRun = read_json(&path)?;
        if workflow_id.is_some_and(|id| run.workflow_id != id) {
            continue;
        }
        result.push(run);
    }
    result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(result)
}

fn list_events_at(root: &Path, run_id: &str) -> Result<Vec<RunEvent>, String> {
    validate_id(run_id, "run id")?;
    let base = root.join("runs").join(run_id).join("events");
    let Ok(entries) = std::fs::read_dir(base) else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|value| value.to_str()) == Some("json") {
            result.push(read_json(&entry.path())?);
        }
    }
    result.sort_by_key(|event: &RunEvent| event.sequence);
    Ok(result)
}

fn node_definition<'a>(
    definition: &'a WorkflowDefinition,
    id: &str,
) -> Result<&'a WorkflowNode, String> {
    definition
        .nodes
        .iter()
        .find(|node| node.id == id)
        .ok_or_else(|| format!("node not found: {id}"))
}

fn load_run_definition(root: &Path, run: &WorkflowRun) -> Result<WorkflowDefinition, String> {
    get_definition_at(root, &run.workflow_id, Some(run.workflow_version))
}

fn claim_node_at(
    root: &Path,
    run_id: &str,
    node_id: &str,
    app: Option<&AppHandle>,
) -> Result<WorkflowRun, String> {
    let mut run = read_run_at(root, run_id)?;
    if !matches!(
        run.status,
        RunStatus::Running | RunStatus::WaitingForApproval
    ) {
        return Err("run is not active".into());
    }
    let definition = load_run_definition(root, &run)?;
    let node = node_definition(&definition, node_id)?;
    let state = run
        .nodes
        .get_mut(node_id)
        .ok_or_else(|| format!("node state not found: {node_id}"))?;
    if state.status != NodeRunStatus::Ready {
        return Err(format!("node is not ready: {node_id}"));
    }
    state.attempts += 1;
    state.operation_key = Some(format!("{}:{}:{}", run.id, node_id, state.generation));
    state.started_at = Some(Utc::now().to_rfc3339());
    state.error = None;
    state.status = if node.kind == NodeKind::HumanApproval {
        NodeRunStatus::WaitingForApproval
    } else {
        NodeRunStatus::Running
    };
    run.node_executions += 1;
    if run.node_executions > definition.limits.max_node_executions {
        run.status = RunStatus::Failed;
        state.status = NodeRunStatus::Failed;
        state.error = Some("workflow node-execution limit exceeded".into());
    } else if node.kind == NodeKind::HumanApproval {
        run.status = RunStatus::WaitingForApproval;
    }
    run.updated_at = Utc::now().to_rfc3339();
    let event_name = if node.kind == NodeKind::HumanApproval {
        "node.waiting_for_approval"
    } else {
        "node.started"
    };
    let event = append_event_at(root, &mut run, event_name, Some(node_id), Value::Null)?;
    persist_run(root, &run)?;
    emit_event(app, &event);
    Ok(run)
}

fn schedule_downstream(
    definition: &WorkflowDefinition,
    run: &mut WorkflowRun,
    node_id: &str,
    outcomes: &[String],
    root: &Path,
    app: Option<&AppHandle>,
) -> Result<(), String> {
    let source = node_definition(definition, node_id)?;
    let source_generation = run
        .nodes
        .get(node_id)
        .map(|state| state.generation)
        .unwrap_or(0);
    let next_generation = source_generation + u32::from(source.kind == NodeKind::Retry);
    let outgoing: Vec<&WorkflowConnection> = definition
        .connections
        .iter()
        .filter(|connection| {
            connection.from_node == node_id
                && (outcomes.is_empty()
                    || outcomes
                        .iter()
                        .any(|outcome| outcome == &connection.from_port))
        })
        .collect();
    for connection in outgoing {
        let target = node_definition(definition, &connection.to_node)?;
        let target_state = run
            .nodes
            .get(&connection.to_node)
            .ok_or_else(|| format!("node state not found: {}", connection.to_node))?;
        let first_execution = target_state.status == NodeRunStatus::Pending;
        let next_cycle = next_generation > target_state.generation
            && matches!(
                target_state.status,
                NodeRunStatus::Completed
                    | NodeRunStatus::Failed
                    | NodeRunStatus::Skipped
                    | NodeRunStatus::Cancelled
            );
        if !first_execution && !next_cycle {
            continue;
        }
        let join_all = target
            .config
            .get("join")
            .and_then(Value::as_str)
            .is_some_and(|value| value == "all");
        if join_all {
            let all_sources_complete = definition
                .connections
                .iter()
                .filter(|item| item.to_node == connection.to_node)
                .all(|item| {
                    run.nodes
                        .get(&item.from_node)
                        .is_some_and(|state| state.status == NodeRunStatus::Completed)
                });
            if !all_sources_complete {
                continue;
            }
        }
        if let Some(state) = run.nodes.get_mut(&connection.to_node) {
            state.status = NodeRunStatus::Ready;
            state.generation = next_generation;
            state.started_at = None;
            state.completed_at = None;
            state.output = None;
            state.error = None;
            state.operation_key = None;
        }
        let event = append_event_at(
            root,
            run,
            "node.ready",
            Some(&connection.to_node),
            serde_json::json!({ "from_node": node_id, "generation": next_generation }),
        )?;
        emit_event(app, &event);
    }
    Ok(())
}

fn refresh_run_status(definition: &WorkflowDefinition, run: &mut WorkflowRun) {
    if matches!(run.status, RunStatus::Cancelled | RunStatus::Failed) {
        return;
    }
    let output_complete = definition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Output)
        .any(|node| {
            run.nodes
                .get(&node.id)
                .is_some_and(|state| state.status == NodeRunStatus::Completed)
        });
    if output_complete {
        run.status = RunStatus::Completed;
        run.completed_at = Some(Utc::now().to_rfc3339());
        for state in run.nodes.values_mut() {
            if matches!(state.status, NodeRunStatus::Pending | NodeRunStatus::Ready) {
                state.status = NodeRunStatus::Skipped;
            }
        }
    } else if run
        .nodes
        .values()
        .any(|state| state.status == NodeRunStatus::WaitingForApproval)
    {
        run.status = RunStatus::WaitingForApproval;
    } else if run
        .nodes
        .values()
        .any(|state| state.status == NodeRunStatus::Failed)
    {
        run.status = RunStatus::Failed;
    } else {
        run.status = RunStatus::Running;
    }
    run.updated_at = Utc::now().to_rfc3339();
}

fn complete_node_at(
    root: &Path,
    request: CompleteNodeRequest,
    app: Option<&AppHandle>,
) -> Result<WorkflowRun, String> {
    let mut run = read_run_at(root, &request.run_id)?;
    let definition = load_run_definition(root, &run)?;
    let node = node_definition(&definition, &request.node_id)?;
    if node.kind == NodeKind::HumanApproval {
        return Err("approval nodes must use the approval command".into());
    }
    let state = run
        .nodes
        .get_mut(&request.node_id)
        .ok_or_else(|| format!("node state not found: {}", request.node_id))?;
    if state.status != NodeRunStatus::Running {
        return Err("node is not running".into());
    }
    state.status = NodeRunStatus::Completed;
    state.completed_at = Some(Utc::now().to_rfc3339());
    state.output = Some(request.output);
    state.error = None;
    let event = append_event_at(
        root,
        &mut run,
        "node.completed",
        Some(&request.node_id),
        serde_json::json!({ "outcomes": request.outcomes }),
    )?;
    emit_event(app, &event);
    schedule_downstream(
        &definition,
        &mut run,
        &request.node_id,
        &request.outcomes,
        root,
        app,
    )?;
    refresh_run_status(&definition, &mut run);
    if run.status == RunStatus::Completed {
        let event = append_event_at(root, &mut run, "run.completed", None, Value::Null)?;
        emit_event(app, &event);
    }
    persist_run(root, &run)?;
    Ok(run)
}

fn fail_node_at(
    root: &Path,
    request: FailNodeRequest,
    app: Option<&AppHandle>,
) -> Result<WorkflowRun, String> {
    let mut run = read_run_at(root, &request.run_id)?;
    let definition = load_run_definition(root, &run)?;
    let node = node_definition(&definition, &request.node_id)?;
    let state = run
        .nodes
        .get_mut(&request.node_id)
        .ok_or_else(|| format!("node state not found: {}", request.node_id))?;
    if state.status != NodeRunStatus::Running {
        return Err("node is not running".into());
    }
    state.error = Some(request.error.clone());
    state.started_at = None;
    let retry = state.attempts <= node.max_retries;
    state.status = if retry {
        NodeRunStatus::Ready
    } else {
        NodeRunStatus::Failed
    };
    let event = append_event_at(
        root,
        &mut run,
        if retry {
            "node.retry_scheduled"
        } else {
            "node.failed"
        },
        Some(&request.node_id),
        serde_json::json!({ "error": request.error }),
    )?;
    emit_event(app, &event);
    refresh_run_status(&definition, &mut run);
    persist_run(root, &run)?;
    Ok(run)
}

fn approve_node_at(
    root: &Path,
    request: ApprovalRequest,
    app: Option<&AppHandle>,
) -> Result<WorkflowRun, String> {
    if request.actor.trim().is_empty() {
        return Err("approval actor is required".into());
    }
    let mut run = read_run_at(root, &request.run_id)?;
    let definition = load_run_definition(root, &run)?;
    let node = node_definition(&definition, &request.node_id)?;
    if node.kind != NodeKind::HumanApproval {
        return Err("node is not an approval gate".into());
    }
    let state = run
        .nodes
        .get_mut(&request.node_id)
        .ok_or_else(|| format!("node state not found: {}", request.node_id))?;
    if state.status != NodeRunStatus::WaitingForApproval {
        return Err("approval node is not waiting".into());
    }
    state.status = if request.approved {
        NodeRunStatus::Completed
    } else {
        NodeRunStatus::Failed
    };
    state.completed_at = Some(Utc::now().to_rfc3339());
    state.output = Some(serde_json::json!({
        "approved": request.approved,
        "actor": request.actor,
        "comment": request.comment,
    }));
    let event = append_event_at(
        root,
        &mut run,
        if request.approved {
            "approval.granted"
        } else {
            "approval.rejected"
        },
        Some(&request.node_id),
        serde_json::json!({ "actor": request.actor, "comment": request.comment }),
    )?;
    emit_event(app, &event);
    if request.approved {
        schedule_downstream(&definition, &mut run, &request.node_id, &[], root, app)?;
    }
    refresh_run_status(&definition, &mut run);
    persist_run(root, &run)?;
    Ok(run)
}

fn cancel_run_at(
    root: &Path,
    run_id: &str,
    reason: String,
    app: Option<&AppHandle>,
) -> Result<WorkflowRun, String> {
    let mut run = read_run_at(root, run_id)?;
    if matches!(run.status, RunStatus::Completed | RunStatus::Cancelled) {
        return Err("run is already terminal".into());
    }
    run.status = RunStatus::Cancelled;
    run.cancelled_reason = Some(reason.clone());
    run.completed_at = Some(Utc::now().to_rfc3339());
    run.updated_at = Utc::now().to_rfc3339();
    for state in run.nodes.values_mut() {
        if matches!(
            state.status,
            NodeRunStatus::Pending
                | NodeRunStatus::Ready
                | NodeRunStatus::Running
                | NodeRunStatus::WaitingForApproval
        ) {
            state.status = NodeRunStatus::Cancelled;
        }
    }
    let event = append_event_at(
        root,
        &mut run,
        "run.cancelled",
        None,
        serde_json::json!({ "reason": reason }),
    )?;
    persist_run(root, &run)?;
    emit_event(app, &event);
    Ok(run)
}

fn reconcile_timeouts_at(
    root: &Path,
    run_id: &str,
    now: DateTime<Utc>,
    app: Option<&AppHandle>,
) -> Result<WorkflowRun, String> {
    let mut run = read_run_at(root, run_id)?;
    let definition = load_run_definition(root, &run)?;
    let elapsed = now
        .signed_duration_since(
            DateTime::parse_from_rfc3339(&run.created_at)
                .map_err(|error| format!("invalid run timestamp: {error}"))?
                .with_timezone(&Utc),
        )
        .num_seconds();
    if elapsed > definition.limits.max_duration_seconds as i64 {
        run.status = RunStatus::Failed;
        run.completed_at = Some(now.to_rfc3339());
        run.updated_at = now.to_rfc3339();
        for state in run.nodes.values_mut() {
            if matches!(
                state.status,
                NodeRunStatus::Ready | NodeRunStatus::Running | NodeRunStatus::WaitingForApproval
            ) {
                state.status = NodeRunStatus::Failed;
                state.error = Some("workflow duration limit exceeded".into());
            } else if state.status == NodeRunStatus::Pending {
                state.status = NodeRunStatus::Skipped;
            }
        }
        let event = append_event_at(
            root,
            &mut run,
            "run.timed_out",
            None,
            serde_json::json!({ "elapsed_seconds": elapsed }),
        )?;
        persist_run(root, &run)?;
        emit_event(app, &event);
        return Ok(run);
    }
    let timed_out: Vec<String> = definition
        .nodes
        .iter()
        .filter_map(|node| {
            let state = run.nodes.get(&node.id)?;
            let started = state.started_at.as_deref()?;
            if state.status != NodeRunStatus::Running {
                return None;
            }
            let started = DateTime::parse_from_rfc3339(started)
                .ok()?
                .with_timezone(&Utc);
            (now.signed_duration_since(started).num_seconds()
                > node.timeout_seconds.unwrap_or(0) as i64)
                .then(|| node.id.clone())
        })
        .collect();
    for node_id in timed_out {
        run = fail_node_at(
            root,
            FailNodeRequest {
                run_id: run.id.clone(),
                node_id,
                error: "node execution timed out".into(),
            },
            app,
        )?;
    }
    Ok(run)
}

#[tauri::command]
pub fn loops_workflow_validate(definition: WorkflowDefinition) -> ValidationReport {
    validate_definition(&definition)
}

#[tauri::command]
pub fn loops_workflow_list(project: Option<String>) -> Result<Vec<WorkflowSummary>, String> {
    list_definitions_at(&loops_root()?, project.as_deref())
}

#[tauri::command]
pub fn loops_workflow_get(id: String, version: Option<u32>) -> Result<WorkflowDefinition, String> {
    get_definition_at(&loops_root()?, &id, version)
}

#[tauri::command]
pub fn loops_workflow_save(definition: WorkflowDefinition) -> Result<WorkflowDefinition, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    save_definition_at(&loops_root()?, definition)
}

#[tauri::command]
pub fn loops_workflow_activate(id: String, version: u32) -> Result<WorkflowSummary, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    activate_definition_at(&loops_root()?, &id, version)
}

#[tauri::command]
pub fn loops_workflow_clone(request: CloneWorkflowRequest) -> Result<WorkflowDefinition, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    clone_definition_at(&loops_root()?, request)
}

#[tauri::command]
pub fn loops_run_start(app: AppHandle, request: StartRunRequest) -> Result<WorkflowRun, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    start_run_at(&loops_root()?, request, Some(&app))
}

#[tauri::command]
pub fn loops_run_list(workflow_id: Option<String>) -> Result<Vec<WorkflowRun>, String> {
    list_runs_at(&loops_root()?, workflow_id.as_deref())
}

#[tauri::command]
pub fn loops_run_get(id: String) -> Result<WorkflowRun, String> {
    read_run_at(&loops_root()?, &id)
}

#[tauri::command]
pub fn loops_run_events(run_id: String) -> Result<Vec<RunEvent>, String> {
    list_events_at(&loops_root()?, &run_id)
}

#[tauri::command]
pub fn loops_run_claim_node(
    app: AppHandle,
    run_id: String,
    node_id: String,
) -> Result<WorkflowRun, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    claim_node_at(&loops_root()?, &run_id, &node_id, Some(&app))
}

#[tauri::command]
pub fn loops_run_complete_node(
    app: AppHandle,
    request: CompleteNodeRequest,
) -> Result<WorkflowRun, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    complete_node_at(&loops_root()?, request, Some(&app))
}

#[tauri::command]
pub fn loops_run_fail_node(
    app: AppHandle,
    request: FailNodeRequest,
) -> Result<WorkflowRun, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    fail_node_at(&loops_root()?, request, Some(&app))
}

#[tauri::command]
pub fn loops_run_approve(app: AppHandle, request: ApprovalRequest) -> Result<WorkflowRun, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    approve_node_at(&loops_root()?, request, Some(&app))
}

#[tauri::command]
pub fn loops_run_cancel(
    app: AppHandle,
    run_id: String,
    reason: String,
) -> Result<WorkflowRun, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    cancel_run_at(&loops_root()?, &run_id, reason, Some(&app))
}

#[tauri::command]
pub fn loops_run_reconcile(app: AppHandle, run_id: String) -> Result<WorkflowRun, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Loops mutation lock is unavailable")?;
    reconcile_timeouts_at(&loops_root()?, &run_id, Utc::now(), Some(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("xnaut-loops-test-{}", uuid::Uuid::new_v4()))
    }

    fn port(id: &str, data_type: &str) -> WorkflowPort {
        WorkflowPort {
            id: id.into(),
            data_type: data_type.into(),
            required: true,
        }
    }

    fn node(
        id: &str,
        kind: NodeKind,
        inputs: &[(&str, &str)],
        outputs: &[(&str, &str)],
    ) -> WorkflowNode {
        WorkflowNode {
            id: id.into(),
            kind,
            name: id.into(),
            inputs: inputs.iter().map(|(id, ty)| port(id, ty)).collect(),
            outputs: outputs.iter().map(|(id, ty)| port(id, ty)).collect(),
            config: Value::Null,
            permissions: Vec::new(),
            model_policy: None,
            timeout_seconds: Some(30),
            max_retries: 1,
        }
    }

    fn connection(id: &str, from: &str, out: &str, to: &str, input: &str) -> WorkflowConnection {
        WorkflowConnection {
            id: id.into(),
            from_node: from.into(),
            from_port: out.into(),
            to_node: to.into(),
            to_port: input.into(),
        }
    }

    fn definition() -> WorkflowDefinition {
        WorkflowDefinition {
            schema_version: 1,
            id: "test-flow".into(),
            version: 1,
            name: "Test flow".into(),
            description: String::new(),
            project: Some("XNAUT".into()),
            status: WorkflowStatus::Draft,
            limits: WorkflowLimits::default(),
            nodes: vec![
                node("trigger", NodeKind::Trigger, &[], &[("ticket", "ticket")]),
                node(
                    "work",
                    NodeKind::Action,
                    &[("ticket", "ticket")],
                    &[("result", "result")],
                ),
                node("output", NodeKind::Output, &[("result", "result")], &[]),
            ],
            connections: vec![
                connection("c1", "trigger", "ticket", "work", "ticket"),
                connection("c2", "work", "result", "output", "result"),
            ],
            presentation: WorkflowPresentation::default(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn validates_a_minimal_workflow() {
        let report = validate_definition(&definition());
        assert!(report.valid, "{:?}", report.findings);
    }

    #[test]
    fn rejects_incompatible_connections() {
        let mut value = definition();
        value.nodes[1].inputs[0].data_type = "document".into();
        let report = validate_definition(&value);
        assert!(!report.valid);
        assert!(report
            .findings
            .iter()
            .any(|item| item.code == "connection.incompatible_types"));
    }

    #[test]
    fn rejects_unreachable_nodes() {
        let mut value = definition();
        value.nodes.push(node("orphan", NodeKind::Action, &[], &[]));
        let report = validate_definition(&value);
        assert!(report
            .findings
            .iter()
            .any(|item| item.code == "node.unreachable"));
    }

    #[test]
    fn fixed_model_policy_requires_provider_and_model() {
        let mut value = definition();
        value.nodes[1].model_policy = Some(ModelPolicy {
            kind: ModelPolicyKind::Fixed,
            provider: Some("openrouter".into()),
            model: None,
        });
        let report = validate_definition(&value);
        assert!(report
            .findings
            .iter()
            .any(|item| item.code == "node.incomplete_fixed_model"));
    }

    #[test]
    fn rejects_cycles_without_retry_node() {
        let mut value = definition();
        value.nodes[1].inputs.push(port("again", "result"));
        value.nodes[1].outputs.push(port("again", "result"));
        value
            .connections
            .push(connection("c3", "work", "again", "work", "again"));
        let report = validate_definition(&value);
        assert!(!report.valid);
        assert!(report.findings.iter().any(|item| {
            item.code == "workflow.unbounded_cycle" || item.code == "connection.self_cycle"
        }));
    }

    #[test]
    fn persists_immutable_versions_and_activation() {
        let root = temp_root();
        let first = save_definition_at(&root, definition()).unwrap();
        assert_eq!(first.version, 1);
        let mut second = definition();
        second.version = 2;
        second.name = "Test flow revised".into();
        save_definition_at(&root, second).unwrap();
        let active = activate_definition_at(&root, "test-flow", 2).unwrap();
        assert_eq!(active.active_version, Some(2));
        assert_eq!(list_definitions_at(&root, Some("XNAUT")).unwrap().len(), 1);
        assert_eq!(
            get_definition_at(&root, "test-flow", Some(1)).unwrap().name,
            "Test flow"
        );
        let clone = clone_definition_at(
            &root,
            CloneWorkflowRequest {
                source_id: "test-flow".into(),
                source_version: Some(2),
                new_id: "cloned-flow".into(),
                new_name: "Cloned flow".into(),
                project: None,
            },
        )
        .unwrap();
        assert_eq!(clone.id, "cloned-flow");
        assert_eq!(clone.version, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runs_to_completion_and_persists_events() {
        let root = temp_root();
        save_definition_at(&root, definition()).unwrap();
        activate_definition_at(&root, "test-flow", 1).unwrap();
        let run = start_run_at(
            &root,
            StartRunRequest {
                workflow_id: "test-flow".into(),
                workflow_version: None,
                project: None,
                input: serde_json::json!({ "ticket": "XNAUT-6" }),
            },
            None,
        )
        .unwrap();
        assert_eq!(run.nodes["work"].status, NodeRunStatus::Ready);
        let run = claim_node_at(&root, &run.id, "work", None).unwrap();
        let run = complete_node_at(
            &root,
            CompleteNodeRequest {
                run_id: run.id,
                node_id: "work".into(),
                output: serde_json::json!({ "ok": true }),
                outcomes: vec!["result".into()],
            },
            None,
        )
        .unwrap();
        assert_eq!(run.nodes["output"].status, NodeRunStatus::Ready);
        let run = claim_node_at(&root, &run.id, "output", None).unwrap();
        let run = complete_node_at(
            &root,
            CompleteNodeRequest {
                run_id: run.id,
                node_id: "output".into(),
                output: Value::Null,
                outcomes: Vec::new(),
            },
            None,
        )
        .unwrap();
        assert_eq!(run.status, RunStatus::Completed);
        assert!(list_events_at(&root, &run.id).unwrap().len() >= 6);
        assert_eq!(
            read_run_at(&root, &run.id).unwrap().status,
            RunStatus::Completed
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn retries_are_bounded() {
        let root = temp_root();
        save_definition_at(&root, definition()).unwrap();
        activate_definition_at(&root, "test-flow", 1).unwrap();
        let run = start_run_at(
            &root,
            StartRunRequest {
                workflow_id: "test-flow".into(),
                workflow_version: None,
                project: None,
                input: Value::Null,
            },
            None,
        )
        .unwrap();
        let run = claim_node_at(&root, &run.id, "work", None).unwrap();
        let operation_key = run.nodes["work"].operation_key.clone();
        let run = fail_node_at(
            &root,
            FailNodeRequest {
                run_id: run.id,
                node_id: "work".into(),
                error: "first".into(),
            },
            None,
        )
        .unwrap();
        assert_eq!(run.nodes["work"].status, NodeRunStatus::Ready);
        let run = claim_node_at(&root, &run.id, "work", None).unwrap();
        assert_eq!(run.nodes["work"].operation_key, operation_key);
        let run = fail_node_at(
            &root,
            FailNodeRequest {
                run_id: run.id,
                node_id: "work".into(),
                error: "second".into(),
            },
            None,
        )
        .unwrap();
        assert_eq!(run.nodes["work"].status, NodeRunStatus::Failed);
        assert_eq!(run.status, RunStatus::Failed);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn timed_out_node_is_reconciled_to_retry() {
        let root = temp_root();
        let mut value = definition();
        value.nodes[1].timeout_seconds = Some(1);
        save_definition_at(&root, value).unwrap();
        activate_definition_at(&root, "test-flow", 1).unwrap();
        let run = start_run_at(
            &root,
            StartRunRequest {
                workflow_id: "test-flow".into(),
                workflow_version: None,
                project: None,
                input: Value::Null,
            },
            None,
        )
        .unwrap();
        let run = claim_node_at(&root, &run.id, "work", None).unwrap();
        let started =
            DateTime::parse_from_rfc3339(run.nodes["work"].started_at.as_deref().unwrap())
                .unwrap()
                .with_timezone(&Utc);
        let run =
            reconcile_timeouts_at(&root, &run.id, started + chrono::Duration::seconds(2), None)
                .unwrap();
        assert_eq!(run.nodes["work"].status, NodeRunStatus::Ready);
        assert_eq!(
            run.nodes["work"].error.as_deref(),
            Some("node execution timed out")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn retry_node_advances_cycle_generation() {
        let root = temp_root();
        let mut value = definition();
        value.nodes[1].inputs.push(port("again", "result"));
        value.nodes[1].outputs.push(port("retry", "result"));
        value.nodes.insert(
            2,
            node(
                "retry",
                NodeKind::Retry,
                &[("again", "result")],
                &[("loop", "result")],
            ),
        );
        value.connections = vec![
            connection("c1", "trigger", "ticket", "work", "ticket"),
            connection("c2", "work", "result", "output", "result"),
            connection("c3", "work", "retry", "retry", "again"),
            connection("c4", "retry", "loop", "work", "again"),
        ];
        assert!(validate_definition(&value).valid);
        save_definition_at(&root, value).unwrap();
        activate_definition_at(&root, "test-flow", 1).unwrap();
        let run = start_run_at(
            &root,
            StartRunRequest {
                workflow_id: "test-flow".into(),
                workflow_version: None,
                project: None,
                input: Value::Null,
            },
            None,
        )
        .unwrap();
        let run = claim_node_at(&root, &run.id, "work", None).unwrap();
        let first_key = run.nodes["work"].operation_key.clone();
        let run = complete_node_at(
            &root,
            CompleteNodeRequest {
                run_id: run.id,
                node_id: "work".into(),
                output: Value::Null,
                outcomes: vec!["retry".into()],
            },
            None,
        )
        .unwrap();
        let run = claim_node_at(&root, &run.id, "retry", None).unwrap();
        let run = complete_node_at(
            &root,
            CompleteNodeRequest {
                run_id: run.id,
                node_id: "retry".into(),
                output: Value::Null,
                outcomes: vec!["loop".into()],
            },
            None,
        )
        .unwrap();
        assert_eq!(run.nodes["work"].status, NodeRunStatus::Ready);
        assert_eq!(run.nodes["work"].generation, 1);
        let run = claim_node_at(&root, &run.id, "work", None).unwrap();
        assert_ne!(run.nodes["work"].operation_key, first_key);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn approval_gate_suspends_and_resumes() {
        let root = temp_root();
        let mut value = definition();
        value.nodes.insert(
            2,
            node(
                "approval",
                NodeKind::HumanApproval,
                &[("result", "result")],
                &[("approved", "result")],
            ),
        );
        value.connections = vec![
            connection("c1", "trigger", "ticket", "work", "ticket"),
            connection("c2", "work", "result", "approval", "result"),
            connection("c3", "approval", "approved", "output", "result"),
        ];
        save_definition_at(&root, value).unwrap();
        activate_definition_at(&root, "test-flow", 1).unwrap();
        let run = start_run_at(
            &root,
            StartRunRequest {
                workflow_id: "test-flow".into(),
                workflow_version: None,
                project: None,
                input: Value::Null,
            },
            None,
        )
        .unwrap();
        let run = claim_node_at(&root, &run.id, "work", None).unwrap();
        let run = complete_node_at(
            &root,
            CompleteNodeRequest {
                run_id: run.id,
                node_id: "work".into(),
                output: Value::Null,
                outcomes: Vec::new(),
            },
            None,
        )
        .unwrap();
        let run = claim_node_at(&root, &run.id, "approval", None).unwrap();
        assert_eq!(run.status, RunStatus::WaitingForApproval);
        let run = approve_node_at(
            &root,
            ApprovalRequest {
                run_id: run.id,
                node_id: "approval".into(),
                actor: "tester".into(),
                approved: true,
                comment: String::new(),
            },
            None,
        )
        .unwrap();
        assert_eq!(run.nodes["output"].status, NodeRunStatus::Ready);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cancellation_is_durable() {
        let root = temp_root();
        save_definition_at(&root, definition()).unwrap();
        activate_definition_at(&root, "test-flow", 1).unwrap();
        let run = start_run_at(
            &root,
            StartRunRequest {
                workflow_id: "test-flow".into(),
                workflow_version: None,
                project: None,
                input: Value::Null,
            },
            None,
        )
        .unwrap();
        let run = cancel_run_at(&root, &run.id, "user requested".into(), None).unwrap();
        assert_eq!(run.status, RunStatus::Cancelled);
        assert_eq!(
            read_run_at(&root, &run.id).unwrap().status,
            RunStatus::Cancelled
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
