// Browser dropdown — XNAUT-149.
//
// The top-bar globe (btn-new-browser) no longer opens a browser TAB; it
// toggles an anchored dropdown panel floating under the top bar (Xirp-style,
// ⌘⇧B). The panel hosts the same Tauri child webview the browser panes use
// (browser_pane_* commands, chrome-offset trick included), so this file is a
// placement, not a new engine. Shift-click on the globe keeps the old
// open-as-tab behaviour; so does the panel's "open as tab" button.
//
// Hiding parks the webview offscreen via browser_pane_set_visible(false),
// which preserves page state across toggles — same pattern as inactive tabs.
(function () {
  'use strict';

  const inv = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
  const LABEL = 'browser-dropdown';
  const URL_KEY = 'xnaut-browser-dropdown-url';
  const INSET = 6; // same gutter as browser-pane.js so edges stay grabbable

  // Copied from browser-pane.js: Tauri's macOS child-webview origin is the
  // NSWindow top (title bar included), not the viewport top.
  function getChromeOffsetY() {
    const isMac = /Mac/i.test(navigator.userAgent);
    if (!isMac) return 0;
    const chrome = window.outerHeight - window.innerHeight;
    return chrome > 0 && chrome < 100 ? chrome : 28;
  }

  let overlay = null;     // the dropdown DOM, created lazily
  let barEl = null;
  let placeholderEl = null;
  let urlInputEl = null;
  let webviewCreated = false;
  let open = false;
  let resizeObs = null;

  function currentUrl() {
    let u = '';
    try { u = localStorage.getItem(URL_KEY) || ''; } catch (_) {}
    return u || 'https://duckduckgo.com';
  }
  function rememberUrl(u) {
    try { localStorage.setItem(URL_KEY, String(u || '')); } catch (_) {}
  }

  function buildOverlay() {
    overlay = document.createElement('div');
    overlay.id = 'browser-dropdown';
    overlay.style.cssText = [
      'position:fixed',
      'top:44px',
      'right:12px',
      'width:min(62vw, 980px)',
      'height:min(72vh, 760px)',
      'display:flex',
      'flex-direction:column',
      'background:var(--bg-secondary)',
      'border:1px solid var(--border)',
      'border-radius:10px',
      'box-shadow:0 16px 48px rgba(0,0,0,0.55)',
      'z-index:900',
      'overflow:hidden',
    ].join('; ');

    barEl = document.createElement('div');
    barEl.className = 'browser-bar';
    barEl.style.cssText = 'display:flex; align-items:center; gap:4px; padding:6px 8px; border-bottom:1px solid var(--border); flex:0 0 auto;';
    barEl.innerHTML = `
      <button class="btn-icon bd-back" title="Back" aria-label="Back"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><polyline points="10 3 5 8 10 13"/></svg></button>
      <button class="btn-icon bd-forward" title="Forward" aria-label="Forward"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><polyline points="6 3 11 8 6 13"/></svg></button>
      <button class="btn-icon bd-reload" title="Reload" aria-label="Reload"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><path d="M2 8a6 6 0 1 1 1.76 4.24"/><polyline points="2 13 2 9 6 9"/></svg></button>
      <input type="text" class="browser-url bd-url" placeholder="Enter URL or search" spellcheck="false" autocomplete="off" style="flex:1; min-width:0;" />
      <button class="btn-icon bd-astab" title="Open as tab" aria-label="Open as browser tab"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><rect x="2.5" y="2.5" width="11" height="11" rx="1.5"/><polyline points="8 5 11 8 8 11"/></svg></button>
      <button class="btn-icon bd-close" title="Close (Esc)" aria-label="Close browser dropdown"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg></button>
    `;
    overlay.appendChild(barEl);

    placeholderEl = document.createElement('div');
    placeholderEl.style.cssText = 'flex:1; min-width:0; min-height:0;';
    overlay.appendChild(placeholderEl);

    document.body.appendChild(overlay);

    urlInputEl = barEl.querySelector('.bd-url');
    urlInputEl.value = currentUrl();

    const invoke = inv();
    barEl.querySelector('.bd-back').onclick = () => invoke && invoke('browser_pane_back', { label: LABEL }).catch(() => {});
    barEl.querySelector('.bd-forward').onclick = () => invoke && invoke('browser_pane_forward', { label: LABEL }).catch(() => {});
    barEl.querySelector('.bd-reload').onclick = () => invoke && invoke('browser_pane_reload', { label: LABEL }).catch(() => {});
    barEl.querySelector('.bd-close').onclick = () => hide();
    barEl.querySelector('.bd-astab').onclick = () => {
      const u = urlInputEl.value.trim() || currentUrl();
      hide();
      if (typeof window.xnautNewBrowserTab === 'function') window.xnautNewBrowserTab(u);
    };
    urlInputEl.addEventListener('keydown', (e) => {
      if (e.key !== 'Enter') return;
      e.preventDefault();
      e.stopPropagation();
      const target = urlInputEl.value.trim();
      inv()('browser_pane_navigate', { label: LABEL, url: target }).then((finalUrl) => {
        urlInputEl.value = finalUrl;
        rememberUrl(finalUrl);
      }).catch((err) => {
        console.error('[browser-dropdown] navigate failed', err);
        urlInputEl.style.borderColor = '#f85149';
      });
    });

    resizeObs = new ResizeObserver(syncBounds);
    resizeObs.observe(overlay);
    window.addEventListener('resize', () => { if (open) syncBounds(); });
  }

  function webviewBounds() {
    const or = overlay.getBoundingClientRect();
    const br = barEl.getBoundingClientRect();
    const off = getChromeOffsetY();
    return {
      x: or.left + INSET,
      y: br.bottom + off,
      width: Math.max(or.width - INSET * 2, 1),
      height: Math.max(or.bottom - br.bottom - INSET, 1),
    };
  }

  function syncBounds() {
    if (!open || !webviewCreated) return;
    const invoke = inv();
    if (!invoke) return;
    const b = webviewBounds();
    invoke('browser_pane_set_bounds', { req: { label: LABEL, ...b } }).catch(() => {});
  }

  async function show() {
    const invoke = inv();
    if (!invoke) return;
    if (!overlay) buildOverlay();
    overlay.style.display = 'flex';
    open = true;
    // Two frames so the flex layout settles before we sample rects — the
    // same guard browser-pane.js needs on creation.
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const b = webviewBounds();
    if (!webviewCreated) {
      try {
        await invoke('browser_pane_create', {
          req: { window_label: 'main', label: LABEL, url: currentUrl(), ...b },
        });
        webviewCreated = true;
      } catch (e) {
        console.error('[browser-dropdown] create failed', e);
        overlay.style.display = 'none';
        open = false;
        return;
      }
    } else {
      invoke('browser_pane_set_visible', { label: LABEL, visible: true }).catch(() => {});
      invoke('browser_pane_set_bounds', { req: { label: LABEL, ...b } }).catch(() => {});
    }
    urlInputEl.focus();
    urlInputEl.select();
  }

  function hide() {
    if (!open) return;
    open = false;
    if (overlay) overlay.style.display = 'none';
    const invoke = inv();
    if (invoke && webviewCreated) invoke('browser_pane_set_visible', { label: LABEL, visible: false }).catch(() => {});
    if (urlInputEl) rememberUrl(urlInputEl.value.trim());
  }

  function toggle() { if (open) hide(); else show(); }

  document.addEventListener('keydown', (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.shiftKey && (e.key === 'b' || e.key === 'B')) {
      e.preventDefault();
      toggle();
    } else if (e.key === 'Escape' && open) {
      hide();
    }
  });

  function wireButton() {
    const btn = document.getElementById('btn-new-browser');
    if (!btn) return;
    btn.title = 'Browser (⌘⇧B) · shift-click for a tab';
    btn.onclick = (e) => {
      if (e.shiftKey && typeof window.xnautNewBrowserTab === 'function') {
        window.xnautNewBrowserTab().catch((err) => console.error('new browser tab failed:', err));
        return;
      }
      toggle();
    };
  }
  // browser-pane.js wires the old open-a-tab onclick on DOMContentLoaded; we
  // load after it and must win, so wire on DOMContentLoaded too (later
  // registration runs later) and once more on load as a belt.
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wireButton);
  } else {
    wireButton();
  }
  window.addEventListener('load', wireButton);

  window.xnautToggleBrowserDropdown = toggle;
})();
