// Shared chat affordances (XNAUT-61) — one implementation for every chat box
// in xNAUT, not a copy per panel.
//
// Usage from any chat renderer:
//   `<div class="bubble">${text}${window.xnautCopyBtn(text)}</div>`
// The click is handled by one delegated listener installed here, so panels
// never wire their own and re-rendering a thread cannot lose the binding.
(function () {
  'use strict';

  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  let styled = false;
  function injectStyles() {
    if (styled) return; styled = true;
    const st = document.createElement('style');
    st.textContent = `
.xn-copy { position:absolute; top:6px; right:6px; width:22px; height:22px; display:flex; align-items:center;
  justify-content:center; border:0; border-radius:6px; background:rgba(255,255,255,.04); color:#8a8f98;
  cursor:pointer; opacity:0; transition:opacity .12s ease; font:inherit; font-size:11px; padding:0; }
.xn-copy:hover { background:rgba(255,255,255,.10); color:#fafafa; }
.xn-copy.ok { color:#7ec98f; opacity:1; }
/* Reveal on hover of the bubble; keep it visible once clicked. */
.xn-copyable { position:relative; }
.xn-copyable:hover .xn-copy { opacity:1; }`;
    document.head.appendChild(st);
  }

  /// Copy button markup. `text` is stored base64-encoded so quotes, newlines
  /// and markup in a message can never break the attribute.
  function copyBtn(text) {
    injectStyles();
    let payload = '';
    try { payload = btoa(unescape(encodeURIComponent(String(text == null ? '' : text)))); } catch (_) { payload = ''; }
    return `<button class="xn-copy" data-xn-copy="${esc(payload)}" title="Copy message" aria-label="Copy message">⧉</button>`;
  }

  // One delegated listener for the whole app.
  document.addEventListener('click', (ev) => {
    const btn = ev.target.closest && ev.target.closest('[data-xn-copy]');
    if (!btn) return;
    ev.stopPropagation();
    ev.preventDefault();
    let text = '';
    try { text = decodeURIComponent(escape(atob(btn.dataset.xnCopy || ''))); } catch (_) { text = ''; }
    // Fall back to the bubble's own text if the payload did not survive.
    if (!text) {
      const bubble = btn.closest('.xn-copyable');
      text = bubble ? bubble.innerText.trim() : '';
    }
    navigator.clipboard.writeText(text).then(() => {
      const previous = btn.textContent;
      btn.textContent = '✓';
      btn.classList.add('ok');
      setTimeout(() => {
        if (!btn.isConnected) return;
        btn.textContent = previous;
        btn.classList.remove('ok');
      }, 1000);
    }).catch((error) => console.warn('[chat-copy] clipboard refused:', error));
  });

  window.xnautCopyBtn = copyBtn;
})();
