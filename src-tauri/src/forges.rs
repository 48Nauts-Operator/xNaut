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
            if raw.contains("api.github.com") {
                Ok(raw.to_string())
            } else {
                Ok("https://api.github.com".to_string())
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

/// List open issues or PRs for owner/repo on the given host.
pub async fn list_issues(
    host: &ForgeHost,
    repo: &str,
    kind: IssueKind,
) -> Result<Vec<ForgeIssue>, String> {
    let client = http_client()?;
    let token = token_for(host)?;
    let base = api_base(host)?;
    let owner = &host.owner;
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
    let client = http_client()?;
    let token = token_for(host)?;
    let base = api_base(host)?;
    let owner = &host.owner;
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

/// Append a comment to an issue without modifying the reporter's original body.
pub async fn add_issue_comment(
    host: &ForgeHost,
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
    let owner = &host.owner;
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
}
