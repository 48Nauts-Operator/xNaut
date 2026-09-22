// The project switcher (XNAUT-435). One control, one list, one place.
//
// It existed once and in the wrong room. The Delivery panel drew its own
// dropdown at the top of its left column (delivery-panel.js `projectSelect`)
// and the workspace drew none at all, so the only way to change project on the
// Code tab was the sidebar's project tree — and the Sessions list replaces that
// tree, which leaves the workspace with no way in and no way across. André,
// 2026-09-22, on the Delivery tab: "This dropdown has to be on the code tab
// too. Same dropdown."
//
// So the markup here IS Delivery's markup, lifted out rather than reimplemented,
// and both call sites now render from this one module. What changed on the way
// out is the class prefix: `dlv-` on a control that renders inside the workspace
// would be a lie about which file owns it, so these are `xps-` and carry their
// own rules. The Delivery stylesheet keeps its own for the rest of its column.
//
// Assigned as an OBJECT with two methods, and both call sites use it as one.
// Per CLAUDE.md: an undefined `window.*` is a silent no-op, and a function
// exported where a value is expected is worse, so the shape is stated here and
// checked at every call site before it is used.
(function () {
  'use strict';

  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  function injectStyles() {
    if (document.getElementById('project-select-styles')) return;
    const style = document.createElement('style');
    style.id = 'project-select-styles';
    // The measurements are Delivery's (delivery-panel.js `.dlv-side-h`,
    // `.dlv-side-pad`, `.dlv-select`), so the control reads as the same control
    // wherever it is mounted. Copied rather than shared because the Delivery
    // stylesheet is scoped to `.dlv-pane` and the workspace is not inside one.
    style.textContent = `
      .xps { display:block; flex:0 0 auto; }
      .xps-h { padding:4px 12px 6px; font-size:10.5px; letter-spacing:.09em; text-transform:uppercase;
        color:var(--text-muted,#8a8f98); }
      .xps-pad { padding:0 12px 8px; }
      .xps-select { width:100%; min-height:30px; padding:4px 8px;
        border:1px solid var(--border-color,#3a3d45); border-radius:6px;
        background:var(--bg-secondary,#141419); color:var(--text-primary,#e0e0e0);
        font:inherit; font-size:12px; }
      .xps-select:focus { outline:2px solid var(--accent,#4f8cff); outline-offset:1px; }
    `;
    document.head.appendChild(style);
  }

  // The options, in the order the caller's list arrived in. `selectedKey` is a
  // project key; a key that is not in the list selects nothing, which is the
  // honest rendering of "the workspace is on a project this list does not have"
  // rather than silently lighting up the first row.
  function html(projects, selectedKey) {
    injectStyles();
    const list = Array.isArray(projects) ? projects : [];
    const options = list.map((project) => {
      const key = String(project.key || '');
      return `<option value="${esc(key)}"${key && key === selectedKey ? ' selected' : ''}>${
        esc(project.name || key)}</option>`;
    }).join('');
    return `<div class="xps"><div class="xps-h">Project</div>`
      + `<div class="xps-pad"><select class="xps-select" aria-label="Project">${
        options || '<option value="">No projects</option>'}</select></div></div>`;
  }

  // Wires whichever select `html` put inside `container`, and hands it back so a
  // caller can focus it. Returns null when the container has none — a caller
  // must handle that rather than assume, because `html` is not always rendered
  // (the Delivery panel folded into the workspace renders no select of its own).
  function bind(container, onChange) {
    const select = container && container.querySelector('.xps-select');
    if (!select) return null;
    select.onchange = () => { if (typeof onChange === 'function') onChange(select.value); };
    return select;
  }

  window.xnautProjectSelect = { html, bind };
})();
