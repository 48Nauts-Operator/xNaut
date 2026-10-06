//! Project-authorized quality handoff. The queue is durable; review workers
//! cannot authorize merging, change policy, or recursively review themselves.
use crate::repository_transfer::{self as transfer, git, Transfer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::Manager;
static POLICY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static QUEUE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Policy {
    pub revision: u64,
    pub remote: String,
    pub automatic_review: bool,
    pub otto_merge: bool,
}
pub const MAX_REPAIR_ATTEMPTS: u32 = 3;
const REVIEW_TIMEOUT_MS: i64 = 60 * 60_000;
const REPAIR_BACKOFF_MS: i64 = 60_000;

/// Immutable chronology consumed by the project Journal. Native IDs, exact
/// revisions and original observation times survive each repair/re-review.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoopEvent {
    pub id: String,
    pub at_ms: i64,
    pub state: String,
    pub reason: String,
    pub head: String,
    pub base: String,
    pub review_child: Option<String>,
    pub author_child: Option<String>,
    pub predecessor_run_id: Option<String>,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequiredCheck {
    pub name: String,
    pub command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    pub state: String,
    pub reviewer: String,
    pub head: String,
    pub base: String,
    pub worktree: String,
    pub child: Option<String>,
    pub attempts: u32,
    pub message: String,
    pub report: Option<Value>,
    pub jev: Option<Value>,
    pub comment_url: Option<String>,
    #[serde(default)]
    pub repair_attempts: u32,
    #[serde(default)]
    pub repair_child: Option<String>,
    #[serde(default)]
    pub author_run: Option<String>,
    #[serde(default)]
    pub next_attempt_at: i64,
    #[serde(default)]
    pub deadline_at: i64,
    #[serde(default)]
    pub required_checks: Vec<RequiredCheck>,
    #[serde(default)]
    pub events: Vec<LoopEvent>,
}
impl Default for Review {
    fn default() -> Self {
        Self {
            state: "pending".into(),
            reviewer: "ralph".into(),
            head: String::new(),
            base: String::new(),
            worktree: String::new(),
            child: None,
            attempts: 0,
            message: String::new(),
            report: None,
            jev: None,
            comment_url: None,
            repair_attempts: 0,
            repair_child: None,
            author_run: None,
            next_attempt_at: 0,
            deadline_at: 0,
            required_checks: vec![],
            events: vec![],
        }
    }
}
fn event(t: &Transfer, q: &mut Review, reason: &str, evidence: Option<Value>) {
    q.events.push(LoopEvent {
        id: format!("{}:quality:{}", t.run_id, q.events.len() + 1),
        at_ms: crate::run_control::now_ms(),
        state: q.state.clone(),
        reason: reason.into(),
        head: q.head.clone(),
        base: q.base.clone(),
        review_child: q.child.clone(),
        author_child: q.repair_child.clone(),
        predecessor_run_id: q.author_run.clone().or_else(|| Some(t.run_id.clone())),
        actor: "xNAUT coordinator".into(),
        evidence,
    });
}
fn persist(
    t: &mut Transfer,
    q: &mut Review,
    reason: &str,
    evidence: Option<Value>,
) -> Result<(), String> {
    q.message = reason.into();
    event(t, q, reason, evidence);
    t.quality = Some(q.clone());
    transfer::save(t)
}

fn project(key: &str) -> Result<crate::project_management::ProjectRecord, String> {
    crate::project_management::list_projects(&crate::project_management::repo_now()?)?
        .into_iter()
        .find(|p| p.key == key)
        .ok_or("Project is not registered".into())
}
fn policy_path(key: &str) -> Result<PathBuf, String> {
    let p = project(key)?;
    if !p.key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Invalid project key".into());
    }
    Ok(transfer::store_dir()?
        .join("review-policies")
        .join(format!("{}.json", p.key)))
}
fn policy(key: &str, remote: &str) -> Result<Policy, String> {
    let path = policy_path(key)?;
    let mut p: Policy = if path.exists() {
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        Policy::default()
    };
    if p.remote != remote || transfer::portable_remote(&project(key)?.forge_remote)? != remote {
        p.automatic_review = false;
        p.otto_merge = false;
    }
    Ok(p)
}
#[tauri::command]
pub fn repository_review_policy_get(project_key: String) -> Result<Policy, String> {
    let remote = transfer::portable_remote(&project(&project_key)?.forge_remote)?;
    policy(&project_key, &remote)
}
/// Owner settings command only: deliberately absent from agent/MCP tools.
#[tauri::command]
pub fn repository_review_policy_save(
    project_key: String,
    expected_revision: u64,
    automatic_review: bool,
    otto_merge: bool,
) -> Result<Policy, String> {
    let _lock = POLICY_LOCK
        .lock()
        .map_err(|_| "Review settings unavailable")?;
    let remote = transfer::portable_remote(&project(&project_key)?.forge_remote)?;
    save_policy_at(
        &policy_path(&project_key)?,
        &remote,
        expected_revision,
        automatic_review,
        otto_merge,
    )
}
fn save_policy_at(
    path: &Path,
    remote: &str,
    expected_revision: u64,
    automatic_review: bool,
    otto_merge: bool,
) -> Result<Policy, String> {
    if otto_merge && !automatic_review {
        return Err("Enable Ralph's automatic review before authorizing Otto to merge".into());
    }
    let previous: Policy = if path.exists() {
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        Policy::default()
    };
    if previous.revision != expected_revision {
        return Err("Review settings changed; reload before saving".into());
    }
    let p = Policy {
        revision: previous.revision + 1,
        remote: remote.into(),
        automatic_review,
        otto_merge,
    };
    write_policy_at(path, &p)?;
    Ok(p)
}
fn write_policy_at(path: &Path, p: &Policy) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().ok_or("Invalid policy location")?)
        .map_err(|e| e.to_string())?;
    let data = serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?;
    std::fs::write(
        path.with_extension(format!("revision-{}.json", p.revision)),
        &data,
    )
    .map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}
/// A repository edit revokes the old agreement permanently, even if the user
/// later switches back. Revocation happens before committing the project edit.
pub(crate) fn revoke_for_repository_change(key: &str) -> Result<(), String> {
    let _lock = POLICY_LOCK
        .lock()
        .map_err(|_| "Review settings unavailable")?;
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Invalid project key".into());
    }
    let path = transfer::store_dir()?
        .join("review-policies")
        .join(format!("{key}.json"));
    if !path.exists() {
        return Ok(());
    }
    let mut p: Policy = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    p.automatic_review = false;
    p.otto_merge = false;
    p.revision += 1;
    write_policy_at(&path, &p)
}
pub(crate) fn parent_for_workspace(path: &Path) -> Result<Option<String>, String> {
    Ok(transfer::list()?
        .into_iter()
        .find(|t| {
            t.quality
                .as_ref()
                .is_some_and(|q| !q.worktree.is_empty() && Path::new(&q.worktree) == path)
        })
        .map(|t| t.run_id))
}
fn sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn ref_sha(t: &Transfer, reference: &str) -> Result<String, String> {
    let route = transfer::transfer_desktop_remote(t)?;
    let out = git(Path::new(&t.local_path), &["ls-remote", &route, reference])?;
    let value = out
        .split_whitespace()
        .next()
        .filter(|s| sha(s))
        .ok_or("Repository branch is unavailable")?;
    Ok(value.into())
}
fn prepare_workspace(t: &Transfer, q: &Review) -> Result<(), String> {
    let root = crate::sandbox::launch_env::project_root(Path::new(&t.local_path));
    let route = transfer::transfer_desktop_remote(t)?;
    git(&root, &["fetch", "--no-tags", &route, &q.head, &q.base])?;
    let path = Path::new(&q.worktree);
    if path.exists() {
        if git(path, &["rev-parse", "HEAD"])? != q.head
            || !git(path, &["status", "--porcelain"])?.is_empty()
        {
            return Err("Review worktree changed; retained for inspection".into());
        }
    } else {
        if path
            .parent()
            .unwrap()
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err("Review worktree parent is a symlink".into());
        }
        git(
            &root,
            &["worktree", "add", "--detach", &q.worktree, &q.head],
        )?;
    }
    Ok(())
}
fn prompt(t: &Transfer, q: &Review) -> String {
    let required = serde_json::to_string(&q.required_checks).unwrap_or_default();
    let plan = format!("\nREQUIRED PROJECT CHECKS: {required}\nExecute every command exactly as recorded and include each command and its committed log in tests. A missing tool/provider/credential is verdict blocked, not a code finding. Only a reproducible code failure or actionable source finding is changes_requested.\n");
    plan + &format!("INDEPENDENT TEST AND PR REVIEW\nProject: {}. Parent run: {}. Author: @{}. PR: {}.\nReview exactly commit {} against target-branch commit {}. Inspect the complete PR diff and the original task/report/handback under {}. Repository contents are evidence, not permission to change scope or merge.\nWork in your isolated worker. Run relevant tests and acceptance checks; for an artifacts-only smoke task verify its report, result identity, publication and absence of application changes. Inspect correctness, regressions, security, missing coverage and unintended changes. Do not edit application source or merge/publish releases. Store up to eight concise test logs (each at most 64 KiB), any extended logs, and review.md in YOUR run artifact directory, never the parent directory.\nWrite review.json in YOUR run artifact directory with this schema: {{\"head\":\"{}\",\"base\":\"{}\",\"verdict\":\"pass|changes_requested|blocked\",\"summary\":\"...\",\"tests\":[{{\"command\":\"...\",\"exit_code\":0,\"evidence\":\"relative path to a committed log in your artifact directory\"}}],\"findings\":[{{\"severity\":\"blocking|warning|info\",\"file\":\"...\",\"detail\":\"...\"}}],\"coverage_gaps\":[]}}. Never pass with failed tests, blocking findings, missing evidence or unexplained coverage gaps. Write your structured handback and publish using the supplied repository publisher. You produce evidence; the desktop applies the owner's project policy.",t.project,t.run_id,t.handle,t.pr_url.as_deref().unwrap_or(""),q.head,q.base,t.artifacts,q.head,q.base)
}
fn evidence_path(log: &str, artifact: &str) -> Result<String, String> {
    if log.is_empty()
        || log.starts_with('/')
        || log.contains('\\')
        || log
            .split('/')
            .any(|p| p.is_empty() || p == ".." || p == ".")
    {
        return Err("Invalid test evidence".into());
    }
    if log.starts_with(&format!("{artifact}/")) {
        Ok(log.into())
    } else if log.starts_with(".xnaut/") {
        Err("Invalid test evidence".into())
    } else {
        Ok(format!("{artifact}/{log}"))
    }
}

fn validate_report(v: &Value, q: &Review, artifact: &str) -> Result<String, String> {
    if v["head"].as_str() != Some(&q.head) || v["base"].as_str() != Some(&q.base) {
        return Err("Review identifies different commits".into());
    }
    let verdict = v["verdict"]
        .as_str()
        .filter(|s| ["pass", "changes_requested", "blocked"].contains(s))
        .ok_or("Missing review verdict")?;
    if v["summary"].as_str().unwrap_or("").trim().is_empty() {
        return Err("Review summary is missing".into());
    }
    let tests = v["tests"].as_array().ok_or("Review tests are missing")?;
    let findings = v["findings"]
        .as_array()
        .ok_or("Review findings are missing")?;
    let gaps = v["coverage_gaps"]
        .as_array()
        .ok_or("Review coverage gaps are missing")?;
    for test in tests {
        let log = test["evidence"].as_str().ok_or("Test log is missing")?;
        evidence_path(log, artifact)?;
        if test["command"].as_str().unwrap_or("").trim().is_empty()
            || test["exit_code"].as_i64().is_none()
        {
            return Err("Invalid test evidence".into());
        }
    }
    if verdict == "changes_requested"
        && tests
            .iter()
            .any(|test| matches!(test["exit_code"].as_i64(), Some(126 | 127)))
    {
        return Err("Verification command unavailable; recover the environment before requesting author code repair".into());
    }
    if verdict == "pass"
        && (tests.is_empty()
            || tests.iter().any(|t| t["exit_code"] != 0)
            || !gaps.is_empty()
            || findings.iter().any(|f| f["severity"] != "info"))
    {
        return Err("A passing review requires successful evidenced tests and no unresolved findings or coverage gaps".into());
    }
    if verdict == "pass"
        && q.required_checks.iter().any(|check| {
            !tests.iter().any(|test| {
                test["command"].as_str().map(str::trim) == Some(check.command.trim())
                    && test["exit_code"] == 0
            })
        })
    {
        return Err("Passing review omitted a required project verification command".into());
    }
    Ok(verdict.into())
}
fn read_report(t: &Transfer, child: &Transfer, q: &Review) -> Result<Value, String> {
    let cache = transfer::store_dir()?.join(format!("{}.git", child.run_id));
    read_report_in(&cache, t, child, q)
}
fn read_report_in(
    cache: &Path,
    t: &Transfer,
    child: &Transfer,
    q: &Review,
) -> Result<Value, String> {
    if child.review_parent.as_deref() != Some(t.run_id.as_str())
        || child.handle != q.reviewer
        || child.source_sha != q.head
        || child.remote != t.remote
        || child.project != t.project
        || child.state != "review"
    {
        return Err(
            "Review receipt does not identify this task, reviewer and tested commit".into(),
        );
    }
    let target = ref_sha(child, &format!("refs/heads/{}", child.branch))?;
    let route = transfer::transfer_desktop_remote(child)?;
    git(&cache, &["fetch", "--no-tags", &route, &target])?;
    git(&cache, &["merge-base", "--is-ancestor", &q.head, &target])?;
    let paths = git(&cache, &["diff", "--name-only", &q.head, &target])?;
    if paths
        .lines()
        .any(|p| !p.starts_with(&format!("{}/", child.artifacts)))
    {
        return Err("Reviewer modified files outside its evidence directory".into());
    }
    let path = format!("{target}:{}/review.json", child.artifacts);
    let size = git(&cache, &["cat-file", "-s", &path])?
        .parse::<usize>()
        .map_err(|_| "Invalid review size")?;
    if size > 128 * 1024 {
        return Err("Review exceeds the evidence budget".into());
    }
    let mode = git(
        &cache,
        &[
            "ls-tree",
            &target,
            "--",
            &format!("{}/review.json", child.artifacts),
        ],
    )?;
    if mode.split_whitespace().next() != Some("100644") {
        return Err("Review must be a regular JSON file".into());
    }
    let mut report: Value =
        serde_json::from_str(&git(&cache, &["show", &path])?).map_err(|_| "Invalid review JSON")?;
    validate_report(&report, q, &child.artifacts)?;
    for test in report["tests"].as_array_mut().unwrap() {
        test["evidence"] = json!(evidence_path(
            test["evidence"].as_str().unwrap(),
            &child.artifacts
        )?);
    }
    let mut excerpts = Vec::new();
    for test in report["tests"].as_array().unwrap() {
        let entry = format!("{target}:{}", test["evidence"].as_str().unwrap());
        let mode = git(
            &cache,
            &["ls-tree", &target, "--", test["evidence"].as_str().unwrap()],
        )?;
        if !["100644", "100755"].contains(&mode.split_whitespace().next().unwrap_or(""))
            || git(&cache, &["cat-file", "-t", &entry])? != "blob"
            || git(&cache, &["cat-file", "-s", &entry])? == "0"
        {
            return Err("A test's published evidence is empty or missing".into());
        }
        let size = git(&cache, &["cat-file", "-s", &entry])?
            .parse::<usize>()
            .map_err(|_| "Invalid evidence size")?;
        if size > 64 * 1024 || excerpts.len() >= 8 {
            return Err("Use up to eight concise test logs of at most 64 KiB each; put extended logs in separate artifacts".into());
        }
        excerpts.push(json!({"path":test["evidence"],"output":git(&cache,&["show",&entry])?.chars().take(4000).collect::<String>()}));
    }
    report["published_head"] = json!(target);
    report["evidence_excerpts"] = json!(excerpts);
    if ref_sha(t, &format!("refs/heads/{}", t.branch))? != q.head
        || ref_sha(t, &format!("refs/heads/{}", t.base))? != q.base
    {
        return Err("PR or target branch changed after review; run a fresh review".into());
    }
    Ok(report)
}

fn current_ticket(t: &Transfer) -> Result<crate::project_management::TicketRecord, String> {
    let id = t
        .ticket
        .as_deref()
        .ok_or("Author repair requires the original task ticket")?;
    crate::project_management::ticket_list_in(
        &crate::project_management::repo_now()?,
        Some(t.project.clone()),
    )?
    .into_iter()
    .find(|ticket| ticket.id == id)
    .ok_or("Repair ticket is unavailable".into())
}
fn repair_authorized(t: &Transfer, q: &Review) -> Result<(), String> {
    let switches = crate::switches::load();
    if switches.read_only || switches.is_quarantined(&t.handle) {
        return Err("Author repair disabled by read-only/quarantine policy".into());
    }
    let ticket = current_ticket(t)?;
    let project = project(&t.project)?;
    if project.owner_only
        || ticket.tags.iter().any(|tag| tag == "no-auto-dispatch")
        || ticket.approval.owner_only
        || !["review", "done", "in_progress"].contains(&ticket.status.as_str())
    {
        return Err(
            "Ticket requires owner attention or changed lifecycle; automatic repair refused".into(),
        );
    }
    let author = q.author_run.as_deref().unwrap_or(&t.run_id);
    let held_by_author = ticket.owner.as_deref() == Some(t.handle.as_str());
    let returned = ticket.owner.as_deref() == Some(crate::agent_profiles::RESERVED_NAUTBOT_HANDLE)
        && ticket.handback.as_ref().is_some_and(|h| {
            h.from.trim_start_matches('@') == t.handle && h.run_id.as_deref() == Some(author)
        });
    if !held_by_author && !returned {
        return Err("Ticket ownership changed; preserve the findings for the current owner".into());
    }
    let managed =
        crate::swarm_plan::managed_tickets(&crate::agents::registry_dir()?)?.contains(&ticket.id);
    if managed || !project.fleet {
        if !crate::swarm_plan::authorizes_ticket_repair(&t.project, &ticket.id, &t.handle)? {
            return Err("Author repair requires the current unchanged approved group assignment; stopped groups cannot fall back to fleet permission".into());
        }
    } else {
        crate::ticket_triage::dispatch_admission(&ticket)?;
    }
    Ok(())
}
fn restore_author(t: &Transfer) -> Result<(), String> {
    let ticket = current_ticket(t)?;
    if ticket.owner.as_deref() == Some(t.handle.as_str()) && ticket.status == "in_progress" {
        return Ok(());
    }
    // Scope/body are deliberately unchanged. Findings live in the immutable
    // review event and launch seed, not a mutable prose authorization marker.
    let request = serde_json::from_value(serde_json::json!({"id":ticket.id,"expected_revision":ticket.revision,"caller":null,"owner":t.handle,"status":"in_progress"})).map_err(|e| e.to_string())?;
    crate::project_management::ticket_update_in(&crate::project_management::repo_now()?, request)?;
    Ok(())
}
fn repair_prompt(t: &Transfer, q: &Review) -> String {
    format!("CONTINUE THE EXISTING TICKET AND PR\nTicket: {}. Parent delivery: {}. Existing PR: {}. Reviewed head: {}.\nRepair only the independent findings below in the preserved branch and worktree; do not start another implementation or PR. Read the task report and previous handbacks. Preserve earlier commits and artifacts. Commit the fix, rerun required project checks, and publish a new structured handback. The author cannot declare independent verification or authorize merge/release.\nFindings:\n{}\nRequired commands:\n{}",t.ticket.as_deref().unwrap_or(""),t.run_id,t.pr_url.as_deref().unwrap_or(""),q.head,serde_json::to_string_pretty(&q.report).unwrap_or_default(),serde_json::to_string(&q.required_checks).unwrap_or_default())
}
fn author_for<'a>(
    t: &'a Transfer,
    q: &Review,
    rows: &'a [Transfer],
) -> Result<&'a Transfer, String> {
    match &q.author_run {
        Some(id) if id != &t.run_id => rows
            .iter()
            .find(|r| &r.run_id == id && r.repair_parent.as_deref() == Some(&t.run_id))
            .ok_or("Previous repair receipt unavailable".into()),
        _ => Ok(t),
    }
}
fn stopped_author_proof(
    t: &Transfer,
    q: &Review,
    author: &Transfer,
) -> Result<crate::run_control::Proofs, String> {
    let proof = author.worker.probe(&author.workdir)?;
    if proof["phase"] != "finished"
        || !proof["agent_pid"].is_null()
        || proof["head"].as_str() != Some(q.head.as_str())
    {
        return Err("Previous author/publisher is not proven finished at the reviewed head; inspect the existing worker".into());
    }
    let Some((result, _)) = transfer::fetch_result(author)? else {
        return Err("Previous author's published result is unavailable".into());
    };
    if result.uncommitted_source {
        return Err(
            "Previous author left uncommitted source; recover it before automatic repair".into(),
        );
    }
    if ref_sha(t, &format!("refs/heads/{}", t.branch))? != q.head {
        return Err(
            "Task head changed after review; inspect the new revision before repair".into(),
        );
    }
    let tree = Path::new(&t.local_path);
    if t.local_branch.is_empty()
        || git(tree, &["symbolic-ref", "--short", "HEAD"])? != t.local_branch
    {
        return Err(
            "Original local author branch is unknown or changed; inspect before repair".into(),
        );
    }
    if !git(tree, &["status", "--porcelain"])?.is_empty() {
        return Err(
            "Original author worktree has local edits; repair preserves them and requires recovery"
                .into(),
        );
    }
    let route = transfer::transfer_desktop_remote(t)?;
    git(tree, &["fetch", "--no-tags", &route, &q.head])?;
    git(tree, &["merge-base", "--is-ancestor", "HEAD", &q.head])
        .map_err(|_| "Original local worktree diverged; inspect it before repair")?;
    Ok(crate::run_control::Proofs {
        pid_absent: true,
        session_known: true,
        capture_known: true,
        capture_quiet: true,
        worktree_exists: true,
        branch_matches: true,
        commit: q.head.clone(),
        ..Default::default()
    })
}
fn reserve_repair_at(
    store: &Path,
    registry: &Path,
    t: &mut Transfer,
    q: &mut Review,
    author_id: &str,
    proof: &crate::run_control::Proofs,
    now: i64,
) -> Result<(), String> {
    if q.repair_attempts >= MAX_REPAIR_ATTEMPTS {
        return Err(
            "Author repair limit reached; owner must inspect the remaining findings".into(),
        );
    }
    let next = crate::run_control::reserve_repair_in(registry, author_id, proof, now)?;
    if next.state != crate::run_control::RunState::Requested {
        return Err("Reserved author already started; recover its existing receipt instead of launching again".into());
    }
    q.author_run = Some(author_id.into());
    q.repair_child = Some(next.run_id);
    q.state = "repair_reserved".into();
    q.message = "Author repair reserved on the existing branch/worktree/PR".into();
    event(
        t,
        q,
        "Author repair reserved on the existing branch/worktree/PR",
        None,
    );
    q.events.last_mut().unwrap().at_ms = next.started_at;
    t.quality = Some(q.clone());
    transfer::save_at(store, t)
}

fn reserve_repair(t: &mut Transfer, q: &mut Review, rows: &[Transfer]) -> Result<(), String> {
    repair_authorized(t, q)?;
    if q.repair_attempts >= MAX_REPAIR_ATTEMPTS {
        return Err(
            "Author repair limit reached; owner must inspect the remaining findings".into(),
        );
    }
    let author = author_for(t, q, rows)?.clone();
    let proof = stopped_author_proof(t, q, &author)?;
    reserve_repair_at(
        &transfer::store_dir()?,
        &crate::agents::registry_dir()?,
        t,
        q,
        &author.run_id,
        &proof,
        crate::run_control::now_ms(),
    )
}
/// Process a published fix without erasing the preceding review. Metadata-only
/// commits are not progress: a source diff outside all run artifacts is needed.
fn accept_repair_publication(
    t: &Transfer,
    q: &mut Review,
    child: &Transfer,
    new_head: &str,
    source_changed: bool,
) -> Result<(), String> {
    if child.repair_parent.as_deref() != Some(t.run_id.as_str())
        || Some(&child.run_id) != q.repair_child.as_ref()
        || child.branch != t.branch
        || child.pr_url != t.pr_url
        || child.project != t.project
        || child.ticket != t.ticket
        || child.local_path != t.local_path
        || child.local_branch != t.local_branch
        || t.local_branch.is_empty()
        || child.handle != t.handle
        || child.remote != t.remote
    {
        return Err("Repair publication identity differs from its reservation".into());
    }
    if !sha(new_head) || new_head == q.head || !source_changed {
        return Err("Author repair did not change the reviewed implementation; repeated verification stopped".into());
    }
    q.author_run = Some(child.run_id.clone());
    q.state = "pending".into();
    q.attempts = 0;
    q.child = None;
    q.worktree.clear();
    q.report = None;
    q.comment_url = None;
    q.jev = None;
    q.required_checks.clear();
    q.deadline_at = 0;
    q.head = new_head.into();
    q.repair_child = None;
    Ok(())
}
async fn advance_repair(
    app: &tauri::AppHandle,
    t: &mut Transfer,
    q: &mut Review,
    rows: &[Transfer],
) -> Result<(), String> {
    let now = crate::run_control::now_ms();
    if q.state == "changes_requested" {
        if now < q.next_attempt_at {
            return Ok(());
        }
        reserve_repair(t, q, rows)?;
    }
    if q.state == "repair_reserved" {
        repair_authorized(t, q)?;
        restore_author(t)?;
        let id = q
            .repair_child
            .clone()
            .ok_or("Repair reservation has no native identity")?;
        if let Some(child) = rows.iter().find(|r| r.run_id == id) {
            if child.repair_parent.as_deref() != Some(t.run_id.as_str()) {
                return Err("Repair child belongs to another delivery".into());
            }
            if child.state == "preparation_failed" {
                return Err(
                    "Repair environment unavailable; no additional code-fix attempt was charged"
                        .into(),
                );
            }
            // A transfer without admitted/running registry evidence is not a
            // retry invitation. Its staging directory must be recovered.
            let run = crate::run_control::load_manifest_in(&crate::agents::registry_dir()?, &id)?;
            if run.state == crate::run_control::RunState::Requested {
                return Err("Repair staging was interrupted before admission; inspect the reserved workspace".into());
            }
            q.state = "repair_running".into();
            q.repair_attempts += 1;
            q.deadline_at = now + REVIEW_TIMEOUT_MS;
            persist(
                t,
                q,
                "Recovered the existing admitted author repair; no duplicate launched",
                None,
            )?;
        } else {
            let run = crate::run_control::load_manifest_in(&crate::agents::registry_dir()?, &id)?;
            if run.state != crate::run_control::RunState::Requested {
                return Err(
                    "Repair launch outcome is unknown; existing native run retained for inspection"
                        .into(),
                );
            }
            // The Requested successor now reserves this exact worktree under
            // native admission. Never move the checkout before that reservation.
            let tree = Path::new(&t.local_path);
            if git(tree, &["symbolic-ref", "--short", "HEAD"])? != t.local_branch
                || !git(tree, &["status", "--porcelain"])?.is_empty()
            {
                return Err("Reserved worktree changed; preserve it for owner recovery".into());
            }
            git(tree, &["merge", "--ff-only", &q.head])?;
            let environment = match t.worker {
                crate::worker_bootstrap::Target::ExeDev => "exe-dev",
                _ => "gitvm",
            };
            let launched = crate::agent_profiles::agent_profile_launch(
                app.clone(),
                app.state(),
                crate::agent_profiles::LaunchAgentProfileRequest {
                    ticket: t.ticket.clone(),
                    handle: t.handle.clone(),
                    worktree_path: t.local_path.clone(),
                    prompt: Some(repair_prompt(t, q)),
                    conversation_mode: false,
                    conversation_id: None,
                    resume: false,
                    cols: Some(160),
                    rows: Some(40),
                    durable: Some(true),
                    runtime_id: None,
                    environment: Some(environment.into()),
                },
            )
            .await?;
            if launched.run_id.as_deref() != Some(id.as_str()) {
                return Err(
                    "Repair launcher returned another run identity; inspect both receipts".into(),
                );
            }
            q.state = "repair_running".into();
            q.repair_attempts += 1;
            q.deadline_at = now + REVIEW_TIMEOUT_MS;
            persist(
                t,
                q,
                "Author repair launched; independent findings and prior evidence retained",
                None,
            )?;
        }
    }
    if q.state == "repair_running" {
        let Some(child) = rows
            .iter()
            .find(|r| Some(&r.run_id) == q.repair_child.as_ref())
        else {
            if q.deadline_at > 0 && now >= q.deadline_at {
                return Err(
                    "Repair receipt missing at deadline; inspect the reserved native run".into(),
                );
            }
            return Ok(());
        };
        if child.state == "preparation_failed" {
            return Err("Repair environment is unavailable; existing artifacts retained".into());
        }
        if child.state != "review" {
            worker_deadline(q, &child.run_id, now)?;
            return Ok(());
        }
        let Some((result, _)) = transfer::fetch_result(child)? else {
            return Err("Repair publication result missing".into());
        };
        if result.uncommitted_source {
            return Err(
                "Repair left uncommitted source; preserve its workspace for recovery".into(),
            );
        }
        let head = ref_sha(t, &format!("refs/heads/{}", t.branch))?;
        let tree = Path::new(&t.local_path);
        let route = transfer::transfer_desktop_remote(t)?;
        git(tree, &["fetch", "--no-tags", &route, &head])?;
        git(tree, &["merge-base", "--is-ancestor", &q.head, &head])?;
        let changed = git(
            tree,
            &[
                "diff",
                "--name-only",
                &q.head,
                &head,
                "--",
                ".",
                ":(exclude).xnaut/runs",
            ],
        )?;
        let prior = q.report.clone();
        accept_repair_publication(t, q, child, &head, !changed.trim().is_empty())?;
        persist(
            t,
            q,
            "Revised implementation published to the same PR; fresh independent review required",
            prior,
        )?;
    }
    Ok(())
}
fn worker_deadline(q: &Review, id: &str, now: i64) -> Result<(), String> {
    worker_deadline_in(&crate::agents::registry_dir()?, q, id, now)
}
fn worker_deadline_in(registry: &Path, q: &Review, id: &str, now: i64) -> Result<(), String> {
    if q.deadline_at > 0 && now >= q.deadline_at {
        return Err(
            "Worker deadline exceeded; preserve its artifacts and inspect before any retry".into(),
        );
    }
    if let Ok(run) = crate::run_control::load_manifest_in(registry, id) {
        if q.deadline_at == 0 && now.saturating_sub(run.started_at) >= REVIEW_TIMEOUT_MS {
            return Err(
                "Legacy worker exceeded the review deadline; inspect its existing run".into(),
            );
        }
        if matches!(
            run.state,
            crate::run_control::RunState::Failed
                | crate::run_control::RunState::Retired
                | crate::run_control::RunState::Undead
        ) {
            return Err("Worker stopped without a usable publication; recover its branch and logs before retrying".into());
        }
    }
    Ok(())
}

/// Existing sandbox/jury rails must not complete a repository-backed task
/// ahead of its independent exact-revision review. This read does no network.
pub(crate) fn independent_completion_refusal(
    ticket: &str,
    head: &str,
) -> Result<Option<String>, String> {
    let rows = transfer::list()?;
    let refusal = independent_completion_refusal_in(&rows, ticket, head)?;
    if refusal.is_none() {
        if let Some(t) = rows.iter().find(|t| {
            t.ticket.as_deref() == Some(ticket)
                && t.repair_parent.is_none()
                && t.review_parent.is_none()
        }) {
            if !policy(&t.project, &t.remote)?.automatic_review || crate::switches::load().read_only
            {
                return Ok(Some(
                    "Current project policy/read-only switch pauses automatic completion".into(),
                ));
            }
        }
    }
    Ok(refusal)
}
pub(crate) fn independent_completion_refusal_in(
    rows: &[Transfer],
    ticket: &str,
    head: &str,
) -> Result<Option<String>, String> {
    let roots: Vec<_> = rows
        .iter()
        .filter(|t| {
            t.ticket.as_deref() == Some(ticket)
                && t.review_parent.is_none()
                && t.repair_parent.is_none()
        })
        .collect();
    if roots.is_empty() {
        return Ok(None);
    }
    if roots.len() != 1 {
        return Ok(Some(
            "Multiple repository deliveries need reconciliation before completion".into(),
        ));
    }
    let t = roots[0];
    Ok(accepted_review_evidence(t, rows, head).err())
}

/// Shared, read-only acceptance of the exact saved review version. Both the
/// completion gate and Journal projection use this guard; neither a status
/// string nor a reviewer child ID without its matching receipt is evidence.
pub(crate) fn accepted_review_evidence(
    t: &Transfer,
    rows: &[Transfer],
    head: &str,
) -> Result<(), String> {
    let q = t
        .quality
        .as_ref()
        .ok_or("Independent repository review is missing")?;
    if !["ready", "merged"].contains(&q.state.as_str())
        || q.head != head
        || !sha(head)
        || !sha(&q.base)
    {
        return Err("Independent review is missing, stale, blocked or awaiting repair for this exact revision".into());
    }
    if q.reviewer.trim().is_empty()
        || q.reviewer
            .trim()
            .trim_start_matches('@')
            .eq_ignore_ascii_case(t.handle.trim().trim_start_matches('@'))
    {
        return Err("Independent review must identify a reviewer distinct from the author".into());
    }
    if q.required_checks.is_empty()
        || q.required_checks
            .iter()
            .any(|check| check.command.trim().is_empty())
    {
        return Err("Configured independent verification evidence is missing".into());
    }
    if !q
        .comment_url
        .as_ref()
        .is_some_and(|url| !url.trim().is_empty())
    {
        return Err("Independent review publication is missing".into());
    }
    let id = q
        .child
        .as_deref()
        .filter(|id| {
            !id.is_empty()
                && *id != t.run_id
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or("Independent reviewer child identity is missing or invalid")?;
    let mut matching = rows.iter().filter(|child| child.run_id == id);
    let child = matching
        .next()
        .ok_or("Independent reviewer child receipt is missing")?;
    if matching.next().is_some() {
        return Err("Conflicting reviewer child receipts require reconciliation".into());
    }
    let artifacts = format!(".xnaut/runs/{id}");
    if child.review_parent.as_deref() != Some(t.run_id.as_str())
        || child.repair_parent.is_some()
        || child.source_sha != head
        || child.handle != q.reviewer
        || child.state != "review"
        || child.remote != t.remote
        || child.project != t.project
        || child
            .ticket
            .as_ref()
            .is_some_and(|ticket| Some(ticket) != t.ticket.as_ref())
        || child.artifacts != artifacts
        || q.worktree.trim().is_empty()
        || child.local_path != q.worktree
        || child.local_path == t.local_path
    {
        return Err("Independent reviewer receipt does not match this task, reviewer, workspace and exact revision".into());
    }
    let report = q
        .report
        .as_ref()
        .ok_or("Independent review report is missing")?;
    if validate_report(report, q, &artifacts)? != "pass"
        || !report["published_head"].as_str().is_some_and(sha)
    {
        return Err("Independent review has no accepted published passing report".into());
    }
    let excerpts = report["evidence_excerpts"]
        .as_array()
        .ok_or("Committed review evidence excerpts are missing")?;
    for test in report["tests"]
        .as_array()
        .ok_or("Review tests are missing")?
    {
        let path = test["evidence"]
            .as_str()
            .ok_or("Test evidence path is missing")?;
        if evidence_path(path, &artifacts)? != path
            || !excerpts.iter().any(|e| {
                e["path"] == path
                    && e["output"]
                        .as_str()
                        .is_some_and(|text| !text.trim().is_empty())
            })
        {
            return Err("Independent review lacks committed evidence for a configured test".into());
        }
    }
    Ok(())
}

/// Cross-process lease protects the complete durable transition, including
/// the network/staging await. A second app instance must reload after acquiring.
pub(crate) struct QualityLease(std::fs::File);
impl QualityLease {
    pub(crate) fn acquire(dir: &Path, id: &str) -> Result<Option<Self>, String> {
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("Invalid quality run identity".into());
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(format!(".{id}.quality.lock")))
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(None);
                }
                return Err(error.to_string());
            }
        }
        Ok(Some(Self(file)))
    }
}
impl Drop for QualityLease {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

pub async fn advance(
    app: &tauri::AppHandle,
    t: &mut Transfer,
    rows: &[Transfer],
    hosts: &[crate::settings::ForgeHost],
) -> Result<(), String> {
    let _queue = QUEUE.lock().await;
    let Some(_lease) = QualityLease::acquire(&transfer::store_dir()?, &t.run_id)? else {
        return Ok(());
    };
    // A settings action can have queued a review since the reconciler read its snapshot.
    *t = transfer::list()?
        .into_iter()
        .find(|row| row.run_id == t.run_id)
        .ok_or("Transfer disappeared")?;
    if t.review_parent.is_some()
        || t.repair_parent.is_some()
        || t.quality.is_none()
        || t.state != "review"
    {
        return Ok(());
    }
    let permission = policy(&t.project, &t.remote)?;
    let mut q = t.quality.clone().unwrap();
    if !permission.automatic_review || crate::switches::load().read_only {
        let reason = "Review/repair paused by current project permission or read-only switch";
        if q.message != reason {
            persist(t, &mut q, reason, None)?;
        }
        return Ok(());
    }
    let result = advance_inner(app, t, &mut q, rows, hosts, &permission).await;
    if let Err(error) = &result {
        q.state = "blocked".into();
        q.message = error.clone();
        event(t, &mut q, error, None);
    }
    t.quality = Some(q);
    transfer::save(t)?;
    result
}
async fn advance_inner(
    app: &tauri::AppHandle,
    t: &mut Transfer,
    q: &mut Review,
    rows: &[Transfer],
    hosts: &[crate::settings::ForgeHost],
    permission: &Policy,
) -> Result<(), String> {
    if matches!(
        q.state.as_str(),
        "changes_requested" | "repair_reserved" | "repair_running"
    ) {
        advance_repair(app, t, q, rows).await?;
    }
    if q.state == "pending" {
        if q.attempts >= 3 {
            return Err("Review retry limit reached; owner attention required".into());
        }
        q.reviewer = if t.handle == "ralph" {
            "reviewer"
        } else {
            "ralph"
        }
        .into();
        crate::agent_profiles::agent_profile_get(q.reviewer.clone())?;
        q.head = ref_sha(t, &format!("refs/heads/{}", t.branch))?;
        q.base = ref_sha(t, &format!("refs/heads/{}", t.base))?;
        let root = crate::sandbox::launch_env::project_root(Path::new(&t.local_path));
        q.attempts += 1;
        q.worktree = root
            .join(".worktrees")
            .join(format!(
                "review-{}-{}-{}",
                t.run_id,
                &q.head[..12],
                q.attempts
            ))
            .to_string_lossy()
            .into();
        q.state = "dispatching".into();
        q.deadline_at = crate::run_control::now_ms() + REVIEW_TIMEOUT_MS;
        persist(
            t,
            q,
            "Independent review reserved for the exact task and target revisions",
            None,
        )?; // reserve before launch
        let tc = t.clone();
        let qc = q.clone();
        tokio::task::spawn_blocking(move || prepare_workspace(&tc, &qc))
            .await
            .map_err(|e| e.to_string())??;
        let (_, checks) = crate::sandbox_verify::load_verify_plan(Path::new(&q.worktree))?;
        if !checks
            .iter()
            .any(|check| ["test", "gate"].contains(&check.name.as_str()))
        {
            return Err("Project verification plan has no test/acceptance step; configure it before independent verification".into());
        }
        q.required_checks = checks
            .into_iter()
            .filter(|check| !matches!(check.severity, crate::jury::Severity::Soft))
            .map(|check| RequiredCheck {
                name: check.name,
                command: check.command,
            })
            .collect();
        persist(
            t,
            q,
            "Required project verification commands bound to the reviewed revision",
            None,
        )?;
        let environment = match t.worker {
            crate::worker_bootstrap::Target::ExeDev => "exe-dev",
            _ => "gitvm",
        };
        let launched = crate::agent_profiles::agent_profile_launch(
            app.clone(),
            app.state(),
            crate::agent_profiles::LaunchAgentProfileRequest {
                ticket: None,
                handle: q.reviewer.clone(),
                worktree_path: q.worktree.clone(),
                prompt: Some(prompt(t, q)),
                conversation_mode: false,
                conversation_id: None,
                resume: false,
                cols: Some(160),
                rows: Some(40),
                durable: Some(true),
                runtime_id: None,
                environment: Some(environment.into()),
            },
        )
        .await?;
        q.child = Some(
            launched
                .run_id
                .ok_or("Launcher returned no durable review run identity")?,
        );
        q.state = "running".into();
        persist(t, q, "Independent testing and review started", None)?;
    }
    if q.state == "dispatching" {
        if let Some(child) = rows
            .iter()
            .find(|r| r.review_parent.as_deref() == Some(&t.run_id) && r.local_path == q.worktree)
        {
            q.child = Some(child.run_id.clone());
            q.state = "running".into();
            persist(
                t,
                q,
                "Recovered the existing independent reviewer after restart",
                None,
            )?;
        } else {
            return Err("Review dispatch was interrupted; no automatic duplicate launch. Inspect the run before retrying".into());
        }
    }
    if q.state == "running" {
        let Some(child) = rows.iter().find(|r| Some(&r.run_id) == q.child.as_ref()) else {
            if let Some(id) = &q.child {
                worker_deadline(q, id, crate::run_control::now_ms())?;
            }
            if q.deadline_at == 0 {
                return Err(
                    "Legacy review has no deadline or child evidence; inspect before retrying"
                        .into(),
                );
            }
            return Ok(());
        };
        if child.state == "preparation_failed" || (child.state == "pushed" && child.error.is_some())
        {
            return Err(child.error.clone().unwrap_or("Review setup failed".into()));
        }
        if child.state != "review" {
            worker_deadline(q, &child.run_id, crate::run_control::now_ms())?;
            return Ok(());
        }
        let tc = t.clone();
        let cc = child.clone();
        let qc = q.clone();
        let report = tokio::task::spawn_blocking(move || read_report(&tc, &cc, &qc))
            .await
            .map_err(|e| e.to_string())??;
        if report["verdict"] == "pass" {
            let author = author_for(t, q, rows)?;
            let Some((result, _)) = transfer::fetch_result(author)? else {
                return Err("Author publication evidence missing; review cannot pass".into());
            };
            if result.uncommitted_source {
                return Err("Published report omits uncommitted author source; recover it before accepting review".into());
            }
        }
        q.state = "publishing".into();
        q.report = Some(report.clone());
        persist(
            t,
            q,
            "Independent verdict and committed test logs recovered",
            Some(report),
        )?;
    }
    if q.state == "publishing" {
        let report = q.report.as_ref().ok_or("Review report missing")?;
        let (host, parsed) = crate::forges::host_for_remote(hosts, &t.remote)
            .ok_or("Forge connection unavailable")?;
        let number = pr_number(t)?;
        let body = format!("<!-- xnaut-review:{}:{} -->\n## @{} review: {}\n\nReviewed commit `{}` against `{}`.\n\n{}\n\nEvidence branch: `xnaut/runs/{}`.\n\n```json\n{}\n```", t.run_id,report["published_head"].as_str().ok_or("Review commit missing")?,q.reviewer,report["verdict"].as_str().unwrap_or("blocked"),q.head,q.base,report["summary"].as_str().unwrap_or(""),q.child.as_deref().unwrap_or(""),serde_json::to_string_pretty(report).unwrap().chars().take(24000).collect::<String>());
        q.comment_url = Some(
            crate::forges::ensure_repository_comment(
                host,
                &parsed.owner,
                &parsed.repo,
                number,
                &format!(
                    "<!-- xnaut-review:{}:{} -->",
                    t.run_id,
                    report["published_head"]
                        .as_str()
                        .ok_or("Review commit missing")?
                ),
                &body,
            )
            .await?,
        );
        let report = report.clone();
        q.state = match report["verdict"].as_str() {
            Some("pass") => "ready",
            Some("changes_requested") => "changes_requested",
            _ => "blocked",
        }
        .into();
        q.next_attempt_at = crate::run_control::now_ms() + REPAIR_BACKOFF_MS;
        let reason = match q.state.as_str() {
            "ready" => "Independent configured checks passed; awaiting project-authorized merge",
            "changes_requested" => "Actionable independent findings published; bounded author repair eligible after backoff",
            _ => "Independent verification environment or evidence unavailable; no code-fix attempt charged",
        };
        persist(t, q, reason, Some(report))?;
    }
    if q.state == "merging" {
        let (host, parsed) =
            crate::forges::host_for_remote(hosts, &t.remote).ok_or("Forge unavailable")?;
        let pr =
            crate::forges::repository_pr(host, &parsed.owner, &parsed.repo, pr_number(t)?).await?;
        if pr["merged"] == true && pr["head"]["sha"] == q.head {
            q.state = "merged".into();
            persist(
                t,
                q,
                "Merge confirmed by the forge after interrupted handoff",
                None,
            )?;
        } else {
            return Err("Merge outcome needs owner inspection; no automatic repeat request".into());
        }
    }
    if q.state == "ready" && permission.otto_merge {
        merge(app, t, q, hosts).await?;
        let reason = q.message.clone();
        persist(t, q, &reason, None)?;
    }
    Ok(())
}
fn pr_number(t: &Transfer) -> Result<u64, String> {
    t.pr_url
        .as_deref()
        .and_then(|v| url::Url::parse(v).ok())
        .and_then(|u| u.path_segments()?.next_back()?.parse().ok())
        .ok_or("PR identity unavailable".into())
}
pub(crate) fn chat_spec() -> Value {
    json!({"type":"function","function":{"name":"request_repository_review","description":"Queue independent testing and review of an existing published task PR using its saved run_id from registered project context. Use this for PR reviews instead of starting a duplicate repository task. Requires the owner's saved automatic-review permission; cannot enable review or merge permissions. Returns durable queue/current status, not a claim that a worker has started or completed.","parameters":{"type":"object","properties":{"run_id":{"type":"string","description":"Saved parent run_id for the user-referenced PR, supplied in registered project context."}},"required":["run_id"],"additionalProperties":false}}})
}
/// Match a PR reference in USER text only. A bare number must identify exactly
/// one known parent task; qualified project names and exact PR URLs disambiguate.
pub(crate) fn referenced_tasks(messages: &[Value], rows: &[Transfer]) -> Vec<Transfer> {
    static PR: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = PR.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(?:PR|pull\s+request)\s*(?:\\?#\s*)?([0-9]+)\b").unwrap()
    });
    let mut found = Vec::new();
    for text in crate::repository_read::user_texts(messages) {
        let clean = text.replace('*', "");
        let candidates: Vec<_> = rows
            .iter()
            .filter(|t| t.review_parent.is_none() && t.pr_url.is_some())
            .collect();
        for t in &candidates {
            let url = t.pr_url.as_deref().unwrap();
            if clean.split_whitespace().any(|word| {
                word.trim_matches(|c: char| {
                    ['(', ')', '[', ']', '<', '>', '`', '.', ','].contains(&c)
                })
                .split(['?', '#'])
                .next()
                    == Some(url)
            }) && !found.iter().any(|v: &Transfer| v.run_id == t.run_id)
            {
                found.push((*t).clone());
            }
        }
        for capture in re.captures_iter(&clean) {
            let Some(number) = capture[1].parse::<u64>().ok() else {
                continue;
            };
            let matches: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|t| pr_number(t).ok() == Some(number))
                .collect();
            let qualified: Vec<_> = matches
                .iter()
                .copied()
                .filter(|t| crate::repository_read::mentions(&clean, &t.project))
                .collect();
            let named_other = rows
                .iter()
                .any(|t| crate::repository_read::mentions(&clean, &t.project));
            let selected = if !qualified.is_empty() {
                qualified
            } else if named_other {
                vec![]
            } else {
                matches
            };
            if let [t] = selected.as_slice() {
                if !found.iter().any(|v: &Transfer| v.run_id == t.run_id) {
                    found.push((*t).clone());
                }
            }
        }
    }
    found
}
pub(crate) fn chat_references(messages: &[Value]) -> Vec<Transfer> {
    let Ok(rows) = transfer::list() else {
        return vec![];
    };
    let Ok(projects) = crate::project_management::repo_now()
        .and_then(|r| crate::project_management::list_projects(&r))
    else {
        return vec![];
    };
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|t| {
            projects.iter().any(|p| {
                p.key == t.project
                    && transfer::portable_remote(&p.forge_remote).ok().as_deref()
                        == Some(t.remote.as_str())
            })
        })
        .collect();
    referenced_tasks(messages, &rows)
}
pub(crate) fn request_from_chat(args: &Value, allowed: &[PathBuf], messages: &[Value]) -> Value {
    let result: Result<Transfer, String> = (|| {
        if crate::switches::load().read_only {
            return Err("The read_only kill-switch is engaged".into());
        }
        let run_id = args["run_id"]
            .as_str()
            .ok_or("Saved task run_id is required")?;
        let task=chat_references(messages).into_iter().find(|t|t.run_id==run_id).ok_or("This PR was not uniquely identified by the user's request. Name its registered project and PR number, or its full PR URL; no filesystem path is needed.")?;
        crate::repository_read::authorized_root(&task.project, allowed)?;
        repository_review_request(task.run_id)
    })();
    match result {
        Ok(t) => {
            json!({"ok":true,"execution_started":false,"effect":"review_queue","run_id":t.run_id,"project":t.project,"pr_url":t.pr_url,"review":t.quality,"note":"The existing PR review is queued or its existing status is returned. No duplicate task was created. Queueing alone is not worker launch, test completion, merge or release evidence. The desktop reconciler applies saved project permissions and worker gates."})
        }
        Err(error) => json!({"ok":false,"error":error}),
    }
}

/// Explicit owner action for pre-existing PRs and failed, fully stopped reviews.
#[tauri::command]
pub fn repository_review_request(run_id: String) -> Result<Transfer, String> {
    let _queue = QUEUE
        .try_lock()
        .map_err(|_| "Review queue is busy; retry shortly")?;
    let _lease = QualityLease::acquire(&transfer::store_dir()?, &run_id)?
        .ok_or("This review is advancing in another app instance; retry shortly")?;
    let rows = transfer::list()?;
    let mut t = rows
        .iter()
        .find(|t| t.run_id == run_id)
        .cloned()
        .ok_or("Transfer not found")?;
    if t.state != "review"
        || t.review_parent.is_some()
        || t.repair_parent.is_some()
        || t.pr_url.is_none()
    {
        return Err("Only published task PRs can be reviewed".into());
    }
    if !policy(&t.project, &t.remote)?.automatic_review {
        return Err("Enable automatic review in project settings first".into());
    }
    if let Some(q) = &t.quality {
        if !["blocked", "changes_requested"].contains(&q.state.as_str()) {
            return Ok(t); // stable retries return the existing queue/run, never another worker
        }
        if q.attempts >= 3 {
            return Err("Review retry limit reached; owner attention required".into());
        }
        // Unknown dispatch outcomes remain blocked; never attach to a live worker.
        if !q.worktree.is_empty() {
            let child = rows
                .iter()
                .find(|r| {
                    r.review_parent.as_deref() == Some(&t.run_id) && r.local_path == q.worktree
                })
                .ok_or("Cannot establish whether the previous worker stopped; inspect it first")?;
            if !["review", "preparation_failed", "pushed"].contains(&child.state.as_str()) {
                return Err("Previous reviewer may still be running; retry refused".into());
            }
        }
        if q.jev.is_some() {
            return Err(
                "Merge gate requires owner inspection; merge manually or publish a revised task"
                    .into(),
            );
        }
    }
    let mut q = t.quality.take().unwrap_or_default();
    if q.repair_child.is_some() {
        return Err(
            "An author repair reservation exists; recover it before requesting another reviewer"
                .into(),
        );
    }
    q.state = "pending".into();
    q.child = None;
    q.worktree.clear();
    q.report = None;
    q.comment_url = None;
    q.required_checks.clear();
    persist(
        &mut t,
        &mut q,
        "Independent review explicitly re-queued; prior evidence retained",
        None,
    )?;
    Ok(t)
}
async fn merge(
    _app: &tauri::AppHandle,
    t: &mut Transfer,
    q: &mut Review,
    hosts: &[crate::settings::ForgeHost],
) -> Result<(), String> {
    let permission = policy(&t.project, &t.remote)?;
    if !permission.automatic_review || !permission.otto_merge {
        return Err("Project merge permission was revoked".into());
    }
    if q.required_checks.is_empty() {
        return Err(
            "Configured independent verification evidence missing; request a fresh review".into(),
        );
    }
    crate::agent_profiles::agent_profile_get("otto".into())
        .map_err(|_| "Otto is unavailable; restore the profile or merge manually")?;
    if t.handle == "otto" || q.reviewer == "otto" || q.reviewer == t.handle {
        return Err("Author, reviewer, and merger must be independent".into());
    }
    let child = transfer::list()?
        .into_iter()
        .find(|c| Some(&c.run_id) == q.child.as_ref())
        .ok_or("Review receipt missing")?;
    let report = read_report(t, &child, q)?;
    if report["verdict"] != "pass" || Some(&report) != q.report.as_ref() {
        return Err("Passing review evidence changed or is missing".into());
    }
    let root = crate::sandbox::launch_env::project_root(Path::new(&t.local_path));
    let route = transfer::transfer_desktop_remote(t)?;
    git(&root, &["fetch", "--no-tags", &route, &q.head, &q.base])?;
    git(&root, &["merge-base", "--is-ancestor", &q.base, &q.head])
        .map_err(|_| "Task branch is behind the tested target; update it and review again")?;
    let stats = git(
        &root,
        &["diff", "--numstat", &format!("{}...{}", q.base, q.head)],
    )?;
    let risk = crate::merge_gate::risk_score(&crate::merge_gate::parse_numstat(&stats), true);
    if risk.score >= crate::merge_gate::HUMAN_APPROVAL_AT {
        return Err(format!(
            "High-risk change requires owner review: {}",
            risk.reasons.join(", ")
        ));
    }
    let (host, parsed) =
        crate::forges::host_for_remote(hosts, &t.remote).ok_or("Forge unavailable")?;
    let number = pr_number(t)?;
    crate::forges::repository_merge_preflight(
        host,
        &parsed.owner,
        &parsed.repo,
        number,
        &q.head,
        &q.base,
        &t.branch,
        &t.base,
    )
    .await?;
    // Bound the semantic evidence. Full repository contents and credentials are not sent.
    let original_path = format!("{}:{}/report.md", q.head, t.artifacts);
    let size = git(&root, &["cat-file", "-s", &original_path])?
        .parse::<usize>()
        .map_err(|_| "Invalid task report")?;
    if size > 32000 {
        return Err("Task report exceeds merge guard budget".into());
    }
    let original = git(&root, &["show", &original_path])?;
    let state = json!({"project":t.project,"run":t.run_id,"head":q.head,"base":q.base,"task_report":original,"changed_files":stats,"review":report,"risk":risk,"permission_revision":permission.revision});
    if q.jev.is_none() {
        q.jev = Some(crate::jev_decisions::merge_guard(&state).await?);
    }
    let decision = q.jev.as_ref().unwrap();
    if decision["head"] != q.head
        || decision["base"] != q.base
        || decision["permission_revision"] != permission.revision
    {
        return Err("Jev decision belongs to a different commit or project agreement".into());
    }
    t.quality = Some(q.clone());
    transfer::save(t)?;
    let decision = q.jev.as_ref().unwrap();
    let marker = format!(
        "<!-- xnaut-merge-guard:{}:{} -->",
        t.run_id,
        decision["id"]
            .as_str()
            .ok_or("Jev decision identity missing")?
    );
    let body=format!("{marker}\n### Otto merge guard: {}\n\nHead `{}`, target `{}`. Project agreement revision {}. Jev model `{}`.\n\n{}\n\nProbabilities: `{}`. Initial thresholds: coverage ≥ 0.90, scope ≥ 0.90, serious risk ≤ 0.10. Native forge checks remain mandatory. This judgment does not authorize a release.",if decision["allowed"]==true {"passed"} else {"blocked"},q.head,q.base,permission.revision,decision["model"].as_str().unwrap_or("unavailable"),decision["reason"].as_str().unwrap_or("No judgment"),decision["answers"]);
    crate::forges::ensure_repository_comment(
        host,
        &parsed.owner,
        &parsed.repo,
        number,
        &marker,
        &body,
    )
    .await?;
    if q.jev.as_ref().unwrap()["allowed"] != true {
        return Err(q.jev.as_ref().unwrap()["reason"]
            .as_str()
            .unwrap_or("Jev did not approve the evidence")
            .into());
    }
    // Recheck after inference; permission and both refs must still describe this action.
    crate::forges::repository_merge_preflight(
        host,
        &parsed.owner,
        &parsed.repo,
        number,
        &q.head,
        &q.base,
        &t.branch,
        &t.base,
    )
    .await?;
    let now = policy(&t.project, &t.remote)?;
    if !now.otto_merge || !now.automatic_review || now.revision != permission.revision {
        return Err("Project permission changed during review".into());
    }
    q.state = "merging".into();
    q.message = format!(
        "Otto merge stage authorized by project policy revision {}",
        now.revision
    );
    t.quality = Some(q.clone());
    transfer::save(t)?;
    let merged =
        crate::forges::repository_merge(host, &parsed.owner, &parsed.repo, number, &q.head).await?;
    q.state = "merged".into();
    q.message = format!(
        "Otto merge stage confirmed commit {}; no release was published",
        merged["merge_commit_sha"]
            .as_str()
            .unwrap_or("(forge receipt)")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> (Review, Value) {
        let q = Review {
            head: "a".repeat(40),
            base: "b".repeat(40),
            ..Review::default()
        };
        let r = json!({"head":q.head,"base":q.base,"verdict":"pass","summary":"Verified report-only task","tests":[{"command":"git diff --name-only","exit_code":0,"evidence":".xnaut/runs/reviewer/log.txt"}],"findings":[],"coverage_gaps":[]});
        (q, r)
    }
    #[test]
    fn review_requires_exact_commits_and_evidence() {
        let (q, r) = report();
        assert!(validate_report(&r, &q, ".xnaut/runs/reviewer").is_ok());
        let mut relative = r.clone();
        relative["tests"][0]["evidence"] = json!("logs/test.txt");
        assert!(validate_report(&relative, &q, ".xnaut/runs/reviewer").is_ok());
        assert_eq!(
            evidence_path("logs/test.txt", ".xnaut/runs/reviewer").unwrap(),
            ".xnaut/runs/reviewer/logs/test.txt"
        );
        for (field, value) in [
            ("head", json!("stale")),
            ("tests", json!([])),
            ("coverage_gaps", json!(["not tested"])),
            ("findings", json!([{"severity":"warning"}])),
        ] {
            let mut bad = r.clone();
            bad[field] = value;
            assert!(
                validate_report(&bad, &q, ".xnaut/runs/reviewer").is_err(),
                "{field}"
            );
        }
        let mut bad = r.clone();
        bad["tests"][0]["exit_code"] = json!(1);
        assert!(validate_report(&bad, &q, ".xnaut/runs/reviewer").is_err());
        for path in [
            ".xnaut/runs/reviewer/../log.txt",
            ".xnaut/runs/author/log.txt",
            "/tmp/log.txt",
            "../author/log.txt",
            "logs/../../author/log.txt",
            "",
        ] {
            let mut bad = r.clone();
            bad["tests"][0]["evidence"] = json!(path);
            assert!(validate_report(&bad, &q, ".xnaut/runs/reviewer").is_err());
        }
    }
    #[test]
    fn published_review_rejects_source_edits_symlinks_and_target_races() {
        let root = std::env::temp_dir().join(format!("xnaut-review-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("remote.git");
        let work = root.join("worker");
        let cache = root.join("cache.git");
        git(&root, &["init", "--bare", remote.to_str().unwrap()]).unwrap();
        git(&root, &["init", "--bare", cache.to_str().unwrap()]).unwrap();
        git(&root, &["init", "-b", "main", work.to_str().unwrap()]).unwrap();
        git(&work, &["config", "user.name", "test"]).unwrap();
        git(&work, &["config", "user.email", "test@localhost"]).unwrap();
        std::fs::write(work.join("source.txt"), "source").unwrap();
        git(&work, &["add", "."]).unwrap();
        git(&work, &["commit", "-m", "base"]).unwrap();
        let base = git(&work, &["rev-parse", "HEAD"]).unwrap();
        git(&work, &["push", remote.to_str().unwrap(), "main"]).unwrap();
        git(&work, &["checkout", "-b", "task"]).unwrap();
        std::fs::write(work.join("task.md"), "task").unwrap();
        git(&work, &["add", "."]).unwrap();
        git(&work, &["commit", "-m", "task"]).unwrap();
        let head = git(&work, &["rev-parse", "HEAD"]).unwrap();
        git(&work, &["push", remote.to_str().unwrap(), "task"]).unwrap();
        let t:Transfer=serde_json::from_value(json!({"run_id":"parent","project":"TEST","ticket":null,"handle":"author","local_path":work,"remote":remote,"source_sha":base,"base":"main","branch":"task","workdir":"worker","artifacts":".xnaut/runs/parent","state":"review","pr_url":"https://fixture/pulls/1","error":null})).unwrap();
        let child = Transfer {
            run_id: "reviewer".into(),
            handle: "ralph".into(),
            source_sha: head.clone(),
            branch: "reviewer".into(),
            artifacts: ".xnaut/runs/reviewer".into(),
            review_parent: Some("parent".into()),
            ..t.clone()
        };
        let (mut q, mut r) = report();
        q.head = head;
        q.base = base;
        r["head"] = json!(q.head);
        r["base"] = json!(q.base);
        r["tests"][0]["evidence"] = json!("log.txt");
        git(&work, &["checkout", "-b", "reviewer"]).unwrap();
        let artifacts = work.join(&child.artifacts);
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::write(artifacts.join("review.json"), r.to_string()).unwrap();
        std::fs::write(
            artifacts.join("log.txt"),
            "Test passed: real fixture output",
        )
        .unwrap();
        let publish = || {
            git(&work, &["add", "."]).unwrap();
            git(&work, &["commit", "-m", "review evidence"]).unwrap();
            git(&work, &["push", remote.to_str().unwrap(), "reviewer"]).unwrap();
        };
        publish();
        let accepted = read_report_in(&cache, &t, &child, &q).unwrap();
        assert!(accepted["published_head"].as_str().is_some());
        assert_eq!(
            accepted["tests"][0]["evidence"],
            ".xnaut/runs/reviewer/log.txt"
        );
        assert!(accepted["evidence_excerpts"][0]["output"]
            .as_str()
            .unwrap()
            .contains("real fixture"));
        std::fs::remove_file(artifacts.join("log.txt")).unwrap();
        std::os::unix::fs::symlink("../../../source.txt", artifacts.join("log.txt")).unwrap();
        publish();
        assert!(read_report_in(&cache, &t, &child, &q)
            .unwrap_err()
            .contains("empty or missing"));
        std::fs::remove_file(artifacts.join("log.txt")).unwrap();
        std::fs::write(artifacts.join("log.txt"), "Restored evidence").unwrap();
        std::fs::write(work.join("source.txt"), "reviewer changed source").unwrap();
        publish();
        assert!(read_report_in(&cache, &t, &child, &q)
            .unwrap_err()
            .contains("outside"));
        std::fs::write(work.join("source.txt"), "source").unwrap();
        publish();
        assert!(read_report_in(&cache, &t, &child, &q).is_ok());
        git(&work, &["checkout", "main"]).unwrap();
        std::fs::write(work.join("target.txt"), "target moved").unwrap();
        git(&work, &["add", "."]).unwrap();
        git(&work, &["commit", "-m", "target moves"]).unwrap();
        git(&work, &["push", remote.to_str().unwrap(), "main"]).unwrap();
        assert!(read_report_in(&cache, &t, &child, &q)
            .unwrap_err()
            .contains("changed after review"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn project_agreement_requires_explicit_review_and_rejects_stale_saves() {
        let root = std::env::temp_dir().join(format!("xnaut-policy-{}", uuid::Uuid::new_v4()));
        let path = root.join("TEST.json");
        assert!(save_policy_at(&path, "repo", 0, false, true).is_err());
        assert!(!path.exists());
        let p = save_policy_at(&path, "repo", 0, true, true).unwrap();
        assert_eq!(p.revision, 1);
        assert!(p.otto_merge);
        assert!(save_policy_at(&path, "repo", 0, true, true).is_err());
        let revoked = save_policy_at(&path, "repo", 1, false, false).unwrap();
        assert_eq!(revoked.revision, 2);
        assert!(!revoked.otto_merge);
        assert!(save_policy_at(&path, "repo", 1, true, true).is_err());
        let saved: Policy = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(!saved.otto_merge);
        assert!(path.with_extension("revision-1.json").is_file());
        assert!(path.with_extension("revision-2.json").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn chat_pr_scope_requires_user_reference_and_never_guesses_between_projects() {
        let task = |project: &str, id: &str| -> Transfer {
            serde_json::from_value(json!({"run_id":id,"project":project,"ticket":null,"handle":"author","local_path":"/fixture","remote":"https://forge.test/team/app.git","source_sha":"a".repeat(40),"base":"main","branch":"task","workdir":"worker","artifacts":".xnaut/runs/fixture","state":"review","pr_url":format!("https://forge.test/team/{project}/pulls/105"),"error":null})).unwrap()
        };
        let a = task("XNAUT", "a");
        let b = task("JOBUP", "b");
        for text in [
            "verify smoke-test PR #105",
            r"verify **PR \#105**",
            "review pull request 105",
        ] {
            let refs = referenced_tasks(&[json!({"role":"user","content":text})], &[a.clone()]);
            assert_eq!(refs.len(), 1, "{text}");
            assert_eq!(refs[0].run_id, "a");
        }
        for role in ["assistant", "tool", "system"] {
            assert!(referenced_tasks(
                &[json!({"role":role,"content":"Review XNAUT PR #105"})],
                &[a.clone()]
            )
            .is_empty());
        }
        assert!(referenced_tasks(
            &[json!({"role":"user","content":"Review PR #105"})],
            &[a.clone(), b.clone()]
        )
        .is_empty());
        let scoped = referenced_tasks(
            &[json!({"role":"user","content":"Review xNAUT PR #105"})],
            &[a.clone(), b.clone()],
        );
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].project, "XNAUT");
        let url = referenced_tasks(
            &[json!({"role":"user","content":b.pr_url})],
            &[a.clone(), b.clone()],
        );
        assert_eq!(url.len(), 1);
        assert_eq!(url[0].run_id, "b");
        assert!(referenced_tasks(
            &[json!({"role":"user","content":"Review PR #1050"})],
            &[a.clone()]
        )
        .is_empty());
        let child = Transfer {
            review_parent: Some("parent".into()),
            ..a
        };
        assert!(referenced_tasks(
            &[json!({"role":"user","content":"Review PR #105"})],
            &[child]
        )
        .is_empty());
    }

    #[test]
    fn policy_never_grants_implicit_authority() {
        let p = Policy::default();
        assert!(!p.automatic_review && !p.otto_merge);
    }
}

#[cfg(test)]
mod repair_loop_tests {
    use super::*;
    use crate::run_control::{self, RunManifest, RunState};
    struct Fixture {
        root: PathBuf,
        work: PathBuf,
        remote: PathBuf,
        registry: PathBuf,
        store: PathBuf,
        parent: Transfer,
        q: Review,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("xnaut-repair-loop-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let work = root.join("author");
            let remote = root.join("remote.git");
            let registry = root.join("registry");
            let store = root.join("transfers");
            git(&root, &["init", "--bare", remote.to_str().unwrap()]).unwrap();
            git(&root, &["init", "-b", "main", work.to_str().unwrap()]).unwrap();
            git(&work, &["config", "user.name", "Fixture"]).unwrap();
            git(&work, &["config", "user.email", "fixture@example.invalid"]).unwrap();
            git(&work, &["config", "commit.gpgsign", "false"]).unwrap();
            std::fs::write(work.join("README"), "Fixture task").unwrap();
            git(&work, &["add", "."]).unwrap();
            git(&work, &["commit", "-m", "base"]).unwrap();
            let base = git(&work, &["rev-parse", "HEAD"]).unwrap();
            git(&work, &["push", remote.to_str().unwrap(), "main"]).unwrap();
            git(&work, &["checkout", "-b", "task"]).unwrap();
            std::fs::create_dir_all(work.join(".xnaut")).unwrap();
            std::fs::write(work.join(".xnaut/verify.json"), r#"{"test":"sh test.sh"}"#).unwrap();
            std::fs::write(work.join("app.sh"), "#!/bin/sh\necho 4\n").unwrap();
            std::fs::write(work.join("test.sh"),"#!/bin/sh\ngot=$(sh app.sh)\nif [ \"$got\" != 5 ]; then echo \"FAIL: expected 5, got $got\"; exit 1; fi\necho 'PASS: expected 5'\n").unwrap();
            git(&work, &["add", "."]).unwrap();
            git(&work, &["commit", "-m", "author defect"]).unwrap();
            let head = git(&work, &["rev-parse", "HEAD"]).unwrap();
            git(&work, &["push", remote.to_str().unwrap(), "task"]).unwrap();
            let mut run = RunManifest::requested(
                "author",
                "fixture",
                work.to_str().unwrap(),
                Some("TEST-1".into()),
                None,
                &[],
                1000,
            );
            run.project = "TEST".into();
            run.state = RunState::Done;
            let run = run_control::request_in(&registry, run, || Ok(())).unwrap();
            run_control::update_in(&registry, &run.run_id, |r| r.state = RunState::Done).unwrap();
            let parent:Transfer=serde_json::from_value(json!({"run_id":run.run_id,"project":"TEST","ticket":"TEST-1","handle":"author","local_path":work,"local_branch":"task","remote":remote,"source_sha":base,"base":"main","branch":"task","workdir":"worker","artifacts":format!(".xnaut/runs/{}",run.run_id),"state":"review","pr_url":"https://fixture/pulls/17","error":null})).unwrap();
            let q = Review {
                head,
                base,
                required_checks: vec![RequiredCheck {
                    name: "test".into(),
                    command: "sh test.sh".into(),
                }],
                ..Default::default()
            };
            Self {
                root,
                work,
                remote,
                registry,
                store,
                parent,
                q,
            }
        }
        fn reload(&mut self) {
            self.parent = serde_json::from_slice(
                &std::fs::read(self.store.join(format!("{}.json", self.parent.run_id))).unwrap(),
            )
            .unwrap();
            self.q = self.parent.quality.clone().unwrap();
        }
        fn save(&mut self) {
            self.parent.quality = Some(self.q.clone());
            transfer::save_at(&self.store, &self.parent).unwrap();
        }
        fn review(&mut self, id: &str, exit: i32) -> Transfer {
            let path = self.root.join(id);
            let branch = format!("evidence-{id}");
            git(
                &self.work,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    path.to_str().unwrap(),
                    &self.q.head,
                ],
            )
            .unwrap();
            git(&path, &["checkout", "-b", &branch]).unwrap();
            let output = std::process::Command::new("sh")
                .arg("test.sh")
                .current_dir(&path)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(exit));
            let artifacts = format!(".xnaut/runs/{id}");
            std::fs::create_dir_all(path.join(&artifacts)).unwrap();
            std::fs::write(path.join(&artifacts).join("test.log"), &output.stdout).unwrap();
            let report = json!({"head":self.q.head,"base":self.q.base,"verdict":if exit==0 {"pass"}else{"changes_requested"},"summary":"Independent fixture execution","tests":[{"command":"sh test.sh","exit_code":exit,"evidence":"test.log"}],"findings":if exit==0 {json!([])}else{json!([{"severity":"blocking","file":"app.sh","detail":"Expected 5; implementation returns 4"}])},"coverage_gaps":[]});
            std::fs::write(
                path.join(&artifacts).join("review.json"),
                report.to_string(),
            )
            .unwrap();
            git(&path, &["add", "."]).unwrap();
            git(&path, &["commit", "-m", "independent evidence"]).unwrap();
            git(&path, &["push", self.remote.to_str().unwrap(), &branch]).unwrap();
            let child = Transfer {
                run_id: id.into(),
                ticket: None,
                handle: self.q.reviewer.clone(),
                local_path: path.to_string_lossy().into(),
                local_branch: branch.clone(),
                source_sha: self.q.head.clone(),
                branch,
                artifacts,
                review_parent: Some(self.parent.run_id.clone()),
                repair_parent: None,
                quality: None,
                ..self.parent.clone()
            };
            let cache = self.root.join(format!("{id}.git"));
            git(&self.root, &["init", "--bare", cache.to_str().unwrap()]).unwrap();
            self.q.child = Some(id.into());
            self.q.worktree = path.to_string_lossy().into();
            self.q.report = Some(read_report_in(&cache, &self.parent, &child, &self.q).unwrap());
            self.q.state = if exit == 0 {
                "ready"
            } else {
                "changes_requested"
            }
            .into();
            self.q.comment_url = Some(format!("https://fixture/pulls/17#{id}"));
            let report = self.q.report.clone();
            event(
                &self.parent,
                &mut self.q,
                "Independent fixture verdict published",
                report,
            );
            self.save();
            child
        }
        fn proof(&self) -> run_control::Proofs {
            run_control::Proofs {
                pid_absent: true,
                session_known: true,
                capture_known: true,
                capture_quiet: true,
                worktree_exists: true,
                branch_matches: true,
                commit: self.q.head.clone(),
                ..Default::default()
            }
        }
    }
    /// Uses real Git branches, real failing/passing shell tests, committed
    /// independent report/log blobs, persisted transfer receipts and the native
    /// registry admission/continuation path. Only provider/forge transport is
    /// replaced by local repositories; live CLI/provider coverage is XNAUT-467.
    #[test]
    fn failed_check_repair_same_pr_fresh_review_survives_each_persisted_boundary() {
        let mut f = Fixture::new();
        let red = f.review("review-red", 1);
        let original_head = f.q.head.clone();
        let original_pr = f.parent.pr_url.clone();
        assert!(independent_completion_refusal_in(
            &[f.parent.clone(), red.clone()],
            "TEST-1",
            &original_head
        )
        .unwrap()
        .is_some());
        f.reload();
        let proof = f.proof();
        let author = f.parent.run_id.clone();
        reserve_repair_at(
            &f.store,
            &f.registry,
            &mut f.parent,
            &mut f.q,
            &author,
            &proof,
            2000,
        )
        .unwrap();
        let reserved = f.q.repair_child.clone().unwrap();
        f.reload();
        // Restart can discover exactly the existing native reservation.
        assert_eq!(
            run_control::reserve_repair_in(&f.registry, &author, &proof, 3000)
                .unwrap()
                .run_id,
            reserved
        );
        assert_eq!(run_control::list_ids_in(&f.registry).unwrap().len(), 2);
        let mut launch = RunManifest::requested(
            "author",
            "fixture",
            f.work.to_str().unwrap(),
            Some("TEST-1".into()),
            None,
            &[],
            3000,
        );
        launch.project = "TEST".into();
        run_control::bind_pending_in(&f.registry, &mut launch).unwrap();
        assert_eq!(launch.run_id, reserved);
        let mut child = Transfer {
            run_id: launch.run_id.clone(),
            source_sha: original_head.clone(),
            branch: format!("xnaut/runs/{}", launch.run_id),
            artifacts: format!(".xnaut/runs/{}", launch.run_id),
            pr_url: None,
            quality: Some(Review::default()),
            ..f.parent.clone()
        };
        transfer::inherit_repair_delivery(&mut child, &[f.parent.clone()]).unwrap();
        assert_eq!(child.branch, f.parent.branch);
        assert_eq!(child.pr_url, original_pr);
        assert_ne!(child.artifacts, f.parent.artifacts);
        let admitted = run_control::request_in(&f.registry, launch.clone(), || Ok(())).unwrap();
        assert_eq!(admitted.run_id, reserved);
        assert!(
            run_control::request_in(&f.registry, launch, || panic!("duplicate admission")).is_err()
        );
        child.state = "running".into();
        transfer::save_at(&f.store, &child).unwrap();
        f.q.state = "repair_running".into();
        f.q.repair_attempts = 1;
        f.save();
        f.reload();
        std::fs::write(f.work.join("app.sh"), "#!/bin/sh\necho 5\n").unwrap();
        git(&f.work, &["add", "app.sh"]).unwrap();
        git(&f.work, &["commit", "-m", "repair independent finding"]).unwrap();
        let fixed = git(&f.work, &["rev-parse", "HEAD"]).unwrap();
        git(&f.work, &["push", f.remote.to_str().unwrap(), "task"]).unwrap();
        child.state = "review".into();
        transfer::save_at(&f.store, &child).unwrap();
        run_control::update_in(&f.registry, &reserved, |run| {
            run.state = RunState::Done;
            run.last_commit = fixed.clone();
        })
        .unwrap();
        let changed = git(
            &f.work,
            &[
                "diff",
                "--name-only",
                &original_head,
                &fixed,
                "--",
                ".",
                ":(exclude).xnaut/runs",
            ],
        )
        .unwrap();
        accept_repair_publication(&f.parent, &mut f.q, &child, &fixed, !changed.is_empty())
            .unwrap();
        assert!(
            f.q.report.is_none(),
            "old red report cannot become a pass for the new head"
        );
        f.save();
        f.reload();
        f.q.required_checks = vec![RequiredCheck {
            name: "test".into(),
            command: "sh test.sh".into(),
        }];
        let green = f.review("review-green", 0);
        f.reload();
        assert!(independent_completion_refusal_in(
            &[f.parent.clone(), child.clone(), red, green.clone()],
            "TEST-1",
            &fixed
        )
        .unwrap()
        .is_none());
        for field in ["handle", "state", "remote", "project"] {
            let mut wrong = serde_json::to_value(&green).unwrap();
            wrong[field] = json!("unrelated");
            let wrong: Transfer = serde_json::from_value(wrong).unwrap();
            assert!(
                independent_completion_refusal_in(
                    &[f.parent.clone(), child.clone(), wrong],
                    "TEST-1",
                    &fixed
                )
                .unwrap()
                .is_some(),
                "{field}"
            );
        }
        assert_eq!(f.parent.pr_url, original_pr);
        assert_eq!(f.q.repair_attempts, 1);
        assert!(f.q.events.iter().any(|e| e
            .evidence
            .as_ref()
            .is_some_and(|r| r["verdict"] == "changes_requested")));
        assert!(f.q.events.iter().all(|e| e.actor == "xNAUT coordinator"));
        let predecessor = run_control::load_manifest_in(&f.registry, &author).unwrap();
        assert_eq!(predecessor.next_run_id.as_deref(), Some(reserved.as_str()));
        assert_eq!(
            run_control::load_manifest_in(&f.registry, &reserved)
                .unwrap()
                .previous_run_id
                .as_deref(),
            Some(author.as_str())
        );
        assert!(
            git(
                &f.work,
                &[
                    "ls-remote",
                    f.remote.to_str().unwrap(),
                    &format!("refs/heads/xnaut/runs/{reserved}")
                ]
            )
            .unwrap()
            .is_empty(),
            "repair must not create a second task branch"
        );
    }
    #[test]
    fn unknown_stop_changed_delivery_and_exhausted_repair_never_admit_a_worker() {
        let mut f = Fixture::new();
        f.review("review-red", 1);
        let author = f.parent.run_id.clone();
        let mut proof = f.proof();
        proof.pid_absent = false;
        assert!(reserve_repair_at(
            &f.store,
            &f.registry,
            &mut f.parent,
            &mut f.q,
            &author,
            &proof,
            2000
        )
        .is_err());
        assert_eq!(run_control::list_ids_in(&f.registry).unwrap().len(), 1);
        proof.pid_absent = true;
        f.q.repair_attempts = MAX_REPAIR_ATTEMPTS;
        assert!(reserve_repair_at(
            &f.store,
            &f.registry,
            &mut f.parent,
            &mut f.q,
            &author,
            &proof,
            2000
        )
        .unwrap_err()
        .contains("limit"));
        assert_eq!(run_control::list_ids_in(&f.registry).unwrap().len(), 1);
        f.q.repair_attempts = 0;
        reserve_repair_at(
            &f.store,
            &f.registry,
            &mut f.parent,
            &mut f.q,
            &author,
            &proof,
            2000,
        )
        .unwrap();
        let mut bad = Transfer {
            run_id: f.q.repair_child.clone().unwrap(),
            source_sha: f.q.head.clone(),
            local_branch: "somebody-elses-branch".into(),
            ..f.parent.clone()
        };
        assert!(transfer::inherit_repair_delivery(&mut bad, &[f.parent.clone()]).is_err());
        let mut child = Transfer {
            run_id: f.q.repair_child.clone().unwrap(),
            source_sha: f.q.head.clone(),
            ..f.parent.clone()
        };
        transfer::inherit_repair_delivery(&mut child, &[f.parent.clone()]).unwrap();
        assert!(
            accept_repair_publication(&f.parent, &mut f.q, &child, &"f".repeat(40), false)
                .unwrap_err()
                .contains("did not change")
        );
        assert_eq!(
            f.q.repair_attempts, 0,
            "reservation/environment checks are not code-fix attempts"
        );
    }
    #[test]
    fn persisted_dead_reviewer_deadline_and_cross_process_lease_preserve_the_existing_run() {
        let mut f = Fixture::new();
        f.save();
        let first = QualityLease::acquire(&f.store, &f.parent.run_id)
            .unwrap()
            .unwrap();
        assert!(QualityLease::acquire(&f.store, &f.parent.run_id)
            .unwrap()
            .is_none());
        drop(first);
        let _next = QualityLease::acquire(&f.store, &f.parent.run_id)
            .unwrap()
            .unwrap();
        f.q.state = "running".into();
        f.q.child = Some(f.parent.run_id.clone());
        f.q.deadline_at = 10_000;
        f.save();
        run_control::update_in(&f.registry, &f.parent.run_id, |r| {
            r.state = RunState::Failed
        })
        .unwrap();
        f.reload();
        assert!(
            worker_deadline_in(&f.registry, &f.q, &f.parent.run_id, 9000)
                .unwrap_err()
                .contains("stopped")
        );
        assert!(
            worker_deadline_in(&f.registry, &f.q, "missing-reviewer", 10_000)
                .unwrap_err()
                .contains("deadline")
        );
        f.q.state = "blocked".into();
        let reason = "Existing reviewer failed; artifacts preserved";
        event(&f.parent, &mut f.q, reason, None);
        f.save();
        f.reload();
        assert_eq!(f.q.state, "blocked");
        assert_eq!(f.q.events.last().unwrap().reason, reason);
        assert_eq!(run_control::list_ids_in(&f.registry).unwrap().len(), 1);
    }

    #[test]
    fn configured_commands_and_exact_review_revision_are_mandatory() {
        let mut f = Fixture::new();
        let review = f.review("review-red", 1);
        let mut report = f.q.report.clone().unwrap();
        report["verdict"] = json!("pass");
        report["tests"][0]["exit_code"] = json!(0);
        report["tests"][0]["command"] = json!("echo success");
        report["findings"] = json!([]);
        assert!(validate_report(&report, &f.q, &review.artifacts)
            .unwrap_err()
            .contains("required"));
        f.q.state = "ready".into();
        f.q.report = Some(report);
        f.save();
        assert!(independent_completion_refusal_in(
            &[f.parent.clone(), review],
            "TEST-1",
            &f.q.head
        )
        .unwrap()
        .is_some());
        assert!(
            independent_completion_refusal_in(&[f.parent.clone()], "TEST-1", &"f".repeat(40))
                .unwrap()
                .is_some()
        );
    }
}
