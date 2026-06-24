// Browser panes — Phase 6 of the Orca port.
//
// Architecture: each browser pane reserves a placeholder <div> in xNaut's
// terminal-area layout. The actual native webview is a separate Tauri child
// webview that we float over the placeholder by syncing its bounds whenever
// the placeholder moves/resizes. Same pattern Electron apps use with
// <webview> tags, except the floating layer is OS-native instead of a DOM
// child.
//
// Visibility: a webview NOT in the active tab gets shoved to (-32000, 1×1)
// via browser_pane_set_visible(false). Cheap, preserves state, no flicker.
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);
  const inv = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);

  // The vertical gap between the OS window's top edge (which Tauri's child
  // webview positioning uses as origin on macOS) and the parent webview's
  // viewport top (which getBoundingClientRect uses). Without this offset the
  // child webview lands too high and overlaps the address bar. On macOS with
  // a native title bar this is typically 28px. outerHeight − innerHeight gives
  // total window chrome — on a window with no bottom chrome that equals the
  // title bar height. Clamps to 28 if measurement looks bogus.
  function getChromeOffsetY() {
    const isMac = /Mac/i.test(navigator.userAgent);
    if (!isMac) return 0;
    const chrome = window.outerHeight - window.innerHeight;
    return chrome > 0 && chrome < 100 ? chrome : 28;
  }

  // label -> { paneEl, placeholderEl, resizeObs, tabId, currentUrl }
  const panes = new Map();
  let labelCounter = 0;

  function nextLabel() {
    labelCounter += 1;
    return `browser-${Date.now().toString(36)}-${labelCounter}`;
  }

  /**
   * Build the DOM for a browser pane and ask Rust to attach a child webview
   * over the placeholder rect. Returns the pane element so callers (the tab
   * system) can manage it like a terminal pane.
   *
   * tab.terminals[] entries for browser panes look like:
   *   { kind: 'browser', label, pane: HTMLElement, url }
   */
  async function createBrowserPane(tabId, parentContainer, initialUrl) {
    const invoke = inv();
    if (!invoke) throw new Error('Tauri not available');

    const label = nextLabel();
    const url = initialUrl || 'https://duckduckgo.com';

    // Pane wrapper — explicit width/height because the terminal-container is
    // display:flex with no direction set (defaults to row); without explicit
    // sizes the pane collapses to 0×0 and the bar disappears with it. The
    // bright outline is a temporary diagnostic — strip after URL bar is confirmed.
    const pane = document.createElement('div');
    pane.className = 'browser-pane';
    pane.dataset.browserLabel = label;
    pane.style.cssText = [
      'display:flex',
      'flex-direction:column',
      'flex:1 1 0%',
      'width:100%',
      'height:100%',
      'min-width:0',
      'min-height:0',
      'overflow:hidden',
      'background:var(--editor-surface)',
      'border-radius:var(--radius-md)',
      'outline:2px solid #ff0080',           /* DIAG: bright pink so we can see the box */
    ].join('; ');
    console.log('[browser-pane] creating pane', { tabId, label, parentContainer, parentRect: parentContainer.getBoundingClientRect() });

    // Address bar
    const bar = document.createElement('div');
    bar.className = 'browser-bar';
    bar.innerHTML = `
      <button class="btn-icon browser-back" title="Back" aria-label="Back"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><polyline points="10 3 5 8 10 13"/></svg></button>
      <button class="btn-icon browser-forward" title="Forward" aria-label="Forward"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><polyline points="6 3 11 8 6 13"/></svg></button>
      <button class="btn-icon browser-reload" title="Reload" aria-label="Reload"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><path d="M2 8a6 6 0 1 1 1.76 4.24"/><polyline points="2 13 2 9 6 9"/></svg></button>
      <input type="text" class="browser-url" placeholder="Enter URL or search" spellcheck="false" autocomplete="off" />
      <button class="btn-icon browser-close" data-variant="destructive" title="Close pane" aria-label="Close browser pane"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg></button>
    `;
    pane.appendChild(bar);

    // Transparent placeholder where the native webview will float
    const placeholder = document.createElement('div');
    placeholder.className = 'browser-placeholder';
    placeholder.style.cssText = 'flex:1; min-width:0; min-height:0;';
    pane.appendChild(placeholder);

    parentContainer.appendChild(pane);

    const urlInput = bar.querySelector('.browser-url');
    urlInput.value = url;

    // Wait two frames so the flex layout has been resolved before we sample
    // the placeholder's rect — sampling too early can return 0×0 (or worse:
    // the pane's full bounds because the bar hasn't been laid out yet) and
    // the resulting webview ends up covering the address bar.
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    if (!document.body.contains(placeholder)) {
      // Pane was removed (e.g. tab closed) before we got here.
      return null;
    }
    const r0 = placeholder.getBoundingClientRect();
    if (r0.width < 2 || r0.height < 2) {
      console.warn('[browser-pane] placeholder has zero size; deferring creation', r0);
      // Try once more on the next frame.
      await new Promise((r) => requestAnimationFrame(r));
    }
    // Compute the webview bounds from BAR rect, not placeholder — guarantees
    // we start below the bar even if the placeholder hasn't laid out yet.
    // Then offset Y by the OS chrome height because Tauri's macOS child-webview
    // positioning starts at the NSWindow top, not the viewport top.
    const barRect = bar.getBoundingClientRect();
    const paneRect = pane.getBoundingClientRect();
    const placeholderRect = placeholder.getBoundingClientRect();
    const yOffset = getChromeOffsetY();
    const finalX = paneRect.left;
    const finalY = barRect.bottom + yOffset;
    const finalW = paneRect.width;
    const finalH = Math.max(paneRect.bottom - barRect.bottom, 1);
    console.log('[browser-pane] rects', {
      label,
      yOffset,
      pane: { x: paneRect.left, y: paneRect.top, w: paneRect.width, h: paneRect.height },
      bar: { x: barRect.left, y: barRect.top, w: barRect.width, h: barRect.height, bottom: barRect.bottom },
      placeholder: { x: placeholderRect.left, y: placeholderRect.top, w: placeholderRect.width, h: placeholderRect.height },
      webview_target: { x: finalX, y: finalY, w: finalW, h: finalH },
    });
    try {
      await invoke('browser_pane_create', {
        req: {
          window_label: 'main',
          label,
          url,
          x: finalX,
          y: finalY,
          width: Math.max(finalW, 1),
          height: Math.max(finalH, 1),
        },
      });
    } catch (e) {
      pane.remove();
      throw e;
    }

    // Keep the webview pinned below the bar on any reflow. Using bar+pane
    // rects (not placeholder) ensures the webview never overlaps the bar
    // even during transient flex layout states. Y gets the chrome offset
    // added so it lines up with the visual position of the bar's bottom.
    const syncBounds = () => {
      if (!document.body.contains(pane)) return;
      const pr = pane.getBoundingClientRect();
      const br = bar.getBoundingClientRect();
      const off = getChromeOffsetY();
      invoke('browser_pane_set_bounds', {
        req: {
          label,
          x: pr.left,
          y: br.bottom + off,
          width: Math.max(pr.width, 1),
          height: Math.max(pr.bottom - br.bottom, 1),
        },
      }).catch(() => {});
    };
    const ro = new ResizeObserver(syncBounds);
    ro.observe(pane);
    ro.observe(bar);

    // Address-bar wiring
    bar.querySelector('.browser-back').onclick = () => invoke('browser_pane_back', { label }).catch(() => {});
    bar.querySelector('.browser-forward').onclick = () => invoke('browser_pane_forward', { label }).catch(() => {});
    bar.querySelector('.browser-reload').onclick = () => invoke('browser_pane_reload', { label }).catch(() => {});
    urlInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        const target = urlInput.value.trim();
        console.log('[browser-pane] navigate ->', target);
        invoke('browser_pane_navigate', { label, url: target }).then((finalUrl) => {
          urlInput.value = finalUrl;
          const entry = panes.get(label);
          if (entry) entry.currentUrl = finalUrl;
        }).catch((err) => {
          console.error('[browser-pane] navigate failed', err);
          urlInput.title = 'navigate failed: ' + String(err);
          urlInput.style.borderColor = '#f85149';
        });
      }
    });
    bar.querySelector('.browser-close').onclick = async () => {
      await destroyBrowserPane(label);
    };

    panes.set(label, { paneEl: pane, placeholderEl: placeholder, barEl: bar, resizeObs: ro, tabId, currentUrl: url, urlInputEl: urlInput });
    return { kind: 'browser', label, pane, url };
  }

  async function destroyBrowserPane(label) {
    const invoke = inv();
    const entry = panes.get(label);
    if (!entry) return;
    entry.resizeObs.disconnect();
    if (invoke) await invoke('browser_pane_destroy', { label }).catch(() => {});
    if (entry.paneEl && entry.paneEl.parentNode) entry.paneEl.parentNode.removeChild(entry.paneEl);
    panes.delete(label);
    // Let the tab system know a pane disappeared. We don't reach into tabs[]
    // directly here; the tab close button drives that path. Manual close from
    // the browser-bar updates panes Map; the tab object's terminals[] array
    // may now contain a stale entry that points at a removed DOM element —
    // that's safe because layout code iterates by parent containers, not by
    // the stale entry.
  }

  /**
   * Called by app.js's switchTab. Iterates all browser panes and decides
   * which to show vs hide. Active tab's browsers get bounds sync; inactive
   * tabs' browsers get parked offscreen.
   */
  function onActiveTabChanged(activeTabId) {
    const invoke = inv();
    if (!invoke) return;
    const off = getChromeOffsetY();
    panes.forEach((entry, label) => {
      const visible = entry.tabId === activeTabId && document.body.contains(entry.paneEl);
      if (visible) {
        const pr = entry.paneEl.getBoundingClientRect();
        const br = entry.barEl.getBoundingClientRect();
        invoke('browser_pane_set_visible', { label, visible: true }).catch(() => {});
        invoke('browser_pane_set_bounds', {
          req: {
            label,
            x: pr.left,
            y: br.bottom + off,
            width: Math.max(pr.width, 1),
            height: Math.max(pr.bottom - br.bottom, 1),
          },
        }).catch(() => {});
      } else {
        invoke('browser_pane_set_visible', { label, visible: false }).catch(() => {});
      }
    });
  }

  // Sync bounds on window resize too — ResizeObserver fires on placeholder
  // size changes but not window-position changes (e.g. moving the window).
  let resizeRaf = 0;
  window.addEventListener('resize', () => {
    if (resizeRaf) return;
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0;
      const off = getChromeOffsetY();
      panes.forEach((entry, label) => {
        if (!document.body.contains(entry.paneEl)) return;
        const pr = entry.paneEl.getBoundingClientRect();
        const br = entry.barEl.getBoundingClientRect();
        inv()('browser_pane_set_bounds', {
          req: { label, x: pr.left, y: br.bottom + off, width: Math.max(pr.width, 1), height: Math.max(pr.bottom - br.bottom, 1) },
        }).catch(() => {});
      });
    });
  });

  // ── public API hooks ────────────────────────────────────────────────────
  window.xnautCreateBrowserPane = createBrowserPane;
  window.xnautDestroyBrowserPane = destroyBrowserPane;
  window.xnautOnTabSwitched = onActiveTabChanged;

  /**
   * Top-bar "New Browser" handler — creates a fresh tab with a single
   * browser pane. If app.js's tab plumbing is loaded, we hook into it.
   */
  async function newBrowserTab(initialUrl) {
    if (typeof window.xnautAttachBrowserTab !== 'function') {
      console.warn('xnautAttachBrowserTab not yet defined in app.js');
      return;
    }
    return window.xnautAttachBrowserTab(initialUrl);
  }
  window.xnautNewBrowserTab = newBrowserTab;

  function wireButton() {
    const btn = $('btn-new-browser');
    if (btn) btn.onclick = () => newBrowserTab().catch((e) => console.error('new browser tab failed:', e));
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wireButton);
  } else {
    wireButton();
  }
})();
