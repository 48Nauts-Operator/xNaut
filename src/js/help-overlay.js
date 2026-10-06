// ABOUTME: Self-contained Help overlay — slide-out panel listing keyboard shortcuts.
// Deliberately independent of app.js/Tauri so it works in the packaged app *and*
// in a plain browser (app.js blocks on window.__TAURI__, this must not).
(function () {
  'use strict';

  // Mirror of app.js DEFAULT_KEYBINDINGS so the overlay shows the real defaults
  // even before the main app has booted. When the user has rebound keys we merge
  // their saved overrides from localStorage on top.
  const DEFAULTS = {
    newTab:          { key: 't', ctrl: true,  label: 'New Tab' },
    closeTab:        { key: 'w', ctrl: true,  label: 'Close Tab' },
    historySearch:   { key: 'r', ctrl: true,  label: 'Command History' },
    splitHorizontal: { code: 'KeyD', shift: true, alt: true, label: 'Split Horizontal' },
    splitVertical:   { code: 'KeyD', alt: true, label: 'Split Vertical' },
    splitBrowser:    { code: 'KeyB', alt: true, label: 'Split → Browser' },
    splitMarkdown:   { code: 'KeyM', alt: true, label: 'Split → Markdown' },
    closePane:       { code: 'KeyW', alt: true, label: 'Close Pane' },
    paneLeft:        { code: 'ArrowLeft',  alt: true, label: 'Focus Pane Left' },
    paneRight:       { code: 'ArrowRight', alt: true, label: 'Focus Pane Right' },
    paneUp:          { code: 'ArrowUp',    alt: true, label: 'Focus Pane Up' },
    paneDown:        { code: 'ArrowDown',  alt: true, label: 'Focus Pane Down' },
    toggleRalph:     { code: 'KeyR', ctrl: true, shift: true, label: 'Toggle Ralph Panel' },
  };

  // Grouping for a friendlier layout in the overlay.
  const GROUPS = [
    { title: 'Tabs & History', actions: ['newTab', 'closeTab', 'historySearch'] },
    { title: 'Split Panes',    actions: ['splitVertical', 'splitHorizontal', 'splitBrowser', 'splitMarkdown', 'closePane'] },
    { title: 'Navigate Panes', actions: ['paneLeft', 'paneRight', 'paneUp', 'paneDown'] },
    { title: 'Panels',         actions: ['toggleRalph'] },
  ];

  function loadBindings() {
    const merged = {};
    for (const k of Object.keys(DEFAULTS)) merged[k] = { ...DEFAULTS[k] };
    try {
      const saved = JSON.parse(localStorage.getItem('xnaut-keybindings') || 'null');
      if (saved && typeof saved === 'object') {
        for (const k of Object.keys(saved)) {
          // keep our label if the saved override has none
          merged[k] = { ...(merged[k] || {}), ...saved[k] };
        }
      }
    } catch (_) { /* ignore malformed storage */ }
    return merged;
  }

  function formatBinding(binding) {
    if (!binding) return '';
    const parts = [];
    if (binding.ctrl) parts.push('Ctrl');
    if (binding.alt) parts.push('Opt');
    if (binding.shift) parts.push('Shift');
    if (binding.meta) parts.push('Cmd');
    const arrows = { ArrowLeft: '←', ArrowRight: '→', ArrowUp: '↑', ArrowDown: '↓' };
    let keyName = binding.key || binding.code || '';
    if (arrows[keyName]) keyName = arrows[keyName];
    else keyName = keyName.replace(/^Key/, '').replace(/^Digit/, '');
    parts.push(keyName.length === 1 ? keyName.toUpperCase() : keyName);
    return parts;
  }

  function renderList() {
    const list = document.getElementById('help-overlay-list');
    if (!list) return;
    const bindings = loadBindings();
    list.innerHTML = '';

    for (const group of GROUPS) {
      const section = document.createElement('div');
      section.className = 'help-group';

      const h = document.createElement('h3');
      h.className = 'help-group-title';
      h.textContent = group.title;
      section.appendChild(h);

      for (const action of group.actions) {
        const binding = bindings[action];
        if (!binding) continue;
        const row = document.createElement('div');
        row.className = 'help-row';

        const label = document.createElement('span');
        label.className = 'help-row-label';
        label.textContent = binding.label || action;

        const keys = document.createElement('span');
        keys.className = 'help-row-keys';
        for (const part of formatBinding(binding)) {
          const kbd = document.createElement('kbd');
          kbd.className = 'help-kbd';
          kbd.textContent = part;
          keys.appendChild(kbd);
        }

        row.appendChild(label);
        row.appendChild(keys);
        section.appendChild(row);
      }
      list.appendChild(section);
    }

    // A couple of global helpers that live outside the keybinding registry.
    const extra = document.createElement('div');
    extra.className = 'help-group';
    extra.innerHTML =
      '<h3 class="help-group-title">Help</h3>' +
      '<div class="help-row"><span class="help-row-label">Open this menu</span>' +
      '<span class="help-row-keys"><kbd class="help-kbd">?</kbd></span></div>' +
      '<div class="help-row"><span class="help-row-label">Close this menu</span>' +
      '<span class="help-row-keys"><kbd class="help-kbd">Esc</kbd></span></div>';
    list.appendChild(extra);
  }

  function isOpen() {
    const ov = document.getElementById('help-overlay');
    return !!(ov && ov.classList.contains('open'));
  }

  // A hidden native webview freezes CSS transitions at their starting values.
  // The transparent backdrop still intercepts clicks while the panel remains
  // offscreen. Apply the real final state without motion in that lifecycle;
  // ordinary visible-window transitions remain unchanged. XNAUT-467.
  function syncMotion() {
    const hidden = document.hidden;
    for (const id of ['help-overlay', 'help-overlay-backdrop']) {
      document.getElementById(id)?.classList.toggle('help-no-motion', hidden);
    }
    if (hidden && !isOpen()) {
      const ov = document.getElementById('help-overlay');
      if (ov) ov.hidden = true;
    }
    return hidden;
  }

  function openOverlay() {
    const ov = document.getElementById('help-overlay');
    const btn = document.getElementById('btn-help');
    const backdrop = document.getElementById('help-overlay-backdrop');
    if (!ov) return;
    const hidden = syncMotion();
    renderList();
    ov.hidden = false;
    // force reflow so the transform transition runs from the hidden state
    void ov.offsetWidth;
    ov.classList.add('open');
    if (backdrop) backdrop.classList.add('show');
    if (btn) btn.setAttribute('aria-expanded', 'true');
    const closeBtn = document.getElementById('btn-help-close');
    if (closeBtn && !hidden) closeBtn.focus();
  }

  function closeOverlay() {
    const ov = document.getElementById('help-overlay');
    const btn = document.getElementById('btn-help');
    const backdrop = document.getElementById('help-overlay-backdrop');
    if (!ov) return;
    const hidden = syncMotion();
    ov.classList.remove('open');
    if (backdrop) backdrop.classList.remove('show');
    if (btn) {
      btn.setAttribute('aria-expanded', 'false');
      if (!hidden) btn.focus();
    }
    if (hidden) { ov.hidden = true; return; }
    // hide after the slide-out transition completes
    const onEnd = () => { if (!isOpen()) ov.hidden = true; ov.removeEventListener('transitionend', onEnd); };
    ov.addEventListener('transitionend', onEnd);
    // fallback in case transitionend doesn't fire
    setTimeout(() => { if (!isOpen()) ov.hidden = true; }, 400);
  }

  function toggleOverlay() { isOpen() ? closeOverlay() : openOverlay(); }

  // Expose for the main app / other callers.
  window.toggleHelpOverlay = toggleOverlay;
  window.openHelpOverlay = openOverlay;
  window.closeHelpOverlay = closeOverlay;

  function typingInField(e) {
    const el = e.target;
    if (!el) return false;
    const tag = (el.tagName || '').toLowerCase();
    return tag === 'input' || tag === 'textarea' || el.isContentEditable ||
      // xterm's helper textarea
      (el.classList && el.classList.contains('xterm-helper-textarea'));
  }

  function wire() {
    const btn = document.getElementById('btn-help');
    if (btn && !btn.dataset.helpWired) {
      btn.dataset.helpWired = '1';
      btn.addEventListener('click', (e) => { e.stopPropagation(); toggleOverlay(); });
    }
    const closeBtn = document.getElementById('btn-help-close');
    if (closeBtn && !closeBtn.dataset.helpWired) {
      closeBtn.dataset.helpWired = '1';
      closeBtn.addEventListener('click', closeOverlay);
    }
    const backdrop = document.getElementById('help-overlay-backdrop');
    if (backdrop && !backdrop.dataset.helpWired) {
      backdrop.dataset.helpWired = '1';
      backdrop.addEventListener('click', closeOverlay);
    }
  }

  // Global keyboard: `?` opens, Esc closes. Registered once.
  document.addEventListener('visibilitychange', syncMotion);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && isOpen()) {
      e.preventDefault();
      closeOverlay();
      return;
    }
    if (e.key === '?' && !typingInField(e) && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      toggleOverlay();
    }
  });

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }
})();
