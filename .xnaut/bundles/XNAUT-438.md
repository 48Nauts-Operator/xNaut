# XNAUT-438 · Wiki tab in the Build run pane

A sub-menu before History called Wiki: a URL bar, and a docs page rendered as
content only. The site's own navigation on the left, the article on the right,
header/search/footer/marketing gone. Every page opened is recorded per project
in a named vault collection `@<slug>-docu`, which the URL bar's history dropdown
reads and which renders the page offline when the site is down.

Design: `work:xnaut/Development/features/2026-09-22_Wiki-Tab-In-The-Build-Run-Pane.md`

## What changed

**Backend, `src-tauri/src/wiki.rs` (new).** Six commands.

- `wiki_fetch` gets the HTML in Rust, because CORS makes the webview unable to.
  Narrow on purpose: `check_url` refuses anything that is not http or https, so
  `file:///etc/passwd` cannot be typed into a URL bar that fetches with the
  app's own permissions. Plus an 8 MB cap, an HTML content type, and a browser
  User-Agent, without which most docs CDNs answer 403.
- `wiki_slug` names a collection from a host: `adk.dev` becomes `adk`. The rule
  lives here and the pane asks for it, because the slug is the collection's
  filename and two implementations would eventually write two files for one
  site.
- `wiki_collection_read` / `_record` / `_pin` / `_forget` read and write ONE
  Markdown file per project and docs site, at
  `work/<Project>/Development/docu/<slug>-docu.md`. Each page is fenced by an
  HTML comment carrying url/opened/pinned, with the extracted article between
  the fences as Markdown, so the file round-trips and still reads as a vault
  note. Pinned first then newest first; capped at 300 pages or 4 MB of stored
  article, whichever comes first.

Registered in `main.rs` and granted by a new `allow-wiki` block in
`permissions/default.toml`, added to `allow-all-commands`.

**Frontend, `src/js/wiki-pane.js` (new), `window.xnautWiki`.** Parses the
fetched HTML TWICE: once to lift the site's own sidebar out, once for
Readability, which mutates the document it is given and throws the navigation
away. Left column: the candidate `nav`/`aside`/`.sidebar`/`.toc` with the most
in-scope links wins, walked so the site's `<ul>` nesting and its group headings
survive, current page marked. Right column: chrome stripped before Readability
sees it, then sanitised (no scripts, no event handlers, absolute links and
images, no permalink pilcrows) and rendered in the app's `xnaut-md` type with
code blocks highlighted. In-docs links navigate in the tab; other hosts open the
browser pane; a page with no article opens there too and says why.

**Readability, bundled offline.** `src/js/vendor/readability.bundle.js`, built
from `frontend/readability-entry.js` by `npm run build:wiki`. Borrowed work:
`@mozilla/readability` 0.6.0, Apache 2.0, (c) 2010 Arc90 Inc, credited in the
entry file and in the bundle's banner. Bundled like the loops editor, because a
release build embeds the frontend and the tab has to work offline.

**The tab.** `right-pane-workspace.js` sub-tabs are now
Plan · Looms · Output · **Wiki** · History; `index.html` loads the bundle and the
pane before it.

**`scripts/wiki-capture.mjs` (new)** runs the shipped extractor against a live
docs page outside the app, with the same fetch and the same `wiki-pane.js`. It
is how the three real-site defects below were found.

## Proved against the real site

`node scripts/wiki-capture.mjs https://adk.dev/agents/llm-agents/` gives:
title *Simple agents with LlmAgent*, 152 nav links in 11 groups, 78 KB of
article Markdown, no pilcrows. Two real pages are recorded in the vault by the
app's own writer at
`/Users/zelda/.xnaut-vault/work/xnaut/Development/docu/adk-docu.md`
(`wiki::tests::record_the_real_page_from_a_capture`, `#[ignore]`d because it
writes outside the repository from a live capture).

Three defects the fixture had not shown, all now fixed and covered by tests:
adk.dev names every sidebar section twice (a link and a heading), Readability's
title carries the site name, and MkDocs headings carry a `¶` permalink that was
landing in the collection's headings.

## Verify by hand

```bash
cd <worktree>/src-tauri && cargo tauri dev
```

1. Open the right pane's Workspace view with a project open. The sub-tabs read
   Plan · Looms · Output · Wiki · History.
2. Wiki, paste `https://adk.dev/agents/llm-agents/`, Open. The article renders
   on the right with its code blocks; the site's sidebar is on the left, nested,
   with the current page marked. No header, search, cookie banner or footer.
3. Click a sidebar entry: it loads in the tab. Click an off-site link in the
   prose: it opens in a browser pane.
4. The line under the bar names the collection and its vault path. Open that
   file: the page is in it, as Markdown, under its own `## Title`.
5. History ▾ lists the pages; ★ pins one; clicking a row reopens it.
6. Turn the network off and Open the same URL: it renders from the stored copy
   and says so.
7. Type `@adk-docu` and Open: the last page of that collection comes back.

The extractor alone, against any site, without the app:

```bash
node scripts/wiki-capture.mjs https://docs.python.org/3/library/asyncio.html
```

## Totals

- `cargo test --manifest-path src-tauri/Cargo.toml` : 1262 passed, 0 failed, 46 ignored
- `XNAUT_TEST_PORT=4291 npx playwright test` : 246 passed, 0 failed, 10 of them
  `tests/wiki-tab.spec.mjs`

XNAUT_TEST_TOTALS={"rust":[{"passed":1262,"failed":0,"ignored":46}],"ui":[246]}

## Not done, on purpose

- No composer mention for `@adk-docu`. No mention system exists in the composer
  today, and the ticket says so. The vault file is what one would point at.
- No client-rendered docs sites: an empty shell filled by JavaScript has nothing
  to extract and opens in the browser pane. Rendering it would mean shipping a
  second browser.
- When no PM project's `source_path` matches the open root, the collection is
  filed under the directory's own name. The collection name and its vault path
  are shown in the tab, so a wrong guess is visible rather than surprising.

## Pre-existing, untouched

`node scripts/hygiene-check.mjs` fails on two things that predate this branch
and live in files it does not touch: three unattributed test fns
(`foundation.rs`, `main.rs`, `veto.rs`) and an undefined identifier at
`vault-pane.js:491`.
