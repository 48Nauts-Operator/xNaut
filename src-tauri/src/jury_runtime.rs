// XNAUT-303 jury execution. Reviewer isolation and registry accounting are
// shared by plan approval and sign-off. No reviewer receives another verdict.
use crate::jury::*;
use crate::run_control::{self, RunKind, RunManifest, RunState};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tauri::AppHandle;

pub(crate) struct ChildGuard(pub std::process::Child);
impl std::ops::Deref for ChildGuard {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", "--", &format!("-{}", self.0.id())])
                .status();
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

static ACTIVE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, usize>>> =
    std::sync::OnceLock::new();
pub struct Active(String);
impl Active {
    pub fn new(id: &str) -> Self {
        *ACTIVE
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .entry(id.into())
            .or_default() += 1;
        Self(id.into())
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        let mut m = ACTIVE.get().unwrap().lock().unwrap();
        if let Some(n) = m.get_mut(&self.0) {
            *n -= 1;
            if *n == 0 {
                m.remove(&self.0);
            }
        }
    }
}
fn active(id: &str) -> bool {
    ACTIVE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .contains_key(id)
}

pub fn store() -> Result<PathBuf, String> {
    Ok(crate::agents::registry_dir()?.join("jury"))
}
pub fn ticket(repo: &Path, id: &str) -> Result<crate::project_management::TicketRecord, String> {
    crate::project_management::ticket_list_in(repo, None)?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("ticket {id} not found"))
}
pub fn ticket_snapshot(t: &crate::project_management::TicketRecord) -> String {
    let mut evidence = serde_json::to_value(t).unwrap();
    if let Some(object) = evidence.as_object_mut() {
        object.remove("jury_reviews");
        object.remove("signoff");
    }
    serde_json::to_string_pretty(&evidence).unwrap()
}
pub fn scope_hash(t: &crate::project_management::TicketRecord) -> String {
    let mut data = serde_json::to_value(t).unwrap();
    if let Some(object) = data.as_object_mut() {
        for key in [
            "jury_reviews",
            "signoff",
            "revision",
            "updated_at",
            "status",
        ] {
            object.remove(key);
        }
    }
    hash(&serde_json::to_string(&data).unwrap())
}
/// The integration branch name without needing a repo or a project: the
/// already-merged check runs before either is resolved, and the default is
/// the same one `Policy::default()` carries.
pub fn policy_integration_branch() -> String {
    crate::jury::Policy::default().integration_branch
}

pub fn policy(repo: &Path, project: &str) -> Result<Policy, String> {
    if !project
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("invalid project key".into());
    }
    let config = dirs::home_dir()
        .ok_or("home unavailable")?
        .join(".config/xnaut");
    load_policy(&repo.join("projects").join(project), &config)
}
pub fn git(tree: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(tree)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
pub fn new_job(
    gate: Gate,
    t: &crate::project_management::TicketRecord,
    tree: &Path,
    input: String,
    policy: Policy,
    author_run: Option<String>,
    inbox: Option<String>,
) -> Result<Job, String> {
    let round = 1 + t
        .approval
        .jury_reviews
        .iter()
        .filter(|j| j.gate == gate && j.decision == Some(Decision::ChangesRequested))
        .count() as u32;
    Ok(Job {
        id: uuid::Uuid::new_v4().to_string(),
        gate,
        ticket: t.id.clone(),
        project: t.project.clone(),
        worktree: tree
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into(),
        author: t
            .handback
            .as_ref()
            .map(|h| h.from.clone())
            .filter(|s| !s.is_empty())
            .or_else(|| t.owner.clone())
            .unwrap_or_default(),
        author_run,
        ticket_revision: t.revision,
        ticket_scope_hash: scope_hash(t),
        input_hash: hash(&input),
        input,
        source_sha: git(tree, &["rev-parse", "HEAD"])?,
        deadline: run_control::now_ms() + policy.deadline_seconds as i64 * 1000,
        policy,
        round,
        restarts: 0,
        verify_restarts: 0,
        reviews: vec![],
        decision: None,
        owner_approved: false,
        reason: String::new(),
        inbox_id: inbox,
        notify_id: None,
        state: "requested".into(),
        signoff: None,
        plan_file: None,
        plan_hash: None,
    })
}

fn private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn copy_auth(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_file() {
        std::fs::copy(from, to).map_err(|_| "could not provision reviewer authentication")?;
    }
    Ok(())
}

/// Native process sandbox, not a prompt promise. Other platforms escalate
/// until they have an equivalent OS boundary, rather than silently running free.
pub fn sandbox_profile(scratch: &Path, denied: &[PathBuf]) -> String {
    // Seatbelt matches `subpath` against the REAL path of what a process opens.
    // macOS temp dirs are symlinks (/var -> /private/var, /tmp -> /private/tmp),
    // so a rule written against the symlinked spelling never matches and the
    // deny silently does nothing: a reviewer could read its peer. Found when the
    // isolation test passed under an in-worktree TMPDIR and failed under the
    // system one, 2026-09-07. Canonicalise what exists; a path that does not
    // exist yet is kept as given.
    let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let scratch = real(scratch);
    let denied: Vec<PathBuf> = denied.iter().map(|p| real(p)).collect();
    let scratch = scratch.as_path();
    let denied = denied.as_slice();
    let q = |p: &Path| serde_json::to_string(&p.to_string_lossy()).unwrap();
    let mut s = format!(
        "(version 1) (allow default) (deny file-write* (require-not (subpath {})))",
        q(scratch)
    );
    for path in denied {
        s.push_str(&format!(
            " (deny file-read-data (require-all (subpath {}) (require-not (subpath {}))))",
            q(path),
            q(scratch)
        ));
    }
    s
}
fn prepare_command(
    runtime: &str,
    scratch: &Path,
    deny: &[PathBuf],
    deadline: i64,
) -> Result<Command, String> {
    if !cfg!(target_os = "macos") {
        return Err("reviewer OS isolation unavailable on this host".into());
    }
    let home = dirs::home_dir().ok_or("home unavailable")?;
    let profile = scratch.join("sandbox.sb");
    let q = |p: &Path| serde_json::to_string(&p.to_string_lossy()).unwrap();
    let mut sandbox = sandbox_profile(scratch, deny);
    // Provider auth is copied into this review's private directory. All other
    // home data, including the owner's credentials and other agent sessions,
    // is unreadable. Only installed executable packages are exceptions.
    sandbox.push_str(&format!(" (deny file-read-data (require-all (subpath {}) (require-not (subpath {})) (require-not (subpath {})) (require-not (subpath {}))))",
        q(&home),q(scratch),q(&home.join(".codex/packages")),q(&home.join(".local/bin"))));
    std::fs::write(&profile, sandbox).map_err(|e| e.to_string())?;
    let mut c = Command::new("/usr/bin/sandbox-exec");
    c.args(["-f"]).arg(&profile);
    match runtime {
        "codex" => {
            let auth = scratch.join("codex");
            private_dir(&auth)?;
            let source = std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"));
            copy_auth(&source.join("auth.json"), &auth.join("auth.json"))?;
            c.args([
                "codex",
                "exec",
                "--ignore-user-config",
                "--ignore-rules",
                "--ephemeral",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "-c",
                "web_search=\"disabled\"",
                "--disable",
                "shell_tool",
                "--disable",
                "multi_agent",
                "--disable",
                "unified_exec",
                "--json",
                "-",
            ]);
            c.env("CODEX_HOME", auth);
        }
        "gemini" => {
            let auth = scratch.join(".gemini");
            private_dir(&auth)?;
            if !home.join(".gemini/oauth_creds.json").is_file() {
                return Err("Gemini OAuth authentication is unavailable; owner must sign in before a jury launch".into());
            }
            for file in ["oauth_creds.json", "google_accounts.json"] {
                copy_auth(&home.join(".gemini").join(file), &auth.join(file))?;
            }
            std::fs::write(auth.join("settings.json"),r#"{"security":{"auth":{"selectedType":"oauth-personal"}},"tools":{"core":[]},"mcpServers":{},"hooks":{},"general":{"enableAutoUpdate":false}}"#).map_err(|e|e.to_string())?;
            let rule = scratch.join("deny-tools.toml");
            std::fs::write(
                &rule,
                "[[rule]]\ntoolName = \"*\"\ndecision = \"deny\"\npriority = 999\n",
            )
            .map_err(|e| e.to_string())?;
            c.args([
                "gemini",
                "--output-format",
                "json",
                "--approval-mode",
                "default",
                "--extensions",
                "none",
                "--admin-policy",
            ])
            .arg(rule)
            .args([
                "-p",
                "Review the evidence on stdin and return the requested JSON.",
            ]);
            c.env("GEMINI_CLI_HOME", scratch);
        }
        "claude" => {
            let auth = scratch.join("claude");
            private_dir(&auth)?;
            // Claude's effective macOS credential is the Keychain item. The
            // fallback file can remain stale after a successful login repair.
            // Capture credentials privately; never put provider output in errors.
            let keychain = Command::new("/usr/bin/security")
                .args([
                    "find-generic-password",
                    "-s",
                    "Claude Code-credentials",
                    "-w",
                ])
                .output()
                .ok()
                .filter(|o| o.status.success());
            let bytes = match keychain {
                Some(output) => output.stdout,
                None => std::fs::read(home.join(".claude/.credentials.json"))
                    .map_err(|_| "Claude authentication unavailable")?,
            };
            let credentials: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| "Claude authentication is invalid")?;
            let expires = credentials["claudeAiOauth"]["expiresAt"]
                .as_u64()
                .unwrap_or(0);
            if expires <= deadline.saturating_add(60_000) as u64 {
                return Err(
                    "Claude credential expires before the review deadline; refresh login first"
                        .into(),
                );
            }
            std::fs::write(auth.join(".credentials.json"), bytes)
                .map_err(|_| "could not provision reviewer authentication")?;
            c.args([
                "claude",
                "--print",
                "--output-format",
                "json",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--no-session-persistence",
            ]);
            c.env("CLAUDE_CONFIG_DIR", auth);
        }
        _ => return Err("unsupported reviewer runtime".into()),
    }
    for (k, _) in std::env::vars() {
        if k.starts_with("XNAUT_") || k.starts_with("ZELLIJ") {
            c.env_remove(k);
        }
    }
    c.current_dir(scratch);
    private_dir(&scratch.join("tmp"))?;
    c.env("TMPDIR", scratch.join("tmp"));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    Ok(c)
}

pub fn review_one(
    registry: &Path,
    root: &Path,
    control: &Path,
    job: &Job,
    runtime: &str,
    admitted: &RunManifest,
) -> ReviewRecord {
    let mut result = ReviewRecord {
        run_id: admitted.run_id.clone(),
        runtime: runtime.into(),
        input_hash: job.input_hash.clone(),
        finished_at: run_control::now_ms(),
        review: None,
        error: None,
    };
    let scratch = root.join(format!("review-{}", admitted.run_id));
    let execute = || -> Result<Review, String> {
        private_dir(&scratch)?;
        let scratch = scratch.canonicalize().map_err(|e| e.to_string())?;
        let home = dirs::home_dir().ok_or("home unavailable")?;
        let mut c = prepare_command(
            runtime,
            &scratch,
            &[
                PathBuf::from(&job.worktree),
                registry.to_path_buf(),
                control.to_path_buf(),
                home.join(".config/xnaut"),
                home.join("Library/Application Support/xnaut"),
            ],
            job.deadline,
        )?;
        // File-backed stdin cannot block the supervisor while a hung reviewer
        // fails to consume a large diff. The deadline loop owns every wait.
        let input_path = scratch.join("input.txt");
        std::fs::write(&input_path, prompt(job)).map_err(|e| e.to_string())?;
        c.stdin(Stdio::from(
            std::fs::File::open(&input_path).map_err(|e| e.to_string())?,
        ));
        let output_path = scratch.join("output.jsonl");
        let output = std::fs::File::create(&output_path).map_err(|e| e.to_string())?;
        c.stdout(output.try_clone().map_err(|e| e.to_string())?)
            .stderr(output);
        let mut child = ChildGuard(c.spawn().map_err(|e| format!("reviewer spawn: {e}"))?);
        let pid = child.id();
        run_control::update_in(registry, &admitted.run_id, |r| {
            r.pid = Some(pid);
            r.process_birth = run_control::process_birth(pid);
            r.state = RunState::Running;
            r.output_path = Some(output_path.to_string_lossy().into());
            r.last_signal = "isolated blind reviewer running".into();
        })?;
        loop {
            if std::fs::metadata(&output_path)
                .map(|m| m.len() > 2_000_000)
                .unwrap_or(false)
            {
                return Err("reviewer output exceeds limit".into());
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    return Err(format!("reviewer exited {status}"));
                }
                let text = std::fs::read_to_string(&output_path).map_err(|e| e.to_string())?;
                if text.len() > 2_000_000 {
                    return Err("reviewer output exceeds limit".into());
                }
                return parse_review(&text);
            }
            if run_control::now_ms() >= job.deadline {
                // Kill the process group, not just the shell or Node viewport.
                let _ = Command::new("/bin/kill")
                    .args(["-TERM", "--", &format!("-{pid}")])
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                return Err("reviewer absent at registry deadline".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    };
    match execute() {
        Ok(v) => result.review = Some(v),
        Err(e) => result.error = Some(e),
    }
    result.finished_at = run_control::now_ms();
    let _ = run_control::update_in(registry, &admitted.run_id, |r| {
        r.state = if result.error.is_some() {
            RunState::Failed
        } else {
            RunState::Done
        };
        r.last_progress_at = result.finished_at;
        r.last_signal = serde_json::to_string(&result).unwrap_or_default();
    });
    // Authentication material is transient, never included in the proof bundle.
    for sub in ["codex", "claude", ".gemini"] {
        let _ = std::fs::remove_dir_all(scratch.join(sub));
    }
    result
}

pub fn run_pair(registry: &Path, root: &Path, control: &Path, job: &mut Job) -> Result<(), String> {
    let mut runs = vec![];
    let mut refusals = vec![];
    for runtime in &job.policy.reviewers {
        let mut run = RunManifest::requested(
            "nautbot-jury",
            runtime,
            &job.worktree,
            Some(job.ticket.clone()),
            None,
            run_control::now_ms(),
        );
        run.kind = RunKind::Review;
        let admitted = run_control::request_in(registry, run.clone(), || {
            let switches = crate::switches::load();
            if switches.read_only || switches.is_quarantined(runtime) {
                return Err("reviewer admission disabled".into());
            }
            let live = run_control::list_ids_in(registry)?
                .iter()
                .filter_map(|id| run_control::load_manifest_in(registry, id).ok())
                .filter(|r| !r.state.terminal())
                .count();
            crate::spend::admit_review_launch(live.saturating_sub(1))
        });
        match admitted {
            Ok(admitted) => {
                runs.push((runtime.clone(), admitted));
                refusals.push(None);
            }
            Err(e) => {
                runs.push((
                    runtime.clone(),
                    run_control::load_manifest_in(registry, &run.run_id)?,
                ));
                refusals.push(Some(e));
            }
        }
    }
    // Store both identities BEFORE launch so a crash/kill remains an absence,
    // and a sweep can escalate without inventing successful review records.
    job.reviews = runs
        .iter()
        .map(|(runtime, r)| ReviewRecord {
            run_id: r.run_id.clone(),
            runtime: runtime.clone(),
            input_hash: job.input_hash.clone(),
            finished_at: 0,
            review: None,
            error: None,
        })
        .collect();
    if refusals.iter().any(Option::is_some) {
        for (i, record) in job.reviews.iter_mut().enumerate() {
            record.error = Some(
                refusals[i]
                    .clone()
                    .unwrap_or_else(|| "pair admission refused; peer was not launched".into()),
            );
            record.finished_at = run_control::now_ms();
            run_control::update_in(registry, &record.run_id, |r| {
                r.state = RunState::Failed;
                r.last_signal = record.error.clone().unwrap();
            })?;
        }
        write_job(root, job)?;
        return Ok(());
    }
    job.state = "reviewing".into();
    write_job(root, job)?;
    job.reviews = std::thread::scope(|s| {
        let handles: Vec<_> = runs
            .iter()
            .map(|(runtime, r)| s.spawn(|| review_one(registry, root, control, job, runtime, r)))
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    write_job(root, job)
}

pub fn run_job(
    app: Option<&AppHandle>,
    repo: &Path,
    registry: &Path,
    root: &Path,
    mut job: Job,
    tier_reason: Option<String>,
) -> Result<Job, String> {
    let _active = Active::new(&job.id);
    write_job(root, &job)?;
    let mut reason = tier_reason;
    if reason.is_none() {
        if let Err(e) = run_pair(registry, root, repo, &mut job) {
            reason = Some(e);
        }
    }
    let current = ticket(repo, &job.ticket)?;
    if let Some(file) = &job.plan_file {
        if std::fs::read_to_string(file).ok().map(|s| hash(&s)) != job.plan_hash {
            reason = Some("plan changed during review".into());
        }
    }
    if current.revision != job.ticket_revision {
        reason = Some("ticket changed during review; owner must review the new scope".into());
    }
    if git(Path::new(&job.worktree), &["rev-parse", "HEAD"])? != job.source_sha {
        reason = Some("source commit changed during review".into());
    }
    let (decision, why) = decide(
        &job.policy,
        &job.reviews,
        &job.input_hash,
        job.deadline,
        job.round,
        reason.as_deref(),
    );
    job.decision = Some(decision);
    job.reason = why;
    job.state = "decided".into();
    crate::memory::note(crate::memory::Entry {
        kind: "decision".into(),
        project: job.project.clone(),
        ticket: job.ticket.clone(),
        run_id: job.author_run.clone().unwrap_or_default(),
        text: format!("{:?} gate decided {:?}: {}", job.gate, decision, job.reason.lines().next().unwrap_or("")),
        source: format!("jury:{}:decided", job.id),
        ..Default::default()
    });
    write_job(root, &job)?;
    crate::project_management::attach_jury_in(repo, &job, None)?;
    announce_job(app, root, &mut job)?;
    crate::project_management::attach_jury_in(repo, &job, None)?;
    Ok(job)
}

pub fn plan(
    app: &AppHandle,
    project: &Path,
    session: Option<&str>,
    file: &Path,
    text: &str,
    inbox: &str,
    paths: &[String],
    estimate: Option<f64>,
) -> Result<(), String> {
    let registry = crate::agents::registry_dir()?;
    let tree = project.canonicalize().map_err(|e| e.to_string())?;
    let run = run_control::list_ids_in(&registry)?
        .iter()
        .filter_map(|id| run_control::load_manifest_in(&registry, id).ok())
        .filter(|r| {
            r.kind == RunKind::Agent
                && !r.state.terminal()
                && Path::new(&r.worktree_path).canonicalize().ok().as_ref() == Some(&tree)
                && session.is_some_and(|s| {
                    r.pty_session.as_deref() == Some(s) || r.zellij_session.as_deref() == Some(s)
                })
        })
        .max_by_key(|r| r.started_at)
        .ok_or("plan has no authenticated registry/worktree binding; owner review required")?;
    let repo = crate::project_management::repo_now()?;
    let t = ticket(&repo, run.ticket.as_deref().ok_or("run has no ticket")?)?;
    if t.owner
        .as_deref()
        .map(|s| s.trim_start_matches('@').to_lowercase())
        .as_deref()
        != Some(run.agent_handle.as_str())
    {
        return Err("ticket was reassigned; the previous run cannot obtain plan approval".into());
    }
    let policy = policy(&repo, &t.project)?;
    let project_owner = crate::project_management::list_projects(&repo)?
        .iter()
        .find(|p| p.key == t.project)
        .is_none_or(|p| p.owner_only);
    let reason = owner_reason(
        &policy,
        t.approval.owner_only || project_owner,
        text,
        paths,
        &tree,
        estimate,
    );
    let input=format!("Ticket scope:\n{}\nAssigned worktree: {}\nPlan:\n{}\nDeclared paths:\n{}\nSpend estimate: {:?}",ticket_snapshot(&t),tree.display(),text,paths.join("\n"),estimate);
    let mut job = new_job(
        Gate::Plan,
        &t,
        &tree,
        input,
        policy,
        Some(run.run_id),
        Some(inbox.into()),
    )?;
    job.plan_file = Some(file.to_string_lossy().into());
    job.plan_hash = Some(hash(text));
    let root = store()?;
    write_job(&root, &job)?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = run_job(Some(&app), &repo, &registry, &root, job.clone(), reason) {
            job.decision = Some(Decision::Owner);
            job.reason = e;
            job.state = "owner_required".into();
            let _ = write_job(&root, &job);
            let _ = announce_job(Some(&app), &root, &mut job);
        }
    });
    Ok(())
}

/// A restarted supervisor never reruns a missing reviewer and treats silence
/// as a vote. Deadline expiry goes through the same owner escalation path.
/// How many times an integration build may be restarted before the owner is
/// told. A verifier killed by the supervisor's own restart deserves to run
/// again; one that keeps dying is being stopped by something this arm cannot
/// see, and re-running it forever costs a full build each time.
pub(crate) const MAX_VERIFY_RESTARTS: u32 = 2;

pub fn reconcile(
    app: Option<&AppHandle>,
    repo: &Path,
    registry: &Path,
    root: &Path,
) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        if entry.path().extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Ok(mut job) =
            serde_json::from_slice::<Job>(&std::fs::read(entry.path()).unwrap_or_default())
        else {
            continue;
        };
        if active(&job.id) {
            continue;
        }
        if job.state == "decided" {
            let current = ticket(repo, &job.ticket)?;
            if scope_hash(&current) != job.ticket_scope_hash {
                job.decision = Some(Decision::Owner);
                job.reason = "ticket scope changed during interrupted approval".into();
            }
            crate::project_management::attach_jury_in(repo, &job, None)?;
            announce_job(app, root, &mut job)?;
            crate::project_management::attach_jury_in(repo, &job, None)?;
        }
        if job.state == "signed" || job.state == "owner_signed" {
            if let Err(e) =
                crate::jury_signoff::merge_and_verify(app, repo, registry, root, &mut job)
            {
                job.decision = Some(Decision::Owner);
                job.state = "owner_required".into();
                job.reason = e;
                write_job(root, &job)?;
                crate::project_management::attach_jury_in(repo, &job, Some("blocked"))?;
                announce_job(app, root, &mut job)?;
            }
        }
        if ["requested", "reviewing"].contains(&job.state.as_str())
            && run_control::now_ms() > job.deadline
        {
            // `run_pair` reviews in-process, and a job a thread is working is
            // Active and was skipped above. So a job still sitting here means
            // the supervisor that owned it is gone: an install, a crash, a
            // quit. That is not a reviewer declining to answer, and treating
            // it as one manufactures an owner decision out of an app restart.
            // The integration verifier got this right on 2026-09-08, after a
            // restart revoked a merge that had passed 974 Rust and 133 UI
            // tests; the reviewers were left with the same hole.
            //
            // Run the pair again, once, and only when nothing was heard at
            // all. A half-answered round is a real split and still goes to
            // the owner, because re-asking would put the same question to a
            // reviewer who has already answered it.
            if job.restarts == 0 && job.reviews.is_empty() {
                job.restarts += 1;
                job.deadline =
                    run_control::now_ms() + (job.policy.deadline_seconds as i64) * 1000;
                job.reason = "reviewers absent after a supervisor restart; reviewing again".into();
                write_job(root, &job)?;
                job = run_job(app, repo, registry, root, job, None)?;
            } else {
                job.decision = Some(Decision::Owner);
                job.reason = if job.reviews.is_empty() {
                    "reviewers absent after a second supervisor restart".into()
                } else {
                    "reviewer absent at registry deadline or supervisor restarted".into()
                };
                job.state = "owner_required".into();
                write_job(root, &job)?;
                crate::project_management::attach_jury_in(repo, &job, None)?;
                announce_job(app, root, &mut job)?;
            }
        }
        if job.state == "revoke_requested" {
            crate::jury_signoff::revoke(app, repo, root, &job.id)?;
        }
        if job.state == "rollback_requested" {
            crate::jury_signoff::rollback(repo, root, &mut job)?;
        }
        if job.state == "verifying" {
            if let Some(id) = job
                .signoff
                .as_ref()
                .and_then(|s| s.integration_verify_run.as_ref())
            {
                let alive = run_control::load_manifest_in(registry, id)
                    .ok()
                    .and_then(|r| r.pid.zip(r.process_birth))
                    .is_some_and(|(pid, birth)| run_control::process_birth(pid) == Some(birth));
                // ... but only so many times. An integration build that
                // dies for a reason OUTSIDE the merge keeps dying, and this
                // arm cannot tell why the process is gone. On tron on
                // 2026-09-09 the volume was full, so every re-run died in
                // cargo, and reconcile started a fresh one roughly every
                // fifteen minutes from 16:03 to 21:29: eleven builds for
                // XNAUT-75, a ticket that had merged and promoted at 17:50.
                // Each build was several gigabytes, which is a fair part of
                // why the disk reached 100% in the first place.
                if !alive && job.verify_restarts >= MAX_VERIFY_RESTARTS {
                    job.decision = Some(Decision::Owner);
                    job.state = "owner_required".into();
                    job.reason = format!(
                        "integration verifier died {MAX_VERIFY_RESTARTS} times without \
                         finishing; something outside this merge is stopping the build"
                    );
                    write_job(root, &job)?;
                    crate::project_management::attach_jury_in(repo, &job, Some("blocked"))?;
                    announce_job(app, root, &mut job)?;
                } else if !alive {
                    job.verify_restarts += 1;
                    // The supervisor's own restart killed the verifier. That
                    // is not a red build; it is no build. Run it again. On
                    // 2026-09-08 the 09:31 install did this to XNAUT-306: a
                    // merge that had passed 974 Rust and 133 UI tests in the
                    // sandbox was revoked, its rollback failed every tick,
                    // and the ticket sat blocked with a good merge on the
                    // branch. Only a build that actually fails rolls back.
                    job.reason = "integration verifier absent after restart; verifying again".into();
                    write_job(root, &job)?;
                    if let Err(e) = crate::jury_signoff::verify_integration(app, repo, registry, root, &mut job) {
                        job.reason = format!("integration re-verification could not start: {e}");
                        crate::jury_signoff::rollback(repo, root, &mut job)?;
                    }
                }
            }
        }
        if ["merging", "merge_prepared", "merged"].contains(&job.state.as_str()) {
            if let Some(signoff) = &job.signoff {
                let tree = Path::new(&job.worktree);
                let reference = format!("refs/heads/{}", job.policy.integration_branch);
                if git(
                    tree,
                    &[
                        "merge-base",
                        "--is-ancestor",
                        &signoff.merge_sha,
                        &reference,
                    ],
                )
                .is_ok()
                {
                    job.reason = "supervisor interrupted integration before verified green".into();
                    crate::jury_signoff::rollback(repo, root, &mut job)?;
                    continue;
                }
            }
            job.decision = Some(Decision::Owner);
            job.state = "owner_required".into();
            job.reason = "interrupted merge requires owner review; no automatic retry".into();
            write_job(root, &job)?;
            crate::project_management::attach_jury_in(repo, &job, Some("blocked"))?;
            announce_job(app, root, &mut job)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_decision_is_delivered_once_and_deadline_absence_escalates() {
        let (_root, control, registry, store, t, mut job) =
            crate::jury_signoff::tests::fixture("decision-recovery");
        job.gate = Gate::Plan;
        job.state = "decided".into();
        write_job(&store, &job).unwrap();
        reconcile(None, &control, &registry, &store).unwrap();
        let recovered = read_job(&store, &job.id).unwrap();
        assert_eq!(recovered.state, "settled");
        assert!(recovered.notify_id.is_some());
        let revision = ticket(&control, &t.id).unwrap().revision;
        reconcile(None, &control, &registry, &store).unwrap();
        assert_eq!(ticket(&control, &t.id).unwrap().revision, revision);
        let current = ticket(&control, &t.id).unwrap();
        let mut missing = new_job(
            Gate::Plan,
            &current,
            Path::new(&job.worktree),
            "new plan".into(),
            job.policy.clone(),
            None,
            None,
        )
        .unwrap();
        missing.state = "reviewing".into();
        missing.deadline = 0;
        write_job(&store, &missing).unwrap();
        reconcile(None, &control, &registry, &store).unwrap();
        let missing = read_job(&store, &missing.id).unwrap();
        assert_eq!(missing.decision, Some(Decision::Owner));
        assert!(missing.inbox_id.is_some());
    }
    #[test]
    fn a_restart_reviews_again_before_it_ever_asks_the_owner() {
        // The reviewers run in-process, so a job left in `reviewing` that no
        // thread is working means the supervisor died holding it. Escalating
        // that manufactures an owner decision out of an install. The
        // integration verifier was fixed on 2026-09-08; this is the same hole
        // one gate earlier.
        let (_root, control, registry, store, t, job) =
            crate::jury_signoff::tests::fixture("restart-review");
        let make = |name: &str| {
            let current = ticket(&control, &t.id).unwrap();
            let mut j = new_job(
                Gate::Plan,
                &current,
                Path::new(&job.worktree),
                name.into(),
                job.policy.clone(),
                None,
                None,
            )
            .unwrap();
            j.state = "reviewing".into();
            j.deadline = 0;
            j
        };

        // Nothing heard, first restart: reviewed again, not escalated.
        let first = make("first restart");
        assert_eq!(first.restarts, 0);
        write_job(&store, &first).unwrap();
        reconcile(None, &control, &registry, &store).unwrap();
        let after = read_job(&store, &first.id).unwrap();
        assert_eq!(after.restarts, 1, "the round was run again: {}", after.reason);
        assert!(
            !after.reason.contains("reviewer absent at registry deadline"),
            "a restart is not an absent reviewer: {}",
            after.reason
        );

        // Nothing heard again: the second restart is a real question, and it
        // says which one it is rather than blaming the reviewers.
        let mut second = make("second restart");
        second.restarts = 1;
        write_job(&store, &second).unwrap();
        reconcile(None, &control, &registry, &store).unwrap();
        let after = read_job(&store, &second.id).unwrap();
        assert_eq!(after.state, "owner_required");
        assert_eq!(after.restarts, 1, "no third review");
        assert!(
            after.reason.contains("second supervisor restart"),
            "{}",
            after.reason
        );

        // Half a round IS a real split. Re-asking would put the same question
        // to a reviewer who already answered, so it goes to the owner.
        let mut partial = make("half answered");
        partial.reviews.push(ReviewRecord {
            run_id: "r1".into(),
            runtime: "claude".into(),
            input_hash: partial.input_hash.clone(),
            finished_at: 1,
            review: None,
            error: Some("absent".into()),
        });
        write_job(&store, &partial).unwrap();
        reconcile(None, &control, &registry, &store).unwrap();
        let after = read_job(&store, &partial.id).unwrap();
        assert_eq!(after.state, "owner_required");
        assert_eq!(after.restarts, 0, "a partial round is never re-reviewed");
        assert!(
            after.reason.contains("reviewer absent at registry deadline"),
            "{}",
            after.reason
        );
    }

    #[test]
    fn interrupted_published_merge_is_compensated() {
        let (_root, control, registry, store, _, mut job) =
            crate::jury_signoff::tests::fixture("merge-recovery");
        let tree = PathBuf::from(&job.worktree);
        let before = git(&tree, &["rev-parse", "dev"]).unwrap();
        crate::jury_signoff::merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
        job.state = "merged".into();
        write_job(&store, &job).unwrap();
        reconcile(None, &control, &registry, &store).unwrap();
        let recovered = read_job(&store, &job.id).unwrap();
        assert_eq!(recovered.state, "reverted");
        assert!(git(&tree, &["diff", &before, "dev"])
            .unwrap()
            .is_empty());
    }
    #[test]
    fn native_sandbox_blocks_peer_reads_and_outside_writes() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let root = std::env::temp_dir().join(format!("jury-isolation-{}", uuid::Uuid::new_v4()));
        let own = root.join("own");
        private_dir(&own).unwrap();
        std::fs::write(root.join("peer"), "blind review").unwrap();
        let profile = sandbox_profile(&own, &[root.clone()]);
        let result = Command::new("sandbox-exec")
            .args(["-p", &profile, "/bin/cat"])
            .arg(root.join("peer"))
            .output()
            .unwrap();
        assert!(!result.status.success(), "a reviewer read its peer");
        let result = Command::new("sandbox-exec")
            .args(["-p", &profile, "/usr/bin/touch"])
            .arg(root.join("outside"))
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "a reviewer wrote outside its scratch directory"
        );
        let result = Command::new("sandbox-exec")
            .args(["-p", &profile, "/usr/bin/touch"])
            .arg(own.join("allowed"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "sandbox must permit its own runtime state: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[test]
    #[ignore = "real Codex and Gemini subscription runs on tron; explicit local proof"]
    fn jury_live_review_pair() {
        let (_root, control, registry, store, t, mut job) =
            crate::jury_signoff::tests::fixture("live-pair");
        job.gate = Gate::Plan;
        job.decision = None;
        job.reviews.clear();
        job.policy.reviewers = crate::jury::tests::live_policy().reviewers;
        job.policy.deadline_seconds = 120;
        job.deadline = run_control::now_ms() + 120_000;
        job.input=format!("Ticket {}: Replace the single line baseline in feature.txt with reviewed implementation. Assigned worktree {}.\nPlan: edit only feature.txt in this assigned isolated worktree, run an exact content assertion against feature.txt, and git diff --check. Use a temporary fixture to prove the assertion fails for the old baseline text. Live proof is reading the changed file and recording the command results in the ticket. No network publication, no shared-directory changes, no irreversible steps. Spend estimate 0.20, ceiling 5.00. Scope is exactly the ticket's single text replacement.",t.id,job.worktree);
        job.input_hash = hash(&job.input);
        let job = run_job(None, &control, &registry, &store, job, None).unwrap();
        println!(
            "JURY_LIVE_PROOF={}",
            serde_json::json!({"store":store,"job":job})
        );
        assert_eq!(job.reviews.len(), 2);
        assert_eq!(job.decision, Some(Decision::Approved), "{}", job.reason);
        for r in &job.reviews {
            assert_eq!(
                run_control::load_manifest_in(&registry, &r.run_id)
                    .unwrap()
                    .kind,
                RunKind::Review
            );
        }
        assert!(job.notify_id.is_some());
    }
}
/// One line of history for an escalation, or nothing. Reads the run registry,
/// the jury store and the tickets; writes nothing and raises nothing, because
/// a memory that cannot be built must not stop an owner hearing about a
/// decision.
fn recognised_failure(root: &Path, job: &Job) -> Option<String> {
    // The memory first (XNAUT-331): notes written as things happened, on
    // every machine that shares the vault. The registry join second, for
    // what predates the memory.
    let first_line = job.reason.lines().next().unwrap_or("").to_string();
    if let Ok(vault) = crate::memory::default_root() {
        if let Ok(all) = crate::memory::load(&vault, Some(&job.project)) {
            let hits: Vec<&crate::memory::Memory> = crate::memory::search(&all, &first_line, 6)
                .into_iter()
                .filter(|m| m.kind == "incident" || m.kind == "decision")
                .filter(|m| m.ticket != job.ticket || m.source != format!("jury:{}:decided", job.id))
                .collect();
            if let Some(newest) = hits.first() {
                let tickets: Vec<&str> = { let mut t: Vec<&str> = hits.iter().map(|m| m.ticket.as_str()).filter(|t| !t.is_empty()).collect(); t.dedup(); t.truncate(4); t };
                let closed = if !newest.fix.is_empty() { format!("; closed last time by {}", &newest.fix[..newest.fix.len().min(8)]) } else if !newest.cause.is_empty() { format!("; last time: {}", newest.cause.trim()) } else { String::new() };
                return Some(format!("Remembered {} time{} before, on {}{closed}.", hits.len(), if hits.len()==1 {""} else {"s"}, tickets.join(", ")));
            }
        }
    }
    let registry = root.parent()?;
    let mut incidents = crate::incidents::from_runs(registry).ok()?;
    incidents.extend(crate::incidents::from_jury(root));
    crate::incidents::recognised(
        &incidents,
        job.reason.lines().next().unwrap_or_default(),
        Some(&job.id),
    )
}

pub fn announce_job(app: Option<&AppHandle>, root: &Path, job: &mut Job) -> Result<(), String> {
    use std::collections::BTreeMap;
    let reviews = serde_json::to_string_pretty(&job.reviews).map_err(|e| e.to_string())?;
    if job.decision == Some(Decision::Owner) {
        // Has anybody seen this before? Sign-off escalated XNAUT-305 four
        // times on four pieces of bad evidence and each was investigated as
        // if new. Best effort and read-only: a memory that cannot be built
        // must never stop a decision reaching its owner.
        if let Some(seen) = recognised_failure(root, job) {
            if !job.reason.contains(&seen) {
                job.reason = format!("{}\n\n{seen}", job.reason);
            }
        }
        if let Some(id) = &job.inbox_id {
            crate::inbox::jury_context(id, &job.id, &job.reason, &reviews)?;
        } else {
            let item = crate::inbox::jury_post(
                app,
                "approve",
                crate::inbox::PostRequest {
                    project: job.project.clone(),
                    from: "nautbot".into(),
                    ticket: Some(job.ticket.clone()),
                    title: format!("Owner review: {} {:?}", job.ticket, job.gate),
                    body: format!("{}\n{reviews}", job.reason),
                    context: BTreeMap::from([("jury_id".into(), job.id.clone())]),
                    ..Default::default()
                },
                None,
            )?;
            job.inbox_id = Some(item.id);
        }
        if app.is_some() {
            crate::push::notify(crate::push::PushNote {
                title: format!("{} needs owner review", job.ticket),
                body: job.reason.clone(),
                kind: "approve".into(),
                inbox_id: job.inbox_id.clone(),
                project: Some(job.project.clone()),
            });
        }
    } else {
        if let Some(id) = &job.inbox_id {
            crate::inbox::jury_decide(
                app,
                id,
                if job.decision == Some(Decision::Approved) {
                    "approved"
                } else {
                    "denied"
                },
                &job.id,
                &job.reason,
                &reviews,
            )?;
        }
        if job.decision == Some(Decision::Approved) && job.notify_id.is_none() {
            let item = crate::inbox::jury_post(
                app,
                "notify",
                crate::inbox::PostRequest {
                    project: job.project.clone(),
                    from: "nautbot".into(),
                    ticket: Some(job.ticket.clone()),
                    title: format!("Jury approved: {} {:?}", job.ticket, job.gate),
                    body: format!("{}\nInput: {}\n{reviews}", job.reason, job.input_hash),
                    context: BTreeMap::from([
                        ("jury_id".into(), job.id.clone()),
                        ("revocable".into(), "true".into()),
                    ]),
                    ..Default::default()
                },
                None,
            )?;
            job.notify_id = Some(item.id);
        }
    }
    if job.state == "decided" {
        job.state = match (job.gate, job.decision) {
            (_, Some(Decision::Owner)) => "owner_required",
            (Gate::Signoff, Some(Decision::Approved)) => "signed",
            _ => "settled",
        }
        .into();
        if job.state != "owner_required" {
            crate::inbox::jury_archive_asks(app, &job.id, &job.ticket);
        }
    }
    write_job(root, job)
}
