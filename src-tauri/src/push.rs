// Push notifications for the mobile bridge (1.22.2 requirements, item 1).
// The design note from the iOS side, kept verbatim because it is the whole
// architecture: "put this behind one trait with a single method, so the
// transport is swappable. The original plan was ntfy, then became APNs when
// the App Store became the target. Whatever ships, nothing upstream of the
// emitter should know which."
//
// So: emit points call `notify()` with a PushNote and know nothing else.
// Today's transport is ntfy (a plain HTTP POST, works without any Apple
// setup — subscribe to the topic in the ntfy app); APNs becomes a second
// transport the day the push key exists, using the apns_token already
// collected per device. A missing configuration is a debug-log no-op, never
// an error: push is best-effort by nature.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PushNote {
    pub title: String,
    pub body: String,
    /// "approve" | "ask" | "status" | "run"
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbox_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// The one seam. Fire-and-forget: spawns the send so no emit point ever
/// blocks on the network, and a failed push is a debug-log line, not an
/// error surfaced to whatever was busy creating an inbox item.
pub fn notify(note: PushNote) {
    let cfg = crate::mobile::load_or_init_config();
    let topic = cfg.push_ntfy_topic.trim().to_string();
    if topic.is_empty() {
        let _ = crate::debug_log::debug_log_append(vec![format!("[push] no transport configured, dropped: {}", note.title)]);
        return;
    }
    tauri::async_runtime::spawn(async move {
        // ntfy: POST https://ntfy.sh/<topic> with headers. The topic is the
        // secret; it lives in mobile.json next to the pairing token.
        let client = reqwest::Client::new();
        let result = client
            .post(format!("https://ntfy.sh/{topic}"))
            .header("Title", note.title.replace(['\n', '\r'], " "))
            .header("Priority", if note.kind == "approve" { "high" } else { "default" })
            .header("Tags", note.kind.clone())
            .body(note.body.clone())
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;
        if let Err(error) = result {
            let _ = crate::debug_log::debug_log_append(vec![format!("[push] ntfy send failed: {error}")]);
        }
    });
}
