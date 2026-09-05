// In-app confirm / prompt / alert (XNAUT-80).
//
// window.confirm, window.alert and window.prompt are all no-ops in Tauri's
// WKWebView: they can return without ever rendering. That silently disarmed
// every confirm-gated destructive action in the app (a mis-click on "Remove
// from list" fired straight through) and made every alert() invisible — the
// same blindness that hid the auto-update failure in XNAUT-75.
//
// Loaded before app.js so the natives are replaced before anything can call
// them. confirm/prompt cannot be shimmed in place (they must return
// synchronously), so callers await xnautConfirmDialog / xnautPromptDialog
// instead; the natives are left in place only as a loud, visible failure.
//
// Message strings are HTML: callers that interpolate user data escape it
// themselves, which is what the existing call sites already do. Newlines
// render, so "\n\n" in a message still reads as a paragraph break.
(function () {
  'use strict';

  function ensureStyles() {
    if (document.getElementById('xnaut-dialog-styles')) return;
    const style = document.createElement('style');
    style.id = 'xnaut-dialog-styles';
    style.textContent = `
      .xdlg { position:fixed; inset:0; z-index:1200; display:flex; align-items:center; justify-content:center;
        background:rgba(0,0,0,.55); }
      .xdlg-box { background:var(--bg-secondary,#1a1a1f); border:1px solid var(--border,#2a2a2f); border-radius:10px;
        padding:18px 20px; width:min(420px,90vw); display:flex; flex-direction:column; gap:12px;
        font-family:var(--font-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); }
      .xdlg-msg { color:var(--text-primary,#e0e0e0); font-size:13px; line-height:1.5; white-space:pre-wrap; }
      .xdlg-detail { color:var(--text-secondary,#a0a0a0); font-size:12px; line-height:1.5; white-space:pre-wrap; }
      .xdlg-input { padding:9px 11px; border:1px solid var(--border,#2a2a2f); border-radius:8px;
        background:var(--bg-primary,#0a0a0f); color:var(--text-primary,#e0e0e0); font:inherit; font-size:13px; }
      .xdlg-actions { display:flex; gap:8px; justify-content:flex-end; }
      .xdlg-actions button { font:inherit; font-size:12px; padding:6px 14px; border-radius:7px; cursor:pointer; }
      .xdlg-cancel { border:1px solid var(--border,#2a2a2f); background:transparent; color:var(--text-secondary,#a0a0a0); }
      .xdlg-ok { border:none; font-weight:600; background:#f5b840; color:#0a0a0f; }
      .xdlg-ok.xdlg-danger { background:#ef4444; color:#fff; }
      .xdlg-toasts { position:fixed; right:16px; bottom:16px; z-index:1300; display:flex; flex-direction:column;
        gap:8px; align-items:flex-end; pointer-events:none; }
      .xdlg-toast { pointer-events:auto; max-width:min(420px,80vw); padding:10px 14px; border-radius:8px;
        border:1px solid var(--border,#2a2a2f); background:var(--bg-secondary,#1a1a1f);
        color:var(--text-primary,#e0e0e0); font-family:var(--font-ui,-apple-system,sans-serif); font-size:12px;
        line-height:1.5; white-space:pre-wrap; box-shadow:0 6px 20px rgba(0,0,0,.45); cursor:pointer; }
    `;
    document.head.appendChild(style);
  }

  // One dialog shape for both confirm and prompt: the only difference is
  // whether there is an input, and what the resolved value means.
  function dialog(opts) {
    ensureStyles();
    return new Promise((resolve) => {
      const overlay = document.createElement('div');
      overlay.className = 'xdlg';
      overlay.setAttribute('role', 'dialog');
      overlay.setAttribute('aria-modal', 'true');
      overlay.innerHTML = `<div class="xdlg-box">
        <div class="xdlg-msg">${opts.message || ''}</div>
        ${opts.detail ? `<div class="xdlg-detail">${opts.detail}</div>` : ''}
        ${opts.input ? '<input class="xdlg-input" data-value />' : ''}
        <div class="xdlg-actions">
          <button class="xdlg-cancel" data-cancel>Cancel</button>
          <button class="xdlg-ok${opts.danger ? ' xdlg-danger' : ''}" data-ok>${opts.actionLabel || 'OK'}</button>
        </div></div>`;
      const input = overlay.querySelector('[data-value]');
      if (input) input.value = opts.value || '';
      const done = (value) => {
        document.removeEventListener('keydown', onKey, true);
        overlay.remove();
        resolve(value);
      };
      const accept = () => done(input ? (input.value.trim() || null) : true);
      const cancel = () => done(input ? null : false);
      function onKey(event) {
        if (event.key === 'Escape') { event.preventDefault(); cancel(); }
        // Enter confirms only from the input; a bare Enter on a destructive
        // dialog is exactly the reflex this guard exists to catch.
        else if (event.key === 'Enter' && input && event.target === input) { event.preventDefault(); accept(); }
      }
      overlay.querySelector('[data-ok]').onclick = accept;
      overlay.querySelector('[data-cancel]').onclick = cancel;
      overlay.onclick = (event) => { if (event.target === overlay) cancel(); };
      document.addEventListener('keydown', onKey, true);
      document.body.appendChild(overlay);
      if (input) { input.focus(); input.select(); } else overlay.querySelector('[data-cancel]').focus();
    });
  }

  // Non-blocking, so replacing alert() cannot wedge a flow that fired three of
  // them in a row. Click to dismiss early.
  function toast(message) {
    ensureStyles();
    let stack = document.querySelector('.xdlg-toasts');
    if (!stack) {
      stack = document.createElement('div');
      stack.className = 'xdlg-toasts';
      document.body.appendChild(stack);
    }
    const el = document.createElement('div');
    el.className = 'xdlg-toast';
    el.setAttribute('role', 'status');
    el.textContent = String(message == null ? '' : message);
    el.onclick = () => el.remove();
    stack.appendChild(el);
    setTimeout(() => el.remove(), 8000);
    return el;
  }

  window.xnautConfirmDialog = (message, actionLabel, detail) =>
    dialog({ message, actionLabel: actionLabel || 'Delete', detail, danger: true });
  window.xnautPromptDialog = (message, value, actionLabel) =>
    dialog({ message, value, actionLabel: actionLabel || 'OK', input: true });
  window.xnautToast = toast;

  // The natives stay reachable but stop being silent. alert() has no return
  // value, so it becomes a toast and every existing call site works as
  // written; confirm/prompt must answer synchronously, so they can only
  // refuse loudly and point at the replacement.
  window.alert = (message) => { toast(message); };
  window.confirm = (message) => {
    console.error('[dialogs] window.confirm is a no-op in this webview — use xnautConfirmDialog:', message);
    toast('This action needs a confirmation dialog that has not been wired yet, so nothing was changed.');
    return false;
  };
  window.prompt = (message) => {
    console.error('[dialogs] window.prompt is a no-op in this webview — use xnautPromptDialog:', message);
    toast('This action needs an input dialog that has not been wired yet, so nothing was changed.');
    return null;
  };
})();
