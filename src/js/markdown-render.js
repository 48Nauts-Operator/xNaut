// Shared dependency-free Markdown renderer (window.xnautMarkdown).
//
// Used by plan-pane.js and markdown-pane.js. Deliberately avoids the CDN ESM
// TipTap path, which fails under this WebKit ("Can't find variable: Editor").
// Covers what real docs use: headings, bold/italic/code, links, lists + task
// checkboxes, blockquotes, hr, fenced code (syntax-highlighted via the global
// highlight.js UMD), tables, and ```mermaid diagrams (lazy UMD mermaid).
(function () {
  'use strict';

  const esc = (s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));

  // Shared rendered-markdown styling — apply class "xnaut-md" to a container.
  function injectStyles() {
    if (typeof document === 'undefined' || document.getElementById('xnaut-md-styles')) return;
    const st = document.createElement('style');
    st.id = 'xnaut-md-styles';
    st.textContent = `
.xnaut-md { color:var(--text, #d7dae0); font-family:-apple-system,"SF Pro Text",Segoe UI,Roboto,sans-serif; font-size:14px; line-height:1.65; }
.xnaut-md h1 { font-size:24px; font-weight:700; margin:0 0 20px; padding-bottom:10px; border-bottom:1px solid var(--border, rgba(255,255,255,.1)); }
.xnaut-md h2 { font-size:19px; font-weight:650; margin:34px 0 12px; padding-bottom:6px; border-bottom:1px solid var(--border, rgba(255,255,255,.08)); }
.xnaut-md h3 { font-size:16px; font-weight:600; margin:28px 0 10px; color:var(--agent-thinking, #4dffd0); }
.xnaut-md h4 { font-size:14px; font-weight:600; margin:22px 0 8px; }
.xnaut-md h1:first-child, .xnaut-md h2:first-child, .xnaut-md h3:first-child, .xnaut-md h4:first-child { margin-top:0; }
.xnaut-md p { margin:0 0 12px; }
.xnaut-md ul, .xnaut-md ol { margin:0 0 12px; padding-left:24px; }
.xnaut-md li { margin:3px 0; }
.xnaut-md li.task { list-style:none; margin-left:-20px; }
.xnaut-md li.task input { margin-right:8px; vertical-align:middle; accent-color:var(--agent-thinking, #4dffd0); }
.xnaut-md a { color:var(--accent, #4f8cff); text-decoration:none; }
.xnaut-md a:hover { text-decoration:underline; }
.xnaut-md code { font-family:"SF Mono",Menlo,monospace; font-size:.88em; background:var(--input-bg, rgba(255,255,255,.06)); padding:1px 5px; border-radius:4px; }
.xnaut-md pre { background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border, rgba(255,255,255,.1)); border-radius:8px; padding:12px 14px; overflow:auto; margin:0 0 14px; }
.xnaut-md pre code { background:none; padding:0; font-size:12.5px; line-height:1.5; }
.xnaut-md blockquote { margin:0 0 12px; padding:4px 14px; border-left:3px solid var(--agent-thinking, #4dffd0); color:var(--text-muted, #9aa0aa); }
.xnaut-md hr { border:none; border-top:1px solid var(--border, rgba(255,255,255,.12)); margin:20px 0; }
.xnaut-md table { border-collapse:separate; border-spacing:0; width:100%; margin:16px 0 22px; font-size:13px; line-height:1.5; border:1px solid var(--border, rgba(255,255,255,.14)); border-radius:8px; overflow:hidden; background:rgba(255,255,255,.025); }
.xnaut-md th, .xnaut-md td { border-right:1px solid var(--border, rgba(255,255,255,.1)); border-bottom:1px solid var(--border, rgba(255,255,255,.08)); padding:8px 11px; text-align:left; vertical-align:top; }
.xnaut-md th:last-child, .xnaut-md td:last-child { border-right:none; }
.xnaut-md tbody tr:last-child td { border-bottom:none; }
.xnaut-md th { background:rgba(255,255,255,.08); color:var(--text-primary, #f0f2f5); font-weight:650; }
.xnaut-md tbody tr:nth-child(odd) { background:rgba(255,255,255,.025); }
.xnaut-md tbody tr:hover { background:rgba(79,140,255,.09); }
.xnaut-md td code { white-space:nowrap; }
.xnaut-md .mermaid { margin:0 0 14px; text-align:center; background:var(--input-bg, rgba(255,255,255,.04)); border:1px solid var(--border, rgba(255,255,255,.08)); border-radius:8px; padding:14px; overflow:auto; }
.xnaut-md .mermaid svg { max-width:100%; height:auto; }
`;
    document.head.appendChild(st);
  }
  injectStyles();

  function highlightCode(code, lang) {
    const hljs = (typeof window !== 'undefined') && window.hljs;
    if (hljs) {
      try {
        const valid = lang && hljs.getLanguage(lang) ? lang : null;
        const html = valid ? hljs.highlight(code, { language: valid }).value : esc(code);
        return `<pre><code class="hljs${valid ? ' language-' + valid : ''}">${html}</code></pre>`;
      } catch (_) { /* fall through */ }
    }
    return `<pre><code>${esc(code)}</code></pre>`;
  }

  // Lazy-load mermaid as a UMD <script> (same approach as highlight.js).
  let mermaidPromise = null;
  function loadMermaid() {
    if (mermaidPromise) return mermaidPromise;
    mermaidPromise = new Promise((resolve, reject) => {
      if (window.mermaid) return resolve(window.mermaid);
      const s = document.createElement('script');
      s.src = 'https://cdn.jsdelivr.net/npm/mermaid@10.9.1/dist/mermaid.min.js';
      s.onload = () => {
        try { window.mermaid.initialize({ startOnLoad: false, theme: 'dark', securityLevel: 'loose' }); } catch (_) { /* defaults */ }
        resolve(window.mermaid);
      };
      s.onerror = () => reject(new Error('mermaid failed to load'));
      document.head.appendChild(s);
    });
    return mermaidPromise;
  }

  // Render any not-yet-rendered .mermaid blocks inside a view element to SVG.
  async function enhance(view) {
    const nodes = Array.from(view.querySelectorAll('.mermaid:not([data-rendered])'));
    if (!nodes.length) return;
    nodes.forEach((n) => n.setAttribute('data-rendered', '1'));
    try {
      const m = await loadMermaid();
      await m.run({ nodes });
    } catch (e) {
      console.warn('[markdown-render] mermaid failed', e);
      nodes.forEach((n) => { n.style.whiteSpace = 'pre'; n.style.fontFamily = 'monospace'; });
    }
  }

  function render(src) {
    const inline = (s) => esc(s)
      .replace(/`([^`]+)`/g, '<code>$1</code>')
      .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
      .replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<em>$2</em>')
      .replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2">$1</a>');
    const lines = String(src).replace(/\r\n/g, '\n').split('\n');
    const out = [];
    let i = 0;
    const isBlockStart = (l) => /^(#{1,6}\s|```|\s*>|\s*([-*+]|\d+\.)\s|\s*([-*_])\3\3)/.test(l) || /\|/.test(l);
    while (i < lines.length) {
      const line = lines[i];
      if (/^```/.test(line)) {
        const lang = line.replace(/^```/, '').trim().toLowerCase();
        const buf = [];
        i++;
        while (i < lines.length && !/^```/.test(lines[i])) { buf.push(lines[i]); i++; }
        i++;
        const code = buf.join('\n');
        if (lang === 'mermaid') out.push(`<div class="mermaid">${esc(code)}</div>`);
        else out.push(highlightCode(code, lang));
        continue;
      }
      if (/\|/.test(line) && i + 1 < lines.length && /\|/.test(lines[i + 1]) && /^[\s|:-]*-[\s|:-]*$/.test(lines[i + 1])) {
        const parseRow = (l) => l.replace(/^\s*\|/, '').replace(/\|\s*$/, '').split('|').map((c) => c.trim());
        const headers = parseRow(line);
        i += 2;
        const rows = [];
        while (i < lines.length && /\|/.test(lines[i]) && lines[i].trim() !== '') { rows.push(parseRow(lines[i])); i++; }
        let t = '<table><thead><tr>' + headers.map((h) => `<th>${inline(h)}</th>`).join('') + '</tr></thead><tbody>';
        t += rows.map((r) => '<tr>' + r.map((c) => `<td>${inline(c)}</td>`).join('') + '</tr>').join('');
        out.push(t + '</tbody></table>');
        continue;
      }
      let m;
      if ((m = line.match(/^(#{1,6})\s+(.*)$/))) { const lv = m[1].length; out.push(`<h${lv}>${inline(m[2])}</h${lv}>`); i++; continue; }
      if (/^\s*([-*_])\1\1+\s*$/.test(line)) { out.push('<hr>'); i++; continue; }
      if (/^\s*>\s?/.test(line)) {
        const buf = [];
        while (i < lines.length && /^\s*>\s?/.test(lines[i])) { buf.push(lines[i].replace(/^\s*>\s?/, '')); i++; }
        out.push(`<blockquote>${inline(buf.join(' '))}</blockquote>`);
        continue;
      }
      if (/^\s*([-*+]|\d+\.)\s+/.test(line)) {
        const ordered = /^\s*\d+\.\s+/.test(line);
        const items = [];
        while (i < lines.length && /^\s*([-*+]|\d+\.)\s+/.test(lines[i])) {
          const item = lines[i].replace(/^\s*([-*+]|\d+\.)\s+/, '');
          const task = item.match(/^\[([ xX])\]\s+(.*)$/);
          if (task) items.push(`<li class="task"><input type="checkbox" disabled ${task[1].toLowerCase() === 'x' ? 'checked' : ''}> ${inline(task[2])}</li>`);
          else items.push(`<li>${inline(item)}</li>`);
          i++;
        }
        out.push(`<${ordered ? 'ol' : 'ul'}>${items.join('')}</${ordered ? 'ol' : 'ul'}>`);
        continue;
      }
      if (line.trim() === '') { i++; continue; }
      const buf = [line];
      i++;
      while (i < lines.length && lines[i].trim() !== '' && !isBlockStart(lines[i])) { buf.push(lines[i]); i++; }
      out.push(`<p>${inline(buf.join(' '))}</p>`);
    }
    return out.join('\n');
  }

  // Real markdown engine — marked (UMD <script>, reliable in this WebKit; the
  // ESM path is what broke TipTap). Gives GFM tables/task-lists/strikethrough
  // and raw-HTML passthrough (so docs that embed <h1>/<strong>/<code> render
  // instead of showing literal tags), matching how Orca renders markdown.
  let markedPromise = null;
  function loadMarked() {
    if (markedPromise) return markedPromise;
    markedPromise = new Promise((resolve, reject) => {
      if (window.marked) return resolve(window.marked);
      const s = document.createElement('script');
      s.src = 'https://cdn.jsdelivr.net/npm/marked@12.0.2/marked.min.js';
      s.onload = () => resolve(window.marked);
      s.onerror = () => reject(new Error('marked failed to load'));
      document.head.appendChild(s);
    });
    return markedPromise;
  }

  // Render markdown into a DOM element: marked → highlight code → render mermaid.
  // Falls back to the built-in renderer if marked can't load.
  async function renderInto(el, src) {
    el.classList.add('xnaut-md');
    let html;
    try {
      const marked = await loadMarked();
      html = marked.parse(String(src == null ? '' : src), { gfm: true, breaks: false });
    } catch (e) {
      console.warn('[markdown-render] marked unavailable, using fallback', e);
      el.innerHTML = render(src);
      return enhance(el);
    }
    el.innerHTML = html;
    // Code blocks → mermaid diagrams or highlight.js. marked emits
    // <pre><code class="language-xxx">; we transform in place.
    el.querySelectorAll('pre > code').forEach((code) => {
      const cls = Array.from(code.classList).find((c) => c.indexOf('language-') === 0);
      const lang = cls ? cls.slice(9).toLowerCase() : '';
      if (lang === 'mermaid') {
        const div = document.createElement('div');
        div.className = 'mermaid';
        div.textContent = code.textContent;
        const pre = code.parentElement;
        if (pre && pre.parentElement) pre.parentElement.replaceChild(div, pre);
      } else if (window.hljs) {
        try { window.hljs.highlightElement(code); } catch (_) { /* leave plain */ }
      }
    });
    return enhance(el);
  }

  window.xnautMarkdown = { render, renderInto, enhance, highlightCode, loadMermaid, loadMarked };
})();
