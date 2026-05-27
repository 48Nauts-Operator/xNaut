// Browser panes — Phase 6 of the Orca port. A browser pane is a Tauri child
// Webview attached to the main window. Frontend reserves a placeholder div in
// its layout and tracks the div's bounding rect via ResizeObserver; whenever
// the rect changes, JS calls browser_pane_set_bounds to keep the webview
// positioned to match. Tab switches set inactive webviews to (offscreen, 1x1)
// so they remain hot (no reload cost) but don't paint over the active pane.
//
// Security posture: child webviews load arbitrary URLs and MUST be treated as
// untrusted. They never get the Tauri API global injected, and they live under
// a strict CSP-free namespace separate from the main window.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, webview::WebviewBuilder};
use tokio::sync::Mutex;
use url::Url;

/// Per-app registry of browser panes so we can find them by label later.
/// Tauri doesn't expose a "list all child webviews" API, hence the map.
#[derive(Default)]
pub struct BrowserPaneRegistry {
    inner: Mutex<HashMap<String, BrowserPaneInfo>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserPaneInfo {
    pub label: String,
    pub url: String,
}

impl BrowserPaneRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    /// Parent window label — usually "main".
    pub window_label: String,
    /// Caller-supplied webview label (must be unique across the window).
    pub label: String,
    pub url: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Deserialize)]
pub struct BoundsRequest {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn parse_url(raw: &str) -> Result<Url, String> {
    // Helpful smoothing: bare strings without a scheme get http://; obvious
    // search terms (whitespace, no dot) route through DuckDuckGo so a user
    // who types "rust webview" in the address bar gets a sensible result.
    let s = raw.trim();
    if s.is_empty() {
        return "about:blank".parse().map_err(|e| format!("{e}"));
    }
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("about:") {
        return s.parse().map_err(|e| format!("{e}"));
    }
    if s.contains(' ') || !s.contains('.') {
        let q = urlencoding_lite(s);
        return format!("https://duckduckgo.com/?q={q}")
            .parse()
            .map_err(|e| format!("{e}"));
    }
    format!("https://{s}").parse().map_err(|e| format!("{e}"))
}

/// Minimal URL-encode for the search-fallback path. We don't pull in a whole
/// urlencoding crate for a single use site.
fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn browser_pane_create(
    app: AppHandle,
    registry: tauri::State<'_, Arc<BrowserPaneRegistry>>,
    req: CreateRequest,
) -> Result<BrowserPaneInfo, String> {
    let url = parse_url(&req.url)?;
    // add_child lives on Window (not WebviewWindow). The main WebviewWindow's
    // underlying Window is what we want to host multiple webviews on.
    let window = app
        .get_window(&req.window_label)
        .ok_or_else(|| format!("window '{}' not found", req.window_label))?;

    // Critically, do NOT inject the Tauri API into untrusted page contexts.
    let builder = WebviewBuilder::new(&req.label, WebviewUrl::External(url.clone()))
        .initialization_script("window.__TAURI__ = undefined;");

    let _webview = window
        .add_child(
            builder,
            LogicalPosition::new(req.x, req.y),
            LogicalSize::new(req.width.max(1.0), req.height.max(1.0)),
        )
        .map_err(|e| format!("add_child failed: {e}"))?;

    let info = BrowserPaneInfo {
        label: req.label.clone(),
        url: url.to_string(),
    };
    registry.inner.lock().await.insert(req.label, info.clone());
    Ok(info)
}

#[tauri::command]
pub fn browser_pane_set_bounds(
    app: AppHandle,
    req: BoundsRequest,
) -> Result<(), String> {
    let webview = app
        .get_webview(&req.label)
        .ok_or_else(|| format!("webview '{}' not found", req.label))?;
    webview
        .set_position(LogicalPosition::new(req.x, req.y))
        .map_err(|e| format!("set_position: {e}"))?;
    webview
        .set_size(LogicalSize::new(req.width.max(1.0), req.height.max(1.0)))
        .map_err(|e| format!("set_size: {e}"))?;
    Ok(())
}

/// Hide a pane by shoving it offscreen and shrinking to 1×1. Cheaper than
/// destroying/recreating, and preserves scroll/auth state when the user
/// flips back to it.
#[tauri::command]
pub fn browser_pane_set_visible(
    app: AppHandle,
    label: String,
    visible: bool,
) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("webview '{}' not found", label))?;
    if visible {
        // Caller is responsible for set_bounds() right after visible=true.
        Ok(())
    } else {
        webview
            .set_position(LogicalPosition::new(-32000.0, -32000.0))
            .map_err(|e| format!("set_position: {e}"))?;
        webview
            .set_size(LogicalSize::new(1.0, 1.0))
            .map_err(|e| format!("set_size: {e}"))?;
        Ok(())
    }
}

#[tauri::command]
pub async fn browser_pane_navigate(
    app: AppHandle,
    registry: tauri::State<'_, Arc<BrowserPaneRegistry>>,
    label: String,
    url: String,
) -> Result<String, String> {
    let parsed = parse_url(&url)?;
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("webview '{}' not found", label))?;
    // navigate is the cleanest Tauri 2 API for changing URL in place.
    webview
        .navigate(parsed.clone())
        .map_err(|e| format!("navigate: {e}"))?;
    if let Some(info) = registry.inner.lock().await.get_mut(&label) {
        info.url = parsed.to_string();
    }
    Ok(parsed.to_string())
}

#[tauri::command]
pub fn browser_pane_back(app: AppHandle, label: String) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("webview '{}' not found", label))?;
    webview
        .eval("history.back()")
        .map_err(|e| format!("eval: {e}"))
}

#[tauri::command]
pub fn browser_pane_forward(app: AppHandle, label: String) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("webview '{}' not found", label))?;
    webview
        .eval("history.forward()")
        .map_err(|e| format!("eval: {e}"))
}

#[tauri::command]
pub fn browser_pane_reload(app: AppHandle, label: String) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("webview '{}' not found", label))?;
    webview
        .eval("location.reload()")
        .map_err(|e| format!("eval: {e}"))
}

#[tauri::command]
pub async fn browser_pane_destroy(
    app: AppHandle,
    registry: tauri::State<'_, Arc<BrowserPaneRegistry>>,
    label: String,
) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.close();
    }
    registry.inner.lock().await.remove(&label);
    Ok(())
}

#[tauri::command]
pub async fn browser_pane_list(
    registry: tauri::State<'_, Arc<BrowserPaneRegistry>>,
) -> Result<Vec<BrowserPaneInfo>, String> {
    Ok(registry.inner.lock().await.values().cloned().collect())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_preserves_http_and_https() {
        assert_eq!(parse_url("https://example.com").unwrap().as_str(), "https://example.com/");
        assert_eq!(parse_url("http://example.com").unwrap().as_str(), "http://example.com/");
    }

    #[test]
    fn parse_url_adds_https_to_bare_host() {
        assert_eq!(parse_url("example.com").unwrap().as_str(), "https://example.com/");
    }

    #[test]
    fn parse_url_routes_search_terms_to_duckduckgo() {
        let u = parse_url("rust webview").unwrap();
        assert!(u.as_str().starts_with("https://duckduckgo.com/?q="));
        assert!(u.as_str().contains("rust+webview"));
    }

    #[test]
    fn parse_url_handles_empty_as_blank() {
        assert_eq!(parse_url("").unwrap().as_str(), "about:blank");
    }

    #[test]
    fn urlencoding_lite_handles_specials() {
        assert_eq!(urlencoding_lite("hello world"), "hello+world");
        assert_eq!(urlencoding_lite("a&b"), "a%26b");
    }
}
