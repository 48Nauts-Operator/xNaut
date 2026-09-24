// The Wiki tab's backend (XNAUT-438): fetch a docs page, and keep the
// per-project collection of pages that have been opened.
//
// Two jobs, and only two.
//
// 1. FETCH. The webview cannot read `https://adk.dev/` itself. CORS refuses a
//    cross-origin read and there is no preflight a docs site would answer. So
//    the HTML comes through Rust, where the same-origin policy does not apply,
//    and the pane does the extraction on the string it gets back.
//
// 2. REMEMBER. Every page opened is recorded in ONE Markdown file per project
//    and per docs site, at `work/<Project>/Development/docu/<slug>-docu.md`, the
//    collection André calls `@adk-docu`. The file is the source of truth: the
//    URL bar's history dropdown is rendered from it, a page reopens from it
//    when the site is down, and it is a readable vault note either way.
//
// The file has to survive a round trip, so each page is fenced by an HTML
// comment carrying its metadata and the extracted article sits between the
// fences as plain Markdown. A comment rather than YAML because the result must
// still read as a document when the owner opens it in the vault, and an editor
// that reflows the prose cannot corrupt an attribute it never touches.

use serde::{Deserialize, Serialize};

/// One docs page, as it is stored in the collection file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WikiEntry {
    pub url: String,
    pub title: String,
    /// RFC 3339, UTC. When this page was last opened.
    pub opened_at: String,
    pub pinned: bool,
    /// The extracted article as Markdown; what an offline reopen renders.
    pub markdown: String,
}

/// A project's collection for one docs site.
#[derive(Debug, Clone, Serialize)]
pub struct WikiCollection {
    /// `@adk-docu`; the name the owner types in the URL bar.
    pub name: String,
    /// `work:xnaut/Development/docu/adk-docu.md`; the vault reference.
    pub reference: String,
    /// The absolute path on this machine, for a handback to quote.
    pub path: String,
    pub entries: Vec<WikiEntry>,
}

/// What a fetch hands the pane.
#[derive(Debug, Clone, Serialize)]
pub struct WikiPage {
    /// The URL after redirects; the one links must resolve against.
    pub url: String,
    pub status: u16,
    pub content_type: String,
    pub html: String,
}

/// A page bigger than this is not a docs page, it is a download. The whole
/// body is held in memory and shipped over IPC, so the cap is what keeps a
/// mistyped URL from freezing the pane.
const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Most docs sites are behind a CDN that answers a bare client with 403.
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                  (KHTML, like Gecko) Version/17.0 Safari/605.1.15 xNAUT-Wiki";

/// The only two schemes the Wiki tab fetches.
///
/// It is a separate function because this is the check worth testing on its
/// own: `file:///etc/passwd` typed into a URL bar that fetches through Rust
/// reads a local file with the app's own permissions, and `javascript:` in a
/// link the extractor rewrote would be handed straight back to the webview.
/// Both are refused here, before a client is ever built.
pub fn check_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("no URL given".into());
    }
    let parsed = url::Url::parse(trimmed)
        .map_err(|_| format!("{trimmed} is not a URL the Wiki can open"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "the Wiki opens http and https pages only, not {other}:"
            ))
        }
    }
    if parsed.host_str().unwrap_or("").is_empty() {
        return Err(format!("{trimmed} names no host"));
    }
    Ok(parsed.to_string())
}

/// The collection name for a docs site: `https://adk.dev/docs/x` → `adk`.
///
/// The registrable label, which is the second-to-last: `docs.python.org` is
/// `python` and not `docs`, and `adk.dev` is `adk`, which is the name André
/// asked for. A single-label host (localhost, an intranet box) keeps its own
/// name rather than becoming empty.
pub fn slug_for(url: &str) -> String {
    let host = url::Url::parse(url.trim())
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    let host = host.trim_start_matches("www.").to_lowercase();
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    let pick = match labels.len() {
        0 => "docs",
        1 => labels[0],
        n => labels[n - 2],
    };
    let cleaned: String = pick
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        "docs".into()
    } else {
        cleaned
    }
}

/// The vault path of a collection, relative to the vault root.
fn rel_for(project: &str, slug: &str) -> Result<String, String> {
    let project = project.trim().trim_matches('/');
    let slug = slug.trim();
    if project.is_empty() {
        return Err("which project?".into());
    }
    if slug.is_empty() {
        return Err("which docs site?".into());
    }
    if project.contains("..") || slug.contains('/') || slug.contains("..") {
        return Err("a collection path cannot climb out of the vault".into());
    }
    Ok(format!("work/{project}/Development/docu/{slug}-docu.md"))
}

const OPEN: &str = "<!-- xnaut:page";
const CLOSE: &str = "<!-- xnaut:end -->";

fn attr(line: &str, key: &str) -> String {
    let needle = format!("{key}=\"");
    let Some(start) = line.find(&needle) else {
        return String::new();
    };
    let rest = &line[start + needle.len()..];
    match rest.find('"') {
        Some(end) => rest[..end].to_string(),
        None => String::new(),
    }
}

/// Read the pages out of a collection file. Anything that is not inside a
/// fence, meaning the frontmatter, the heading and the explanatory paragraph, is prose
/// for a human and is ignored here, then written afresh on the next save.
pub fn parse_collection(text: &str) -> Vec<WikiEntry> {
    let mut entries = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if !line.trim_start().starts_with(OPEN) {
            continue;
        }
        let url = attr(line, "url");
        if url.is_empty() {
            continue;
        }
        let entry = WikiEntry {
            url,
            title: String::new(),
            opened_at: attr(line, "opened"),
            pinned: attr(line, "pinned") == "yes",
            markdown: String::new(),
        };
        let mut body: Vec<&str> = Vec::new();
        let mut title = String::new();
        for inner in lines.by_ref() {
            if inner.trim_start().starts_with(CLOSE) {
                break;
            }
            if title.is_empty() {
                if let Some(rest) = inner.strip_prefix("## ") {
                    title = rest.trim().to_string();
                    continue;
                }
                if inner.trim().is_empty() {
                    continue;
                }
            }
            body.push(inner);
        }
        entries.push(WikiEntry {
            title,
            markdown: body.join("\n").trim().to_string(),
            ..entry
        });
    }
    entries
}

/// Render the collection back to Markdown. Pinned pages first, then newest
/// first, which is the order the history dropdown shows and therefore the
/// order the file should read in.
pub fn render_collection(name: &str, entries: &[WikiEntry]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# @{name}-docu\n\n"));
    out.push_str(
        "Docs pages opened in the Wiki tab of the Build run pane. Pinned first, then newest \
         first. Each page keeps the extracted article, so it reopens from here when the site \
         is unreachable. Written by xNAUT; edit the prose freely, but leave the \
         `xnaut:page` fences alone.\n\n",
    );
    for entry in ordered(entries) {
        out.push_str(&format!(
            "{OPEN} url=\"{}\" opened=\"{}\" pinned=\"{}\" -->\n\n## {}\n\n{}\n\n{CLOSE}\n\n",
            entry.url,
            entry.opened_at,
            if entry.pinned { "yes" } else { "no" },
            if entry.title.trim().is_empty() {
                &entry.url
            } else {
                &entry.title
            },
            entry.markdown.trim(),
        ));
    }
    out
}

/// Pinned first, then newest first. Borrowed order so both the file and the
/// dropdown are built from one rule.
pub fn ordered(entries: &[WikiEntry]) -> Vec<&WikiEntry> {
    let mut out: Vec<&WikiEntry> = entries.iter().collect();
    out.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.opened_at.cmp(&a.opened_at))
    });
    out
}

/// A collection stops growing at this many pages, or at this many bytes of
/// stored article, whichever comes first. The byte budget is the one that
/// matters: a real ADK reference page extracts to 78 KB of Markdown, so a
/// count alone would allow a 23 MB vault note that no editor will open and
/// that every recorded page rewrites from scratch.
///
/// What falls off is the far end of the order, which is pinned first then newest,
/// so pinning is what makes a page permanent.
const MAX_ENTRIES: usize = 300;
const MAX_STORED_BYTES: usize = 4 * 1024 * 1024;

fn within_budget(entries: &[WikiEntry]) -> Vec<WikiEntry> {
    let mut kept = Vec::new();
    let mut bytes = 0usize;
    for entry in ordered(entries) {
        if kept.len() >= MAX_ENTRIES {
            break;
        }
        bytes += entry.markdown.len();
        if bytes > MAX_STORED_BYTES && !kept.is_empty() {
            break;
        }
        kept.push(entry.clone());
    }
    kept
}

fn load(project: &str, slug: &str) -> Result<(String, Vec<WikiEntry>), String> {
    let rel = rel_for(project, slug)?;
    let root = crate::vault_tools::vault_root()?;
    let path = root.join(&rel);
    let entries = match std::fs::read_to_string(&path) {
        Ok(text) => parse_collection(&text),
        Err(_) => Vec::new(),
    };
    Ok((rel, entries))
}

fn collection(
    project: &str,
    slug: &str,
    entries: Vec<WikiEntry>,
) -> Result<WikiCollection, String> {
    let rel = rel_for(project, slug)?;
    let root = crate::vault_tools::vault_root()?;
    Ok(WikiCollection {
        name: format!("@{slug}-docu"),
        reference: format!("work:{}", rel.trim_start_matches("work/")),
        path: root.join(&rel).to_string_lossy().to_string(),
        entries: ordered(&entries).into_iter().cloned().collect(),
    })
}

fn save(project: &str, slug: &str, entries: &[WikiEntry]) -> Result<(), String> {
    let rel = rel_for(project, slug)?;
    let body = render_collection(slug, entries);
    crate::vault_tools::write(&rel, &body, "xNAUT Wiki")?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch a docs page as HTML. The pane extracts from what comes back.
#[tauri::command]
pub async fn wiki_fetch(url: String) -> Result<WikiPage, String> {
    let url = check_url(&url)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .user_agent(UA)
        .build()
        .map_err(|e| format!("could not build an HTTP client: {e}"))?;
    let response = client
        .get(&url)
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .map_err(|e| format!("could not reach {url}: {e}"))?;
    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !status.is_success() {
        return Err(format!("{url} answered {}", status.as_u16()));
    }
    if !content_type.is_empty() && !content_type.contains("html") && !content_type.contains("xml") {
        return Err(format!(
            "{url} is {content_type}, not a page the Wiki can read"
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("could not read {url}: {e}"))?;
    if bytes.len() > MAX_BYTES {
        return Err(format!(
            "{url} is {} MB, too large to read as a docs page",
            bytes.len() / (1024 * 1024)
        ));
    }
    Ok(WikiPage {
        url: final_url,
        status: status.as_u16(),
        content_type,
        html: String::from_utf8_lossy(&bytes).to_string(),
    })
}

/// The collection name for a URL, so the pane and the file agree on one rule.
#[tauri::command]
pub fn wiki_slug(url: String) -> Result<String, String> {
    check_url(&url)?;
    Ok(slug_for(&url))
}

/// Read a project's collection for one docs site.
#[tauri::command]
pub fn wiki_collection_read(project: String, slug: String) -> Result<WikiCollection, String> {
    let (_, entries) = load(&project, &slug)?;
    collection(&project, &slug, entries)
}

/// Record a page that was just opened, and return the collection as it now
/// stands. A page already in the collection is moved to the front and keeps
/// its pin rather than being duplicated.
#[tauri::command]
pub fn wiki_collection_record(
    project: String,
    slug: String,
    url: String,
    title: String,
    markdown: String,
) -> Result<WikiCollection, String> {
    let url = check_url(&url)?;
    let (_, mut entries) = load(&project, &slug)?;
    let pinned = entries
        .iter()
        .find(|e| e.url == url)
        .map(|e| e.pinned)
        .unwrap_or(false);
    entries.retain(|e| e.url != url);
    entries.push(WikiEntry {
        url,
        title: title.trim().to_string(),
        opened_at: chrono::Utc::now().to_rfc3339(),
        pinned,
        markdown: markdown.trim().to_string(),
    });
    entries = within_budget(&entries);
    save(&project, &slug, &entries)?;
    collection(&project, &slug, entries)
}

/// Pin or unpin a page. Pinned pages sort first and survive the cap.
#[tauri::command]
pub fn wiki_collection_pin(
    project: String,
    slug: String,
    url: String,
    pinned: bool,
) -> Result<WikiCollection, String> {
    let (_, mut entries) = load(&project, &slug)?;
    let mut found = false;
    for entry in entries.iter_mut() {
        if entry.url == url {
            entry.pinned = pinned;
            found = true;
        }
    }
    if !found {
        return Err(format!("{url} is not in this collection"));
    }
    save(&project, &slug, &entries)?;
    collection(&project, &slug, entries)
}

/// Forget a page. The collection is the owner's note, so it has to be possible
/// to take something out of it from the pane that put it in.
#[tauri::command]
pub fn wiki_collection_forget(
    project: String,
    slug: String,
    url: String,
) -> Result<WikiCollection, String> {
    let (_, mut entries) = load(&project, &slug)?;
    let before = entries.len();
    entries.retain(|e| e.url != url);
    if entries.len() == before {
        return Err(format!("{url} is not in this collection"));
    }
    save(&project, &slug, &entries)?;
    collection(&project, &slug, entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Staged {
        _lock: std::sync::MutexGuard<'static, ()>,
        root: PathBuf,
    }

    impl Drop for Staged {
        fn drop(&mut self) {
            std::env::remove_var("XNAUT_TEST_VAULT");
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// A vault of our own: the same staging vault_tools.rs uses, taking the
    /// same process-global lock, because XNAUT_TEST_VAULT is one variable and
    /// a second mutex would only let two tests move the root out from under
    /// each other.
    fn staged(name: &str) -> Staged {
        let lock = crate::vault::test_vault_lock();
        let root =
            std::env::temp_dir().join(format!("xnaut-vault-wiki-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("work")).expect("staged vault");
        std::env::set_var("XNAUT_TEST_VAULT", &root);
        crate::vault::use_test_vault(root.clone());
        Staged { _lock: lock, root }
    }

    // ── the fetch guard ──────────────────────────────────────────────────────

    #[test]
    fn the_fetch_refuses_a_non_http_url() {
        // The whole point of fetching through Rust is that Rust is not bound by
        // the same-origin policy. That is also why a scheme check has to exist:
        // file:// would read the owner's disk with the app's permissions.
        for raw in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<h1>hi</h1>",
            "about:blank",
            "ftp://example.com/docs",
            "chrome://settings",
        ] {
            let refused = check_url(raw);
            assert!(refused.is_err(), "{raw} was accepted by the Wiki fetch");
            let why = refused.unwrap_err();
            assert!(
                why.contains("http and https"),
                "{raw} was refused without saying why: {why}"
            );
        }
    }

    #[test]
    fn the_fetch_accepts_http_and_https_and_refuses_nonsense() {
        assert_eq!(
            check_url("https://adk.dev/docs/get-started").unwrap(),
            "https://adk.dev/docs/get-started"
        );
        assert!(check_url("http://127.0.0.1:8000/docs/").is_ok());
        assert!(
            check_url("  https://adk.dev/  ").is_ok(),
            "a pasted URL carries whitespace"
        );
        assert!(check_url("").is_err());
        assert!(
            check_url("adk.dev/docs").is_err(),
            "a scheme-less string is not a URL"
        );
        assert!(
            check_url("https://").is_err(),
            "a URL with no host names nothing to fetch"
        );
    }

    // ── the collection name ──────────────────────────────────────────────────

    #[test]
    fn a_collection_is_named_after_the_docs_site() {
        // The name André asked for, and the shapes that would otherwise pick a
        // subdomain nobody calls the site by.
        assert_eq!(slug_for("https://adk.dev/"), "adk");
        assert_eq!(slug_for("https://docs.python.org/3/library/"), "python");
        assert_eq!(slug_for("https://www.rust-lang.org/learn"), "rust-lang");
        assert_eq!(slug_for("https://react.dev/reference/react"), "react");
        assert_eq!(slug_for("http://localhost:8000/docs"), "localhost");
        assert_eq!(slug_for("not a url"), "docs");
    }

    // ── the file round-trips ─────────────────────────────────────────────────

    fn entry(url: &str, title: &str, when: &str, pinned: bool, body: &str) -> WikiEntry {
        WikiEntry {
            url: url.into(),
            title: title.into(),
            opened_at: when.into(),
            pinned,
            markdown: body.into(),
        }
    }

    #[test]
    fn a_collection_survives_a_round_trip() {
        let entries = vec![
            entry(
                "https://adk.dev/docs/get-started",
                "Get started",
                "2026-09-22T18:00:00+00:00",
                false,
                "# Get started\n\nInstall the SDK:\n\n```bash\npip install adk\n```",
            ),
            entry(
                "https://adk.dev/docs/agents",
                "Agents",
                "2026-09-22T19:00:00+00:00",
                true,
                "An agent is a loop.\n\n- one\n- two",
            ),
        ];
        let rendered = render_collection("adk", &entries);
        let back = parse_collection(&rendered);

        // Pinned first, so the file reads in the order the dropdown shows.
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].url, "https://adk.dev/docs/agents");
        assert!(back[0].pinned);
        assert_eq!(back[1].title, "Get started");
        // The article is what an offline reopen renders, fences and all.
        assert!(
            back[1].markdown.contains("```bash\npip install adk\n```"),
            "the fenced code block did not survive: {:?}",
            back[1].markdown
        );
        assert_eq!(back[1].opened_at, "2026-09-22T18:00:00+00:00");
    }

    #[test]
    fn prose_around_the_fences_is_not_read_as_a_page() {
        let text =
            "---\nAuthor: xNAUT Wiki\n---\n\n# @adk-docu\n\nA paragraph the owner wrote.\n\n\
                    ## Not a page, just a heading\n\n<!-- xnaut:page url=\"https://adk.dev/a\" \
                    opened=\"2026-09-22T10:00:00+00:00\" pinned=\"no\" -->\n\n## A\n\nBody.\n\n\
                    <!-- xnaut:end -->\n\nA closing note.\n";
        let back = parse_collection(text);
        assert_eq!(back.len(), 1, "prose outside the fences became a page");
        assert_eq!(back[0].title, "A");
        assert_eq!(back[0].markdown, "Body.");
    }

    // ── recording ────────────────────────────────────────────────────────────

    #[test]
    fn recording_a_page_writes_the_vault_file_and_reopening_it_does_not_duplicate() {
        let _staged = staged("record");

        let first = wiki_collection_record(
            "xnaut".into(),
            "adk".into(),
            "https://adk.dev/docs/get-started".into(),
            "Get started".into(),
            "Install it.".into(),
        )
        .expect("record");
        assert_eq!(first.name, "@adk-docu");
        assert_eq!(
            first.reference, "work:xnaut/Development/docu/adk-docu.md",
            "the reference a later composer mention would point at"
        );
        assert!(
            std::path::Path::new(&first.path).is_file(),
            "no file at {}",
            first.path
        );
        let on_disk = std::fs::read_to_string(&first.path).expect("read back");
        assert!(
            on_disk.starts_with("---\nAuthor:"),
            "no vault frontmatter:\n{on_disk}"
        );
        assert!(on_disk.contains("Install it."));

        // The same page again is the same entry, not a second one.
        let again = wiki_collection_record(
            "xnaut".into(),
            "adk".into(),
            "https://adk.dev/docs/get-started".into(),
            "Get started".into(),
            "Install it, revised.".into(),
        )
        .expect("record again");
        assert_eq!(again.entries.len(), 1);
        assert_eq!(again.entries[0].markdown, "Install it, revised.");
    }

    #[test]
    fn a_pin_survives_reopening_the_page_and_sorts_first() {
        let _staged = staged("pin");
        for (url, title) in [
            ("https://adk.dev/docs/a", "A"),
            ("https://adk.dev/docs/b", "B"),
        ] {
            wiki_collection_record(
                "xnaut".into(),
                "adk".into(),
                url.into(),
                title.into(),
                "body".into(),
            )
            .expect("record");
        }
        // B was opened last, so B leads until A is pinned.
        let pinned = wiki_collection_pin(
            "xnaut".into(),
            "adk".into(),
            "https://adk.dev/docs/a".into(),
            true,
        )
        .expect("pin");
        assert_eq!(pinned.entries[0].url, "https://adk.dev/docs/a");

        let reopened = wiki_collection_record(
            "xnaut".into(),
            "adk".into(),
            "https://adk.dev/docs/a".into(),
            "A".into(),
            "body again".into(),
        )
        .expect("reopen");
        assert!(
            reopened.entries[0].pinned,
            "reopening a page dropped its pin"
        );
    }

    #[test]
    fn a_collection_stops_growing_at_the_byte_budget_and_keeps_the_pinned_pages() {
        // A real ADK reference page is 78 KB of Markdown, so the budget bites
        // long before the count does. Half a megabyte each: nine fit, the tenth
        // pushes the oldest unpinned page out.
        let big = "x".repeat(512 * 1024);
        let mut entries: Vec<WikiEntry> = (0..12)
            .map(|i| {
                entry(
                    &format!("https://adk.dev/docs/p{i}"),
                    &format!("P{i}"),
                    &format!("2026-09-{:02}T10:00:00+00:00", i + 1),
                    i == 0, // the oldest page, pinned
                    &big,
                )
            })
            .collect();
        entries = within_budget(&entries);

        let total: usize = entries.iter().map(|e| e.markdown.len()).sum();
        assert!(
            total <= MAX_STORED_BYTES,
            "kept {total} bytes, over the budget"
        );
        assert!(
            entries.len() >= 4 && entries.len() < 12,
            "kept {} pages",
            entries.len()
        );
        assert_eq!(entries[0].title, "P0", "the pinned page was not kept first");
        assert!(
            !entries.iter().any(|e| e.title == "P1"),
            "the oldest unpinned page survived while newer ones were dropped"
        );
    }

    #[test]
    fn one_page_larger_than_the_whole_budget_is_still_kept() {
        // Otherwise a single enormous docs page would record as an empty
        // collection, which reads as "the feature did not work".
        let entries = within_budget(&[entry(
            "https://adk.dev/docs/huge",
            "Huge",
            "2026-09-22T10:00:00+00:00",
            false,
            &"x".repeat(MAX_STORED_BYTES + 4096),
        )]);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn a_collection_that_was_never_opened_reads_as_empty_rather_than_failing() {
        let _staged = staged("empty");
        let empty = wiki_collection_read("xnaut".into(), "adk".into()).expect("read");
        assert!(empty.entries.is_empty());
        assert_eq!(empty.name, "@adk-docu");
    }

    #[test]
    fn forgetting_a_page_removes_it_and_an_unknown_page_says_so() {
        let _staged = staged("forget");
        wiki_collection_record(
            "xnaut".into(),
            "adk".into(),
            "https://adk.dev/docs/a".into(),
            "A".into(),
            "body".into(),
        )
        .expect("record");
        let gone = wiki_collection_forget(
            "xnaut".into(),
            "adk".into(),
            "https://adk.dev/docs/a".into(),
        )
        .expect("forget");
        assert!(gone.entries.is_empty());
        assert!(wiki_collection_forget(
            "xnaut".into(),
            "adk".into(),
            "https://adk.dev/docs/a".into()
        )
        .is_err());
    }

    /// Write a REAL docs page into the REAL vault, with the same writer the
    /// pane calls. This is the end of the chain the ticket asks to be proved:
    /// fetch → extract → the file an owner can open.
    ///
    /// `#[ignore]` because it writes outside the repository and depends on a
    /// capture taken from the live internet; both are things a suite run must not
    /// do on its own. Run it deliberately:
    ///
    ///   node scripts/wiki-capture.mjs https://adk.dev/agents/llm-agents/ cap.json
    ///   XNAUT_WIKI_CAPTURE=$PWD/cap.json XNAUT_WIKI_PROJECT=xnaut \
    ///     cargo test --manifest-path src-tauri/Cargo.toml \
    ///     wiki::tests::record_the_real_page_from_a_capture -- --ignored --nocapture
    #[test]
    #[ignore = "writes the real vault from a live capture; run it on purpose"]
    fn record_the_real_page_from_a_capture() {
        let _lock = crate::vault::test_vault_lock();
        std::env::remove_var("XNAUT_TEST_VAULT");

        let capture = std::env::var("XNAUT_WIKI_CAPTURE")
            .expect("set XNAUT_WIKI_CAPTURE to the JSON from scripts/wiki-capture.mjs");
        let project = std::env::var("XNAUT_WIKI_PROJECT").unwrap_or_else(|_| "xnaut".into());
        let raw = std::fs::read_to_string(&capture).expect("read the capture");
        let page: serde_json::Value = serde_json::from_str(&raw).expect("the capture is JSON");
        let url = page["url"].as_str().expect("url").to_string();
        let slug = slug_for(&url);

        let written = wiki_collection_record(
            project,
            slug,
            url,
            page["title"].as_str().unwrap_or_default().to_string(),
            page["markdown"].as_str().unwrap_or_default().to_string(),
        )
        .expect("record the real page");

        println!("collection {}", written.name);
        println!("reference  {}", written.reference);
        println!("path       {}", written.path);
        println!("pages      {}", written.entries.len());
        assert!(std::path::Path::new(&written.path).is_file());
    }

    #[test]
    fn a_collection_path_cannot_climb_out_of_the_vault() {
        assert!(rel_for("../../etc", "adk").is_err());
        assert!(rel_for("xnaut", "../../etc/passwd").is_err());
        assert!(rel_for("", "adk").is_err());
        assert_eq!(
            rel_for("xnaut", "adk").unwrap(),
            "work/xnaut/Development/docu/adk-docu.md"
        );
    }
}
