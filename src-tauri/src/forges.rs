// Unified forge API client for Forgejo (Gitea v1), GitHub, and GitLab.
// Powers the v1.6 Tasks panel and project scaffolding (scaffold.rs).

use crate::settings::{resolve_forge_token, ForgeHost};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One issue, pull request, or merge request, normalized across forge dialects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeIssue {
    pub number: u64,
    pub title: String,
    pub body: String,
    /// "open" | "closed" | "merged"
    pub state: String,
    pub labels: Vec<String>,
    pub author: String,
    pub updated_at: String,
    pub html_url: String,
    pub is_pr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeAttachment {
    pub url: String,
    pub media_type: String,
    pub size_bytes: usize,
    #[serde(default)]
    pub text: Option<String>,
}

/// Which list the Tasks panel is asking for.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IssueKind {
    Issues,
    Prs,
}

// ─── HTTP plumbing ───────────────────────────────────────────────────────────

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("xnaut")
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))
}

fn token_for(host: &ForgeHost) -> Result<String, String> {
    resolve_forge_token(host).ok_or_else(|| {
        format!(
            "no token configured for {} host {}",
            host.kind, host.base_url
        )
    })
}

/// API base URL for the host's dialect, no trailing slash.
fn api_base(host: &ForgeHost) -> Result<String, String> {
    let raw = host.base_url.trim_end_matches('/');
    match host.kind.as_str() {
        "forgejo" => Ok(format!("{raw}/api/v1")),
        "github" => {
            // github.com is the WEB host and its API lives elsewhere, so that
            // one spelling is rewritten. Anything else written out in full is
            // taken at its word, which is what makes a GitHub Enterprise
            // install reachable, and what lets the intake tests point a
            // "github" host at a recorded-payload server on localhost. The old
            // rule ("unless it contains api.github.com, use api.github.com")
            // silently sent every Enterprise request to github.com instead.
            let authority = raw
                .split_once("://")
                .map(|(_, rest)| rest)
                .unwrap_or(raw)
                .split('/')
                .next()
                .unwrap_or_default();
            match authority {
                "" | "github.com" | "www.github.com" => Ok("https://api.github.com".to_string()),
                _ => Ok(raw.to_string()),
            }
        }
        "gitlab" => {
            let base = if raw.is_empty() {
                "https://gitlab.com"
            } else {
                raw
            };
            Ok(format!("{base}/api/v4"))
        }
        other => Err(format!("unknown forge kind: {other}")),
    }
}

/// Sends one request with dialect-appropriate auth headers; returns (status, body text).
async fn send(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    host: &ForgeHost,
    token: &str,
    body: Option<&Value>,
) -> Result<(reqwest::StatusCode, String), String> {
    let mut req = client.request(method.clone(), url);
    req = match host.kind.as_str() {
        "forgejo" => req.header("Authorization", format!("token {token}")),
        "github" => req
            .header("Authorization", format!("Bearer {token}"))
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("Accept", "application/vnd.github+json"),
        "gitlab" => req.header("PRIVATE-TOKEN", token),
        other => return Err(format!("unknown forge kind: {other}")),
    };
    if let Some(b) = body {
        req = req.json(b);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("{method} {url} failed: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("{method} {url}: failed to read body: {e}"))?;
    Ok((status, text))
}

/// Sends a request and parses the JSON response; non-2xx becomes a descriptive Err.
async fn request_json(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    host: &ForgeHost,
    token: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let (status, text) = send(client, method.clone(), url, host, token, body).await?;
    if !status.is_success() {
        let snippet: String = text.chars().take(300).collect();
        return Err(format!("{method} {url} failed: {status}: {snippet}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("{method} {url}: invalid JSON response: {e}"))
}

// ─── JSON field mapping ──────────────────────────────────────────────────────

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Maps a Forgejo/Gitea or GitHub issue object (the two dialects share field names).
/// `is_pr` comes from the presence of a non-null "pull_request" key.
fn map_github_like_issue(v: &Value) -> ForgeIssue {
    let labels = v
        .get("labels")
        .and_then(Value::as_array)
        .map(|ls| {
            ls.iter()
                .filter_map(|l| l.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    ForgeIssue {
        number: v.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: str_field(v, "title"),
        body: str_field(v, "body"),
        state: str_field(v, "state"),
        labels,
        author: v
            .get("user")
            .map(|u| str_field(u, "login"))
            .unwrap_or_default(),
        updated_at: str_field(v, "updated_at"),
        html_url: str_field(v, "html_url"),
        is_pr: v.get("pull_request").is_some_and(|p| !p.is_null()),
    }
}

/// Maps a GitLab issue or merge request object.
fn map_gitlab_issue(v: &Value, is_pr: bool) -> ForgeIssue {
    let labels = v
        .get("labels")
        .and_then(Value::as_array)
        .map(|ls| {
            ls.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    // GitLab says "opened"; normalize to the cross-forge "open".
    let state = match str_field(v, "state").as_str() {
        "opened" => "open".to_string(),
        s => s.to_string(),
    };
    ForgeIssue {
        number: v.get("iid").and_then(Value::as_u64).unwrap_or(0),
        title: str_field(v, "title"),
        body: str_field(v, "description"),
        state,
        labels,
        author: v
            .get("author")
            .map(|a| str_field(a, "username"))
            .unwrap_or_default(),
        updated_at: str_field(v, "updated_at"),
        html_url: str_field(v, "web_url"),
        is_pr,
    }
}

/// URL-encodes "{owner}/{repo}" for GitLab's /projects/:id path ('/' → %2F).
fn gitlab_project_path(owner: &str, repo: &str) -> String {
    let raw = format!("{owner}/{repo}");
    url::form_urlencoded::byte_serialize(raw.as_bytes()).collect()
}

fn expect_array(v: Value, url: &str) -> Result<Vec<Value>, String> {
    match v {
        Value::Array(a) => Ok(a),
        _ => Err(format!("{url}: expected a JSON array response")),
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Reduce whatever the user typed to a bare repo name. Accepts a full clone URL
/// (`http://host/owner/repo.git`), an `owner/repo` pair, or just `repo` — and
/// strips a trailing `.git`. Prevents the API path from becoming
/// `repos/{owner}/http://.../repo.git/issues`.
fn normalize_repo(repo: &str) -> String {
    let r = repo.trim().trim_end_matches('/');
    // Drop any scheme + host (everything up to and including the last '/').
    let last = r.rsplit('/').next().unwrap_or(r);
    last.trim_end_matches(".git").to_string()
}

/// List open issues or PRs for the host's default owner.
pub async fn list_issues(
    host: &ForgeHost,
    repo: &str,
    kind: IssueKind,
) -> Result<Vec<ForgeIssue>, String> {
    list_issues_for(host, &host.owner, repo, kind).await
}

/// List open issues or PRs for an EXPLICIT owner/repo on the given host.
///
/// `host.owner` is the owner's default org for the Tasks panel, and issue
/// intake (XNAUT-382) reads a different repo per project: the owner comes from
/// that project's `forge_remote`, which may be any org the token can see.
pub async fn list_issues_for(
    host: &ForgeHost,
    owner: &str,
    repo: &str,
    kind: IssueKind,
) -> Result<Vec<ForgeIssue>, String> {
    let client = http_client()?;
    let token = token_for(host)?;
    let base = api_base(host)?;
    let repo = &normalize_repo(repo);
    match host.kind.as_str() {
        "forgejo" => {
            let issue_type = match kind {
                IssueKind::Issues => "issues",
                IssueKind::Prs => "pulls",
            };
            let url =
                format!("{base}/repos/{owner}/{repo}/issues?state=open&type={issue_type}&limit=50");
            let v = request_json(&client, reqwest::Method::GET, &url, host, &token, None).await?;
            Ok(expect_array(v, &url)?
                .iter()
                .map(|raw| {
                    let mut item = map_github_like_issue(raw);
                    if kind == IssueKind::Prs {
                        item.is_pr = true;
                    }
                    item
                })
                .collect())
        }
        "github" => match kind {
            IssueKind::Issues => {
                // GitHub mixes PRs into /issues — drop entries carrying a "pull_request" key.
                let url = format!("{base}/repos/{owner}/{repo}/issues?state=open&per_page=50");
                let v =
                    request_json(&client, reqwest::Method::GET, &url, host, &token, None).await?;
                Ok(expect_array(v, &url)?
                    .iter()
                    .map(map_github_like_issue)
                    .filter(|i| !i.is_pr)
                    .collect())
            }
            IssueKind::Prs => {
                let url = format!("{base}/repos/{owner}/{repo}/pulls?state=open&per_page=50");
                let v =
                    request_json(&client, reqwest::Method::GET, &url, host, &token, None).await?;
                Ok(expect_array(v, &url)?
                    .iter()
                    .map(|p| {
                        let mut i = map_github_like_issue(p);
                        i.is_pr = true;
                        i
                    })
                    .collect())
            }
        },
        "gitlab" => {
            let project = gitlab_project_path(owner, repo);
            let (resource, is_pr) = match kind {
                IssueKind::Issues => ("issues", false),
                IssueKind::Prs => ("merge_requests", true),
            };
            let url = format!("{base}/projects/{project}/{resource}?state=opened&per_page=50");
            let v = request_json(&client, reqwest::Method::GET, &url, host, &token, None).await?;
            Ok(expect_array(v, &url)?
                .iter()
                .map(|i| map_gitlab_issue(i, is_pr))
                .collect())
        }
        other => Err(format!("unknown forge kind: {other}")),
    }
}

/// Fetch one issue/PR with full body.
pub async fn get_issue(host: &ForgeHost, repo: &str, number: u64) -> Result<ForgeIssue, String> {
    get_issue_for(host, &host.owner, repo, number).await
}

/// Fetch one issue/PR under an EXPLICIT owner. See `list_issues_for`.
pub async fn get_issue_for(
    host: &ForgeHost,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<ForgeIssue, String> {
    let client = http_client()?;
    let token = token_for(host)?;
    let base = api_base(host)?;
    let repo = &normalize_repo(repo);
    match host.kind.as_str() {
        // The Gitea/GitHub issues endpoint serves PRs under the same numbers.
        "forgejo" | "github" => {
            let url = format!("{base}/repos/{owner}/{repo}/issues/{number}");
            let v = request_json(&client, reqwest::Method::GET, &url, host, &token, None).await?;
            Ok(map_github_like_issue(&v))
        }
        "gitlab" => {
            // Issues and MRs have separate iid namespaces — try issue first, then MR on 404.
            let project = gitlab_project_path(owner, repo);
            let issue_url = format!("{base}/projects/{project}/issues/{number}");
            let (status, text) = send(
                &client,
                reqwest::Method::GET,
                &issue_url,
                host,
                &token,
                None,
            )
            .await?;
            if status.is_success() {
                let v: Value = serde_json::from_str(&text)
                    .map_err(|e| format!("GET {issue_url}: invalid JSON response: {e}"))?;
                return Ok(map_gitlab_issue(&v, false));
            }
            if status != reqwest::StatusCode::NOT_FOUND {
                let snippet: String = text.chars().take(300).collect();
                return Err(format!("GET {issue_url} failed: {status}: {snippet}"));
            }
            let mr_url = format!("{base}/projects/{project}/merge_requests/{number}");
            let v =
                request_json(&client, reqwest::Method::GET, &mr_url, host, &token, None).await?;
            Ok(map_gitlab_issue(&v, true))
        }
        other => Err(format!("unknown forge kind: {other}")),
    }
}

/// Recover a published review after an interrupted response without posting twice.
pub async fn ensure_repository_comment(
    host: &ForgeHost,
    owner: &str,
    repo: &str,
    number: u64,
    marker: &str,
    body: &str,
) -> Result<String, String> {
    if !["forgejo", "github"].contains(&host.kind.as_str()) {
        return Err("Repository review supports Forgejo and GitHub".into());
    }
    let client = http_client()?;
    let token = token_for(host)?;
    let base = api_base(host)?;
    for page in 1..=100 {
        let rows=request_json(&client,reqwest::Method::GET,&format!("{base}/repos/{owner}/{repo}/issues/{number}/comments?limit=100&per_page=100&page={page}"),host,&token,None).await?;
        let rows = rows.as_array().ok_or("Invalid comment listing")?;
        for row in rows {
            if row["body"].as_str().is_some_and(|s| s.starts_with(marker)) {
                return row["html_url"]
                    .as_str()
                    .map(String::from)
                    .ok_or("Published comment has no URL".into());
            }
        }
        if rows.is_empty() {
            return add_issue_comment_for(host, owner, repo, number, body).await;
        }
    }
    Err("Review comment listing exceeds recovery budget".into())
}

pub async fn repository_pr(
    host: &ForgeHost,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<Value, String> {
    if !["forgejo", "github"].contains(&host.kind.as_str()) {
        return Err("Automatic merging supports Forgejo and GitHub".into());
    }
    request_json(
        &http_client()?,
        reqwest::Method::GET,
        &format!("{}/repos/{owner}/{repo}/pulls/{number}", api_base(host)?),
        host,
        &token_for(host)?,
        None,
    )
    .await
}
fn validate_merge_pr(
    pr: &Value,
    head: &str,
    base: &str,
    branch: &str,
    target: &str,
) -> Result<(), String> {
    if pr["head"]["sha"] != head
        || pr["base"]["sha"] != base
        || pr["head"]["ref"] != branch
        || pr["base"]["ref"] != target
    {
        return Err("PR identity or tested commits changed; review again".into());
    }
    if pr["state"] != "open"
        || pr["merged"] == true
        || pr["draft"] != false
        || pr["mergeable"] != true
    {
        return Err("PR is closed, draft, conflicting, or mergeability is unknown".into());
    }
    // Only the task branch in this same repository can be authorized by its owner.
    if pr["head"]["repo"]["id"].as_u64().is_none()
        || pr["head"]["repo"]["id"] != pr["base"]["repo"]["id"]
    {
        return Err("Cross-repository PR requires owner review".into());
    }
    Ok(())
}
fn protected_against_stale_base(kind: &str, p: &Value) -> bool {
    match kind {
        "github" => {
            p["required_status_checks"]["strict"] == true && p["enforce_admins"]["enabled"] == true
        }
        "forgejo" => p["block_on_outdated_branch"] == true && p["apply_to_admins"] == true,
        _ => false,
    }
}
/// Preflight requires server enforcement because merge APIs compare the head SHA,
/// but do not atomically compare the tested target SHA. Never bypass protection.
pub async fn repository_merge_preflight(
    host: &ForgeHost,
    owner: &str,
    repo: &str,
    number: u64,
    head: &str,
    base: &str,
    branch: &str,
    target: &str,
) -> Result<(), String> {
    let pr = repository_pr(host, owner, repo, number).await?;
    validate_merge_pr(&pr, head, base, branch, target)?;
    let client = http_client()?;
    let token = token_for(host)?;
    let api = api_base(host)?;
    let target_encoded: String = url::form_urlencoded::byte_serialize(target.as_bytes()).collect();
    let protection_url = if host.kind == "github" {
        format!("{api}/repos/{owner}/{repo}/branches/{target_encoded}/protection")
    } else {
        format!("{api}/repos/{owner}/{repo}/branch_protections/{target_encoded}")
    };
    let protection = request_json(
        &client,
        reqwest::Method::GET,
        &protection_url,
        host,
        &token,
        None,
    )
    .await
    .map_err(|_| "Cannot verify target branch protection; owner merge required")?;
    if !protected_against_stale_base(&host.kind, &protection) {
        return Err(
            "Automatic merge requires up-to-date branch protection enforced for administrators too"
                .into(),
        );
    }
    let status = request_json(
        &client,
        reqwest::Method::GET,
        &format!("{api}/repos/{owner}/{repo}/commits/{head}/status"),
        host,
        &token,
        None,
    )
    .await?;
    let count = status["total_count"]
        .as_u64()
        .ok_or("Missing CI status count")?;
    if status["sha"] != head || (count > 0 && status["state"] != "success") {
        return Err("CI status is failing, pending, or identifies a different commit".into());
    }
    if host.kind == "github" {
        let checks = request_json(
            &client,
            reqwest::Method::GET,
            &format!("{api}/repos/{owner}/{repo}/commits/{head}/check-runs?per_page=100"),
            host,
            &token,
            None,
        )
        .await?;
        let rows = checks["check_runs"]
            .as_array()
            .ok_or("Missing CI check runs")?;
        if checks["total_count"].as_u64() != Some(rows.len() as u64)
            || rows.iter().any(|c| {
                c["head_sha"] != head
                    || c["status"] != "completed"
                    || !["success", "neutral", "skipped"]
                        .contains(&c["conclusion"].as_str().unwrap_or(""))
            })
        {
            return Err("CI checks are incomplete or unsuccessful".into());
        }
    }
    Ok(())
}
/// One immediate head-CAS merge request. No force, queued merge, or branch deletion.
pub async fn repository_merge(
    host: &ForgeHost,
    owner: &str,
    repo: &str,
    number: u64,
    head: &str,
) -> Result<Value, String> {
    let (method, body) = if host.kind == "github" {
        (
            reqwest::Method::PUT,
            json!({"sha":head,"merge_method":"merge"}),
        )
    } else if host.kind == "forgejo" {
        (
            reqwest::Method::POST,
            json!({"Do":"merge","head_commit_id":head,"force_merge":false,"merge_when_checks_succeed":false,"delete_branch_after_merge":false}),
        )
    } else {
        return Err("Unsupported merge forge".into());
    };
    let (status, _) = send(
        &http_client()?,
        method,
        &format!(
            "{}/repos/{owner}/{repo}/pulls/{number}/merge",
            api_base(host)?
        ),
        host,
        &token_for(host)?,
        Some(&body),
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "Forge refused merge (HTTP {}); inspect the PR before retrying",
            status.as_u16()
        ));
    }
    let result = repository_pr(host, owner, repo, number).await?;
    if result["merged"] != true || result["head"]["sha"] != head {
        return Err("Merge response is unconfirmed; inspect the PR before retrying".into());
    }
    Ok(result)
}

/// Append a comment to an issue without modifying the reporter's original body.
pub async fn add_issue_comment(
    host: &ForgeHost,
    repo: &str,
    number: u64,
    body: &str,
) -> Result<String, String> {
    add_issue_comment_for(host, &host.owner, repo, number, body).await
}

/// Comment on an issue under an EXPLICIT owner. See `list_issues_for`.
pub async fn add_issue_comment_for(
    host: &ForgeHost,
    owner: &str,
    repo: &str,
    number: u64,
    body: &str,
) -> Result<String, String> {
    if body.trim().is_empty() {
        return Err("issue comment body is required".into());
    }
    let client = http_client()?;
    let token = token_for(host)?;
    let base = api_base(host)?;
    let repo = normalize_repo(repo);
    let (url, payload, response_url_field) = match host.kind.as_str() {
        "forgejo" | "github" => (
            format!("{base}/repos/{owner}/{repo}/issues/{number}/comments"),
            json!({ "body": body }),
            "html_url",
        ),
        "gitlab" => {
            let project = gitlab_project_path(owner, &repo);
            (
                format!("{base}/projects/{project}/issues/{number}/notes"),
                json!({ "body": body }),
                "web_url",
            )
        }
        other => return Err(format!("unknown forge kind: {other}")),
    };
    let value = request_json(
        &client,
        reqwest::Method::POST,
        &url,
        host,
        &token,
        Some(&payload),
    )
    .await?;
    Ok(value
        .get(response_url_field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

/// Replace an issue's labels with exactly `labels`.
///
/// Issue intake (XNAUT-382) mirrors a ticket's status back onto the issue as an
/// `xnaut:<status>` label, so the person who reported it can see where it got to
/// without an xNAUT login. The caller computes the whole desired set. This is a
/// replace, not an add, because the point is that the OLD `xnaut:` label goes.
///
/// The dialects disagree about what a label is, and that difference is the
/// whole of this function:
///
///   - GitHub takes names and creates any it has not seen.
///   - Forgejo/Gitea takes numeric ids and creates nothing. A name it does not
///     know is silently dropped, so each one is resolved against the repo's
///     label list and created when missing. Without this the mirror would
///     succeed and set no label at all, which is the silent-no-op shape this
///     codebase keeps being bitten by.
///   - GitLab takes a comma-joined string on the issue itself, not a subresource.
pub async fn set_issue_labels(
    host: &ForgeHost,
    owner: &str,
    repo: &str,
    number: u64,
    labels: &[String],
) -> Result<(), String> {
    let client = http_client()?;
    let token = token_for(host)?;
    let base = api_base(host)?;
    let repo = normalize_repo(repo);
    match host.kind.as_str() {
        "github" => {
            let url = format!("{base}/repos/{owner}/{repo}/issues/{number}/labels");
            let payload = json!({ "labels": labels });
            request_json(
                &client,
                reqwest::Method::PUT,
                &url,
                host,
                &token,
                Some(&payload),
            )
            .await?;
            Ok(())
        }
        "forgejo" => {
            let mut ids = Vec::new();
            for name in labels {
                ids.push(ensure_label_id(&client, host, &token, &base, owner, &repo, name).await?);
            }
            let url = format!("{base}/repos/{owner}/{repo}/issues/{number}/labels");
            let payload = json!({ "labels": ids });
            request_json(
                &client,
                reqwest::Method::PUT,
                &url,
                host,
                &token,
                Some(&payload),
            )
            .await?;
            Ok(())
        }
        "gitlab" => {
            let project = gitlab_project_path(owner, &repo);
            let url = format!("{base}/projects/{project}/issues/{number}");
            let payload = json!({ "labels": labels.join(",") });
            request_json(
                &client,
                reqwest::Method::PUT,
                &url,
                host,
                &token,
                Some(&payload),
            )
            .await?;
            Ok(())
        }
        other => Err(format!("unknown forge kind: {other}")),
    }
}

/// The Forgejo/Gitea id for a label name, creating the label if the repo has
/// none by that name. Colour is the brand yellow so an `xnaut:` label is
/// recognisable at a glance in a repo full of other people's labels.
async fn ensure_label_id(
    client: &reqwest::Client,
    host: &ForgeHost,
    token: &str,
    base: &str,
    owner: &str,
    repo: &str,
    name: &str,
) -> Result<u64, String> {
    let list_url = format!("{base}/repos/{owner}/{repo}/labels?limit=200");
    let existing = request_json(client, reqwest::Method::GET, &list_url, host, token, None).await?;
    if let Some(id) = expect_array(existing, &list_url)?.iter().find_map(|label| {
        (label.get("name").and_then(Value::as_str) == Some(name))
            .then(|| label.get("id").and_then(Value::as_u64))
            .flatten()
    }) {
        return Ok(id);
    }
    let create_url = format!("{base}/repos/{owner}/{repo}/labels");
    let payload = json!({ "name": name, "color": "#f5b840", "description": "Mirrored from xNAUT" });
    let created = request_json(
        client,
        reqwest::Method::POST,
        &create_url,
        host,
        token,
        Some(&payload),
    )
    .await?;
    created
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("POST {create_url}: the created label carried no id"))
}

/// A git remote split into the three things an API path needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRemote {
    /// Host and port as written, e.g. "github.com" or "cosmos.tail138398.ts.net:3000".
    pub host: String,
    pub owner: String,
    pub repo: String,
}

/// Split a clone URL into host, owner and repo.
///
/// Accepts the three forms a project's `forge_remote` is ever written in: an
/// http(s) URL, an `ssh://git@host/owner/repo` URL, and the scp-ish
/// `git@host:owner/repo.git`. Anything else, such as a bare name, a path, or a
/// URL with no owner segment, is None rather than a guess, because the guess
/// would be an API call against the wrong repository.
pub fn parse_remote(remote: &str) -> Option<ParsedRemote> {
    let raw = remote.trim().trim_end_matches('/');
    if raw.is_empty() {
        return None;
    }
    let (host, path) = if let Some(rest) = raw.strip_prefix("git@") {
        // git@host:owner/repo.git; the colon is a separator, not a port.
        let (host, path) = rest.split_once(':')?;
        (host.to_string(), path.to_string())
    } else {
        let after_scheme = raw.split_once("://").map(|(_, rest)| rest)?;
        let (authority, path) = after_scheme.split_once('/')?;
        // ssh://git@host/owner/repo; drop any userinfo.
        let host = authority.rsplit('@').next().unwrap_or(authority);
        (host.to_string(), path.to_string())
    };
    let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let repo = segments.pop()?.trim_end_matches(".git");
    let owner = segments.pop()?;
    if host.is_empty() || owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(ParsedRemote {
        host,
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

/// The configured host that serves `remote`, matched on hostname.
///
/// Matching on the HOST and not on `kind` is what makes a second Forgejo or a
/// GitHub Enterprise work: two hosts can share a dialect, and only the one the
/// remote actually points at holds a token that will authenticate.
pub fn host_for_remote<'a>(hosts: &'a [ForgeHost], remote: &str) -> Option<(&'a ForgeHost, ParsedRemote)> {
    let parsed = parse_remote(remote)?;
    let bare = parsed.host.split(':').next().unwrap_or(&parsed.host);
    let remote_url = url::Url::parse(remote).ok().filter(|u| matches!(u.scheme(), "http" | "https"));
    let matches: Vec<_> = hosts.iter().filter(|host| {
        let configured = host
            .base_url
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(&host.base_url);
        let configured = configured.split('/').next().unwrap_or(configured);
        let configured_bare = configured.split(':').next().unwrap_or(configured);
        let same_service = configured_bare == bare && remote_url.as_ref().is_none_or(|remote_url| {
            url::Url::parse(&host.base_url).ok().is_some_and(|configured_url| {
                configured_url.scheme() == remote_url.scheme() && configured_url.port_or_known_default() == remote_url.port_or_known_default()
            })
        });
        same_service
            // api.github.com is configured for remotes written github.com.
            || (host.kind == "github" && bare == "github.com" && matches!(configured_bare, "api.github.com" | "github.com" | "www.github.com"))
    }).collect();
    // Two services on the same SSH hostname cannot be disambiguated by the
    // hostname alone. Require the project's explicit HTTP(S) endpoint.
    if matches.len() != 1 { return None; }
    Some((matches[0], parsed))
}

/// Read small, same-origin issue attachments. Cross-origin URLs and oversized
/// payloads are ignored so ticket text cannot turn triage into an SSRF client.
pub async fn load_issue_attachments(
    host: &ForgeHost,
    issue_body: &str,
) -> Result<Vec<ForgeAttachment>, String> {
    const MAX_ATTACHMENTS: usize = 4;
    const MAX_ATTACHMENT_BYTES: usize = 2 * 1024 * 1024;
    let base = url::Url::parse(&host.base_url)
        .map_err(|error| format!("invalid forge base URL: {error}"))?;
    let pattern = regex::Regex::new(r#"https?://[^\s)\]>\"']+"#)
        .map_err(|error| format!("attachment URL pattern failed: {error}"))?;
    let mut urls = Vec::new();
    for found in pattern.find_iter(issue_body) {
        let Ok(url) = url::Url::parse(found.as_str().trim_end_matches(['.', ','])) else {
            continue;
        };
        if url.scheme() != base.scheme()
            || url.host_str() != base.host_str()
            || url.port_or_known_default() != base.port_or_known_default()
            || urls.iter().any(|existing: &url::Url| existing == &url)
        {
            continue;
        }
        urls.push(url);
        if urls.len() == MAX_ATTACHMENTS {
            break;
        }
    }
    let client = http_client()?;
    let token = token_for(host)?;
    let mut result = Vec::new();
    for url in urls {
        let mut request = client.get(url.clone());
        request = match host.kind.as_str() {
            "forgejo" => request.header("Authorization", format!("token {token}")),
            "github" => request.header("Authorization", format!("Bearer {token}")),
            "gitlab" => request.header("PRIVATE-TOKEN", &token),
            _ => continue,
        };
        let response = request
            .send()
            .await
            .map_err(|error| format!("attachment fetch failed: {error}"))?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > MAX_ATTACHMENT_BYTES as u64)
        {
            continue;
        }
        let media_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("attachment body failed: {error}"))?;
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            continue;
        }
        let text =
            (media_type.starts_with("text/") || media_type == "application/json").then(|| {
                String::from_utf8_lossy(&bytes)
                    .chars()
                    .take(20_000)
                    .collect()
            });
        result.push(ForgeAttachment {
            url: url.to_string(),
            media_type,
            size_bytes: bytes.len(),
            text,
        });
    }
    Ok(result)
}

/// Create a repo under host.owner; returns the clone URL (http).
pub async fn create_repo(
    host: &ForgeHost,
    name: &str,
    private: bool,
    description: &str,
) -> Result<String, String> {
    create_repo_for_owner(host, name, private, description, false).await
}

/// Creates a repository for either the authenticated user or the configured
/// organization. Existing callers retain the organization-first fallback;
/// setup flows can explicitly select a personal repository.
pub async fn create_repo_for_owner(
    host: &ForgeHost,
    name: &str,
    private: bool,
    description: &str,
    personal_owner: bool,
) -> Result<String, String> {
    let client = http_client()?;
    let token = token_for(host)?;
    let base = api_base(host)?;
    let owner = &host.owner;
    match host.kind.as_str() {
        "forgejo" | "github" => {
            let body = json!({
                "name": name,
                "private": private,
                "description": description,
                "auto_init": false,
            });
            if personal_owner {
                let user_url = format!("{base}/user/repos");
                let v = request_json(
                    &client,
                    reqwest::Method::POST,
                    &user_url,
                    host,
                    &token,
                    Some(&body),
                )
                .await?;
                let clone_url = str_field(&v, "clone_url");
                if clone_url.is_empty() {
                    return Err("repo created but response had no clone_url".to_string());
                }
                return Ok(clone_url);
            }
            // Try the org endpoint; a 404 means owner is a user account — fall back.
            let org_url = format!("{base}/orgs/{owner}/repos");
            let (status, text) = send(
                &client,
                reqwest::Method::POST,
                &org_url,
                host,
                &token,
                Some(&body),
            )
            .await?;
            let v: Value = if status.is_success() {
                serde_json::from_str(&text)
                    .map_err(|e| format!("POST {org_url}: invalid JSON response: {e}"))?
            } else if status == reqwest::StatusCode::NOT_FOUND {
                let user_url = format!("{base}/user/repos");
                request_json(
                    &client,
                    reqwest::Method::POST,
                    &user_url,
                    host,
                    &token,
                    Some(&body),
                )
                .await?
            } else {
                let snippet: String = text.chars().take(300).collect();
                return Err(format!("POST {org_url} failed: {status}: {snippet}"));
            };
            let clone_url = str_field(&v, "clone_url");
            if clone_url.is_empty() {
                return Err("repo created but response had no clone_url".to_string());
            }
            Ok(clone_url)
        }
        "gitlab" => {
            let body = json!({
                "name": name,
                "visibility": if private { "private" } else { "public" },
                "description": description,
            });
            let url = format!("{base}/projects");
            let v = request_json(
                &client,
                reqwest::Method::POST,
                &url,
                host,
                &token,
                Some(&body),
            )
            .await?;
            let clone_url = str_field(&v, "http_url_to_repo");
            if clone_url.is_empty() {
                return Err("project created but response had no http_url_to_repo".to_string());
            }
            Ok(clone_url)
        }
        other => Err(format!("unknown forge kind: {other}")),
    }
}

/// Recover an existing review after a lost POST response or app restart.
pub async fn ensure_pr(host: &ForgeHost, repo: &str, head: &str, base: &str, title: &str, body: &str) -> Result<String, String> {
    if !matches!(host.kind.as_str(), "github" | "forgejo") {
        return Err("Repository runs currently support Forgejo and GitHub PRs.".into());
    }
    let client = http_client()?;
    let token = token_for(host)?;
    let api = api_base(host)?;
    for page in 1..=100 {
        let mut url = url::Url::parse(&format!("{api}/repos/{}/{repo}/pulls", host.owner)).map_err(|e| e.to_string())?;
        url.query_pairs_mut().append_pair("state", "all").append_pair("limit", "50").append_pair("per_page", "50").append_pair("page", &page.to_string());
        if host.kind == "github" { url.query_pairs_mut().append_pair("head", &format!("{}:{head}", host.owner)).append_pair("base", base); }
        let rows = request_json(&client, reqwest::Method::GET, url.as_str(), host, &token, None).await?;
        let rows = rows.as_array().ok_or("Invalid pull request list")?;
        for row in rows {
            if row.pointer("/head/ref").and_then(Value::as_str) == Some(head)
                && row.pointer("/base/ref").and_then(Value::as_str) == Some(base)
            {
                let link = str_field(row, "html_url");
                if !link.is_empty() { return Ok(link); }
            }
        }
        if rows.len() < 50 { return create_pr(host, repo, head, base, title, body).await; }
    }
    Err("Could not finish checking existing PRs; creation deferred to avoid a duplicate.".into())
}

/// The forge is authoritative for its SSH clone endpoint, including custom
/// SSH ports. Project setup remains the source of truth for owner/repository.
pub async fn worker_clone_url(host: &ForgeHost, owner: &str, repo: &str) -> Result<String, String> {
    if !matches!(host.kind.as_str(), "github" | "forgejo") {
        return Err("Automatic worker access supports Forgejo and GitHub.".into());
    }
    let client = http_client()?;
    let value = request_json(&client, reqwest::Method::GET,
        &format!("{}/repos/{owner}/{repo}", api_base(host)?), host, &token_for(host)?, None).await
        .map_err(|_| "Could not read the configured repository. Check its forge connection in Settings → Forges.".to_string())?;
    let remote = crate::repository_transfer::validate_remote(&str_field(&value, "ssh_url"))?;
    let parsed = parse_remote(&remote).ok_or("The forge did not return an SSH clone URL.")?;
    if parsed.owner != owner || parsed.repo != repo || !(remote.starts_with("ssh://") || !remote.contains("://")) {
        return Err("The forge returned a different repository; worker setup refused.".into());
    }
    Ok(remote)
}

/// OpenSSH public keys may carry a comment, including the one attached by
/// our own worker bootstrap. Key identity is the algorithm and decoded blob.
fn canonical_worker_key(public_key: &str) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let invalid = || "The worker returned an invalid public key.".to_string();
    let key = public_key.trim();
    if key.len() > 4096 || key.contains(['\r', '\n']) { return Err(invalid()); }
    let mut parts = key.split_whitespace();
    if parts.next() != Some("ssh-ed25519") { return Err(invalid()); }
    let encoded = parts.next().ok_or_else(invalid)?;
    let blob = STANDARD.decode(encoded).map_err(|_| invalid())?;
    // SSH wire format: length + algorithm, length + 32-byte Ed25519 key.
    if blob.len() != 51 || !blob.starts_with(b"\x00\x00\x00\x0bssh-ed25519\x00\x00\x00\x20") {
        return Err(invalid());
    }
    Ok(format!("ssh-ed25519 {}", STANDARD.encode(blob)))
}

/// Idempotent per-repository worker access. Only the PUBLIC key crosses this
/// API. Never copy a desktop private key or broad forge token into the worker.
pub async fn ensure_worker_key(host: &ForgeHost, owner: &str, repo: &str, public_key: &str) -> Result<(), String> {
    if !matches!(host.kind.as_str(), "github" | "forgejo") {
        return Err("Automatic worker access supports Forgejo and GitHub.".into());
    }
    let public_key = canonical_worker_key(public_key)?;
    let client = http_client()?;
    let token = token_for(host)?;
    let endpoint = format!("{}/repos/{owner}/{repo}/keys", api_base(host)?);
    // Check again after a racing POST (two tasks on the same fresh worker).
    for attempt in 0..2 {
        let mut exhausted = false;
        for page in 1..=100 {
            let value = request_json(&client, reqwest::Method::GET,
                &format!("{endpoint}?limit=50&per_page=50&page={page}"), host, &token, None).await
                .map_err(|_| "Worker repository access could not be configured. The forge connection needs permission to manage this repository's deploy keys.".to_string())?;
            let rows = value.as_array().ok_or("Invalid deploy key list from forge")?;
            for row in rows {
                if canonical_worker_key(&str_field(row, "key")).ok().as_deref() == Some(public_key.as_str()) {
                    return if row["read_only"].as_bool() == Some(false) { Ok(()) }
                    else { Err("This worker's repository deploy key is read-only. Enable write access in the repository's deploy-key settings.".into()) };
                }
            }
            if rows.len() < 50 { exhausted = true; break; }
        }
        if !exhausted { return Err("Could not finish checking repository deploy keys; setup deferred.".into()); }
        if attempt == 0 {
            use sha2::{Digest, Sha256};
            let fingerprint = format!("{:x}", Sha256::digest(public_key.as_bytes()));
            let body = json!({"title": format!("xNAUT worker {}", &fingerprint[..16]), "key": public_key, "read_only": false});
            let (status, _) = send(&client, reqwest::Method::POST, &endpoint, host, &token, Some(&body)).await
                .map_err(|_| "Could not register worker repository access; retry setup.".to_string())?;
            if status.is_success() { return Ok(()); }
            if !matches!(status.as_u16(), 409 | 422) {
                return Err("The forge refused worker access. Its connection needs permission to add a write-enabled deploy key to this repository.".into());
            }
        }
    }
    Err("Worker repository key registration was not confirmed; no task was launched.".into())
}

/// Open a PR (merge request on GitLab); returns its html_url.
pub async fn create_pr(
    host: &ForgeHost,
    repo: &str,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
) -> Result<String, String> {
    let client = http_client()?;
    let token = token_for(host)?;
    let api = api_base(host)?;
    let owner = &host.owner;
    match host.kind.as_str() {
        "forgejo" | "github" => {
            let payload = json!({
                "head": head,
                "base": base,
                "title": title,
                "body": body,
            });
            let url = format!("{api}/repos/{owner}/{repo}/pulls");
            let v = request_json(
                &client,
                reqwest::Method::POST,
                &url,
                host,
                &token,
                Some(&payload),
            )
            .await?;
            let html_url = str_field(&v, "html_url");
            if html_url.is_empty() {
                return Err("PR created but response had no html_url".to_string());
            }
            Ok(html_url)
        }
        "gitlab" => {
            let payload = json!({
                "source_branch": head,
                "target_branch": base,
                "title": title,
                "description": body,
            });
            let project = gitlab_project_path(owner, repo);
            let url = format!("{api}/projects/{project}/merge_requests");
            let v = request_json(
                &client,
                reqwest::Method::POST,
                &url,
                host,
                &token,
                Some(&payload),
            )
            .await?;
            let web_url = str_field(&v, "web_url");
            if web_url.is_empty() {
                return Err("MR created but response had no web_url".to_string());
            }
            Ok(web_url)
        }
        other => Err(format!("unknown forge kind: {other}")),
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

async fn host_at(
    state: &tauri::State<'_, crate::state::AppState>,
    forge_index: usize,
) -> Result<ForgeHost, String> {
    state
        .settings
        .lock()
        .await
        .forges
        .get(forge_index)
        .cloned()
        .ok_or_else(|| "forge index out of range".to_string())
}

#[tauri::command]
pub async fn forge_list_issues(
    state: tauri::State<'_, crate::state::AppState>,
    forge_index: usize,
    repo: String,
    kind: IssueKind,
) -> Result<Vec<ForgeIssue>, String> {
    let host = host_at(&state, forge_index).await?;
    list_issues(&host, &repo, kind).await
}

#[tauri::command]
pub async fn forge_get_issue(
    state: tauri::State<'_, crate::state::AppState>,
    forge_index: usize,
    repo: String,
    number: u64,
) -> Result<ForgeIssue, String> {
    let host = host_at(&state, forge_index).await?;
    get_issue(&host, &repo, number).await
}

#[tauri::command]
pub async fn forge_add_issue_comment(
    state: tauri::State<'_, crate::state::AppState>,
    forge_index: usize,
    repo: String,
    number: u64,
    body: String,
) -> Result<String, String> {
    let host = host_at(&state, forge_index).await?;
    add_issue_comment(&host, &repo, number, &body).await
}

#[tauri::command]
pub async fn forge_create_pr(
    state: tauri::State<'_, crate::state::AppState>,
    forge_index: usize,
    repo: String,
    head: String,
    base: String,
    title: String,
    body: String,
) -> Result<String, String> {
    let host = host_at(&state, forge_index).await?;
    create_pr(&host, &repo, &head, &base, &title, &body).await
}

/// Configured forge hosts for the frontend picker — tokens are never included.
#[tauri::command]
pub async fn forge_hosts(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<Value>, String> {
    let settings = state.settings.lock().await;
    Ok(settings
        .forges
        .iter()
        .map(|f| {
            json!({
                "kind": f.kind,
                "base_url": f.base_url,
                "owner": f.owner,
            })
        })
        .collect())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_repo_reduces_to_bare_name() {
        assert_eq!(normalize_repo("JobHunter"), "JobHunter");
        assert_eq!(normalize_repo("48Nauts/JobHunter"), "JobHunter");
        assert_eq!(normalize_repo("JobHunter.git"), "JobHunter");
        assert_eq!(
            normalize_repo("http://cosmos.tail138398.ts.net:3000/48Nauts/JobHunter.git"),
            "JobHunter"
        );
        assert_eq!(normalize_repo("  JobHunter/  "), "JobHunter");
    }

    fn host(kind: &str, base_url: &str) -> ForgeHost {
        ForgeHost {
            kind: kind.into(),
            base_url: base_url.into(),
            owner: "48Nauts".into(),
            token: Some("t".into()),
        }
    }

    #[test]
    fn parse_remote_reads_the_three_forms_a_remote_is_written_in() {
        let p = |url: &str| parse_remote(url).map(|r| (r.host, r.owner, r.repo));
        assert_eq!(
            p("https://github.com/48Nauts/xnaut.git"),
            Some(("github.com".into(), "48Nauts".into(), "xnaut".into()))
        );
        assert_eq!(
            p("http://cosmos.tail138398.ts.net:3000/48Nauts/xnaut"),
            Some((
                "cosmos.tail138398.ts.net:3000".into(),
                "48Nauts".into(),
                "xnaut".into()
            ))
        );
        assert_eq!(
            p("git@github.com:48Nauts/xnaut.git"),
            Some(("github.com".into(), "48Nauts".into(), "xnaut".into()))
        );
        assert_eq!(
            p("ssh://git@cosmos:22/48Nauts/xnaut.git"),
            Some(("cosmos:22".into(), "48Nauts".into(), "xnaut".into())),
            "userinfo is not part of the host"
        );
        assert_eq!(
            p("https://gitlab.com/group/sub/xnaut.git"),
            Some(("gitlab.com".into(), "sub".into(), "xnaut".into())),
            "the owner is the segment before the repo, nested groups included"
        );
        assert_eq!(p("https://github.com/48Nauts/xnaut/"), p("https://github.com/48Nauts/xnaut"));
        // A guess here would be an API call against the wrong repository.
        assert_eq!(p(""), None);
        assert_eq!(p("xnaut"), None);
        assert_eq!(p("48Nauts/xnaut"), None);
        assert_eq!(p("https://github.com/xnaut"), None, "no owner segment");
    }

    #[test]
    fn a_remote_is_matched_to_a_host_by_hostname() {
        let hosts = vec![
            host("forgejo", "http://cosmos.tail138398.ts.net:3000"),
            host("github", "https://api.github.com"),
        ];
        let (matched, parsed) =
            host_for_remote(&hosts, "http://cosmos.tail138398.ts.net:3000/48Nauts/xnaut.git")
                .unwrap();
        assert_eq!(matched.kind, "forgejo");
        assert_eq!(parsed.owner, "48Nauts");
        assert_eq!(parsed.repo, "xnaut");

        // github.com and api.github.com are the same host for this purpose.
        let (matched, parsed) =
            host_for_remote(&hosts, "git@github.com:48Nauts/xnaut.git").unwrap();
        assert_eq!(matched.kind, "github");
        assert_eq!(parsed.owner, "48Nauts");

        // The owner comes from the REMOTE, not from the host's default org.
        assert_eq!(
            host_for_remote(&hosts, "https://github.com/someone-else/their-repo")
                .unwrap()
                .1
                .owner,
            "someone-else"
        );
        // A host nobody configured has no token, so it is None rather than a
        // request that will 401.
        assert!(host_for_remote(&hosts, "https://bitbucket.org/a/b").is_none());
    }

    #[test]
    fn gitlab_project_path_encodes_slash() {
        assert_eq!(gitlab_project_path("48Nauts", "xnaut"), "48Nauts%2Fxnaut");
        assert_eq!(gitlab_project_path("group", "sub.repo"), "group%2Fsub.repo");
    }

    #[test]
    fn api_base_per_dialect() {
        let f = host("forgejo", "http://cosmos.tail138398.ts.net:3000/");
        assert_eq!(
            api_base(&f).unwrap(),
            "http://cosmos.tail138398.ts.net:3000/api/v1"
        );
        let gh = host("github", "https://github.com");
        assert_eq!(api_base(&gh).unwrap(), "https://api.github.com");
        let gh_api = host("github", "https://api.github.com");
        assert_eq!(api_base(&gh_api).unwrap(), "https://api.github.com");
        let gl = host("gitlab", "");
        assert_eq!(api_base(&gl).unwrap(), "https://gitlab.com/api/v4");
        assert!(api_base(&host("svn", "x")).is_err());
        // A GitHub Enterprise install is taken at its word. The old rule sent
        // it to github.com, which is the wrong company's API.
        let ghe = host("github", "https://git.acme.example/api/v3");
        assert_eq!(api_base(&ghe).unwrap(), "https://git.acme.example/api/v3");
    }

    #[test]
    fn maps_forgejo_issue_json() {
        let v = serde_json::json!({
            "number": 7,
            "title": "Fix the thing",
            "body": "It is broken.",
            "state": "open",
            "labels": [{"name": "bug"}, {"name": "needs-triage"}],
            "user": {"login": "cand0rian"},
            "updated_at": "2026-06-10T08:00:00Z",
            "html_url": "http://cosmos.tail138398.ts.net:3000/48Nauts/xnaut/issues/7",
            "pull_request": null,
        });
        let i = map_github_like_issue(&v);
        assert_eq!(i.number, 7);
        assert_eq!(i.title, "Fix the thing");
        assert_eq!(i.body, "It is broken.");
        assert_eq!(i.state, "open");
        assert_eq!(i.labels, vec!["bug", "needs-triage"]);
        assert_eq!(i.author, "cand0rian");
        assert_eq!(
            i.html_url,
            "http://cosmos.tail138398.ts.net:3000/48Nauts/xnaut/issues/7"
        );
        assert!(!i.is_pr);
    }

    #[test]
    fn detects_pull_request_key() {
        let v = serde_json::json!({
            "number": 8,
            "title": "A PR",
            "state": "open",
            "pull_request": {"merged": false},
        });
        assert!(map_github_like_issue(&v).is_pr);
    }

    #[test]
    fn maps_gitlab_issue_json() {
        let v = serde_json::json!({
            "iid": 12,
            "title": "GitLab issue",
            "description": "Details here",
            "state": "opened",
            "labels": ["feature"],
            "author": {"username": "andre"},
            "updated_at": "2026-06-10T08:00:00Z",
            "web_url": "https://gitlab.com/48Nauts/xnaut/-/issues/12",
        });
        let i = map_gitlab_issue(&v, false);
        assert_eq!(i.number, 12);
        assert_eq!(i.body, "Details here");
        assert_eq!(i.state, "open"); // "opened" normalized
        assert_eq!(i.labels, vec!["feature"]);
        assert_eq!(i.author, "andre");
        assert!(!i.is_pr);
    }
    #[tokio::test]
    async fn repository_pr_recovers_a_lost_response_for_both_forges() {
        use axum::{extract::State, Json, Router, routing::get};
        use std::sync::{Arc, Mutex};
        async fn listing(State(rows): State<Arc<Mutex<Vec<Value>>>>) -> Json<Value> { Json(json!(*rows.lock().unwrap())) }
        async fn create(State(rows): State<Arc<Mutex<Vec<Value>>>>, Json(body): Json<Value>) -> Json<Value> {
            let record = json!({"head":{"ref":body["head"]},"base":{"ref":body["base"]},"html_url":"https://example.test/pulls/1","state":"closed"});
            rows.lock().unwrap().push(record.clone()); Json(record)
        }
        for kind in ["forgejo", "github"] {
            let rows = Arc::new(Mutex::new(Vec::<Value>::new()));
            let route = if kind == "forgejo" { "/api/v1/repos/48Nauts/app/pulls" } else { "/repos/48Nauts/app/pulls" };
            let router = Router::new().route(route, get(listing).post(create)).with_state(rows.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
            let config = host(kind, &format!("http://{address}"));
            let first = ensure_pr(&config, "app", "xnaut/runs/fixture", "main", "Results", "Evidence").await.unwrap();
            let second = ensure_pr(&config, "app", "xnaut/runs/fixture", "main", "Results", "Evidence").await.unwrap();
            assert_eq!(first, second); assert_eq!(rows.lock().unwrap().len(), 1);
            server.abort();
        }
    }

    #[test]
    fn worker_key_accepts_openssh_comments_but_rejects_malformed_material() {
        let root = std::env::temp_dir().join(format!("xnaut-public-key-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("key");
        assert!(std::process::Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-C", "xnaut-worker-fixture", "-f"]).arg(&path).status().unwrap().success());
        let output = std::process::Command::new("ssh-keygen").args(["-y", "-f"]).arg(&path).output().unwrap();
        assert!(output.status.success());
        let public = String::from_utf8(output.stdout).unwrap();
        let normalized = canonical_worker_key(&public).unwrap();
        assert_eq!(normalized.split_whitespace().count(), 2);
        assert_eq!(canonical_worker_key(&(normalized.clone() + " another comment")).unwrap(), normalized);
        for bad in ["ssh-ed25519 AAAA", "ssh-rsa AAAA", "command=unsafe ssh-ed25519 AAAA", "ssh-ed25519 %%%"] {
            assert!(canonical_worker_key(bad).is_err());
        }
        assert!(canonical_worker_key(&format!("{normalized}\n{normalized}")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn worker_keys_are_repo_scoped_idempotent_and_write_enabled_on_both_forges() {
        use axum::{extract::State, Json, Router, routing::get};
        use std::sync::{Arc, Mutex};
        async fn listing(State(rows): State<Arc<Mutex<Vec<Value>>>>) -> Json<Value> { Json(json!(*rows.lock().unwrap())) }
        async fn create(State(rows): State<Arc<Mutex<Vec<Value>>>>, Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(body["read_only"], false);
            assert!(body["title"].as_str().unwrap().starts_with("xNAUT worker "));
            rows.lock().unwrap().push(body.clone()); Json(body)
        }
        async fn repo() -> Json<Value> { Json(json!({"ssh_url":"ssh://git@forge.test:2222/project-owner/app.git"})) }
        for kind in ["forgejo", "github"] {
            let rows = Arc::new(Mutex::new(Vec::<Value>::new()));
            let route = if kind == "forgejo" { "/api/v1/repos/project-owner/app" } else { "/repos/project-owner/app" };
            let router = Router::new().route(route, get(repo)).route(&format!("{route}/keys"), get(listing).post(create)).with_state(rows.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
            let config = host(kind, &format!("http://{address}"));
            assert_eq!(worker_clone_url(&config, "project-owner", "app").await.unwrap(), "ssh://git@forge.test:2222/project-owner/app.git");
            for comment in ["", " xnaut-worker-fixture", " changed comment with spaces"] { ensure_worker_key(&config, "project-owner", "app", &format!("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAABAgMEBQYHCAkKCwwNDg8QERITFBUWFxgZGhscHR4f{comment}")).await.unwrap(); }
            assert_eq!(rows.lock().unwrap().len(), 1);
            // A separate worker gets its own key. Repeated tasks never mint
            // duplicates, and a read-only key never passes a write preflight.
            ensure_worker_key(&config, "project-owner", "app", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8g").await.unwrap();
            assert_eq!(rows.lock().unwrap().len(), 2);
            rows.lock().unwrap()[0]["read_only"] = json!(true);
            assert!(ensure_worker_key(&config, "project-owner", "app", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAABAgMEBQYHCAkKCwwNDg8QERITFBUWFxgZGhscHR4f").await.unwrap_err().contains("read-only"));
            assert_eq!(rows.lock().unwrap().len(), 2);
            server.abort();
        }
    }

    #[tokio::test]
    async fn task_merge_checks_both_forges_and_sends_only_head_cas() {
        use axum::{extract::State, Json, Router, routing::get};
        use std::sync::{Arc,Mutex};
        #[derive(Clone)] struct Fixture {kind:String,pr:Arc<Mutex<Value>>,protection:Arc<Mutex<Value>>,status:Arc<Mutex<Value>>,bodies:Arc<Mutex<Vec<Value>>>}
        async fn pr(State(f):State<Fixture>)->Json<Value>{Json(f.pr.lock().unwrap().clone())}
        async fn protection(State(f):State<Fixture>)->Json<Value>{Json(f.protection.lock().unwrap().clone())}
        async fn status(State(f):State<Fixture>)->Json<Value>{Json(f.status.lock().unwrap().clone())}
        async fn checks()->Json<Value>{Json(json!({"total_count":0,"check_runs":[]}))}
        async fn merge(State(f):State<Fixture>,Json(body):Json<Value>)->Json<Value>{
            if f.kind=="github" {assert_eq!(body,json!({"sha":"head","merge_method":"merge"}));}
            else {assert_eq!(body,json!({"Do":"merge","head_commit_id":"head","force_merge":false,"merge_when_checks_succeed":false,"delete_branch_after_merge":false}));}
            f.bodies.lock().unwrap().push(body); f.pr.lock().unwrap()["merged"]=json!(true);Json(json!({"merged":true}))
        }
        for kind in ["forgejo","github"] {
            let f=Fixture{kind:kind.into(),pr:Arc::new(Mutex::new(json!({"head":{"sha":"head","ref":"task","repo":{"id":1}},"base":{"sha":"base","ref":"main","repo":{"id":1}},"draft":false,"state":"open","mergeable":true,"merged":false}))),protection:Arc::new(Mutex::new(json!({"required_status_checks":{"strict":true},"enforce_admins":{"enabled":true},"block_on_outdated_branch":true,"apply_to_admins":true}))),status:Arc::new(Mutex::new(json!({"sha":"head","total_count":1,"state":"success"}))),bodies:Arc::new(Mutex::new(vec![]))};
            let route=if kind=="forgejo" {"/api/v1/repos/team/app"} else {"/repos/team/app"};
            let router=Router::new().route(&format!("{route}/pulls/7"),get(pr)).route(&format!("{route}/pulls/7/merge"),axum::routing::post(merge).put(merge)).route(&format!("{route}/branch_protections/main"),get(protection)).route(&format!("{route}/branches/main/protection"),get(protection)).route(&format!("{route}/commits/head/status"),get(status)).route(&format!("{route}/commits/head/check-runs"),get(checks)).with_state(f.clone());
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
            let server=tokio::spawn(async move{axum::serve(listener,router).await.unwrap();});let config=host(kind,&format!("http://{address}"));
            repository_merge_preflight(&config,"team","app",7,"head","base","task","main").await.unwrap();
            for bad in ["pending","failure"] {f.status.lock().unwrap()["state"]=json!(bad);assert!(repository_merge_preflight(&config,"team","app",7,"head","base","task","main").await.is_err());}
            f.status.lock().unwrap()["state"]=json!("success");
            assert!(repository_merge_preflight(&config,"team","app",7,"stale","base","task","main").await.is_err());
            assert!(repository_merge_preflight(&config,"team","app",7,"head","stale","task","main").await.is_err());
            for field in ["draft","mergeable"] {let old=f.pr.lock().unwrap()[field].clone();f.pr.lock().unwrap()[field]=Value::Null;assert!(repository_merge_preflight(&config,"team","app",7,"head","base","task","main").await.is_err());f.pr.lock().unwrap()[field]=old;}
            let protected=f.protection.lock().unwrap().clone();*f.protection.lock().unwrap()=json!({});assert!(repository_merge_preflight(&config,"team","app",7,"head","base","task","main").await.is_err());*f.protection.lock().unwrap()=protected;
            assert_eq!(f.bodies.lock().unwrap().len(),0);
            let merged=repository_merge(&config,"team","app",7,"head").await.unwrap();assert_eq!(merged["merged"],true);assert_eq!(f.bodies.lock().unwrap().len(),1);server.abort();
        }
    }
    #[test] fn task_merge_refuses_cross_repo_and_unprotected_admins() {
        assert!(!protected_against_stale_base("github",&json!({"required_status_checks":{"strict":true},"enforce_admins":{"enabled":false}})));
        assert!(!protected_against_stale_base("forgejo",&json!({"block_on_outdated_branch":true,"apply_to_admins":false})));
        let pr=json!({"head":{"sha":"h","ref":"task","repo":{"id":2}},"base":{"sha":"b","ref":"main","repo":{"id":1}},"draft":false,"state":"open","mergeable":true,"merged":false});
        assert!(validate_merge_pr(&pr,"h","b","task","main").is_err());
    }

    #[test]
    fn github_cloud_never_uses_an_enterprise_connection() {
        let hosts = vec![host("github", "https://enterprise.example/api/v3"), host("github", "https://api.github.com")];
        let (matched, _) = host_for_remote(&hosts, "https://github.com/team/repo").unwrap();
        assert_eq!(matched.base_url, "https://api.github.com");
    }

    #[test]
    fn repository_host_matching_preserves_http_port_and_refuses_ambiguous_ssh() {
        let hosts = vec![host("forgejo", "http://forge.test:3000"), host("forgejo", "http://forge.test:4000")];
        let (matched, _) = host_for_remote(&hosts, "http://forge.test:4000/team/repo").unwrap();
        assert_eq!(matched.base_url, "http://forge.test:4000");
        assert!(host_for_remote(&hosts, "ssh://git@forge.test:2222/team/repo.git").is_none());
    }

}
