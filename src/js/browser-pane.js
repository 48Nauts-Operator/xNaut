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

  // paneKey -> { paneEl, placeholderEl, barEl, tabsEl, resizeObs, tabId,
  //              pages: [{ label, url, chipEl }], activeIdx }
  // XNAUT-149: a pane hosts MULTIPLE pages, each its own child webview; only
  // the active page is visible, the rest are parked offscreen (state kept).
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
    ].join('; ');
    console.log('[browser-pane] creating pane', { tabId, label, parentContainer, parentRect: parentContainer.getBoundingClientRect() });

    // Page-tab strip (XNAUT-149): (+) top-left, then one closable chip per
    // page. A pane hosts multiple pages, each its own child webview.
    const tabsEl = document.createElement('div');
    tabsEl.className = 'browser-tabs';
    tabsEl.style.cssText = 'display:flex; align-items:center; gap:4px; padding:4px 6px 0 6px; flex:0 0 auto; overflow-x:auto;';
    tabsEl.innerHTML = '<button class="btn-icon browser-addpage" title="New page" aria-label="New browser page" style="flex:0 0 auto;"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="12" height="12"><line x1="8" y1="3" x2="8" y2="13"/><line x1="3" y1="8" x2="13" y2="8"/></svg></button>';
    pane.appendChild(tabsEl);

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
    const CREATE_INSET = 6; // keep in sync with syncBounds INSET
    const finalX = paneRect.left + CREATE_INSET;
    const finalY = barRect.bottom + yOffset;
    const finalW = Math.max(paneRect.width - CREATE_INSET * 2, 1);
    const finalH = Math.max(paneRect.bottom - barRect.bottom - CREATE_INSET, 1);
    console.log('[browser-pane] rects', {
      label,
      yOffset,
      pane: { x: paneRect.left, y: paneRect.top, w: paneRect.width, h: paneRect.height },
      bar: { x: barRect.left, y: barRect.top, w: barRect.width, h: barRect.height, bottom: barRect.bottom },
      placeholder: { x: placeholderRect.left, y: placeholderRect.top, w: placeholderRect.width, h: placeholderRect.height },
      webview_target: { x: finalX, y: finalY, w: finalW, h: finalH },
    });
    // Multi-page pane (XNAUT-149). The FIRST page reuses the pane's own
    // label so app.js's direct browser_pane_destroy(terminal.label) path
    // keeps destroying a real webview; extra pages get their own labels and
    // are swept by destroyBrowserPane / xnautForgetBrowserPane.
    const entry = { paneEl: pane, placeholderEl: placeholder, barEl: bar, tabsEl, resizeObs: null, tabId, pages: [], activeIdx: -1, urlInputEl: urlInput };
    const activeLabel = () => (entry.activeIdx >= 0 && entry.pages[entry.activeIdx] ? entry.pages[entry.activeIdx].label : null);

    // Bounds below the address bar. Inset a few px so DOM split dividers stay
    // grabbable — a native child webview eats every mouse event in its rect.
    // Y gets the chrome offset because Tauri's macOS child-webview origin is
    // the NSWindow top, not the viewport top.
    const INSET = 6;
    function currentBounds() {
      const pr = pane.getBoundingClientRect();
      const br = bar.getBoundingClientRect();
      const off = getChromeOffsetY();
      return {
        x: pr.left + INSET,
        y: br.bottom + off,
        width: Math.max(pr.width - INSET * 2, 1),
        height: Math.max(pr.bottom - br.bottom - INSET, 1),
      };
    }

    function chipTitle(u) {
      try { return new URL(/^https?:/i.test(u) ? u : 'https://' + u).host || u; } catch (_) { return u; }
    }

    function renderChips() {
      tabsEl.querySelectorAll('.browser-page-chip').forEach((c) => c.remove());
      entry.pages.forEach((pg, i) => {
        const chip = document.createElement('span');
        chip.className = 'browser-page-chip';
        chip.style.cssText = 'display:inline-flex; align-items:center; gap:6px; padding:2px 8px; border-radius:6px; font-size:11px; cursor:pointer; max-width:160px; white-space:nowrap; overflow:hidden; flex:0 0 auto;'
          + (i === entry.activeIdx ? 'background:var(--bg-tertiary); color:var(--text-primary);' : 'color:var(--text-secondary);');
        const t = document.createElement('span');
        t.textContent = chipTitle(pg.url);
        t.style.cssText = 'overflow:hidden; text-overflow:ellipsis;';
        const x = document.createElement('span');
        x.textContent = '×';
        x.title = 'Close page';
        x.style.cssText = 'flex:0 0 auto; opacity:0.7;';
        chip.appendChild(t);
        chip.appendChild(x);
        chip.onclick = (e) => { if (e.target === x) closePage(i); else activatePage(i); };
        tabsEl.appendChild(chip);
        pg.chipEl = chip;
      });
    }

    async function addPage(pageUrl, fixedLabel) {
      const lbl = fixedLabel || nextLabel();
      const b = currentBounds();
      await invoke('browser_pane_create', {
        req: { window_label: 'main', label: lbl, url: pageUrl, x: b.x, y: b.y, width: b.width, height: b.height },
      });
      const prev = activeLabel();
      if (prev) invoke('browser_pane_set_visible', { label: prev, visible: false }).catch(() => {});
      entry.pages.push({ label: lbl, url: pageUrl, chipEl: null });
      entry.activeIdx = entry.pages.length - 1;
      urlInput.value = pageUrl;
      renderChips();
    }

    function activatePage(i) {
      if (i === entry.activeIdx || !entry.pages[i]) return;
      const prev = activeLabel();
      if (prev) invoke('browser_pane_set_visible', { label: prev, visible: false }).catch(() => {});
      entry.activeIdx = i;
      const pg = entry.pages[i];
      invoke('browser_pane_set_visible', { label: pg.label, visible: true }).catch(() => {});
      invoke('browser_pane_set_bounds', { req: { label: pg.label, ...currentBounds() } }).catch(() => {});
      urlInput.value = pg.url;
      renderChips();
    }

    async function closePage(i) {
      const pg = entry.pages[i];
      if (!pg) return;
      const wasActive = i === entry.activeIdx;
      await invoke('browser_pane_destroy', { label: pg.label }).catch(() => {});
      entry.pages.splice(i, 1);
      if (!entry.pages.length) {
        // Last page closed — close the whole pane through the split-collapse
        // so the layout heals; fall back to a plain destroy.
        if (window.xnautClosePaneByElement) await window.xnautClosePaneByElement(pane, tabId);
        else await destroyBrowserPane(label);
        return;
      }
      if (i < entry.activeIdx) entry.activeIdx -= 1;
      if (entry.activeIdx >= entry.pages.length) entry.activeIdx = entry.pages.length - 1;
      const cur = entry.pages[entry.activeIdx];
      if (wasActive && cur) {
        invoke('browser_pane_set_visible', { label: cur.label, visible: true }).catch(() => {});
        invoke('browser_pane_set_bounds', { req: { label: cur.label, ...currentBounds() } }).catch(() => {});
        urlInput.value = cur.url;
      }
      renderChips();
    }

    try {
      await addPage(url, label); // first page carries the pane's label
    } catch (e) {
      pane.remove();
      throw e;
    }

    const syncBounds = () => {
      if (!document.body.contains(pane)) return;
      const lbl = activeLabel();
      if (!lbl) return;
      invoke('browser_pane_set_bounds', { req: { label: lbl, ...currentBounds() } }).catch(() => {});
    };
    const ro = new ResizeObserver(syncBounds);
    ro.observe(pane);
    ro.observe(bar);
    ro.observe(tabsEl);
    entry.resizeObs = ro;

    // Page-tab strip wiring
    tabsEl.querySelector('.browser-addpage').onclick = () => {
      addPage('https://duckduckgo.com').catch((e) => console.error('[browser-pane] add page failed', e));
    };

    // Address-bar wiring — everything targets the ACTIVE page
    bar.querySelector('.browser-back').onclick = () => { const l = activeLabel(); if (l) invoke('browser_pane_back', { label: l }).catch(() => {}); };
    bar.querySelector('.browser-forward').onclick = () => { const l = activeLabel(); if (l) invoke('browser_pane_forward', { label: l }).catch(() => {}); };
    bar.querySelector('.browser-reload').onclick = () => { const l = activeLabel(); if (l) invoke('browser_pane_reload', { label: l }).catch(() => {}); };
    urlInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        const target = urlInput.value.trim();
        const l = activeLabel();
        if (!l) return;
        console.log('[browser-pane] navigate ->', target);
        invoke('browser_pane_navigate', { label: l, url: target }).then((finalUrl) => {
          urlInput.value = finalUrl;
          const pg = entry.pages[entry.activeIdx];
          if (pg) { pg.url = finalUrl; renderChips(); }
        }).catch((err) => {
          console.error('[browser-pane] navigate failed', err);
          urlInput.title = 'navigate failed: ' + String(err);
          urlInput.style.borderColor = '#f85149';
        });
      }
    });
    bar.querySelector('.browser-close').onclick = async () => {
      // Route through the tab's split-collapse so the layout heals AND the
      // native webviews are destroyed; fall back to a plain destroy if needed.
      if (window.xnautClosePaneByElement) await window.xnautClosePaneByElement(pane, tabId);
      else await destroyBrowserPane(label);
    };

    panes.set(label, entry);
    return { kind: 'browser', label, pane, url };
  }

  async function destroyBrowserPane(label) {
    const invoke = inv();
    const entry = panes.get(label);
    if (!entry) return;
    entry.resizeObs.disconnect();
    if (invoke) {
      for (const pg of entry.pages) {
        await invoke('browser_pane_destroy', { label: pg.label }).catch(() => {});
      }
    }
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
    panes.forEach((entry) => {
      const visible = entry.tabId === activeTabId && document.body.contains(entry.paneEl);
      entry.pages.forEach((pg, i) => {
        if (visible && i === entry.activeIdx) {
          const pr = entry.paneEl.getBoundingClientRect();
          const br = entry.barEl.getBoundingClientRect();
          const INSET = 6;
          invoke('browser_pane_set_visible', { label: pg.label, visible: true }).catch(() => {});
          invoke('browser_pane_set_bounds', {
            req: {
              label: pg.label,
              x: pr.left + INSET,
              y: br.bottom + off,
              width: Math.max(pr.width - INSET * 2, 1),
              height: Math.max(pr.bottom - br.bottom - INSET, 1),
            },
          }).catch(() => {});
        } else {
          invoke('browser_pane_set_visible', { label: pg.label, visible: false }).catch(() => {});
        }
      });
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
      panes.forEach((entry) => {
        if (!document.body.contains(entry.paneEl)) return;
        const pg = entry.pages[entry.activeIdx];
        if (!pg) return;
        const pr = entry.paneEl.getBoundingClientRect();
        const br = entry.barEl.getBoundingClientRect();
        const INSET = 6;
        inv()('browser_pane_set_bounds', {
          req: { label: pg.label, x: pr.left + INSET, y: br.bottom + off, width: Math.max(pr.width - INSET * 2, 1), height: Math.max(pr.bottom - br.bottom - INSET, 1) },
        }).catch(() => {});
      });
    });
  });

  // ── public API hooks ────────────────────────────────────────────────────
  window.xnautCreateBrowserPane = createBrowserPane;
  window.xnautDestroyBrowserPane = destroyBrowserPane;
  // Drop a pane from the Map + stop observing, WITHOUT touching the DOM — used
  // by app.js's split-collapse, which removes the pane element itself.
  window.xnautForgetBrowserPane = (label) => {
    const entry = panes.get(label);
    if (!entry) return;
    try { entry.resizeObs.disconnect(); } catch (_) { /* already gone */ }
    // app.js's close path destroys only terminal.label (the first page); any
    // extra pages would leak their native webviews without this sweep.
    const invoke = inv();
    if (invoke) {
      entry.pages.forEach((pg) => {
        if (pg.label !== label) invoke('browser_pane_destroy', { label: pg.label }).catch(() => {});
      });
    }
    panes.delete(label);
  };
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
    if (!btn) return;
    btn.title = 'Browser · shift-click for another tab';
    // XNAUT-149: the globe opens ONE browser tab. If one already exists,
    // focus it instead of stacking new tabs; pages multiply via the (+)
    // page strip inside the pane. Shift-click forces an additional tab.
    btn.onclick = (e) => {
      if (!e.shiftKey) {
        for (const entry of panes.values()) {
          if (document.body.contains(entry.paneEl) && typeof window.xnautSwitchTab === 'function') {
            window.xnautSwitchTab(entry.tabId);
            return;
          }
        }
      }
      newBrowserTab().catch((err) => console.error('new browser tab failed:', err));
    };
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wireButton);
  } else {
    wireButton();
  }
})();
