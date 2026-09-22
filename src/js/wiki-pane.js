// The Wiki tab (XNAUT-438): a URL bar, and a docs page rendered as content.
//
// André, 2026-09-22: "a web-browser bar and that's it; paste a docs link like
// https://adk.dev/ and the docu page renders below, only the docu, not the
// entire website." Clarified the same evening. Two columns: the docs site's
// OWN navigation on the left, the article on the right, header/search/footer/
// marketing gone. Then: remember every page opened, per project, in a named
// collection `@adk-docu` that lives in the vault.
//
// Three things make that work, and they are worth naming because none of them
// is obvious:
//
//   1. THE FETCH GOES THROUGH RUST. The webview cannot read adk.dev itself.
//      CORS refuses a cross-origin read and no docs site sends the header that
//      would allow it. `wiki_fetch` hands back the HTML as a string and the
//      extraction happens here, on a document this page parsed itself.
//
//   2. THE EXTRACTOR IS MOZILLA READABILITY, bundled offline (see
//      js/vendor/readability.bundle.js and frontend/readability-entry.js for
//      the credit). Borrowed, not invented: it is what Firefox Reader View
//      runs, and it already knows the shapes docs sites come in.
//
//   3. THE NAVIGATION IS READ SEPARATELY, BEFORE Readability runs. Readability
//      MUTATES the document it is given and throws the navigation away; that
//      is its job. So the HTML is parsed TWICE: once to lift the site's own
//      sidebar out, once for Readability to consume. Two parses of a string is
//      cheaper than being clever, and a clone would have to be deep anyway.
//
// The collection file in the vault is the source of truth, not a cache. The
// history dropdown renders from it, a page reopens from it when the site is
// down, and `@adk-docu` typed into the bar reopens its last page.
(function () {
  'use strict';

  const invoke = (...a) => (window.__TAURI__ && window.__TAURI__.core
    ? window.__TAURI__.core.invoke(...a)
    : Promise.reject('Tauri is not available'));

  const esc = (s) => String(s == null ? '' : s)
    .replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  const clean = (s) => String(s == null ? '' : s).replace(/\s+/g, ' ').trim();

  // ───────────────────────────────────────────────────────────────────────────
  // What counts as "inside these docs"
  // ───────────────────────────────────────────────────────────────────────────

  // A docs site keeps its pages under one path prefix. Everything under that
  // prefix navigates INSIDE the tab; everything else is the rest of the web and
  // opens in the browser pane, which is what André asked for: the docu, not the
  // website. `https://adk.dev/docs/agents/llm` → root `/docs/`, so the sidebar's
  // links stay here and the "Pricing" link in the header does not.
  const ROOT_SEGMENTS = /^(docs?|documentation|guide|guides|reference|references|api|manual|manuals|learn|learning|handbook|wiki|tutorial|tutorials|book|en|latest)$/i;

  function docsRootOf(href) {
    let u;
    try { u = new URL(href); } catch (_) { return null; }
    const parts = u.pathname.split('/').filter(Boolean);
    let root = '/';
    for (let i = 0; i < parts.length; i += 1) {
      if (ROOT_SEGMENTS.test(parts[i])) { root = '/' + parts.slice(0, i + 1).join('/') + '/'; break; }
    }
    return { origin: u.origin, root };
  }

  function insideDocs(href, scope) {
    if (!scope) return false;
    let u;
    try { u = new URL(href); } catch (_) { return false; }
    if (u.origin !== scope.origin) return false;
    const path = u.pathname.endsWith('/') ? u.pathname : u.pathname + '/';
    return path.indexOf(scope.root) === 0 || (u.pathname + '/').indexOf(scope.root) === 0;
  }

  // Two URLs are the same page when they differ only by a fragment or a
  // trailing slash. Otherwise "the page you are on" is never marked, because
  // a sidebar writes `/docs/agents/` and the address bar holds `/docs/agents`.
  function samePage(a, b) {
    const norm = (x) => {
      try { const u = new URL(x); return u.origin + u.pathname.replace(/\/+$/, '') + u.search; }
      catch (_) { return String(x || ''); }
    };
    return norm(a) === norm(b);
  }

  // ───────────────────────────────────────────────────────────────────────────
  // Parsing
  // ───────────────────────────────────────────────────────────────────────────

  // A <base> is what makes `element.href` resolve against the page's own URL
  // instead of against this app. Without it every relative link in the docs
  // would resolve to http://127.0.0.1/… and the whole sidebar would leave the
  // docs scope and open in the browser pane.
  function parse(html, pageUrl) {
    const doc = new DOMParser().parseFromString(String(html || ''), 'text/html');
    const head = doc.head || doc.documentElement;
    const existing = doc.querySelector('base[href]');
    if (existing) {
      try { existing.setAttribute('href', new URL(existing.getAttribute('href'), pageUrl).toString()); }
      catch (_) { existing.setAttribute('href', pageUrl); }
    } else {
      const base = doc.createElement('base');
      base.setAttribute('href', pageUrl);
      head.insertBefore(base, head.firstChild);
    }
    return doc;
  }

  // ───────────────────────────────────────────────────────────────────────────
  // The left column: the site's own navigation
  // ───────────────────────────────────────────────────────────────────────────

  const NAV_CANDIDATES = [
    'nav', 'aside', '[role="navigation"]', '[role="doc-toc"]',
    '[class*="sidebar" i]', '[id*="sidebar" i]',
    '[class*="toc" i]', '[id*="toc" i]',
    '[class*="menu" i]', '[class*="nav" i]',
  ].join(',');

  const GROUP_TAGS = /^(H[1-6]|SUMMARY|STRONG|B|SPAN|P|BUTTON|LABEL|DT)$/;

  // Pick ONE container: the site's docs sidebar. Scored by how many in-scope
  // links it holds, because that is what separates a docs sidebar from the
  // header's five marketing links and the footer's twenty. Ties go to the
  // tighter element, so a sidebar wrapped in three divs contributes its
  // innermost copy rather than a wrapper that also swallows the header.
  function pickNavContainer(doc, scope) {
    let best = null;
    let bestCount = 0;
    let bestSize = Infinity;
    doc.querySelectorAll(NAV_CANDIDATES).forEach((el) => {
      let count = 0;
      el.querySelectorAll('a[href]').forEach((a) => { if (insideDocs(a.href, scope)) count += 1; });
      if (count < 2) return;
      const size = el.getElementsByTagName('*').length;
      if (count > bestCount || (count === bestCount && size < bestSize)) {
        best = el; bestCount = count; bestSize = size;
      }
    });
    return best;
  }

  // Walk the chosen container and keep the nesting the site already expresses
  // with <ul>. A group label (a heading that is not itself a link, sitting over
  // a list) is kept too. A docs sidebar reads as "Agents → LLM agents", and
  // dropping the "Agents" turns a tree into a flat pile of leaves.
  function collectNav(doc, pageUrl, scope) {
    const container = pickNavContainer(doc, scope);
    if (!container) return [];
    const out = [];
    const seen = new Set();

    const hasListNearby = (el) => {
      for (let n = el.nextElementSibling; n; n = n.nextElementSibling) {
        if (/^(UL|OL)$/.test(n.tagName) || n.querySelector('ul,ol,a[href]')) return true;
      }
      const parent = el.parentElement;
      return !!(parent && parent !== container && parent.querySelector('ul,ol'));
    };

    const walk = (node, depth) => {
      if (out.length > 400) return;
      for (const child of Array.from(node.children || [])) {
        const tag = child.tagName;
        if (tag === 'A') {
          const href = child.href;
          const label = clean(child.textContent);
          if (label && insideDocs(href, scope) && !seen.has(href)) {
            seen.add(href);
            out.push({ label, href, depth, current: samePage(href, pageUrl) });
          }
          continue;
        }
        if (tag === 'UL' || tag === 'OL') { walk(child, depth + 1); continue; }
        if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'SVG') continue;
        if (GROUP_TAGS.test(tag) && !child.querySelector('a[href]')) {
          const label = clean(child.textContent);
          // A real docs sidebar names a section twice: once as the link to the
          // section's own page, once as the heading over its children (adk.dev
          // does exactly this, and so the first version of this column read
          // "Get Started / Get Started / Python"). The second one carries no
          // information, so it is dropped.
          const last = out[out.length - 1];
          const repeat = last && last.label.toLowerCase() === label.toLowerCase();
          if (label && label.length < 60 && !repeat && hasListNearby(child)) {
            out.push({ label, href: '', depth });
          }
          continue;
        }
        walk(child, depth);
      }
    };
    walk(container, 0);

    // Normalise: the shallowest entry is level 0 however many <ul>s the site
    // wrapped its sidebar in, and nothing indents past four.
    const floor = out.reduce((m, e) => Math.min(m, e.depth), Infinity);
    out.forEach((e) => { e.depth = Math.min(4, e.depth - (Number.isFinite(floor) ? floor : 0)); });
    // A group label with nothing under it is noise, not structure.
    return out.filter((e, i) => e.href || out.slice(i + 1).some((n) => n.depth > e.depth && n.href));
  }

  // ───────────────────────────────────────────────────────────────────────────
  // The right column: the article
  // ───────────────────────────────────────────────────────────────────────────

  // Gone before Readability sees the page, because André named them: header,
  // search, footer, marketing. Readability drops most of this on its own, but
  // "most" is how a cookie banner ends up as the first paragraph of the docs.
  const CHROME = 'script,style,noscript,iframe,object,embed,template,form,header,footer,nav,aside,'
    + '[role="banner"],[role="contentinfo"],[role="search"],[role="navigation"],'
    + '[class*="cookie" i],[class*="banner" i],[class*="announce" i],[class*="newsletter" i],'
    + '[class*="breadcrumb" i],[class*="feedback" i],[class*="edit-this-page" i],[aria-hidden="true"]';

  // Readability's own threshold is 500 characters, which is tuned for news
  // articles. A docs page can legitimately be one paragraph and a code block,
  // so the floor is lower here, and then checked again on the result, because
  // a page that extracts to two sentences is a page that did not extract.
  const MIN_CHARS = 140;

  /**
   * Extract a docs page. Returns { title, html, nav } or null when the page is
   * not readable; the caller falls back to the browser pane and says so.
   */
  function extract(rawHtml, pageUrl) {
    const scope = docsRootOf(pageUrl);
    const source = parse(rawHtml, pageUrl);
    const nav = collectNav(source, pageUrl, scope);
    // The page's OWN heading, read before Readability deletes it. Readability
    // falls back to <title>, which on a docs site is "LLM agents, ADK": the
    // site name is in every entry of the history dropdown and in every heading
    // of the stored collection, for no information at all. Scoped to the main
    // content so a site whose <h1> is its logo does not name every page after
    // itself.
    const own = source.querySelector('article h1, main h1, [role="main"] h1');

    const Readability = window.XnautReadability && window.XnautReadability.Readability;
    if (!Readability) { console.warn('[wiki] the Readability bundle did not load'); return null; }

    // A SECOND parse, not the document above: Readability rewrites the DOM it
    // is handed, so sharing one would extract the article out from under the
    // navigation that was read from it.
    const doc = parse(rawHtml, pageUrl);
    doc.querySelectorAll(CHROME).forEach((el) => el.remove());
    let article = null;
    try {
      article = new Readability(doc, { charThreshold: MIN_CHARS, keepClasses: true }).parse();
    } catch (e) {
      console.warn('[wiki] Readability failed', e);
      return null;
    }
    if (!article || !article.content) return null;
    if (clean(article.textContent || '').length < MIN_CHARS) return null;

    const host = document.createElement('div');
    host.innerHTML = article.content;
    sanitize(host, pageUrl, scope);
    if (clean(host.textContent).length < MIN_CHARS) return null;

    // Readability MOVES the page's own <h1>, either dropping it or demoting it under an
    // <h2> under a title it took from <title>. Correct for a news site, where
    // the headline is chrome above the article; wrong here, where it is the
    // name of the API you came to read. So: take the page's own heading as the
    // title, delete whatever copy of it survived, and put one clean <h1> back.
    const strip = (s) => clean(s).replace(/[¶§#]+\s*$/, '').trim();
    const title = strip(own && own.textContent) || strip(article.title) || pageUrl;
    const first = host.querySelector('h1,h2,h3');
    if (first && strip(first.textContent) === title) first.remove();
    const h1 = document.createElement('h1');
    h1.textContent = title;
    host.insertBefore(h1, host.firstChild);
    return { title, html: host.innerHTML, nav };
  }

  // Everything that arrives from another host is untrusted markup. Scripts and
  // event handlers go; every link and image is made absolute so it resolves
  // against the docs site rather than against this app; and a link is tagged
  // with where it should open so the click handler does not have to re-decide.
  function sanitize(host, pageUrl, scope) {
    host.querySelectorAll('script,style,noscript,iframe,object,embed,form,input,button,link,meta')
      .forEach((el) => el.remove());
    // The pilcrow. Every MkDocs, Sphinx and Docusaurus heading carries a
    // permalink anchor whose text is ¶ or #; it is a hover affordance on the
    // site and pure noise in a reader, and it was ending up in the page title
    // ("Simple agents with LlmAgent¶") and therefore in the collection file.
    host.querySelectorAll('a.headerlink,a[class*="anchor" i],a[class*="permalink" i],a[aria-hidden="true"]')
      .forEach((el) => el.remove());
    host.querySelectorAll('*').forEach((el) => {
      Array.from(el.attributes).forEach((attr) => {
        const name = attr.name.toLowerCase();
        if (name.indexOf('on') === 0) el.removeAttribute(attr.name);
        if ((name === 'href' || name === 'src' || name === 'srcset')
          && /^\s*(javascript|data|vbscript):/i.test(attr.value)) el.removeAttribute(attr.name);
      });
    });
    host.querySelectorAll('a[href]').forEach((a) => {
      let abs = '';
      try { abs = new URL(a.getAttribute('href'), pageUrl).toString(); } catch (_) { /* keep it inert */ }
      a.removeAttribute('target');
      a.removeAttribute('rel');
      if (!abs) { a.removeAttribute('href'); return; }
      a.setAttribute('href', abs);
      a.setAttribute('data-wiki-link', insideDocs(abs, scope) ? 'in' : 'out');
    });
    host.querySelectorAll('img[src]').forEach((img) => {
      try { img.setAttribute('src', new URL(img.getAttribute('src'), pageUrl).toString()); }
      catch (_) { img.remove(); }
      img.removeAttribute('srcset');
      img.setAttribute('loading', 'lazy');
    });
  }

  // Code blocks in the app's own face. highlight.js is a CDN script, so it is
  // there online and absent offline; without it the block still renders in the
  // app's mono styling rather than disappearing.
  function highlight(host) {
    host.querySelectorAll('pre code').forEach((code) => {
      const cls = Array.from(code.classList).concat(Array.from(code.parentElement.classList));
      const found = cls.map((c) => (c.match(/^(?:language|lang)-(.+)$/) || [])[1]).find(Boolean);
      if (found) code.classList.add('language-' + found.toLowerCase());
      if (!window.hljs) return;
      try { window.hljs.highlightElement(code); } catch (_) { /* leave it plain */ }
    });
  }

  // ───────────────────────────────────────────────────────────────────────────
  // The article as Markdown: what gets stored, and what an offline reopen reads
  // ───────────────────────────────────────────────────────────────────────────

  // Reads the node, never writes it. The first version stripped nested lists
  // out of their <li> to serialise them separately and did it on the live
  // article, so storing a page deleted half of what was on screen. It walks a
  // clone now, and list nesting is carried as an argument rather than read back
  // off the tree; a detached node has no ancestors to count.
  function htmlToMarkdown(node) {
    const inline = (el, depth) => Array.from(el.childNodes).map((c) => walk(c, depth)).join('');

    function walk(n, depth) {
      if (n.nodeType === 3) return String(n.nodeValue).replace(/\s+/g, ' ');
      if (n.nodeType !== 1) return '';
      const tag = n.tagName;
      switch (tag) {
        case 'SCRIPT': case 'STYLE': case 'NOSCRIPT': return '';
        case 'BR': return '\n';
        case 'HR': return '\n\n---\n\n';
        case 'H1': case 'H2': case 'H3': case 'H4': case 'H5': case 'H6':
          return `\n\n${'#'.repeat(+tag[1])} ${clean(inline(n, depth))}\n\n`;
        case 'P': return `\n\n${clean(inline(n, depth))}\n\n`;
        case 'STRONG': case 'B': return `**${clean(inline(n, depth))}**`;
        case 'EM': case 'I': return `*${clean(inline(n, depth))}*`;
        case 'CODE':
          if (n.closest('pre')) return n.textContent;
          return '`' + clean(n.textContent) + '`';
        case 'PRE': {
          const code = n.querySelector('code');
          const cls = `${(code && code.className) || ''} ${n.className || ''}`;
          const lang = (cls.match(/(?:language|lang)-([\w+#-]+)/) || [])[1] || '';
          return `\n\n\`\`\`${lang.toLowerCase()}\n${(code || n).textContent.replace(/\n+$/, '')}\n\`\`\`\n\n`;
        }
        case 'BLOCKQUOTE':
          return '\n\n' + clean(inline(n, depth)).split('\n').map((l) => '> ' + l).join('\n') + '\n\n';
        case 'A': {
          const text = clean(inline(n, depth));
          const href = n.getAttribute('href') || '';
          return href && text ? `[${text}](${href})` : text;
        }
        case 'IMG': {
          const src = n.getAttribute('src') || '';
          return src ? `![${clean(n.getAttribute('alt') || '')}](${src})` : '';
        }
        case 'UL': case 'OL': {
          const ordered = tag === 'OL';
          const pad = '  '.repeat(depth);
          const body = Array.from(n.children).filter((c) => c.tagName === 'LI').map((li, i) => {
            const nested = Array.from(li.children).filter((c) => /^(UL|OL)$/.test(c.tagName));
            const head = clean(Array.from(li.childNodes)
              .filter((c) => nested.indexOf(c) === -1)
              .map((c) => walk(c, depth)).join(''));
            const tail = nested.map((c) => walk(c, depth + 1)).join('');
            return `${pad}${ordered ? `${i + 1}.` : '-'} ${head}${tail ? '\n' + tail.replace(/^\n+|\n+$/g, '') : ''}`;
          }).join('\n');
          return `\n\n${body}\n\n`;
        }
        case 'TABLE': {
          const rows = Array.from(n.querySelectorAll('tr'));
          if (!rows.length) return '';
          const cells = (tr) => Array.from(tr.children).map((c) => clean(inline(c, depth)).replace(/\|/g, '\\|'));
          const head = cells(rows[0]);
          const rest = rows.slice(1).map(cells).filter((r) => r.length);
          const line = (r) => `| ${r.join(' | ')} |`;
          return `\n\n${line(head)}\n${line(head.map(() => '---'))}\n${rest.map(line).join('\n')}\n\n`;
        }
        default: return inline(n, depth);
      }
    }

    return walk(node.cloneNode(true), 0)
      .replace(/[ \t]+\n/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
  }

  // The same article, minus the heading that names it. The collection file
  // gives every page its own `## Title` fence, so storing the body with the
  // title still on top printed it twice in a row in the vault, visible the
  // first time a real page was written.
  function storableMarkdown(el, title) {
    const copy = el.cloneNode(true);
    const first = copy.querySelector('h1');
    if (first && clean(first.textContent) === clean(title)) first.remove();
    return htmlToMarkdown(copy);
  }

  // ───────────────────────────────────────────────────────────────────────────
  // Styles
  // ───────────────────────────────────────────────────────────────────────────

  function injectStyles() {
    if (document.getElementById('xnaut-wiki-styles')) return;
    const s = document.createElement('style');
    s.id = 'xnaut-wiki-styles';
    s.textContent = `
.rpwk { display:flex; flex-direction:column; height:100%; min-height:0; }
.rpwk-bar { flex:0 0 auto; display:flex; gap:6px; align-items:center; padding:8px 12px; position:relative; }
.rpwk-url { flex:1 1 auto; min-width:0; height:28px; padding:2px 9px; border:1px solid var(--border); border-radius:var(--radius-md,7px); background:var(--input-bg,var(--secondary,#262626)); color:var(--foreground); font:inherit; font-size:12px; outline:none; }
.rpwk-url:focus { border-color:var(--xnaut-yellow); }
.rpwk-btn { height:28px; padding:0 10px; border:1px solid var(--border); border-radius:var(--radius-md,7px); background:transparent; color:var(--foreground); font:inherit; font-size:12px; cursor:pointer; white-space:nowrap; }
.rpwk-btn:hover { background:var(--accent,#2a2a2f); }
.rpwk-btn[disabled] { opacity:.45; cursor:default; }
.rpwk-scope { flex:0 0 auto; padding:0 12px 6px; font-size:10.5px; color:var(--muted-foreground); display:flex; gap:8px; align-items:baseline; }
.rpwk-scope b { color:var(--xnaut-yellow); font-weight:650; }
.rpwk-scope span { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.rpwk-menu { position:absolute; z-index:40; top:34px; right:12px; left:12px; max-height:340px; overflow-y:auto; border:1px solid var(--border); border-radius:var(--radius-md,8px); background:var(--popover,#1c1c20); box-shadow:0 12px 34px rgba(0,0,0,.45); padding:4px; }
.rpwk-menu-empty { padding:10px 10px 12px; font-size:11.5px; color:var(--muted-foreground); }
.rpwk-row { display:flex; align-items:center; gap:6px; border-radius:6px; }
.rpwk-row:hover { background:var(--accent,#2a2a2f); }
.rpwk-open { flex:1 1 auto; min-width:0; display:flex; flex-direction:column; gap:1px; align-items:flex-start; padding:6px 8px; border:0; background:transparent; color:inherit; font:inherit; text-align:left; cursor:pointer; }
.rpwk-open b { font-size:12px; font-weight:600; max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.rpwk-open i { font-style:normal; font-size:10.5px; color:var(--muted-foreground); max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.rpwk-pin, .rpwk-forget { flex:0 0 auto; width:24px; height:24px; margin-right:4px; border:0; border-radius:6px; background:transparent; color:var(--muted-foreground); font-size:12px; cursor:pointer; }
.rpwk-pin:hover, .rpwk-forget:hover { background:var(--secondary,#262626); color:var(--foreground); }
.rpwk-pin.on { color:var(--xnaut-yellow); }
.rpwk-note { flex:0 0 auto; margin:0 12px 8px; padding:7px 10px; border:1px solid var(--border); border-left:2px solid var(--xnaut-yellow); border-radius:var(--radius-md,7px); background:var(--secondary,#242428); color:var(--foreground); font-size:11.5px; line-height:1.5; }
.rpwk-note a { color:var(--accent,#4f8cff); cursor:pointer; text-decoration:underline; }
.rpwk-cols { flex:1 1 auto; min-height:0; display:flex; border-top:1px solid var(--border); }
.rpwk-nav { flex:0 0 220px; min-width:0; overflow-y:auto; padding:10px 6px 24px; border-right:1px solid var(--border); background:rgba(255,255,255,.015); }
.rpwk-nav:empty { display:none; }
.rpwk-nav a, .rpwk-nav .rpwk-group { display:block; padding:3px 8px; border-radius:5px; font-size:11.5px; line-height:1.45; color:var(--muted-foreground); text-decoration:none; cursor:pointer; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.rpwk-nav a:hover { background:var(--accent,#2a2a2f); color:var(--foreground); }
.rpwk-nav a.current { color:var(--foreground); font-weight:650; background:var(--secondary,#262626); box-shadow:inset 2px 0 0 var(--xnaut-yellow); }
.rpwk-nav .rpwk-group { color:var(--foreground); font-weight:650; font-size:10.5px; letter-spacing:.04em; text-transform:uppercase; margin-top:10px; cursor:default; }
.rpwk-article { flex:1 1 auto; min-width:0; overflow-y:auto; padding:18px 26px 60px; }
.rpwk-article img { max-width:100%; height:auto; border-radius:6px; }
.rpwk-article pre { overflow-x:auto; }
.rpwk-article table { display:block; overflow-x:auto; }
.rpwk-empty { flex:1 1 auto; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:8px; padding:30px 22px; text-align:center; color:var(--muted-foreground); font-size:12px; line-height:1.55; }
.rpwk-empty h3 { margin:0; font-size:13px; color:var(--foreground); font-weight:650; }
.rpwk-empty p { margin:0; max-width:38ch; }
`;
    document.head.appendChild(s);
  }

  // ───────────────────────────────────────────────────────────────────────────
  // The view
  // ───────────────────────────────────────────────────────────────────────────

  const URL_KEY = 'xnaut-wiki-url';
  const basename = (p) => String(p || '').replace(/\/+$/, '').split('/').pop() || '';

  // The project the collection belongs to, as the vault spells it. A PM project
  // whose source_path IS this root is the answer when there is one; otherwise
  // the directory's own name, which is right for a plain checkout and visible
  // in the bar either way, so a wrong guess is a thing the owner can see rather
  // than a file that lands somewhere surprising.
  async function projectNameFor(root) {
    const norm = (p) => String(p || '').replace(/\/+$/, '');
    try {
      const projects = (await invoke('pm_project_list')) || [];
      const hit = projects.find((p) => p.source_path && norm(p.source_path) === norm(root));
      if (hit && hit.key) return String(hit.key).toLowerCase();
    } catch (_) { /* no PM module on this machine; the directory name stands */ }
    return basename(root).toLowerCase();
  }

  function openElsewhere(url) {
    if (typeof window.xnautAttachBrowserTab === 'function') { window.xnautAttachBrowserTab(url); return true; }
    if (typeof window.xnautOpenUrl === 'function') { window.xnautOpenUrl(url); return true; }
    return false;
  }

  function create() {
    let container = null;
    let root = '';
    let project = '';
    let slug = '';
    let collection = null;     // { name, reference, path, entries }
    let currentUrl = '';
    let busy = false;

    const urlKey = () => (root ? `${URL_KEY}:${root}` : URL_KEY);
    const els = () => (container ? {
      input: container.querySelector('[data-wiki-url]'),
      go: container.querySelector('[data-wiki-go]'),
      hist: container.querySelector('[data-wiki-hist]'),
      menu: container.querySelector('[data-wiki-menu]'),
      note: container.querySelector('[data-wiki-note]'),
      scope: container.querySelector('[data-wiki-scope]'),
      nav: container.querySelector('[data-wiki-nav]'),
      article: container.querySelector('[data-wiki-article]'),
    } : null);

    function page() {
      return `<div class="rpwk">
        <div class="rpwk-bar">
          <input class="rpwk-url" data-wiki-url type="text" spellcheck="false"
                 placeholder="Paste a docs link (https://adk.dev/), or @adk-docu" />
          <button class="rpwk-btn" data-wiki-hist title="Pages opened in this project">History ▾</button>
          <button class="rpwk-btn" data-wiki-go>Open</button>
          <div class="rpwk-menu" data-wiki-menu hidden></div>
        </div>
        <div class="rpwk-scope" data-wiki-scope hidden></div>
        <div class="rpwk-note" data-wiki-note hidden></div>
        <div class="rpwk-cols">
          <nav class="rpwk-nav" data-wiki-nav></nav>
          <article class="rpwk-article xnaut-md" data-wiki-article>
            <div class="rpwk-empty">
              <h3>The docs, and nothing else</h3>
              <p>Paste a documentation link. The page is fetched, the article is extracted and
                 rendered here with the site's own navigation on the left. No header, no search,
                 no footer.</p>
              <p>Every page you open is kept in this project's collection, so it reopens in one
                 click and still reads when the site is down.</p>
            </div>
          </article>
        </div>
      </div>`;
    }

    function note(html) {
      const e = els(); if (!e) return;
      e.note.hidden = !html;
      e.note.innerHTML = html || '';
      if (html) {
        const link = e.note.querySelector('[data-wiki-note-open]');
        if (link) link.onclick = () => openElsewhere(link.getAttribute('data-wiki-note-open'));
      }
    }

    function scopeLine() {
      const e = els(); if (!e) return;
      if (!collection) { e.scope.hidden = true; return; }
      e.scope.hidden = false;
      e.scope.innerHTML = `<b>${esc(collection.name)}</b><span>${esc(collection.reference)}</span>`
        + `<span>${collection.entries.length} page${collection.entries.length === 1 ? '' : 's'}</span>`;
    }

    function renderNav(entries) {
      const e = els(); if (!e) return;
      if (!entries || !entries.length) { e.nav.innerHTML = ''; return; }
      e.nav.innerHTML = entries.map((n) => (n.href
        ? `<a data-wiki-navlink="${esc(n.href)}" class="${n.current ? 'current' : ''}"
             style="padding-left:${8 + n.depth * 11}px" title="${esc(n.href)}">${esc(n.label)}</a>`
        : `<div class="rpwk-group" style="padding-left:${8 + n.depth * 11}px">${esc(n.label)}</div>`)).join('');
      e.nav.querySelectorAll('[data-wiki-navlink]').forEach((a) => {
        a.onclick = (ev) => { ev.preventDefault(); open(a.getAttribute('data-wiki-navlink')); };
      });
      const current = e.nav.querySelector('a.current');
      if (current && current.scrollIntoView) current.scrollIntoView({ block: 'nearest' });
    }

    // Links inside the article: the docs navigate HERE, everything else is the
    // rest of the web and belongs in the browser pane.
    function wireArticleLinks() {
      const e = els(); if (!e) return;
      e.article.querySelectorAll('a[href]').forEach((a) => {
        const href = a.getAttribute('href');
        a.onclick = (ev) => {
          ev.preventDefault();
          if (a.getAttribute('data-wiki-link') === 'in') open(href);
          else if (!openElsewhere(href)) window.open(href, '_blank');
        };
      });
    }

    async function refreshCollection() {
      if (!project || !slug) { collection = null; scopeLine(); return; }
      try { collection = await invoke('wiki_collection_read', { project, slug }); }
      catch (e) { collection = null; console.warn('[wiki] could not read the collection', e); }
      scopeLine();
    }

    function setBusy(on) {
      busy = on;
      const e = els(); if (!e) return;
      e.go.disabled = on;
      e.go.textContent = on ? 'Opening…' : 'Open';
    }

    /** Render a stored article: the offline path, and what `@adk-docu` reopens. */
    function renderStored(entry, why) {
      const e = els(); if (!e) return;
      currentUrl = entry.url;
      e.input.value = entry.url;
      // The stored body carries no title, because the collection file names
      // it in the fence, so the heading goes back on for reading.
      const stored = String(entry.markdown || '');
      const md = /^#\s/.test(stored) ? stored : `# ${entry.title || entry.url}\n\n${stored}`;
      e.article.innerHTML = window.xnautMarkdown
        ? window.xnautMarkdown.render(md)
        : `<pre>${esc(md)}</pre>`;
      // The stored copy is Markdown, so its links carry no scope tag. Tag them
      // here or every link in an offline page would leave for the browser pane,
      // including the ones whose stored copy is sitting in the same file.
      const scope = docsRootOf(entry.url);
      e.article.querySelectorAll('a[href]').forEach((a) => {
        a.setAttribute('data-wiki-link', insideDocs(a.getAttribute('href'), scope) ? 'in' : 'out');
      });
      highlight(e.article);
      wireArticleLinks();
      renderNav([]);
      note(`${esc(why)} Showing the copy stored in <b>${esc(collection ? collection.name : 'the collection')}</b>`
        + ` from ${esc((entry.opened_at || '').slice(0, 16).replace('T', ' '))} UTC.`
        + ` <a data-wiki-note-open="${esc(entry.url)}">Open the live page in the browser</a>`);
    }

    /**
     * Open a docs page. `@slug-docu` reopens that collection's last page; a
     * URL is fetched, extracted, rendered, and recorded.
     */
    async function open(raw) {
      const e = els(); if (!e || busy) return;
      const typed = String(raw == null ? e.input.value : raw).trim();
      if (!typed) return;

      // `@adk-docu`: the collection by name, reopening where it left off.
      if (typed.charAt(0) === '@') {
        const wanted = typed.slice(1).replace(/-docu$/, '').toLowerCase();
        if (!wanted) return;
        slug = wanted;
        await refreshCollection();
        const last = collection && collection.entries[0];
        if (!last) {
          note(`Nothing in <b>@${esc(wanted)}-docu</b> yet. Paste a page from that site to start it.`);
          return;
        }
        await open(last.url);
        return;
      }

      const url = /^[a-z][a-z0-9+.-]*:/i.test(typed) ? typed : 'https://' + typed;
      setBusy(true);
      note('');
      let fetched = null;
      let failure = '';
      try { fetched = await invoke('wiki_fetch', { url }); }
      catch (err) { failure = String((err && err.message) || err); }

      // Offline, or the site is down: the collection is the source of truth, so
      // a page that was read once still reads now.
      if (!fetched) {
        const nextSlug = await slugOf(url);
        if (nextSlug && nextSlug !== slug) { slug = nextSlug; await refreshCollection(); }
        const stored = (collection && collection.entries.find((x) => samePage(x.url, url))) || null;
        setBusy(false);
        if (stored) { renderStored(stored, `${failure}.`); return; }
        note(`Could not open ${esc(url)}: ${esc(failure)}.`
          + ` <a data-wiki-note-open="${esc(url)}">Try it in the browser pane</a>`);
        return;
      }

      const finalUrl = fetched.url || url;
      const article = extract(fetched.html, finalUrl);
      if (!article) {
        setBusy(false);
        const opened = openElsewhere(finalUrl);
        note(`${esc(finalUrl)} has no article the Wiki can extract, so it opened`
          + (opened ? ' in the browser pane instead.' : ' nowhere; the browser pane is unavailable.')
          + (opened ? '' : ` <a data-wiki-note-open="${esc(finalUrl)}">Open it</a>`));
        return;
      }

      currentUrl = finalUrl;
      e.input.value = finalUrl;
      e.article.innerHTML = article.html;
      highlight(e.article);
      wireArticleLinks();
      renderNav(article.nav);
      try { localStorage.setItem(urlKey(), finalUrl); } catch (_) { /* private mode */ }

      // Record it. A failure here is worth saying out loud; the collection is
      // the feature, not a cache, and silently not writing it is the bug this
      // line exists to prevent.
      slug = (await slugOf(finalUrl)) || slug;
      if (project && slug) {
        try {
          collection = await invoke('wiki_collection_record', {
            project, slug, url: finalUrl, title: article.title,
            markdown: storableMarkdown(e.article, article.title),
          });
          scopeLine();
        } catch (err) {
          note(`Rendered, but not recorded: ${esc(String((err && err.message) || err))}`);
        }
      }
      setBusy(false);
    }

    // Asked of Rust rather than derived here. The slug IS the filename of the
    // collection, so two implementations of the rule would eventually write two
    // files for one docs site and neither would hold all the pages.
    // `wiki_slug` touches nothing but the string, so it answers offline too.
    async function slugOf(url) {
      try { return (await invoke('wiki_slug', { url })) || ''; }
      catch (err) { console.warn('[wiki] could not name the collection', err); return ''; }
    }

    function closeMenu() { const e = els(); if (e) e.menu.hidden = true; }

    async function toggleMenu() {
      const e = els(); if (!e) return;
      if (!e.menu.hidden) { closeMenu(); return; }
      await refreshCollection();
      const entries = (collection && collection.entries) || [];
      if (!entries.length) {
        e.menu.innerHTML = `<div class="rpwk-menu-empty">No pages yet${collection ? ` in ${esc(collection.name)}` : ''}.
          Open a docs link and it is kept here.</div>`;
      } else {
        e.menu.innerHTML = entries.map((x, i) => `<div class="rpwk-row">
          <button class="rpwk-pin${x.pinned ? ' on' : ''}" data-wiki-pin="${i}"
                  title="${x.pinned ? 'Unpin' : 'Pin to the top'}">${x.pinned ? '★' : '☆'}</button>
          <button class="rpwk-open" data-wiki-reopen="${i}">
            <b>${esc(x.title || x.url)}</b><i>${esc(x.url)}</i></button>
          <button class="rpwk-forget" data-wiki-forget="${i}" title="Forget this page">✕</button>
        </div>`).join('');
      }
      e.menu.hidden = false;
      e.menu.querySelectorAll('[data-wiki-reopen]').forEach((b) => {
        b.onclick = () => { closeMenu(); open(entries[+b.dataset.wikiReopen].url); };
      });
      e.menu.querySelectorAll('[data-wiki-pin]').forEach((b) => {
        b.onclick = async () => {
          const entry = entries[+b.dataset.wikiPin];
          try {
            collection = await invoke('wiki_collection_pin', { project, slug, url: entry.url, pinned: !entry.pinned });
            scopeLine(); closeMenu(); toggleMenu();
          } catch (err) { note(`Could not pin: ${esc(String((err && err.message) || err))}`); }
        };
      });
      e.menu.querySelectorAll('[data-wiki-forget]').forEach((b) => {
        b.onclick = async () => {
          const entry = entries[+b.dataset.wikiForget];
          try {
            collection = await invoke('wiki_collection_forget', { project, slug, url: entry.url });
            scopeLine(); closeMenu(); toggleMenu();
          } catch (err) { note(`Could not forget: ${esc(String((err && err.message) || err))}`); }
        };
      });
    }

    function wire() {
      const e = els(); if (!e) return;
      e.go.onclick = () => open();
      e.hist.onclick = () => toggleMenu();
      e.input.onkeydown = (ev) => { if (ev.key === 'Enter') { ev.preventDefault(); closeMenu(); open(); } };
      e.input.onfocus = () => closeMenu();
    }

    /** Called when the tab is shown: restore the project's remembered page. */
    async function load() {
      const e = els(); if (!e) return;
      project = root ? await projectNameFor(root) : '';
      let remembered = '';
      try { remembered = localStorage.getItem(urlKey()) || ''; } catch (_) { /* private mode */ }
      if (remembered) slug = await slugOf(remembered);
      await refreshCollection();
      if (remembered && !currentUrl) { e.input.value = remembered; await open(remembered); }
    }

    return {
      page,
      mount(el, initialRoot) {
        injectStyles();
        container = el;
        root = initialRoot || '';
        container.innerHTML = page();
        wire();
        return load();
      },
      setRoot(next) {
        if (next === root) return Promise.resolve();
        root = next || '';
        currentUrl = ''; collection = null; slug = '';
        const e = els();
        if (e) { e.input.value = ''; e.article.innerHTML = ''; renderNav([]); note(''); }
        return load();
      },
      // Used when the Wiki lives inside another view's markup (the Build run
      // pane's sub-tabs), where the page HTML is rendered by the host.
      attach(el, initialRoot) {
        injectStyles();
        container = el;
        root = initialRoot || '';
        wire();
        return load();
      },
      load,
      open,
      currentUrl: () => currentUrl,
      collection: () => collection,
    };
  }

  window.xnautWiki = {
    create,
    extract,
    docsRootOf,
    insideDocs,
    htmlToMarkdown,
    storableMarkdown,
    injectStyles,
  };
})();
