// One way to paint code in xNAUT, for a file and for a diff.
//
// There were three copies of this: vault-pane.js's `showFileInCenter` and
// `diffLineHtml`, workspace.js's `highlightWithLineNumbers`, and
// delivery-panel.js's `diffHtml`. Each had its own font size, its own line
// height and its own idea of what a hunk header looks like, so the Code tab
// and the Vault tab rendered the same file two different ways and Andre could
// see it (2026-09-13: "I want this code pane looking the same").
//
// Two exports, one stylesheet, one language map:
//   window.xnautRenderCode(text, path)   a file, numbered and highlighted
//   window.xnautRenderDiff(raw, path)    a unified diff, same face and size,
//                                        with old/new line numbers and a
//                                        background on every changed line
//
// Both return a string of HTML for the caller to insert. Neither touches the
// DOM, so a caller can put the result anywhere.
(function () {
  'use strict';

  const esc = (s) => String(s == null ? '' : s)
    .replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  // Extension to hljs language. Lifted from vault-pane.js:450, where it was a
  // closure and therefore uncopyable; this is now the only copy.
  const HLJS_LANG = {
    js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript',
    ts: 'typescript', tsx: 'typescript', py: 'python', rs: 'rust', go: 'go',
    rb: 'ruby', sh: 'bash', bash: 'bash', zsh: 'bash', fish: 'bash',
    json: 'json', yaml: 'yaml', yml: 'yaml', toml: 'ini', ini: 'ini',
    html: 'xml', css: 'css', scss: 'scss', xml: 'xml', svg: 'xml',
    sql: 'sql', md: 'markdown', markdown: 'markdown', mdx: 'markdown',
    c: 'c', cpp: 'cpp', cc: 'cpp', h: 'cpp', hpp: 'cpp', java: 'java',
    swift: 'swift', kt: 'kotlin', php: 'php', lua: 'lua', vue: 'xml',
  };

  const extOf = (path) => {
    const base = String(path || '').split('/').pop() || '';
    const dot = base.lastIndexOf('.');
    return dot > 0 ? base.slice(dot + 1).toLowerCase() : '';
  };

  function langOf(path) {
    const lang = HLJS_LANG[extOf(path)];
    if (!lang || typeof hljs === 'undefined') return '';
    return hljs.getLanguage(lang) ? lang : '';
  }

  // One line at a time, because a diff has no whole-file text to hand hljs:
  // its lines are interleaved with removals that are not in the new file.
  // `ignoreIllegals` matters for exactly that reason, a line lifted out of its
  // block is often not valid on its own.
  function paint(code, lang) {
    if (!code) return '';
    if (typeof hljs === 'undefined' || !lang) return esc(code);
    try {
      return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
    } catch (_error) {
      return esc(code);
    }
  }

  // A whole file: hljs sees all of it, so its language detection and its
  // multi-line constructs both work.
  function xnautRenderCode(text, path) {
    const content = String(text == null ? '' : text);
    const lang = langOf(path);
    let html;
    try {
      html = (typeof hljs === 'undefined')
        ? esc(content)
        : (lang ? hljs.highlight(content, { language: lang }).value : hljs.highlightAuto(content).value);
    } catch (_error) {
      html = esc(content);
    }
    const rows = html.split('\n')
      .map((line, i) => `<span class="xcr-ln">${i + 1}</span>${line || ' '}`)
      .join('\n');
    return `<pre class="xcr hljs"><code>${rows}</code></pre>`;
  }

  // A unified diff. Two gutters rather than one, because "which line is this
  // in the file I would open" is the question a diff has to answer and a
  // single running count cannot: a removed line has no new number and an added
  // line has no old one.
  function xnautRenderDiff(raw, path) {
    const text = String(raw == null ? '' : raw);
    if (!text.trim()) return '<pre class="xcr hljs"><code><span class="xcr-note">No textual diff.</span></code></pre>';
    // The +++ header names the file the diff produces, which is a better
    // language hint than the path the caller happened to have.
    const named = /^\+\+\+ b\/(.+)$/m.exec(text);
    const lang = langOf((named && named[1]) || path);

    let oldNo = 0;
    let newNo = 0;
    const rows = text.split('\n').map((line) => {
      const gut = (a, b) => `<span class="xcr-ln xcr-ln2">${a}</span><span class="xcr-ln">${b}</span>`;

      if (line.startsWith('@@')) {
        const m = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
        if (m) { oldNo = Number(m[1]); newNo = Number(m[2]); }
        return `<span class="xcr-row xcr-hunk">${gut('', '')}${esc(line)}</span>`;
      }
      if (/^diff --git |^index |^new file|^deleted file|^rename |^similarity |^--- |^\+\+\+ /.test(line)) {
        return `<span class="xcr-row xcr-meta">${gut('', '')}${esc(line) || ' '}</span>`;
      }
      if (line.startsWith('+')) {
        return `<span class="xcr-row xcr-add">${gut('', newNo++)}<span class="xcr-sign">+</span>${paint(line.slice(1), lang) || ' '}</span>`;
      }
      if (line.startsWith('-')) {
        return `<span class="xcr-row xcr-del">${gut(oldNo++, '')}<span class="xcr-sign">-</span>${paint(line.slice(1), lang) || ' '}</span>`;
      }
      if (line === '\\ No newline at end of file') {
        return `<span class="xcr-row xcr-meta">${gut('', '')}${esc(line)}</span>`;
      }
      // Context. The leading space is part of the format, not of the code.
      const body = line.startsWith(' ') ? line.slice(1) : line;
      return `<span class="xcr-row">${gut(oldNo++, newNo++)}<span class="xcr-sign"> </span>${paint(body, lang) || ' '}</span>`;
    }).join('');
    return `<pre class="xcr xcr-diff hljs"><code>${rows}</code></pre>`;
  }

  function injectStyles() {
    if (document.getElementById('xnaut-code-render-styles')) return;
    const st = document.createElement('style');
    st.id = 'xnaut-code-render-styles';
    // The same face, size and leading for both, which is the whole point of
    // the module. A change here changes the file view and the diff together,
    // which is the property three separate copies could not have.
    st.textContent = `
.xcr { margin:0; padding:8px 0; overflow:auto;
  font-family:var(--font-mono,ui-monospace,"SF Mono",Menlo,monospace);
  font-size:12px; line-height:1.5; tab-size:2; }
.xcr code { display:block; white-space:pre; }
.xcr-ln { display:inline-block; width:3.2em; padding-right:12px; text-align:right;
  user-select:none; color:var(--text-muted,#6b7280); opacity:.75; }
.xcr-ln2 { width:3.2em; }
.xcr-diff .xcr-row { display:block; padding-right:12px; }
.xcr-sign { display:inline-block; width:1.1em; user-select:none; opacity:.8; }
/* A background on the whole row, edge to edge, so a run of changed lines
   reads as one block rather than as ragged highlights. */
.xcr-add { background:rgba(74,157,91,.16); }
.xcr-del { background:rgba(192,85,77,.16); }
.xcr-add .xcr-sign { color:#7fd394; }
.xcr-del .xcr-sign { color:#eaa39c; }
.xcr-hunk { color:#5a8bd6; background:rgba(90,139,214,.09); }
.xcr-meta { color:var(--text-muted,#777); }
.xcr-note { color:var(--text-muted,#777); padding-left:12px; }
`;
    document.head.appendChild(st);
  }

  injectStyles();
  window.xnautRenderCode = xnautRenderCode;
  window.xnautRenderDiff = xnautRenderDiff;
})();
