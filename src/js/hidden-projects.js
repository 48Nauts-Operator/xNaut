// Hidden projects — shared by the sidebar and the Projects panel.
//
// For demos: right-click a project, Hide, and it disappears from the lists
// without being removed from the registry or touched on disk. Unhide from the
// "N hidden" affordance each list shows while anything is hidden.
//
// Two scopes because the lists key projects differently: the sidebar uses the
// registry task id, the PM panel uses the project key. Same store, same code,
// one scope each — rather than two implementations that drift.
(function () {
  'use strict';

  const KEY = 'xnaut-hidden-projects';

  function read() {
    try {
      const v = JSON.parse(localStorage.getItem(KEY) || '{}');
      return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
    } catch (_) {
      return {};
    }
  }

  function write(all) {
    try { localStorage.setItem(KEY, JSON.stringify(all)); } catch (_) { /* quota — ignore */ }
  }

  const api = {
    /** Hidden ids for a scope ('sidebar' | 'pm'). */
    list(scope) {
      const v = read()[scope];
      return Array.isArray(v) ? v : [];
    },
    isHidden(scope, id) {
      return id != null && api.list(scope).includes(String(id));
    },
    count(scope) {
      return api.list(scope).length;
    },
    /** Hide or unhide one project. Returns the new hidden state. */
    toggle(scope, id) {
      if (id == null) return false;
      id = String(id);
      const all = read();
      const cur = Array.isArray(all[scope]) ? all[scope] : [];
      const hidden = cur.includes(id);
      all[scope] = hidden ? cur.filter((x) => x !== id) : cur.concat([id]);
      write(all);
      return !hidden;
    },
    clear(scope) {
      const all = read();
      delete all[scope];
      write(all);
    },
  };

  window.xnautHiddenProjects = api;
})();
